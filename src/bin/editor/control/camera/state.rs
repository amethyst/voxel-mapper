use super::ThirdPersonControlConfig;

use voxel_mapper::geometry::{Plane, PolarVector, UP};

use amethyst::core::{
    approx::relative_eq,
    ecs::prelude::*,
    math::{Point3, Vector3},
    Transform,
};

#[derive(Clone, Copy, Debug)]
pub struct ThirdPersonCameraState {
    /// A point directly underneath the camera where floor collisions happen. The purpose of keeping
    /// the camera raised above the floor is to prevent undesired collisions between the camera and
    /// the floor due to sharp curvature in the geometry.
    pub feet: Point3<f32>,
    /// What the camera is looking at.
    pub target: Point3<f32>,
    /// A vector pointing from target to camera. Controlled by the player when rotating.
    pub eye_vec: PolarVector,
    /// The desired distance from camera to target. The actual distance may be less than this if the
    /// camera collides with something.
    radius: f32,
    /// While `get_desired_position` returns the ideal position for the camera, `actual_position` is
    /// the camera position after collisions (if any).
    pub actual_position: Point3<f32>,
}

impl Component for ThirdPersonCameraState {
    type Storage = HashMapStorage<Self>;
}

impl ThirdPersonCameraState {
    pub fn new(position: Point3<f32>, target: Point3<f32>) -> Self {
        let v = position - target;
        let mut eye_vec = PolarVector::default();
        eye_vec.set_vector(&v);
        let radius = v.norm();

        ThirdPersonCameraState {
            feet: target,
            target,
            radius,
            eye_vec,
            actual_position: position,
        }
    }

    pub fn get_position_at_radius(&self, radius: f32) -> Point3<f32> {
        self.target + radius * self.eye_vec.unit_vector()
    }

    pub fn get_desired_position(&self) -> Point3<f32> {
        self.get_position_at_radius(self.radius)
    }

    pub fn set_radius(&mut self, radius: f32, config: &ThirdPersonControlConfig) {
        self.radius = radius.max(config.min_radius).min(config.max_radius);
    }

    pub fn add_pitch(&mut self, dpitch: f32) {
        self.eye_vec.set_pitch(self.eye_vec.get_pitch() + dpitch)
    }

    pub fn add_yaw(&mut self, dyaw: f32) {
        self.eye_vec.set_yaw(self.eye_vec.get_yaw() + dyaw)
    }

    pub fn scale_radius(&mut self, s: f32, config: &ThirdPersonControlConfig) {
        self.set_radius(s * self.radius, config)
    }

    pub fn look_at(&self) -> Point3<f32> {
        // Even though it's simpler, it's possible to get NaN in the transform if you look at the
        // target when it's too close. Make sure we look at a point that's never too close to the
        // position.
        self.actual_position - self.eye_vec.unit_vector()
    }

    fn looking_up(&self) -> bool {
        relative_eq!(self.eye_vec.unit_vector().dot(&Vector3::from(UP)), -1.0)
    }

    pub fn transform(&self) -> Transform {
        let mut transform = Transform::default();
        *transform.translation_mut() = self.actual_position.coords;
        transform.face_towards(self.look_at().coords, Vector3::from(UP));

        assert!(
            !self.looking_up(),
            "Your camera transform is fucked up. Your look direction {} is probably bad.",
            self.eye_vec.unit_vector(),
        );

        transform
    }

    pub fn floor_plane(&self) -> Plane {
        Plane {
            p: self.feet,
            n: Vector3::from(UP),
        }
    }
}
