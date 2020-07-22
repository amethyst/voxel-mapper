use handsome_voxels::control::camera::{MainCameraTag, ThirdPersonCameraState};

use amethyst::{
    core::{ecs::prelude::*, math::Vector3},
    renderer::{debug_drawing::DebugLinesComponent, palette::Srgba},
};

#[derive(Default)]
pub struct CameraFeetHintTag;

impl Component for CameraFeetHintTag {
    type Storage = NullStorage<Self>;
}

pub struct DrawCameraFeetSystem;

impl<'a> System<'a> for DrawCameraFeetSystem {
    type SystemData = (
        WriteStorage<'a, DebugLinesComponent>,
        ReadStorage<'a, CameraFeetHintTag>,
        ReadStorage<'a, MainCameraTag>,
        WriteStorage<'a, ThirdPersonCameraState>,
    );

    fn run(&mut self, (mut debug_lines, is_feet, is_main_camera, tpc_states): Self::SystemData) {
        // Get the camera feet position.
        let feet_position = match (&is_main_camera, &tpc_states).join().next() {
            Some((_, tpc_state)) => tpc_state.feet + Vector3::new(0.0, 0.5, 0.0),
            None => return,
        };

        // Move the feet graphic.
        for (_, lines) in (&is_feet, &mut debug_lines).join() {
            lines.clear();
            lines.add_sphere(feet_position, 0.5, 20, 20, Srgba::new(1.0, 0.0, 1.0, 1.0));
        }
    }
}

pub fn make_camera_feet_lines(world: &mut World) -> Entity {
    world
        .create_entity()
        .with(CameraFeetHintTag)
        .with(DebugLinesComponent::new())
        .build()
}
