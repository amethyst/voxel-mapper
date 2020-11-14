use crate::{
    geometry::UP,
    voxel::{voxel_containing_point, IsFloor},
};

use amethyst::core::math::{Point3, Vector3};
use building_blocks::prelude::*;
use itertools::Itertools;

/// Returns all numbers `t` such that `bias + slope * t` is an integer and `t_0 <= t <= t_f`.
/// Always returns empty vector for constant (`slope == 0.0`) functions, even if the constant is an
/// integer (since there would be infinite solutions for `t`).
fn integer_points_on_line_segment_1d(slope: f32, bias: f32, t_0: f32, t_f: f32) -> Vec<f32> {
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

fn integer_points_on_line_segment_3d(
    start: &Point3<f32>,
    velocity: &Vector3<f32>,
    t_0: f32,
    t_f: f32,
) -> Vec<f32> {
    let mut solutions = Vec::new();
    solutions.append(&mut integer_points_on_line_segment_1d(
        velocity.x, start.x, t_0, t_f,
    ));
    solutions.append(&mut integer_points_on_line_segment_1d(
        velocity.y, start.y, t_0, t_f,
    ));
    solutions.append(&mut integer_points_on_line_segment_1d(
        velocity.z, start.z, t_0, t_f,
    ));
    solutions.sort_by(|a, b| a.partial_cmp(b).unwrap());
    solutions.dedup();

    solutions
}

/// True if the voxel at `p` is not a floor voxel AND the voxel directly under `p` is a floor voxel.
fn voxel_is_on_top_of_floor<V, T>(p: &Point3i, voxels: &V) -> bool
where
    V: for<'r> Get<&'r Point3i, Data = T>,
    T: IsFloor,
{
    if !voxels.get(p).is_floor() {
        let under_p = *p - PointN([0, 1, 0]);
        return voxels.get(&under_p).is_floor();
    }

    false
}

const MAX_PROBE_ITERS: i32 = 10;

// POTENTIAL BUG: can skip over non-floor voxels in a column
fn vertical_probe<V, T>(vertical_iters: i32, start: &Point3i, voxels: &V) -> Option<i32>
where
    V: for<'r> Get<&'r Point3i, Data = T>,
    T: IsFloor,
{
    let dir = vertical_iters.signum();
    let probe_iters = vertical_iters.abs();
    let mut p = *start;
    let mut dh = 0;
    for _ in 0..probe_iters {
        if voxel_is_on_top_of_floor(&p, voxels) {
            return Some(dh);
        }
        p = p + PointN([0, dir, 0]);
        dh += dir;
    }

    None
}

/// Moves a point along a translation vector while doing collision resolution with the floor voxels.
/// The point may only travel above floor voxels, and it will jump on top or fall down onto floor
/// voxels as it crosses voxel boundaries. There are stopping conditions that prevent the point from
/// moving further, including:
///
///   1. Encountering a tall column of solid voxels
///   2. TODO: Tightly enclosed spaces that make it hard for camera collisions
///
pub fn translate_over_floor<V, T>(
    start: &Point3<f32>,
    velocity: &Vector3<f32>,
    voxels: &V,
    blocking_collisions: bool,
) -> Point3<f32>
where
    V: for<'r> Get<&'r Point3i, Data = T>,
    T: IsFloor,
{
    let up = Vector3::from(UP);

    let mut start = *start;
    let start_voxel = voxel_containing_point(start);

    // Sometimes geometry gets created on top of the camera feet, so just probe out of it.
    if voxels.get(&start_voxel).is_floor() {
        if let Some(dh) = vertical_probe(100, &start_voxel, voxels) {
            start += dh as f32 * up;
        }
    } else if !voxel_is_on_top_of_floor(&start_voxel, voxels) {
        if let Some(dh) = vertical_probe(-100, &start_voxel, voxels) {
            start += dh as f32 * up;
        }
    }

    // To detect when the point crosses a voxel boundary, get all of the points on the line segment
    // with any integer coordinates.
    // PERF: use an iterator instead of collecting all of these points at the start
    let voxel_boundary_times = integer_points_on_line_segment_3d(&start, velocity, 0.0, 1.0);
    let mut boundary_points: Vec<(f32, Point3<f32>)> = voxel_boundary_times
        .into_iter()
        .map(|t| (t, start + t * velocity))
        .collect();
    // Include this point so we can get all of the midpoints (see below).
    boundary_points.push((1.0, start + velocity));

    // Translate the point up and down as it travels over floor voxels.
    let mut height_delta = 0;
    let mut last_good_point = start;
    for ((_, p1), (_, p2)) in boundary_points.iter().tuple_windows() {
        // Since we have points on voxel boundaries, it's hard to say what voxel we're leaving or
        // entering, but we know the midpoint between boundaries will fall into the voxel we are
        // entering.
        let midpoint = (p1 + p2.coords) / 2.0;
        let midpoint_voxel = voxel_containing_point(midpoint);

        let voxel_p = midpoint_voxel + PointN([0, height_delta, 0]);

        let voxel_is_floor = voxels.get(&voxel_p).is_floor();
        let probe_vector = if voxel_is_floor {
            MAX_PROBE_ITERS
        } else {
            -MAX_PROBE_ITERS
        };
        if let Some(dh) = vertical_probe(probe_vector, &voxel_p, voxels) {
            height_delta += dh;
            last_good_point = midpoint;
        } else {
            // Probing failed, so just stop.
            if blocking_collisions {
                return last_good_point + (height_delta as f32) * up;
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

    use crate::{
        test_util::{assert_relative_eq_point3, assert_relative_eq_vec},
        voxel::VOXEL_CHUNK_SHAPE,
    };

    #[test]
    fn test_no_integer_points_on_line_segment_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(0.5, 0.0, 0.1, 0.2),
            &vec![],
        );
    }

    #[test]
    fn test_starting_integer_point_on_line() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(0.5, 0.0, 0.0, 1.0),
            &vec![0.0],
        );
    }

    #[test]
    fn test_starting_and_ending_integer_point_on_line() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(0.5, 0.0, 0.0, 2.0),
            &vec![0.0, 2.0],
        );
    }

    #[test]
    fn test_one_integer_point_on_line_before_full_interval_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(0.5, 0.99, 0.0, 0.1),
            &vec![0.01 / 0.5],
        );
    }

    #[test]
    fn test_two_integer_point_on_line_before_two_full_intervals_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(0.5, 0.99, 0.0, 2.5),
            &vec![0.01 / 0.5, 1.01 / 0.5],
        );
    }

    #[test]
    fn test_one_integer_point_on_line_with_negative_slope_1d() {
        assert_relative_eq_vec(
            &integer_points_on_line_segment_1d(-0.5, 0.0, 0.0, 2.0),
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
        ChunkedLatticeMap::new(VOXEL_CHUNK_SHAPE)
    }

    fn make_floor_strip(voxels: &mut ChunkedLatticeMap<TestVoxel>) {
        voxels.fill_extent_or_default(
            &Extent3i::from_min_and_shape([0, 0, 0].into(), [3, 1, 1].into()),
            TestVoxel(true),
            (),
            TestVoxel(false),
        );
        // Make some space above the floor to move through.
        voxels.fill_extent_or_default(
            &Extent3i::from_min_and_shape([0, 1, 0].into(), [3, 2, 1].into()),
            TestVoxel(false),
            (),
            TestVoxel(false),
        );
    }

    fn make_bump(voxels: &mut ChunkedLatticeMap<TestVoxel>) {
        voxels.fill_extent_or_default(
            &Extent3i::from_min_and_shape([1, 1, 0].into(), [1, 1, 1].into()),
            TestVoxel(true),
            (),
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
