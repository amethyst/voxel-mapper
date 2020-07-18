use crate::control::camera::MainCameraTag;

use amethyst::{
    core::{
        ecs::prelude::*,
        math::{Point2, Vector2},
        Transform,
    },
    renderer::camera::Camera,
    window::ScreenDimensions,
};
use ncollide3d::query::Ray;

#[derive(SystemData)]
pub struct CameraData<'a> {
    pub cameras: ReadStorage<'a, Camera>,
    pub is_main_camera: ReadStorage<'a, MainCameraTag>,
    pub transforms: ReadStorage<'a, Transform>,
    pub screen_dims: ReadExpect<'a, ScreenDimensions>,
}

impl<'a> CameraData<'a> {
    pub fn get_main_camera(&self) -> Option<(&Camera, &Transform)> {
        (&self.is_main_camera, &self.cameras, &self.transforms)
            .join()
            .next()
            .map(|(_, cam, cam_tfm)| (cam, cam_tfm))
    }

    pub fn get_camera_ray(&self, x: f32, y: f32) -> Option<Ray<f32>> {
        let screen_pos = Point2::new(x, y);
        let screen_diag = Vector2::new(self.screen_dims.width(), self.screen_dims.height());
        let (cam, cam_tfm) = match self.get_main_camera() {
            Some(x) => x,
            None => return None,
        };
        let ray = cam
            .projection()
            .screen_ray(screen_pos, screen_diag, cam_tfm);

        Some(Ray::new(ray.origin, ray.direction))
    }
}
