use crate::geometry::{project_point_onto_line, Line};

use amethyst::core::math as na;
use building_blocks::{prelude::*, search::greedy_path};
use ordered_float::NotNan;

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
pub fn greedy_path_with_l1_and_linear_heuristic(
    start: Point3i,
    finish: Point3i,
    predicate: impl Fn(&Point3i) -> bool,
    max_iterations: usize,
) -> (bool, Vec<Point3i>) {
    // TODO: amethyst is using an older version of nalgebra than building-blocks, so we can't do the
    // simplest conversion
    let startf = na::Point3::<f32>::from(Point3f::from(start).0);
    let finishf = na::Point3::<f32>::from(Point3f::from(finish).0);
    let line = Line::from_endpoints(startf, finishf);

    let heuristic = |p: &Point3i| {
        let pf = na::Point3::<f32>::from(Point3f::from(*p).0);
        let diff = finishf - pf;
        let exact = diff.x.abs() + diff.y.abs() + diff.z.abs();

        let p_line = project_point_onto_line(&pf, &line);
        let line_dist = (pf - p_line).norm();

        // Break ties using disalignment metric.
        unsafe { NotNan::unchecked_new(exact + 0.001 * line_dist) }
    };

    greedy_path(start, finish, predicate, heuristic, max_iterations)
}
