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
use serde::{Deserialize, Serialize};

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// BUG: it's possible for the camera to get behind walls if the camera target doesn't leave enough
// space; this could be fixed by pushing the feet away from walls or always keeping some minimum
// distance from walls in the floor_translation module

/// Constant parameters for tuning the camera collision controller.
#[derive(Deserialize, Serialize)]
pub struct CameraCollisionConfig {
    /// Size of the collidable ball surrounding the camera.
    ball_radius: f32,
    /// When choosing a point on the camera search path, this gives us an allowable range in
    /// fraction of the total path length (values in [0, 1]).
    search_path_selection_range: (f32, f32),
    /// The maximum distance that the camera search can stray from the eye line. This determines
    /// how wide of an object the search can get around.
    max_orthogonal_dist: f32,
    /// The cutoff distance below which we don't event try doing a camera search.
    not_worth_searching_dist: f32,
    /// The maximum number of A* iterations we will do in the camera search. This is important so
    /// the search stops in a reasonable time if it can't connect with the camera.
    max_search_iterations: usize,
    /// When projecting a point on the search path onto the eye line, we need to make sure it's
    /// still path-connected to the same empty space (to avoid going through solid boundaries). We
    /// use another A* search to determine the connectivity, and this is the max # of iterations.
    /// Correlated with the `max_orthogonal_dist`.
    projection_connection_max_iterations: usize,
    /// If the distance to the camera target falls below this threshold, the camera locks into a
    /// fixed distance from the target.
    camera_lock_threshold: f32,
    /// When the camera is locked to a fixed distance from the target, this is that distance.
    camera_lock_radius: f32,
}

fn move_ball_until_collision(
    ball_radius: f32,
    start: &Point3<f32>,
    end: &Point3<f32>,
    voxel_bvt: &VoxelBVT,
    cmp_fn: impl Fn(TOI<f32>, TOI<f32>) -> TOI<f32>,
    predicate_fn: impl Fn(&TOI<f32>) -> bool,
) -> (bool, Point3<f32>) {
    if let Some(impact) = extreme_ball_voxel_impact(
        ball_radius,
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

        let config = &config.collision;

        // We always try to find a short path around voxels that occlude the target before doing
        // the sphere cast.
        let sphere_cast_start = find_start_of_sphere_cast(
            config,
            empty_path_start,
            cam_state.target,
            desired_position,
            voxel_map,
        );
        let (was_collision, camera_after_collisions) = move_ball_until_collision(
            config.ball_radius,
            &sphere_cast_start,
            &desired_position,
            voxel_bvt,
            earliest_toi,
            |_| true,
        );
        self.colliding = was_collision;

        if (camera_after_collisions - cam_state.target).norm_squared()
            < config.camera_lock_threshold.powi(2)
        {
            // If we're really close to the target, wonky stuff can happen with collisions, so just
            // lock into a tight sphere.
            cam_state.actual_position = cam_state.get_position_at_radius(config.camera_lock_radius);
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
    config: &CameraCollisionConfig,
    path_start: &lat::Point,
    target: Point3<f32>,
    camera: Point3<f32>,
    map: &VoxelMap,
) -> Point3<f32> {
    // If we're already pretty close to the camera, there's not much use in finding a path around
    // occluders.
    if (target - camera).norm_squared() < config.not_worth_searching_dist.powi(2) {
        return target;
    }

    #[cfg(feature = "profiler")]
    profile_scope!("find_start_of_sphere_cast");

    let eye_ray = Line::from_endpoints(target, camera);

    // Graph search away from the target to get as close to the camera as possible.
    let path_finish = voxel_containing_point(&camera);
    let prevent_stray = |p: &lat::Point| {
        let LatPoint3(p) = (*p).into();
        let p_proj = project_point_onto_line(&p, &eye_ray);
        let p_rej = p - p_proj;

        // Don't let the search go too far in directions orthogonal to the eye vector.
        if p_rej.norm_squared() > config.max_orthogonal_dist.powi(2) {
            return false;
        }

        true
    };
    let (_reached_finish, path) = find_path_through_empty_voxels(
        path_start,
        &path_finish,
        map,
        prevent_stray,
        config.max_search_iterations,
    );

    // Figure out the range of the path where we want to start the sphere cast.
    let path_end = if let Some(end) = path.last() {
        let LatPoint3(end_float) = (*end).into();

        project_point_onto_line(&end_float, &eye_ray)
    } else {
        // Path is empty.
        return target;
    };
    let path_len_sq = (target - path_end).norm_squared();
    let target_separation_low_sq = config.search_path_selection_range.0.powi(2) * path_len_sq;
    let target_separation_high_sq = config.search_path_selection_range.1.powi(2) * path_len_sq;

    // Choose a point on the path as the starting point for the sphere cast. It should fall in some
    // constrained range of the path, and it should be in an empty voxel. Prefer points closer to
    // the camera.
    for p in path.iter().rev() {
        let LatPoint3(p_float) = (*p).into();
        let proj_p = project_point_onto_line(&p_float, &eye_ray);
        let p_dist_sq = (target - proj_p).norm_squared();
        if p_dist_sq > target_separation_high_sq {
            continue;
        }
        if p_dist_sq < target_separation_low_sq {
            break;
        }
        let voxel_proj_p = voxel_containing_point(&proj_p);
        if map.voxel_is_empty(&voxel_proj_p) {
            // Projection must still be path-connected to empty space.
            let (reached_finish, _) = find_path_through_empty_voxels(
                &voxel_proj_p,
                p,
                map,
                |_| true,
                config.projection_connection_max_iterations,
            );
            if reached_finish {
                return proj_p;
            }
        }
    }

    target
}
