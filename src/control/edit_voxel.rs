use crate::{
    control::{
        bindings::{ActionBinding, GameBindings},
        camera::data::CameraData,
        hover_3d::ObjectsUnderCursor,
    },
    voxel::{
        decode_distance,
        setter::{SetVoxel, SetVoxelsEvent},
        voxel_containing_point, Voxel, VoxelMap, EMPTY_VOXEL,
    },
};

use amethyst::{
    core::ecs::prelude::*,
    derive::SystemDesc,
    input::{Button, InputEvent, InputHandler, VirtualKeyCode},
    shrev::EventChannel,
};
use ilattice3 as lat;
use ilattice3::{prelude::*, ChunkedLatticeMap};

#[derive(SystemDesc)]
#[system_desc(name(EditVoxelSystemDesc))]
pub struct EditVoxelSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<InputEvent<GameBindings>>,
}

impl EditVoxelSystem {
    pub fn new(reader_id: ReaderId<InputEvent<GameBindings>>) -> Self {
        EditVoxelSystem { reader_id }
    }
}

pub struct PaintBrush {
    pub voxel_address: u8,
    pub radius: u32,
    pub dist_from_camera: Option<f32>,
}

#[derive(Clone, Copy)]
pub enum SetVoxelOperation {
    /// Set voxels in the solid to negative distances from the surface, and surrounding voxels to
    /// the positive distance from the surface.
    MakeSolid,
    /// Set voxels in the solid to positive distances from the surface, and surrounding voxels to
    /// the negative distance from the surface.
    RemoveSolid,
}

impl<'a> System<'a> for EditVoxelSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Read<'a, EventChannel<InputEvent<GameBindings>>>,
        Read<'a, InputHandler<GameBindings>>,
        Read<'a, ObjectsUnderCursor>,
        ReadExpect<'a, VoxelMap>,
        WriteExpect<'a, PaintBrush>,
        Write<'a, EventChannel<SetVoxelsEvent>>,
        CameraData<'a>,
    );

    fn run(
        &mut self,
        (
            input_events,
            input_handler,
            objects,
            voxel_map,
            mut brush,
            mut set_voxel_events,
            ray_data,
        ): Self::SystemData,
    ) {
        // Make sure we at least consume the input events so we don't act on stale ones.
        let input_events: Vec<InputEvent<GameBindings>> =
            input_events.read(&mut self.reader_id).cloned().collect();

        for input_event in input_events.iter() {
            match input_event {
                InputEvent::ActionPressed(ActionBinding::IncreaseBrushRadius) => {
                    brush.radius += 1;
                    log::info!("Set brush radius to {}", brush.radius);
                }
                InputEvent::ActionPressed(ActionBinding::DecreaseBrushRadius) => {
                    brush.radius = (brush.radius - 1).max(1);
                    log::info!("Set brush radius to {}", brush.radius);
                }
                InputEvent::ButtonPressed(Button::Key(key)) => {
                    if key_is_number(*key) {
                        brush.voxel_address = key_number(*key) as u8;
                        log::info!("Set voxel paintbrush to address {}", brush.voxel_address);
                    }
                }
                _ => (),
            }
        }

        let (x, y) = match input_handler.mouse_position() {
            Some((x, y)) => (x, y),
            None => return,
        };

        // Figure out where the brush should go.
        let radius = brush.dist_from_camera.unwrap_or(20.0);
        let camera_ray = match ray_data.get_camera_ray(x, y) {
            Some(r) => r,
            None => return,
        };
        let center = camera_ray.origin + radius * camera_ray.dir;
        let brush_center = voxel_containing_point(&center);

        let mut lock_brush_dist_from_camera = false;
        if input_handler
            .action_is_down(&ActionBinding::CreateVoxel)
            .unwrap()
        {
            lock_brush_dist_from_camera = true;
            send_event_for_sphere(
                SetVoxelOperation::MakeSolid,
                brush_center,
                brush.radius,
                brush.voxel_address,
                &voxel_map.voxels.map,
                &mut set_voxel_events,
            );
        } else if input_handler
            .action_is_down(&ActionBinding::RemoveVoxel)
            .unwrap()
        {
            lock_brush_dist_from_camera = true;
            send_event_for_sphere(
                SetVoxelOperation::RemoveSolid,
                brush_center,
                brush.radius,
                0,
                &voxel_map.voxels.map,
                &mut set_voxel_events,
            );
        }

        if !lock_brush_dist_from_camera {
            if let Some((_cam, cam_tfm)) = ray_data.get_main_camera() {
                let dist_from_xz_point = objects
                    .xz_plane
                    .map(|p| (*cam_tfm.translation() - p.coords).norm());
                brush.dist_from_camera = objects
                    .voxel
                    .as_ref()
                    .map(|v| v.raycast_result.toi)
                    .or(dist_from_xz_point);
            }
        }
    }
}

fn send_event_for_sphere(
    operation: SetVoxelOperation,
    center: lat::Point,
    radius: u32,
    palette_address: u8,
    voxels: &ChunkedLatticeMap<Voxel>,
    set_voxel_events: &mut EventChannel<SetVoxelsEvent>,
) {
    let set_voxels = lat::Extent::from_center_and_radius(center, radius as i32 + 2)
        .into_iter()
        .filter_map(|p| {
            let diff = p - center;
            let dist = (diff.dot(&diff) as f32).sqrt() - radius as f32;
            if dist <= 1.0 {
                let old_voxel = voxels.maybe_get_world_ref(&p).unwrap_or(&EMPTY_VOXEL);
                Some((
                    p,
                    determine_new_voxel(old_voxel, operation, palette_address, dist),
                ))
            } else {
                // No need to set a voxel this far away from the sphere's surface.
                None
            }
        })
        .collect();
    set_voxel_events.single_write(SetVoxelsEvent { voxels: set_voxels });
}

fn determine_new_voxel(
    old_voxel: &Voxel,
    operation: SetVoxelOperation,
    new_addr: u8,
    new_dist: f32,
) -> SetVoxel {
    let old_addr = old_voxel.palette_address;
    let old_dist = decode_distance(old_voxel.distance);
    let old_solid = old_dist < 0.0;

    let (new_dist, mut new_addr) = match operation {
        SetVoxelOperation::MakeSolid => {
            let new_solid = new_dist < 0.0;
            if old_solid && !new_solid {
                // Voxel was already solid, we can't make it empty with this operation.
                (old_dist, old_addr)
            } else {
                (new_dist, new_addr)
            }
        }
        SetVoxelOperation::RemoveSolid => {
            // Negate the distance, since this is a remove operation.
            let new_dist = -new_dist;
            let new_solid = new_dist < 0.0;
            if !old_solid && new_solid {
                // Preserve old positive voxels adjacent to the sphere surface on removal.
                (old_dist, old_addr)
            } else {
                (new_dist, new_addr)
            }
        }
    };

    // Make sure we don't change the material when we change the distance of solid voxels that are
    // adjacent to removed voxels.
    if new_dist < 0.0 && new_addr == EMPTY_VOXEL.palette_address {
        new_addr = old_addr;
    }

    let new_solid = new_dist < 0.0;

    SetVoxel {
        palette_address: if new_solid {
            new_addr
        } else {
            // Non-solid voxels can't have non-empty attributes.
            EMPTY_VOXEL.palette_address
        },
        distance: new_dist,
    }
}

fn key_number(code: VirtualKeyCode) -> u32 {
    (code as u32 + 1) % 10
}

fn key_is_number(code: VirtualKeyCode) -> bool {
    code as u32 + 1 <= 10
}
