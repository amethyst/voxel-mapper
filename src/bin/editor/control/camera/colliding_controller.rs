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
use ncollide3d::query::TOI;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// BUG: it's possible for the camera to get behind walls if the camera target doesn't leave enough
// space; this could be fixed by pushing the feet away from walls or always keeping some minimum
// distance from walls in the floor_translation module

const BALL_RADIUS: f32 = 0.5;

const PATH_PERCENTAGE: f32 = 0.666;
const PATH_PERCENTAGE_SQ: f32 = PATH_PERCENTAGE * PATH_PERCENTAGE;

const MAX_ORTHOGONAL_DIST: f32 = 6.0;
const MAX_ORTHOGONAL_DIST_SQ: f32 = MAX_ORTHOGONAL_DIST * MAX_ORTHOGONAL_DIST;

const NOT_WORTH_SEARCHING_DIST: f32 = 4.0;
const NOT_WORTH_SEARCHING_DIST_SQ: f32 = NOT_WORTH_SEARCHING_DIST * NOT_WORTH_SEARCHING_DIST;

const MAX_SEARCH_ITERATIONS: usize = 200;

// Correlated with the MAX_ORTHOGONAL_DIST.
const PROJECTION_CONNECTION_MAX_ITERATIONS: usize = 10;

const CAMERA_LOCK_THRESHOLD: f32 = 2.0;
const CAMERA_LOCK_THRESHOLD_SQ: f32 = CAMERA_LOCK_THRESHOLD * CAMERA_LOCK_THRESHOLD;

const CAMERA_LOCK_RADIUS: f32 = 0.8;

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
    last_empty_feet_point: Option<lat::Point>,
}

impl CollidingController {
    pub fn new() -> Self {
        Self {
            colliding: false,
            last_empty_feet_point: None,
        }
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

        // Choose an empty voxel to start our search path.
        let feet_voxel = voxel_containing_point(&cam_state.feet);
        self.set_last_empty_feet_voxel(voxel_map, feet_voxel);
        let empty_path_start = self.last_empty_feet_point.as_ref().unwrap();

        // We always try to find a short path around voxels that occlude the target before doing
        // the sphere cast.
        let sphere_cast_start = find_start_of_sphere_cast(
            empty_path_start,
            cam_state.target,
            desired_position,
            voxel_map,
        );
        let (was_collision, camera_after_collisions) = move_ball_until_collision(
            &sphere_cast_start,
            &desired_position,
            voxel_bvt,
            earliest_toi,
            |_| true,
        );
        self.colliding = was_collision;

        if (camera_after_collisions - cam_state.target).norm_squared() < CAMERA_LOCK_THRESHOLD_SQ {
            // If we're really close to the target, wonky stuff can happen with collisions, so just
            // lock into a tight sphere.
            cam_state.actual_position = cam_state.get_position_at_radius(CAMERA_LOCK_RADIUS);
        } else {
            cam_state.actual_position = camera_after_collisions;
        }

        cam_state
    }

    fn set_last_empty_feet_voxel(&mut self, voxel_map: &VoxelMap, new_feet: lat::Point) {
        // HACK: really, the feet should never be in a non-empty voxel
        if self.last_empty_feet_point.is_some() {
            if voxel_map.voxel_is_empty(&new_feet) {
                self.last_empty_feet_point = Some(new_feet);
            }
        } else {
            self.last_empty_feet_point = Some(new_feet);
        }
    }
}

/// Try to find the ideal location to cast a sphere from.
fn find_start_of_sphere_cast(
    path_start: &lat::Point,
    target: Point3<f32>,
    camera: Point3<f32>,
    map: &VoxelMap,
) -> Point3<f32> {
    // If we're already pretty close to the camera, there's not much use in finding a path around
    // occluders.
    if (target - camera).norm_squared() < NOT_WORTH_SEARCHING_DIST_SQ {
        return target;
    }

    #[cfg(feature = "profiler")]
    profile_scope!("find_start_of_sphere_cast");

    let eye_ray = Line::from_endpoints(target, camera);
    let target_to_camera_dist_sq = eye_ray.v.norm_squared();

    // Graph search away from the target to get as close to the camera as possible.
    let path_finish = voxel_containing_point(&camera);
    let prevent_stray = |p: &lat::Point| {
        let LatPoint3(p) = (*p).into();
        let p_proj = project_point_onto_line(&p, &eye_ray);
        let p_rej = p - p_proj;

        // Don't let the search go too far in directions orthogonal to the eye vector.
        if p_rej.norm_squared() > MAX_ORTHOGONAL_DIST_SQ {
            return false;
        }

        // Bound how far the search can get from the target in terms of the desired camera position.
        if (p - target).norm_squared() > PATH_PERCENTAGE_SQ * target_to_camera_dist_sq {
            return false;
        }

        true
    };
    let (_reached_finish, path) = find_path_through_empty_voxels(
        path_start,
        &path_finish,
        map,
        prevent_stray,
        MAX_SEARCH_ITERATIONS,
    );

    // Figure out, at a minimum, how far along the path we want to start the sphere cast.
    let path_end = if let Some(end) = path.last() {
        let LatPoint3(end_float) = (*end).into();

        project_point_onto_line(&end_float, &eye_ray)
    } else {
        // Path is empty.
        return target;
    };
    let target_separation_sq = PATH_PERCENTAGE_SQ * (target - path_end).norm_squared();

    // Choose a point on the path as the starting point for the sphere cast. It should be some
    // minimum distance from the target along the ray subspace, and it should be in an empty voxel.
    for p in path.iter() {
        let LatPoint3(p_float) = (*p).into();
        let proj_p = project_point_onto_line(&p_float, &eye_ray);
        if (target - proj_p).norm_squared() < target_separation_sq {
            continue;
        }
        let voxel_proj_p = voxel_containing_point(&proj_p);
        if map.voxel_is_empty(&voxel_proj_p) {
            // Projection must still be path-connected to empty space.
            let (reached_finish, _) = find_path_through_empty_voxels(
                &voxel_proj_p,
                p,
                map,
                |_| true,
                PROJECTION_CONNECTION_MAX_ITERATIONS,
            );
            if reached_finish {
                return proj_p;
            }
        }
    }

    target
}
