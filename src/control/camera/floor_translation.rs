use crate::{
    geometry::{line_plane_intersection, screen_ray, Line, LinePlaneIntersection, Plane, UP},
    voxel::{voxel_containing_point, IsFloor},
};

use amethyst::{
    core::{
        math::{Point2, Point3, Vector3},
        Transform,
    },
    renderer::camera::Projection,
    window::ScreenDimensions,
};
use ilattice3 as lat;
use ilattice3::prelude::*;
use itertools::Itertools;
use ncollide3d::query::Ray;

fn _floor_drag_translation(
    floor_plane: &Plane,
    prev_screen_ray: &Line,
    screen_ray: &Line,
) -> Vector3<f32> {
    let p_now = line_plane_intersection(screen_ray, floor_plane);
    if let LinePlaneIntersection::IntersectionPoint(p_now) = p_now {
        let p_prev = line_plane_intersection(prev_screen_ray, floor_plane);
        if let LinePlaneIntersection::IntersectionPoint(p_prev) = p_prev {
            return p_prev - p_now;
        }
    }

    Vector3::zeros()
}

pub fn floor_drag_translation(
    floor_plane: &Plane,
    camera_tfm: &Transform,
    camera_proj: &Projection,
    dims: &ScreenDimensions,
    cursor_pos: Point2<f32>,
    prev_cursor_pos: Point2<f32>,
) -> Vector3<f32> {
    let prev_screen_ray = screen_ray(camera_tfm, camera_proj, dims, prev_cursor_pos);
    let screen_ray = screen_ray(camera_tfm, camera_proj, dims, cursor_pos);

    _floor_drag_translation(floor_plane, &prev_screen_ray, &screen_ray)
}

/// Returns all numbers `t` such that `bias + slope * t` is an integer and `t_0 <= t <= t_f`.
/// Always returns empty vector for constant (`slope == 0.0`) functions, even if the constant is an
/// integer (since there would be infinite solutions for `t`).
fn integer_points_on_line_1d(slope: f32, bias: f32, t_0: f32, t_f: f32) -> Vec<f32> {
    debug_assert!(t_0 < t_f, "Invalid time range");

    let mut solutions = Vec::new();

    if slope == 0.0 {
        return solutions;
    }

    let first_int = if slope > 0.0 {
        (bias + slope * t_0).ceil()
    } else {
        (bias + slope * t_0).floor()
    };

    let mut next_int_t = (first_int - bias) / slope;
    let time_per_int = 1.0 / slope.abs();
    while next_int_t <= t_f {
        solutions.push(next_int_t);
        next_int_t += time_per_int;
    }

    solutions
}

fn integer_point_on_line_3d(
    start: &Point3<f32>,
    velocity: &Vector3<f32>,
    t_0: f32,
    t_f: f32,
) -> Vec<f32> {
    let mut solutions = Vec::new();
    solutions.append(&mut integer_points_on_line_1d(
        velocity.x, start.x, t_0, t_f,
    ));
    solutions.append(&mut integer_points_on_line_1d(
        velocity.y, start.y, t_0, t_f,
    ));
    solutions.append(&mut integer_points_on_line_1d(
        velocity.z, start.z, t_0, t_f,
    ));
    solutions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    solutions.dedup();

    solutions
}

/// True if the voxel at `p` is not a floor voxel AND the voxel directly under `p` is a floor voxel.
fn point_is_on_top_of_floor<V, T>(p: &lat::Point, voxels: &V) -> bool
where
    V: MaybeGetWorldRef<Data = T>,
    T: IsFloor,
{
    let p_is_not_floor = if let Some(p_voxel) = voxels.maybe_get_world_ref(p) {
        !p_voxel.is_floor()
    } else {
        true
    };

    if p_is_not_floor {
        let under_p = *p - [0, 1, 0].into();
        let under_p_is_floor = if let Some(voxel) = voxels.maybe_get_world_ref(&under_p) {
            voxel.is_floor()
        } else {
            false
        };

        return under_p_is_floor;
    }

    false
}

const MAX_PROBE_ITERS: usize = 10;

