use super::{input::ProcessedInput, ThirdPersonCameraState, ThirdPersonControlConfig};

use voxel_mapper::{
    collision::{
        earliest_toi, extreme_ball_voxel_impact, floor_translation::translate_over_floor, VoxelBVT,
    },
    geometry::UP,
    voxel::VoxelMap,
};

use amethyst::core::{
    approx::relative_eq,
    math::{Point3, Vector3},
};
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

/// Resolves collisions to prevent occluding the target.
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
        mut cam_state: ThirdPersonCameraState,
        input: &ProcessedInput,
        voxel_map: &VoxelMap,
        voxel_bvt: &VoxelBVT,
    ) -> ThirdPersonCameraState {
        // Figure out the where the camera feet are.
        cam_state.feet = translate_over_floor(
            &cam_state.feet,
            &input.feet_translation,
            &voxel_map.voxels,
            true,
        );

        // Figure out where the camera target is. For simplicity, we look just above the feet. A
        // little separation from the ground helps avoid spurious collisions with the ground near
        // the target.
        cam_state.target = cam_state.feet + 1.0 * Vector3::from(UP);

        // Rotate around the target.
        cam_state.add_yaw(input.delta_yaw);
        cam_state.add_pitch(input.delta_pitch);

        // Scale the camera's distance from the target.
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

        let desired_position = cam_state.get_desired_position();

        // Check for transition from not colliding to colliding.
        if !self.colliding {
            // If we cast a sphere from the previous position to the new desired position and there
            // is a collision, then we assume that desired position is inside of a volume.
            let (was_collision, _) = move_ball_until_collision(
                &cam_state.actual_position,
                &desired_position,
                &voxel_bvt,
                earliest_toi,
                |toi| !relative_eq!(toi.toi, 0.0), // Don't collide starting at the target.
            );
            if !was_collision {
                // All good!
                cam_state.actual_position = desired_position;
                return cam_state;
            }
        }

        // Our desired position is colliding with something, so instead we cast a ball from the
        // target to the desired position and stop at the first collision.
        let (was_collision, camera_after_collisions) = move_ball_until_collision(
            &cam_state.target,
            &desired_position,
            &voxel_bvt,
            earliest_toi,
            |toi| !relative_eq!(toi.toi, 0.0), // Don't collide starting at the target.
        );
        // It is technically possible that we don't have a collision. We hope this case is rare and
        // not problematic.
        self.colliding = was_collision;
        cam_state.actual_position = camera_after_collisions;

        cam_state
    }
}
