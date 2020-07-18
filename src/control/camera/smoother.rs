use super::state::ThirdPersonCameraState;

use amethyst::core::{math::Point3, Transform};

pub struct TransformSmoother {
    weight: f32,
    lerp_state: Option<ThirdPersonCameraState>,
}

impl TransformSmoother {
    pub fn new(weight: f32) -> Self {
        debug_assert!(0.0 < weight);
        debug_assert!(weight <= 1.0);

        TransformSmoother {
            weight,
            lerp_state: None,
        }
    }

    pub fn new_transform(&mut self, new_state: &ThirdPersonCameraState) -> Transform {
        let old_lerp_state = self.lerp_state.unwrap_or(*new_state);

        let lerp_pos = self.weight * old_lerp_state.actual_position.coords
            + (1.0 - self.weight) * new_state.actual_position.coords;
        let lerp_pos = Point3::from(lerp_pos);
        let lerp_target = self.weight * old_lerp_state.target.coords
            + (1.0 - self.weight) * new_state.target.coords;
        let lerp_target = Point3::from(lerp_target);
        let lerp_eye_vec = lerp_pos - lerp_target;

        // Copied values aren't used.
        let mut new_lerp_state = *new_state;

        new_lerp_state.actual_position = lerp_pos;
        new_lerp_state.eye_vec.set_vector(&lerp_eye_vec);
        new_lerp_state.target = lerp_target;

        let transform = new_lerp_state.transform();

        self.lerp_state = Some(new_lerp_state);

        transform
    }
}