// BUG: should check contiguous floor voxels
fn vertical_probe<V, T>(dir: i32, start: &lat::Point, voxels: &V) -> Option<i32>
where
    V: MaybeGetWorldRef<Data = T>,
    T: IsFloor,
{
    let mut p = *start;
    let mut dh = 0;
    for _ in 0..MAX_PROBE_ITERS {
        if point_is_on_top_of_floor(&p, voxels) {
            return Some(dh);
        }
        p = p + [0, dir, 0].into();
        dh += dir;
    }

    None
}

/// Moves a point along a translation vector while doing collision resolution with the floor voxels.
/// The point may only travel above floor voxels, and it will jump on top or fall down onto floor
/// voxels as it crosses voxel boundaries. If the point reaches a vertical column that contains no
/// floor voxels, then it stops.
pub fn translate_over_floor<V, T>(
    start: &Point3<f32>,
    velocity: &Vector3<f32>,
    voxels: &V,
    blocking_collisions: bool,
) -> Point3<f32>
where
    V: MaybeGetWorldRef<Data = T>,
    T: IsFloor,
{
    let ray = Ray::new(*start, *velocity);
    let up = Vector3::from(UP);

    // To detect when the point crosses a voxel boundary, get all of the points on the line segment
    // with any integer coordinates.
    let voxel_boundary_times = integer_point_on_line_3d(start, velocity, 0.0, 1.0);
    let mut boundary_points: Vec<(f32, Point3<f32>)> = voxel_boundary_times
        .into_iter()
        .map(|t| (t, start + t * velocity))
        .collect();
    // Include this point so we can get all of the midpoints (see below).
    boundary_points.push((1.0, start + velocity));

    // Translate the point up and down as it travels over floor voxels.
    let mut height_delta = 0;
    for ((t1, p1), (_, p2)) in boundary_points.iter().tuple_windows() {
        // Since we have points on voxel boundaries, it's hard to say what voxel we're leaving or
        // entering without seeing the delta. At least we know the midpoint between boundaries will
        // fall into the voxel we are entering.
        let midpoint = (p1 + p2.coords) / 2.0;
        let midpoint_voxel = voxel_containing_point(&midpoint);

        let voxel_p = midpoint_voxel + [0, height_delta, 0].into();

        let dh = if let Some(voxel) = voxels.maybe_get_world_ref(&voxel_p) {
            let probe_dir = if voxel.is_floor() { 1 } else { -1 };

            vertical_probe(probe_dir, &voxel_p, voxels)
        } else {
            None
        };

        if let Some(dh) = dh {
            height_delta += dh;
        } else {
            // Probing failed, so just stop.
            if blocking_collisions {
                return ray.point_at(*t1) + (height_delta as f32) * up;
            } else {
                continue;
            }
        }
    }

    start + velocity + (height_delta as f32) * up
}

// ████████╗███████╗███████╗████████╗███████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝██╔════╝
//    ██║   █████╗  ███████╗   ██║   ███████╗
//    ██║   ██╔══╝  ╚════██║   ██║   ╚════██║
//    ██║   ███████╗███████║   ██║   ███████║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝   ╚══════╝

#[cfg(test)]
mod tests {
    use super::*;

    use ilattice3 as lat;
    use ilattice3::ChunkedLatticeMap;

    use crate::{
        test_util::{assert_relative_eq_point3, assert_relative_eq_vec},
        voxel::{VOXEL_CHUNK_SIZE},
    };

    #[test]
    fn test_no_integer_points_on_line_1d() {
        assert_relative_eq_vec(&integer_points_on_line_1d(0.5, 0.0, 0.1, 0.2), &vec![]);
    }

    #[test]
    fn test_starting_integer_point_on_line() {
        assert_relative_eq_vec(&integer_points_on_line_1d(0.5, 0.0, 0.0, 1.0), &vec![0.0]);
    }

    #[test]
    fn test_starting_and_ending_integer_point_on_line() {
        assert_relative_eq_vec(
            &integer_points_on_line_1d(0.5, 0.0, 0.0, 2.0),
            &vec![0.0, 2.0],
        );
    }

