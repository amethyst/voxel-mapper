use super::{
    colliding_controller::CollidingController, input::ProcessedInput, smoother::TransformSmoother,
    CameraController, ThirdPersonCameraState, ThirdPersonControlConfig,
};

use voxel_mapper::{collision::VoxelBVT, voxel::VoxelMap};

use amethyst::core::Transform;
use ilattice3::palette_lattice_map::PaletteLatticeMapReader;

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
        // TODO: flush to the global cache
        let map_reader = PaletteLatticeMapReader::new(&voxel_map.voxels);

        let new_camera_state = self.colliding_controller.apply_input(
            &self.control_config,
            *camera_state,
            input,
            &map_reader,
            voxel_bvt,
        );
        let smooth_tfm = self.smoother.smooth_transform(&new_camera_state);

        (smooth_tfm, new_camera_state)
    }
}
