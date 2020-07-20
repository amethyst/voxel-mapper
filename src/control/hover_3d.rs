use crate::{
    collision::{nearest_bounding_volume_ray_cast, NearestBVRayCastResult, VoxelBVT},
    control::camera::data::CameraData,
    geometry::{line_plane_intersection, Line, LinePlaneIntersection, Plane},
};

use amethyst::{
    core::{
        ecs::prelude::*,
        math::{Point3, Vector3},
    },
    input::{BindingTypes, InputHandler},
};
use ilattice3 as lat;
use ncollide3d::{bounding_volume::AABB, query::Ray};
use std::marker::PhantomData;

#[derive(Default)]
pub struct HoverObjectSystem<B> {
    bindings: PhantomData<B>,
}

// The hover objects are always tracked by ray casting the cursor's position.
pub type VoxelRayCastResult = NearestBVRayCastResult<lat::Point, AABB<f32>>;

#[derive(Clone)]
pub struct HoverVoxel {
    pub raycast_result: VoxelRayCastResult,
    pub ray: Ray<f32>,
}

impl HoverVoxel {
    pub fn point(&self) -> &lat::Point {
        &self.raycast_result.data
    }
}

#[derive(Default)]
pub struct ObjectsUnderCursor {
    // A point on the XZ plane.
    pub xz_plane: Option<Point3<f32>>,
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
        let (x, y) = match input_handler.mouse_position() {
            Some((x, y)) => (x, y),
            None => return,
        };
        let ray = match raycast_data.get_camera_ray(x, y) {
            Some(r) => r,
            None => return,
        };

        // Check for intersection with a voxel.
        let voxel_result = nearest_bounding_volume_ray_cast(&*voxel_bvt, &ray, |_| true);
        objects.voxel = voxel_result.map(|raycast_result| {
            log::debug!("p = {}", raycast_result.data);
            HoverVoxel {
                raycast_result,
                ray,
            }
        });

        // Check for intersection with the XZ plane.
        let xz_plane = Plane {
            p: Point3::new(0.0, 0.0, 0.0),
            n: Vector3::new(0.0, 1.0, 0.0),
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