    #[test]
    fn test_one_integer_point_on_line_before_full_interval_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_1d(0.5, 0.99, 0.0, 0.1),
            &vec![0.01 / 0.5],
        );
    }

    #[test]
    fn test_two_integer_point_on_line_before_two_full_intervals_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_1d(0.5, 0.99, 0.0, 2.5),
            &vec![0.01 / 0.5, 1.01 / 0.5],
        );
    }

    #[test]
    fn test_one_integer_point_on_line_with_negative_slope_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_1d(-0.5, 0.0, 0.0, 2.0),
            &vec![0.0, 2.0],
        );
    }

    #[derive(Clone, Default)]
    struct TestVoxel(bool);

    impl IsFloor for TestVoxel {
        fn is_floor(&self) -> bool {
            self.0
        }
    }

    fn empty_voxels() -> ChunkedLatticeMap<TestVoxel> {
        ChunkedLatticeMap::new(VOXEL_CHUNK_SIZE)
    }

    fn make_floor_strip(voxels: &mut ChunkedLatticeMap<TestVoxel>) {
        voxels.fill_extent(
            &lat::Extent::from_min_and_local_supremum([0, 0, 0].into(), [3, 1, 1].into()),
            TestVoxel(true),
            TestVoxel(false),
        );
        // Make some space above the floor to move through.
        voxels.fill_extent(
            &lat::Extent::from_min_and_local_supremum([0, 1, 0].into(), [3, 2, 1].into()),
            TestVoxel(false),
            TestVoxel(false),
        );
    }

    fn make_bump(voxels: &mut ChunkedLatticeMap<TestVoxel>) {
        voxels.fill_extent(
            &lat::Extent::from_min_and_local_supremum([1, 1, 0].into(), [1, 1, 1].into()),
            TestVoxel(true),
            TestVoxel(false),
        );
    }

    #[test]
    fn test_translate_over_floor_flat() {
        let mut voxels = empty_voxels();
        make_floor_strip(&mut voxels);

        let start = Point3::new(0.5, 1.5, 0.5);
        let velocity = Vector3::new(2.0, 0.0, 0.0);
        assert_relative_eq_point3(
            &translate_over_floor(&start, &velocity, &voxels, true),
            &(start + velocity),
        );
    }

    #[test]
    fn test_translate_over_floor_up_step() {
        let mut voxels = empty_voxels();
        make_floor_strip(&mut voxels);
        make_bump(&mut voxels);

        let start = Point3::new(0.5, 1.5, 0.5);
        let velocity = Vector3::new(1.0, 0.0, 0.0);
        assert_relative_eq_point3(
            &translate_over_floor(&start, &velocity, &voxels, true),
            &(start + velocity + Vector3::from(UP)),
        );
    }

    #[test]
    fn test_translate_over_floor_up_step_negative_velocity() {
        let mut voxels = empty_voxels();
        make_floor_strip(&mut voxels);
        make_bump(&mut voxels);

        let start = Point3::new(2.5, 1.5, 0.5);
        let velocity = Vector3::new(-1.0, 0.0, 0.0);
        assert_relative_eq_point3(
            &translate_over_floor(&start, &velocity, &voxels, true),
            &(start + velocity + Vector3::from(UP)),
        );
    }

    #[test]
    fn test_translate_over_floor_down_step() {
        let mut voxels = empty_voxels();
        make_floor_strip(&mut voxels);
        make_bump(&mut voxels);

        let start = Point3::new(1.5, 2.5, 0.5);
        let velocity = Vector3::new(1.0, 0.0, 0.0);
        assert_relative_eq_point3(
            &translate_over_floor(&start, &velocity, &voxels, true),
            &(start + velocity - Vector3::from(UP)),
        );
    }

    #[test]
    fn test_translate_over_floor_down_step_negative_velocity() {
        let mut voxels = empty_voxels();
        make_floor_strip(&mut voxels);
        make_bump(&mut voxels);

        let start = Point3::new(1.5, 2.5, 0.5);
        let velocity = Vector3::new(-1.0, 0.0, 0.0);
        assert_relative_eq_point3(
            &translate_over_floor(&start, &velocity, &voxels, true),
            &(start + velocity - Vector3::from(UP)),
        );
    }
}
