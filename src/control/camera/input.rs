use super::floor_translation::floor_drag_translation;
use crate::geometry::Plane;

use amethyst::{
    core::{
        ecs::prelude::*,
        math::{Point2, Vector3},
        Transform,
    },
    input::{BindingTypes, InputEvent, InputHandler, ScrollDirection},
    renderer::camera::Projection,
    window::ScreenDimensions,
    winit::MouseButton,
};

pub struct InputConfig {
    pub rotate_sensitivity_x: f32,
    pub rotate_sensitivity_y: f32,
    pub zoom_sensitivity: f32,
}

pub struct ProcessedInput {
    pub radius_scalar: f32,
    pub delta_yaw: f32,
    pub delta_pitch: f32,
    pub feet_translation: Vector3<f32>,
}

pub struct InputProcessor {
    config: InputConfig,
    prev_cursor_pos: Point2<f32>,
}

impl Component for InputProcessor {
    type Storage = HashMapStorage<Self>;
}

impl InputProcessor {
    pub fn new(config: InputConfig) -> Self {
        InputProcessor {
            config,
            prev_cursor_pos: Point2::new(0.0, 0.0),
        }
    }

    fn get_camera_radius_scalar_from_mouse_wheel_events<B>(
        &mut self,
        events: &[InputEvent<B>],
    ) -> f32
    where
        B: BindingTypes,
    {
        let mut radius_scalar = 1.0;
        for event in events.iter() {
            if let InputEvent::MouseWheelMoved(dir) = *event {
                let scale = match dir {
                    ScrollDirection::ScrollDown => 1.0 + self.config.zoom_sensitivity,
                    ScrollDirection::ScrollUp => 1.0 - self.config.zoom_sensitivity,
                    _ => 1.0,
                };

                radius_scalar *= scale;
            }
        }

        radius_scalar
    }

    pub fn process_input<B>(
        &mut self,
        input: &InputHandler<B>,
        events: &[InputEvent<B>],
        floor_plane: &Plane,
        camera_tfm: &Transform,
        camera_proj: &Projection,
        screen_dims: &ScreenDimensions,
    ) -> ProcessedInput
    where
        B: BindingTypes,
    {
        let radius_scalar = self.get_camera_radius_scalar_from_mouse_wheel_events(&events);

        let mut delta_yaw = 0.0;
        let mut delta_pitch = 0.0;
        let mut feet_translation = Vector3::zeros();

        if let Some((x, y)) = input.mouse_position() {
            let cursor_pos = Point2::new(x, y);
            let cursor_delta = cursor_pos - self.prev_cursor_pos;

            if input.mouse_button_is_down(MouseButton::Right) {
                delta_yaw = -cursor_delta.x * self.config.rotate_sensitivity_x;
                delta_pitch = cursor_delta.y * self.config.rotate_sensitivity_y;
            }

            if input.mouse_button_is_down(MouseButton::Left) {
                feet_translation = floor_drag_translation(
                    floor_plane,
                    camera_tfm,
                    camera_proj,
                    screen_dims,
                    cursor_pos,
                    self.prev_cursor_pos,
                );
            }

            self.prev_cursor_pos = cursor_pos;
        }

        ProcessedInput {
            radius_scalar,
            delta_yaw,
            delta_pitch,
            feet_translation,
        }
    }
}
