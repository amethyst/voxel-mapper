use crate::voxel::{voxel_cuboid, voxel_transform};

use amethyst::core::{
    math::{Isometry3, Point3, Translation3, UnitQuaternion, Vector3},
    num::{Bounded, Zero},
};
use ilattice3 as lat;
use ncollide3d::{
    bounding_volume::{BoundingVolume, HasBoundingVolume, AABB},
    partitioning::{DBVTLeafId, VisitStatus, Visitor, BVH, DBVT},
    query::{time_of_impact, visitors::BoundingVolumeInterferencesCollector, Ray, RayCast, TOI},
    shape::Ball,
};
use std::collections::HashMap;

pub type VoxelBVT = DBVT<f32, lat::Point, AABB<f32>>;

#[derive(Default)]
pub struct VoxelBVTLeaves {
    /// Map from chunk key to all volumes in the chunk.
    pub leaves: HashMap<lat::Point, Vec<DBVTLeafId>>,
}

/// Returns the impact between the given ball and any voxel in the BVT, as chosen by `cmp_fn`. The
/// returned TOI will have volume1 being the ball and volume2 being the voxel, and its toi field
/// value is in the range [0.0, 1.0].
pub fn extreme_ball_voxel_impact(
    ball_radius: f32,
    start_pos: Point3<f32>,
    end_pos: Point3<f32>,
    bvt: &VoxelBVT,
    target_distance: f32,
    cmp_fn: impl Fn(TOI<f32>, TOI<f32>) -> TOI<f32>,
    predicate_fn: impl Fn(&TOI<f32>) -> bool,
) -> Option<TOI<f32>> {
    if start_pos == end_pos {
        return None;
    }

    let ball = Ball::new(ball_radius);
    let ball_start_tfm = Isometry3::from_parts(
        Translation3::from(start_pos.coords),
        UnitQuaternion::identity(),
    );
    let ball_end_tfm = Isometry3::from_parts(
        Translation3::from(end_pos.coords),
        UnitQuaternion::identity(),
    );
    // We want the max TOI to be 1.0.
    let ball_velocity = end_pos - start_pos;

    // Make a volume that bounds the sphere through its entire path.
    let ball_start_aabb: AABB<f32> = ball.bounding_volume(&ball_start_tfm);
    let ball_end_aabb: AABB<f32> = ball.bounding_volume(&ball_end_tfm);
    let ball_path_aabb = ball_start_aabb.merged(&ball_end_aabb);

    // Get the lattice points of the voxels that might intersect the path.
    let mut interfering_points = Vec::new();
    let mut visitor =
        BoundingVolumeInterferencesCollector::new(&ball_path_aabb, &mut interfering_points);
    bvt.visit(&mut visitor);

    // Get the voxel that will collide with the ball earliest.
    let mut maybe_best_impact: Option<TOI<f32>> = None;
    for p in interfering_points.iter() {
        let voxel_velocity = Vector3::zero();
        let max_toi = 1.0;
        if let Some(impact) = time_of_impact(
            &ball_start_tfm,
            &ball_velocity,
            &ball,
            &voxel_transform(p),
            &voxel_velocity,
            &voxel_cuboid(p),
            max_toi,
            target_distance,
        ) {
            if !predicate_fn(&impact) {
                continue;
            }

            maybe_best_impact = if let Some(best_impact) = maybe_best_impact.take() {
                Some(cmp_fn(impact, best_impact))
            } else {
                Some(impact)
            };
        }
    }

    maybe_best_impact
}

#[derive(Clone)]
pub struct NearestBVRayCastResult<T, BV> {
    pub data: T,
    pub bounding_volume: BV,
    pub toi: f32,
}

pub fn nearest_bounding_volume_ray_cast<BV, T>(
    bvh: &impl BVH<T, BV>,
    ray: &Ray<f32>,
    predicate_fn: impl Fn(&T) -> bool,
) -> Option<NearestBVRayCastResult<T, BV>>
where
    BV: BoundingVolume<f32> + RayCast<f32> + Clone,
    T: Clone,
{
    let mut visitor = NearestBVRayCast::new(*ray, predicate_fn);
    bvh.visit(&mut visitor);

    match (visitor.nearest_data, visitor.nearest_bv) {
        (Some(data), Some(bounding_volume)) => Some(NearestBVRayCastResult {
            data,
            bounding_volume,
            toi: visitor.earliest_toi,
        }),
        _ => None,
    }
}

pub fn earliest_toi(i1: TOI<f32>, i2: TOI<f32>) -> TOI<f32> {
    if i1.toi < i2.toi {
        i1
    } else {
        i2
    }
}

pub struct NearestBVRayCast<T, BV, F> {
    pub earliest_toi: f32,
    pub nearest_bv: Option<BV>,
    pub nearest_data: Option<T>,
    pub num_ray_casts: usize,
    pub ray: Ray<f32>,
    predicate_fn: F,
}

impl<T, BV, F> NearestBVRayCast<T, BV, F> {
    pub fn new(ray: Ray<f32>, predicate_fn: F) -> Self {
        Self {
            earliest_toi: f32::max_value(),
            nearest_bv: None,
            nearest_data: None,
            num_ray_casts: 0,
            ray,
            predicate_fn,
        }
    }
}

impl<T, BV, F> Visitor<T, BV> for NearestBVRayCast<T, BV, F>
where
    T: Clone,
    BV: Clone + RayCast<f32>,
    F: Fn(&T) -> bool,
{
    fn visit(&mut self, bv: &BV, data: Option<&T>) -> VisitStatus {
        self.num_ray_casts += 1;
        if let Some(toi) = bv.toi_with_ray(&Isometry3::identity(), &self.ray, true) {
            if toi < self.earliest_toi {
                if let Some(data) = data {
                    if (self.predicate_fn)(data) {
                        self.earliest_toi = toi;
                        self.nearest_bv = Some(bv.clone());
                        self.nearest_data = Some(data.clone());
                    }
                }

                return VisitStatus::Continue;
            }
        }

        VisitStatus::Stop
    }
}
