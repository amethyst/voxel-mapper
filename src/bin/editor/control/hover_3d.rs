use crate::control::camera::data::CameraData;

use voxel_mapper::{
    collision::VoxelBVT,
    geometry::{line_plane_intersection, upgrade_ray, Line, LinePlaneIntersection, Plane},
};

use amethyst::{
    core::{ecs::prelude::*, math as na},
    input::{BindingTypes, InputHandler},
};
use building_blocks::{
    prelude::*,
    search::collision::{cast_ray_at_voxels, VoxelRayImpact},
};
use ncollide3d::query::Ray;
use std::marker::PhantomData;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

#[derive(Default)]
pub struct HoverObjectSystem<B> {
    bindings: PhantomData<B>,
}

#[derive(Clone)]
pub struct HoverVoxel {
    pub impact: VoxelRayImpact,
    pub ray: Ray<f32>,
}

impl HoverVoxel {
    pub fn point(&self) -> &Point3i {
        &self.impact.point
    }

    /// Returns the normal vector of the face that the ray hit first.
    pub fn hover_face(&self) -> Point3i {
        Point3f::from(self.impact.impact.normal.normalize())
            .round()
            .into_int()
    }

    /// Returns the point of the adjacent voxel that shares a face with the voxel that was hit by
    /// the ray.
    pub fn hover_adjacent_point(&self) -> Point3i {
        *self.point() + self.hover_face()
    }
}

#[derive(Default)]
pub struct ObjectsUnderCursor {
    // A point on the XZ plane.
    pub xz_plane: Option<na::Point3<f32>>,
    // The closest voxel on the camera ray.
    pub voxel: Option<HoverVoxel>,
}

impl<'a, B> System<'a> for HoverObjectSystem<B>
where
    B: BindingTypes,
{
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Write<'a, ObjectsUnderCursor>,
        ReadExpect<'a, VoxelBVT>,
        Read<'a, InputHandler<B>>,
        CameraData<'a>,
    );

    fn run(&mut self, (mut objects, voxel_bvt, input_handler, raycast_data): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("hover_object");

        let (x, y) = match input_handler.mouse_position() {
            Some((x, y)) => (x, y),
            None => return,
        };
        let ray = match raycast_data.get_camera_ray(x, y) {
            Some(r) => r,
            None => return,
        };

        // Check for intersection with a voxel.
        let max_toi = std::f32::MAX;
        let voxel_impact = cast_ray_at_voxels(&*voxel_bvt, upgrade_ray(ray), max_toi, |_| true);
        objects.voxel = voxel_impact.map(|impact| HoverVoxel { impact, ray });

        // Check for intersection with the XZ plane.
        let xz_plane = Plane {
            p: na::Point3::new(0.0, 0.0, 0.0),
            n: na::Vector3::new(0.0, 1.0, 0.0),
        };
        let ray_line = Line {
            p: ray.origin,
            v: ray.dir,
        };
        let intersection = line_plane_intersection(&ray_line, &xz_plane);
        objects.xz_plane = if let LinePlaneIntersection::IntersectionPoint(mut p) = intersection {
            p.y = 0.0;

            Some(p)
        } else {
            None
        };
    }
}
