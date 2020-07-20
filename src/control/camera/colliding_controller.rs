use super::{
    floor_translation::translate_over_floor, input::ProcessedInput, ThirdPersonCameraState,
    ThirdPersonControlConfig,
};

use crate::{
    collision::{earliest_toi, extreme_ball_voxel_impact, VoxelBVT},
    voxel::VoxelMap,
};

use amethyst::core::{approx::relative_eq, math::Point3};
use ncollide3d::query::TOI;

// BUG: camera can get through walls if the target is on a wall and camera rotated between the wall
// and target

const BALL_RADIUS: f32 = 0.2;

fn move_ball_until_collision(
    start: &Point3<f32>,
    end: &Point3<f32>,
    voxel_bvt: &VoxelBVT,
    cmp_fn: impl Fn(TOI<f32>, TOI<f32>) -> TOI<f32>,
    predicate_fn: impl Fn(&TOI<f32>) -> bool,
) -> (bool, Point3<f32>) {
    if let Some(impact) = extreme_ball_voxel_impact(
        BALL_RADIUS,
        *start,
        *end,
        &voxel_bvt,
        0.01,
        cmp_fn,
        predicate_fn,
    ) {
        // Move ball up until an impact occurs. Make sure not to go in reverse (negative stop_time).
        // Note: this calculation works because `extreme_ball_voxel_impact` ensures the max TOI is
        // 1.0.
        let stop_time = impact.toi;
        debug_assert!(0.0 <= stop_time);
        debug_assert!(stop_time <= 1.0);

        (true, start + stop_time * (end - start))
    } else {
        (false, *end)
    }
}

/// Resolves collisions to prevent occluding the target; it does this by casting a ray from the
/// target to the desired camera position, stopping at the first collision.
pub struct CollidingController {
    colliding: bool,
}

impl CollidingController {
    pub fn new() -> Self {
        Self { colliding: false }
    }

    pub fn apply_input(
        &mut self,
        config: &ThirdPersonControlConfig,
        cam_state: &ThirdPersonCameraState,
        input: &ProcessedInput,
        voxel_map: &VoxelMap,
        voxel_bvt: &VoxelBVT,
    ) -> ThirdPersonCameraState {
        let mut cam_state = *cam_state;
        cam_state.feet = translate_over_floor(
            &cam_state.feet,
            &input.feet_translation,
            &voxel_map.voxels,
            true,
        );
        cam_state.stand_up();
        cam_state.add_yaw(input.delta_yaw);
        cam_state.add_pitch(input.delta_pitch);

        if input.radius_scalar > 1.0 {
            // Don't move the camera if it's colliding.
            if !self.colliding {
                cam_state.scale_radius(input.radius_scalar, config);
            }
        } else if input.radius_scalar < 1.0 {
            // If the desired radius is longer than actual because of collision, just truncate it
            // so the camera moves as soon as the player starts shortening the radius.
            let actual_radius = (cam_state.target - cam_state.actual_position).norm();
            cam_state.set_radius(actual_radius, config);

            cam_state.scale_radius(input.radius_scalar, config);
        }

        let (was_collision, camera_after_collisions) = move_ball_until_collision(
            &cam_state.target,
            &cam_state.get_desired_position(),
            &voxel_bvt,
            earliest_toi,
            |toi| !relative_eq!(toi.toi, 0.0), // Don't collide starting at the target.
        );
        self.colliding = was_collision;
        cam_state.actual_position = camera_after_collisions;

        cam_state
    }
}
