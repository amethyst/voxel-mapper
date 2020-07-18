use amethyst::{
    core::{ecs::prelude::*, Transform},
    renderer::{
        light::{Light, PointLight},
        palette::rgb::Rgb,
        Camera,
    },
};

#[derive(Default)]
pub struct Flashlight;

impl Component for Flashlight {
    type Storage = NullStorage<Self>;
}

pub fn make_flashlight(intensity: f32, world: &mut World) {
    let light: Light = PointLight {
        intensity,
        color: Rgb::new(1.0, 1.0, 1.0),
        ..PointLight::default()
    }
    .into();

    world
        .create_entity()
        .with(Flashlight)
        .with(light)
        .with(Transform::default())
        .build();
}

pub struct FlashlightSystem;

impl<'a> System<'a> for FlashlightSystem {
    type SystemData = (
        ReadStorage<'a, Camera>,
        ReadStorage<'a, Flashlight>,
        WriteStorage<'a, Transform>,
    );

    fn run(&mut self, (cameras, flashlights, mut transforms): Self::SystemData) {
        if let Some(cam_tfm) = (&cameras, &transforms)
            .join()
            .map(|(_, t)| t.clone())
            .next()
        {
            for (_, tfm) in (&flashlights, &mut transforms).join() {
                *tfm.translation_mut() = *cam_tfm.translation();
            }
        }
    }
}
