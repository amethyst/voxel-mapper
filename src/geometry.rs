use amethyst::{
    core::{
        alga::general::RealField,
        approx::relative_eq,
        math::{Point2, Point3, Rotation3, Unit, Vector3},
        num::Zero,
        Transform,
    },
    renderer::{camera::Camera, rendy::mesh::Position},
    window::ScreenDimensions,
};
use ordered_float::NotNan;

// Amethyst coordinates
pub const UP: [f32; 3] = [0.0, 1.0, 0.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub p: Point3<f32>,
    pub v: Vector3<f32>,
}

impl Line {
    pub fn from_endpoints(p1: Point3<f32>, p2: Point3<f32>) -> Self {
        Self { p: p1, v: p2 - p1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub p: Point3<f32>,
    pub n: Vector3<f32>,
}

#[derive(Debug, PartialEq)]
pub enum LinePlaneIntersection {
    LineInPlane,
    NoIntersection,
    IntersectionPoint(Point3<f32>),
}

pub fn line_plane_intersection(l: &Line, p: &Plane) -> LinePlaneIntersection {
    let lp_dot_n = (p.p - l.p).dot(&p.n);
    let lv_dot_n = l.v.dot(&p.n);
    let point_in_plane = relative_eq!(lp_dot_n, f32::zero());
    let line_and_plane_parallel = relative_eq!(lv_dot_n, f32::zero());

    if line_and_plane_parallel {
        if point_in_plane {
            LinePlaneIntersection::LineInPlane
        } else {
            LinePlaneIntersection::NoIntersection
        }
    } else {
        LinePlaneIntersection::IntersectionPoint(l.p + l.v * (lp_dot_n / lv_dot_n))
    }
}

pub fn project_point_onto_line(p: &Point3<f32>, line: &Line) -> Point3<f32> {
    let p_v = p - line.p;
    let line_v_unit = line.v.normalize();
    let proj = p_v.dot(&line_v_unit);

    line.p + proj * line_v_unit
}

pub fn squared_distance_from_line(p: &Point3<f32>, line: &Line) -> f32 {
    let p_proj = project_point_onto_line(p, line);

    (p - p_proj).norm_squared()
}

pub struct Sphere {
    pub center: Point3<f32>,
    pub radius: f32,
}

impl Sphere {
    pub fn contains_point(&self, p: &Point3<f32>) -> bool {
        let diff = self.center - *p;

        self.radius * self.radius > diff.dot(&diff)
    }

    pub fn grow(&mut self, p: &Point3<f32>) {
        self.radius = (self.center - p).norm();
    }
}

pub fn farthest_point(p: &Point3<f32>, others: &[Point3<f32>]) -> Point3<f32> {
    *others
        .iter()
        .max_by_key(|other_p| unsafe {
            let diff = *p - *other_p;
            NotNan::unchecked_new(diff.dot(&diff))
        })
        .unwrap()
}

pub fn ritter_sphere_bounding_points(points: &[Point3<f32>]) -> Sphere {
    let x = points.first().unwrap();
    let y = farthest_point(&x, points);
    let z = farthest_point(&y, points);

    let mut sphere = Sphere {
        center: (y + z.coords) / 2.0,
        radius: (y - z).norm() / 2.0,
    };
    for p in points.iter() {
        if !sphere.contains_point(p) {
            sphere.grow(p);
        }
    }

    sphere
}

pub fn ritter_sphere_bounding_positions(positions: &[Position]) -> Sphere {
    let points: Vec<Point3<f32>> = positions
        .iter()
        .map(|Position(coords)| Point3::from(*coords))
        .collect();

    ritter_sphere_bounding_points(&points)
}

/// Returns pitch and yaw angles that rotates z unit vector to v. The yaw is applied first to z
/// about the y axis to get z'. Then the pitch is applied about some axis orthogonal to z' in the
/// XZ plane to get v.
pub fn yaw_and_pitch_from_vector(v: &Vector3<f32>) -> (f32, f32) {
    debug_assert_ne!(*v, Vector3::zeros());

    let y = Vector3::y_axis().into_inner();
    let z = Vector3::z_axis().into_inner();

    let v_xz = Vector3::new(v.x, 0.0, v.z);

    if v_xz == Vector3::zeros() {
        if v.dot(&y) > 0.0 {
            return (0.0, f32::pi() / 2.0);
        } else {
            return (0.0, -f32::pi() / 2.0);
        }
    }

    let mut yaw = v_xz.angle(&z);
    if v.x < 0.0 {
        yaw *= -1.0;
    }

    let mut pitch = v_xz.angle(&v);
    if v.y < 0.0 {
        pitch *= -1.0;
    }

    (yaw, pitch)
}

pub fn unit_vector_from_yaw_and_pitch(yaw: f32, pitch: f32) -> Vector3<f32> {
    let mut ray = Vector3::z_axis().into_inner();
    let y_axis = Vector3::y_axis();
    ray = Rotation3::from_axis_angle(&y_axis, yaw) * ray;
    let pitch_axis = Unit::new_unchecked(ray.cross(&y_axis));

    Rotation3::from_axis_angle(&pitch_axis, pitch) * ray
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PolarVector {
    // The fields are protected to keep them in an allowable range for the camera transform.
    yaw: f32,
    pitch: f32,
}

impl PolarVector {
    pub fn unit_vector(self) -> Vector3<f32> {
        unit_vector_from_yaw_and_pitch(self.yaw, self.pitch)
    }

    pub fn set_vector(&mut self, v: &Vector3<f32>) {
        let (yaw, pitch) = yaw_and_pitch_from_vector(v);
        self.set_yaw(yaw);
        self.set_pitch(pitch);
    }

    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = yaw % (2.0 * f32::pi());
    }

    pub fn get_yaw(&self) -> f32 {
        self.yaw
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        // Things can get weird if we are parallel to the UP vector.
        let up_eps = 0.01;
        self.pitch = pitch
            .min(f32::pi() / 2.0 - up_eps)
            .max(-f32::pi() / 2.0 + up_eps);
    }

    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }
}

pub fn screen_ray(
    camera: &Camera,
    camera_tfm: &Transform,
    dims: &ScreenDimensions,
    cursor_pos: Point2<f32>,
) -> Line {
    let screen_ray = camera.screen_ray(cursor_pos, dims.diagonal(), camera_tfm);

    Line {
        p: screen_ray.origin,
        v: screen_ray.direction,
    }
}

// TODO: amethyst is using an older version of nalgebra than building-blocks, so we need to upgrade
// the old ncollide types to new ones when using them with building-blocks
use amethyst::core::math as na_old;
use building_blocks::search::ncollide3d as nc_new;
use ncollide3d as nc_old;

pub fn upgrade_ray(old_ray: nc_old::query::Ray<f32>) -> nc_new::query::Ray<f32> {
    nc_new::query::Ray::new(upgrade_point(old_ray.origin), upgrade_vector(old_ray.dir))
}

pub fn upgrade_point(old_p: na_old::Point3<f32>) -> nc_new::na::Point3<f32> {
    nc_new::na::Point3::<f32>::new(old_p.x, old_p.y, old_p.z)
}

pub fn upgrade_vector(old_v: na_old::Vector3<f32>) -> nc_new::na::Vector3<f32> {
    nc_new::na::Vector3::<f32>::new(old_v.x, old_v.y, old_v.z)
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

    use amethyst::core::approx::assert_relative_eq;

    #[test]
    fn test_yaw_and_pitch_identity() {
        let v = Vector3::new(0.0, 0.0, 1.0);
        let (yaw, pitch) = yaw_and_pitch_from_vector(&v);

        assert_relative_eq!(yaw, 0.0);
        assert_relative_eq!(pitch, 0.0);
    }

    #[test]
    fn test_yaw_only() {
        let (yaw, pitch) = yaw_and_pitch_from_vector(&Vector3::new(1.0, 0.0, 0.0));
        assert_relative_eq!(yaw, f32::pi() / 2.0);
        assert_relative_eq!(pitch, 0.0);

        let (yaw, pitch) = yaw_and_pitch_from_vector(&Vector3::new(-1.0, 0.0, 0.0));
        assert_relative_eq!(yaw, -f32::pi() / 2.0);
        assert_relative_eq!(pitch, 0.0);
    }

    #[test]
    fn test_pitch_only() {
        let (yaw, pitch) = yaw_and_pitch_from_vector(&Vector3::new(0.0, 1.0, 0.0));
        assert_relative_eq!(yaw, 0.0);
        assert_relative_eq!(pitch, f32::pi() / 2.0);

        let (yaw, pitch) = yaw_and_pitch_from_vector(&Vector3::new(0.0, -1.0, 0.0));
        assert_relative_eq!(yaw, 0.0);
        assert_relative_eq!(pitch, -f32::pi() / 2.0);
    }

    #[test]
    fn test_yaw_and_pitch() {
        let (yaw, pitch) =
            yaw_and_pitch_from_vector(&Vector3::new(0.5f32.sqrt(), 1.0, 0.5f32.sqrt()));
        assert_relative_eq!(yaw, f32::pi() / 4.0);
        assert_relative_eq!(pitch, f32::pi() / 4.0);

        let (yaw, pitch) =
            yaw_and_pitch_from_vector(&Vector3::new(-0.5f32.sqrt(), -1.0, 0.5f32.sqrt()));
        assert_relative_eq!(yaw, -f32::pi() / 4.0);
        assert_relative_eq!(pitch, -f32::pi() / 4.0);
    }
}
