pub mod data;
pub mod debug_feet;
pub mod input;
pub mod state;

mod colliding_controller;
mod final_controller;
mod floor_translation;
mod smoother;

pub use self::final_controller::FinalController;
pub use self::input::{InputConfig, InputProcessor, ProcessedInput};
pub use self::state::ThirdPersonCameraState;

use crate::{collision::VoxelBVT, control::bindings::GameBindings, voxel::VoxelMap};

use amethyst::{
    core::{
        ecs::prelude::*,
        math::Point3,
        shrev::{EventChannel, ReaderId},
        Transform,
    },
    derive::SystemDesc,
    input::{InputEvent, InputHandler},
    renderer::camera::Camera,
    window::ScreenDimensions,
};

pub fn make_camera(position: Point3<f32>, feet: Point3<f32>, world: &mut World) -> Entity {
    let (width, height) = world.exec(|screen_dims: ReadExpect<ScreenDimensions>| {
        (screen_dims.width(), screen_dims.height())
    });

    let camera_state = ThirdPersonCameraState::new(position, feet);
    let input_processor = InputProcessor::new(CAM_INPUT_CONFIG);
    let controller = CameraControllerComponent(Box::new(FinalController::new(TPC_CONFIG)));

    world
        .create_entity()
        .with(MainCameraTag)
        .with(Camera::standard_3d(width, height))
        .with(camera_state.transform())
        .with(camera_state)
        .with(input_processor)
        .with(controller)
        .build()
}

const CAM_INPUT_CONFIG: InputConfig = InputConfig {
    rotate_sensitivity_x: 0.005,
    rotate_sensitivity_y: 0.005,
    zoom_sensitivity: 0.01,
};

const TPC_CONFIG: ThirdPersonControlConfig = ThirdPersonControlConfig {
    min_radius: 1.0,
    max_radius: 100.0,
    smoothing_weight: 0.8,
};

#[derive(Default)]
pub struct MainCameraTag;

impl Component for MainCameraTag {
    type Storage = NullStorage<Self>;
}

pub struct ThirdPersonControlConfig {
    pub min_radius: f32,
    pub max_radius: f32,
    pub smoothing_weight: f32,
}

pub trait CameraController {
    fn update(
        &mut self,
        camera_state: &ThirdPersonCameraState,
        input: &ProcessedInput,
        voxel_map: &VoxelMap,
        voxel_bvt: &VoxelBVT,
    ) -> (Transform, ThirdPersonCameraState);
}

pub struct CameraControllerComponent(pub Box<dyn CameraController + Send + Sync>);

impl Component for CameraControllerComponent {
    type Storage = HashMapStorage<Self>;
}

#[derive(SystemData)]
pub struct CameraControlData<'a> {
    controllers: WriteStorage<'a, CameraControllerComponent>,
    input_processors: WriteStorage<'a, InputProcessor>,
    tpc_states: WriteStorage<'a, ThirdPersonCameraState>,
    cameras: ReadStorage<'a, Camera>,
    transforms: WriteStorage<'a, Transform>,
    input_handler: Read<'a, InputHandler<GameBindings>>,
    voxel_map: ReadExpect<'a, VoxelMap>,
    voxel_bvt: ReadExpect<'a, VoxelBVT>,
    screen_dims: ReadExpect<'a, ScreenDimensions>,
}

impl CameraControlData<'_> {
    fn update(&mut self, events: &[InputEvent<GameBindings>]) {
        if let Some((ctrlr, input_proc, tpc_state, cam, cam_tfm)) = (
            &mut self.controllers,
            &mut self.input_processors,
            &mut self.tpc_states,
            &self.cameras,
            &mut self.transforms,
        )
            .join()
            .next()
        {
            let proc_input = input_proc.process_input(
                &self.input_handler,
                events,
                &tpc_state.floor_plane(),
                cam_tfm,
                cam.projection(),
                &self.screen_dims,
            );
            let CameraControllerComponent(ctrlr) = ctrlr;
            let (new_cam_tfm, new_camera_state) =
                ctrlr.update(&tpc_state, &proc_input, &self.voxel_map, &self.voxel_bvt);
            *tpc_state = new_camera_state;

            // Make sure not to overwrite the global matrix.
            *cam_tfm.translation_mut() = *new_cam_tfm.translation();
            *cam_tfm.rotation_mut() = *new_cam_tfm.rotation();
        }
    }
}

#[derive(SystemDesc)]
#[system_desc(name(CameraControlSystemDesc))]
pub struct CameraControlSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<InputEvent<GameBindings>>,
}

impl CameraControlSystem {
    pub fn new(reader_id: ReaderId<InputEvent<GameBindings>>) -> Self {
        Self { reader_id }
    }
}

impl<'a> System<'a> for CameraControlSystem {
    type SystemData = (
        CameraControlData<'a>,
        Read<'a, EventChannel<InputEvent<GameBindings>>>,
    );

    fn run(&mut self, (mut data, events): Self::SystemData) {
        let events: Vec<_> = events.read(&mut self.reader_id).cloned().collect();

        data.update(&events);
    }
}
