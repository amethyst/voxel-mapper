use super::{
    colliding_controller::CollidingController, input::ProcessedInput, smoother::TransformSmoother,
    CameraController, ThirdPersonCameraState, ThirdPersonControlConfig,
};
use crate::{collision::VoxelBVT, voxel::VoxelMap};

use amethyst::core::Transform;

pub struct FinalController {
    control_config: ThirdPersonControlConfig,
    colliding_controller: CollidingController,
    smoother: TransformSmoother,
}

impl FinalController {
    pub fn new(control_config: ThirdPersonControlConfig) -> Self {
        let smoother = TransformSmoother::new(control_config.smoothing_weight);

        FinalController {
            control_config,
            colliding_controller: CollidingController::new(),
            smoother,
        }
    }
}

impl CameraController for FinalController {
    fn update(
        &mut self,
        camera_state: &ThirdPersonCameraState,
        input: &ProcessedInput,
        voxel_map: &VoxelMap,
        voxel_bvt: &VoxelBVT,
    ) -> (Transform, ThirdPersonCameraState) {
        let new_camera_state = self.colliding_controller.apply_input(
            &self.control_config,
            camera_state,
            input,
            voxel_map,
            voxel_bvt,
        );

        (
            self.smoother.new_transform(&new_camera_state),
            new_camera_state,
        )
    }
}
