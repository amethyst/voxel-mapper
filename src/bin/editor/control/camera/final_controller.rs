use super::{
    colliding_controller::CollidingController, input::ProcessedInput, smoother::TransformSmoother,
    ThirdPersonCameraState, ThirdPersonControlConfig,
};

use voxel_mapper::{collision::VoxelBVT, voxel::IsFloor};

use amethyst::core::Transform;
use building_blocks::prelude::*;

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

    pub fn update<V, T>(
        &mut self,
        camera_state: &ThirdPersonCameraState,
        input: &ProcessedInput,
        voxels: &V,
        voxel_bvt: &VoxelBVT,
    ) -> (Transform, ThirdPersonCameraState)
    where
        V: for<'r> Get<&'r Point3i, Data = T>,
        T: IsEmpty + IsFloor,
    {
        let new_camera_state = self.colliding_controller.apply_input(
            &self.control_config,
            *camera_state,
            input,
            voxels,
            voxel_bvt,
        );
        let smooth_tfm = self.smoother.smooth_transform(&new_camera_state);

        (smooth_tfm, new_camera_state)
    }
}
