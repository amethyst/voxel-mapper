use super::{input::ProcessedInput, ThirdPersonCameraState, ThirdPersonControlConfig};

use voxel_mapper::{
    collision::{
        earliest_toi, extreme_ball_voxel_impact, floor_translation::translate_over_floor, VoxelBVT,
    },
    geometry::{project_point_onto_line, Line, UP},
    voxel::{search::find_path_through_empty_voxels, voxel_containing_point, LatPoint3, VoxelMap},
};

use amethyst::core::math::{Point3, Vector3};
use ilattice3 as lat;
use ilattice3::{prelude::*, IsEmpty};
use ncollide3d::query::TOI;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// BUG: it's possible for the camera to get behind walls if the camera target doesn't leave enough
// space; this could be fixed by pushing the feet away from walls or always keeping some minimum
// distance from walls in the floor_translation module

const BALL_RADIUS: f32 = 0.5;

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
        0.0,
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
        let prev_pos = cam_state.actual_position;

        // Figure out the where the camera feet are.
        cam_state.feet = translate_over_floor(
            &cam_state.feet,
            &input.feet_translation,
            &voxel_map.voxels,
            true,
        );

        // Figure out where the camera target is.
        cam_state.target = cam_state.feet + Vector3::from(UP);

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

        // Check for transition between colliding states.
        if self.colliding {
            // See if the target has become unobstructed.
            let (was_collision, _) = move_ball_until_collision(
                &cam_state.target,
                &desired_position,
                voxel_bvt,
                earliest_toi,
                |_| true,
            );
            self.colliding = was_collision;
        } else {
            // If we cast a sphere from the previous position to the new desired position and there
            // is a collision, then we assume that desired position is inside of a volume.
            let (was_collision, _) = move_ball_until_collision(
                &cam_state.actual_position,
                &desired_position,
                voxel_bvt,
                earliest_toi,
                |_| true,
            );
            self.colliding = was_collision;
        }

        if !self.colliding {
            // All good!
            cam_state.actual_position = desired_position;
            return cam_state;
        }

        // We're in the colliding state, so instead we cast a ball to the desired position and stop
        // at the first collision.
        let sphere_cast_start =
            find_start_of_sphere_cast(cam_state.target, prev_pos, desired_position, voxel_map);
        let (was_collision, camera_after_collisions) = move_ball_until_collision(
            &sphere_cast_start,
            &desired_position,
            voxel_bvt,
            earliest_toi,
            |_| true,
        );
        self.colliding = was_collision;

        if (camera_after_collisions - cam_state.target).norm_squared() < 4.0 {
            // If we're really close to the target, wonky stuff can happen with collisions, so just
            // lock into a tight sphere.
            cam_state.actual_position = cam_state.get_position_at_radius(0.8);
        } else {
            cam_state.actual_position = camera_after_collisions;
        }

        cam_state
    }
}

/// Try to find the ideal location to cast a sphere from.
fn find_start_of_sphere_cast(
    target: Point3<f32>,
    prev_camera: Point3<f32>,
    camera: Point3<f32>,
    map: &VoxelMap,
) -> Point3<f32> {
    if (target - camera).norm_squared() < 16.0 {
        return target;
    }

    #[cfg(feature = "profiler")]
    profile_scope!("find_start_of_sphere_cast");

    let eye_ray = Line::from_endpoints(target, camera);

    // Graph search away from the target to get as close to the camera as possible.
    let path_start = voxel_containing_point(&target);
    let path_finish = voxel_containing_point(&camera);
    let prevent_stray = |p: &lat::Point| {
        let LatPoint3(p) = (*p).into();
        let p_proj = project_point_onto_line(&p, &eye_ray);
        let p_rej = p - p_proj;

        // Don't let the search go too far in directions orthogonal to the eye vector.
        p_rej.norm_squared() < 100.0
    };
    const MAX_ITERATIONS: usize = 200;
    let (_reached_finish, path) = find_path_through_empty_voxels(
        &path_start,
        &path_finish,
        map,
        prevent_stray,
        MAX_ITERATIONS,
    );

    // Use the previous camera position, since it's a good heuristic for how far the sphere cast
    // should expect to go if there aren't any new collisions.
    let target_camera_dist_sq = (target - prev_camera).norm_squared();

    // Choose a point on the path as the starting point for the sphere cast. It should be some
    // minimum distance from the target along the ray subspace, and it should be in an empty voxel.
    let target_separation_sq = 0.666 * 0.666 * target_camera_dist_sq;
    for p in path.iter() {
        let LatPoint3(p_float) = (*p).into();
        let proj_p = project_point_onto_line(&p_float, &eye_ray);
        if (target - proj_p).norm_squared() < target_separation_sq {
            continue;
        }
        let voxel_proj_p = voxel_containing_point(&proj_p);
        if let Some(v) = map.voxels.maybe_get_world_ref(&voxel_proj_p) {
            if v.is_empty() {
                // Projection must still be path-connected to empty space.
                let (reached_finish, _) =
                    find_path_through_empty_voxels(&voxel_proj_p, p, map, |_| true, 10);
                if reached_finish {
                    return proj_p;
                }
            }
        }
    }

    target
}
