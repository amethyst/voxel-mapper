use super::VoxelMap;

use crate::finite_astar::finite_astar;

use ilattice3 as lat;
use ilattice3::{point::FACE_ADJACENT_OFFSETS, prelude::*};

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub fn find_path_through_empty_voxels(
    start: &lat::Point,
    finish: &lat::Point,
    map: &VoxelMap,
    predicate: impl Fn(&Point) -> bool,
    max_iterations: usize,
) -> (bool, Vec<lat::Point>) {
    #[cfg(feature = "profiler")]
    profile_scope!("find_path_through_empty_voxels");

    // All adjacent empty points.
    let successors = |p: &lat::Point| {
        FACE_ADJACENT_OFFSETS
            .iter()
            .map(|offset| *p + *offset)
            .filter_map(|s| {
                if !predicate(&s) {
                    return None;
                }
                if let Some(v) = map.voxels.maybe_get_world_ref(&s) {
                    if v.is_empty() {
                        return Some((s, 1));
                    }
                } else {
                    // Non-existent is considered empty.
                    return Some((s, 1));
                }

                None
            })
            .collect::<Vec<(lat::Point, i32)>>()
    };

    let heuristic = |p: &lat::Point| {
        let diff = *finish - *p;

        diff.dot(&diff)
    };

    let success = |p: &lat::Point| *p == *finish;

    let (reached_finish, path, _cost) =
        finite_astar(start, successors, heuristic, success, max_iterations);

    (reached_finish, path)
}
