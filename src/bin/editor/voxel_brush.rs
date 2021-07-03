use crate::{
    bindings::{ActionBinding, GameBindings},
    control::{camera::data::CameraData, hover_3d::ObjectsUnderCursor},
};

use voxel_mapper::voxel::{
    centered_extent, chunk_cache_flusher::ChunkCacheFlusher, chunk_processor::MeshMode,
    double_buffer::EditedChunksBackBuffer, voxel_containing_point, Voxel, VoxelChunkReader,
    VoxelMap, VoxelType, EMPTY_VOXEL,
};

use amethyst::{
    core::ecs::prelude::*,
    derive::SystemDesc,
    input::{Button, InputEvent, InputHandler, VirtualKeyCode},
    shrev::EventChannel,
};
use building_blocks::prelude::*;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

#[derive(SystemDesc)]
#[system_desc(name(VoxelBrushSystemDesc))]
pub struct VoxelBrushSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<InputEvent<GameBindings>>,
}

impl VoxelBrushSystem {
    pub fn new(reader_id: ReaderId<InputEvent<GameBindings>>) -> Self {
        VoxelBrushSystem { reader_id }
    }
}

pub struct PaintBrush {
    pub voxel_type: VoxelType,
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

impl<'a> System<'a> for VoxelBrushSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Read<'a, EventChannel<InputEvent<GameBindings>>>,
        Read<'a, InputHandler<GameBindings>>,
        Read<'a, ObjectsUnderCursor>,
        ReadExpect<'a, VoxelMap>,
        ReadExpect<'a, ChunkCacheFlusher>,
        WriteExpect<'a, PaintBrush>,
        WriteExpect<'a, MeshMode>,
        WriteExpect<'a, EditedChunksBackBuffer>,
        CameraData<'a>,
    );

    fn run(
        &mut self,
        (
            input_events,
            input_handler,
            objects,
            voxel_map,
            cache_flusher,
            mut brush,
            mut mesh_mode,
            mut voxel_backbuffer,
            ray_data,
        ): Self::SystemData,
    ) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_brush");

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
                InputEvent::ActionPressed(ActionBinding::ChangeMeshMode) => {
                    *mesh_mode = match *mesh_mode {
                        MeshMode::SurfaceNets => MeshMode::GreedyQuads,
                        MeshMode::GreedyQuads => MeshMode::SurfaceNets,
                    };
                }
                InputEvent::ButtonPressed(Button::Key(key)) => {
                    if key_is_number(*key) {
                        brush.voxel_type = VoxelType(key_number(*key) as u8);
                        log::info!("Set voxel paintbrush to {:?}", brush.voxel_type);
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
        let brush_center = voxel_containing_point(center);

        let local_cache = LocalChunkCache3::new();
        let map_reader = voxel_map.voxels.reader(&local_cache);

        let mut lock_brush_dist_from_camera = false;
        if input_handler
            .action_is_down(&ActionBinding::CreateVoxel)
            .unwrap()
        {
            lock_brush_dist_from_camera = true;
            edit_sphere(
                SetVoxelOperation::MakeSolid,
                brush_center,
                brush.radius,
                brush.voxel_type,
                &map_reader,
                &mut *voxel_backbuffer,
            );
        } else if input_handler
            .action_is_down(&ActionBinding::RemoveVoxel)
            .unwrap()
        {
            lock_brush_dist_from_camera = true;
            edit_sphere(
                SetVoxelOperation::RemoveSolid,
                brush_center,
                brush.radius,
                EMPTY_VOXEL.voxel_type,
                &map_reader,
                &mut *voxel_backbuffer,
            );
        }

        if !lock_brush_dist_from_camera {
            if let Some((_cam, cam_tfm)) = ray_data.get_main_camera() {
                brush.dist_from_camera =
                    objects
                        .voxel
                        .as_ref()
                        .map(|v| v.impact.impact.toi)
                        .or(objects
                            .xz_plane
                            .map(|p| (*cam_tfm.translation() - p.coords).norm()));
            }
        }

        cache_flusher.flush(local_cache);
    }
}

const SDF_GROWTH_FACTOR: f32 = 10.0;

fn edit_sphere(
    operation: SetVoxelOperation,
    center: Point3i,
    radius: u32,
    voxel_type: VoxelType,
    map_reader: &VoxelChunkReader,
    voxel_backbuffer: &mut EditedChunksBackBuffer,
) {
    let fradius = radius as f32;
    let sign = match operation {
        SetVoxelOperation::MakeSolid => -1,
        SetVoxelOperation::RemoveSolid => 1,
    };
    voxel_backbuffer.edit_voxels_out_of_place(
        map_reader,
        &centered_extent(center, radius),
        |p: Point3i, v: &mut Voxel| {
            let dist = (p - center).norm();

            // Change the SDF faster closer to the center.
            let sdf_delta = sign
                * (SDF_GROWTH_FACTOR * (1.0 - dist / fradius))
                    .max(0.0)
                    .round() as i16;
            let new_dist = v.distance.0 as i16 + sdf_delta;

            v.distance.0 = new_dist.max(std::i8::MIN as i16).min(std::i8::MAX as i16) as i8;

            if sdf_delta < 0 && v.distance.0 < 0 {
                // Only set to the brush type if the voxel is solid.
                v.voxel_type = voxel_type;
            } else if sdf_delta > 0 && v.distance.0 >= 0 {
                v.voxel_type = EMPTY_VOXEL.voxel_type;
            }
        },
    );
}

fn key_number(code: VirtualKeyCode) -> u32 {
    (code as u32 + 1) % 10
}

fn key_is_number(code: VirtualKeyCode) -> bool {
    code as u32 + 1 <= 10
}
