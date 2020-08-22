use crate::{
    geometry::{project_point_onto_line, Line},
    search::greedy_best_first,
    voxel::LatPoint3,
};

use ilattice3 as lat;
use ilattice3::{point::FACE_ADJACENT_OFFSETS, prelude::*};
use ordered_float::NotNan;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub fn find_path_though_voxels<C>(
    start: &lat::Point,
    finish: &lat::Point,
    predicate: impl Fn(&Point) -> bool,
    heuristic: impl Fn(&Point) -> C,
    max_iterations: usize,
) -> (bool, Vec<lat::Point>)
where
    C: Copy + Ord,
{
    if !predicate(start) {
        log::warn!("Started search in voxel failing predicate");
        return (false, vec![]);
    }

    // All adjacent empty points.
    let successors = |p: &lat::Point| {
        FACE_ADJACENT_OFFSETS
            .iter()
            .map(|offset| *p + *offset)
            .filter_map(|s| if predicate(&s) { Some(s) } else { None })
            .collect::<Vec<lat::Point>>()
    };

    let success = |p: &lat::Point| *p == *finish;

    let (reached_finish, path) =
        greedy_best_first(start, successors, heuristic, success, max_iterations);

    (reached_finish, path)
}

pub fn find_path_through_empty_voxels(
    start: &lat::Point,
    finish: &lat::Point,
    voxel_is_empty: impl Fn(&lat::Point) -> bool,
    max_iterations: usize,
) -> (bool, Vec<lat::Point>) {
    #[cfg(feature = "profiler")]
    profile_scope!("find_path_on_line_through_empty_voxels");

    let heuristic = |p: &lat::Point| {
        let diff = *finish - *p;

        diff.x.abs() + diff.y.abs() + diff.z.abs()
    };

    find_path_though_voxels(start, finish, voxel_is_empty, heuristic, max_iterations)
}

/// Finds a path from `start` to `finish` along empty voxels. Prioritizes staying close to the
/// line from `start` to `finish`, so you should get a path like:
///
/// ```text
///  S ____________ ++++  _______________ F
///               | ++++ |
///               |______|
/// ```
///
/// instead of:
///
/// ```text
///  S ____________ ++++           ______ F
///               | ++++   ______|
///               |_______|
/// ```
pub fn find_path_on_line_through_empty_voxels(
    start: &lat::Point,
    finish: &lat::Point,
    voxel_is_empty: impl Fn(&lat::Point) -> bool,
    max_iterations: usize,
) -> (bool, Vec<lat::Point>) {
    #[cfg(feature = "profiler")]
    profile_scope!("find_path_on_line_through_empty_voxels");

    let LatPoint3(start_float) = (*start).into();
    let LatPoint3(finish_float) = (*finish).into();
    let line = Line::from_endpoints(start_float, finish_float);

    let heuristic = |p: &lat::Point| {
        let LatPoint3(p_float) = (*p).into();
        let diff = finish_float - p_float;
        let exact = diff.x.abs() + diff.y.abs() + diff.z.abs();

        let p_line = project_point_onto_line(&p_float, &line);
        let line_dist = (p_float - p_line).norm();

        // Break ties using disalignment metric.
        unsafe { NotNan::unchecked_new(exact + 0.001 * line_dist) }
    };

    find_path_though_voxels(start, finish, voxel_is_empty, heuristic, max_iterations)
}
