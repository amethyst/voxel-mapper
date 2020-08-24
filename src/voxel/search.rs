use crate::{
    geometry::{project_point_onto_line, Line},
    voxel::LatPoint3,
};

use ilattice3 as lat;
use ilattice3::algos::find_path_through_voxels;
use ordered_float::NotNan;

// NOTE: This function is excluded from ilattice3 in order to avoid the dependency on nalgebra.

/// Finds a path from `start` to `finish` along voxels. Prioritizes staying close to the
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
pub fn find_path_through_voxels_with_l1_and_linear_heuristic(
    start: &lat::Point,
    finish: &lat::Point,
    predicate: impl Fn(&lat::Point) -> bool,
    max_iterations: usize,
) -> (bool, Vec<lat::Point>) {
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

    find_path_through_voxels(start, finish, predicate, heuristic, max_iterations)
}
