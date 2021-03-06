pub mod data;
pub mod input;
pub mod state;

mod colliding_controller;
mod final_controller;
mod smoother;

pub use self::final_controller::FinalController;
pub use self::input::{InputConfig, InputProcessor, ProcessedInput};
pub use self::state::ThirdPersonCameraState;

use self::colliding_controller::CameraCollisionConfig;

use voxel_mapper::{
    collision::VoxelBVT,
    voxel::{chunk_cache_flusher::ChunkCacheFlusher, VoxelMap},
};

use amethyst::{
    config::Config,
    core::{
        ecs::prelude::*,
        math::Point3,
        shrev::{EventChannel, ReaderId},
        SystemDesc, Transform,
    },
    input::{BindingTypes, InputEvent, InputHandler},
    renderer::camera::Camera,
    utils::application_dir,
    window::ScreenDimensions,
};
use building_blocks::prelude::*;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub fn make_camera(position: Point3<f32>, target: Point3<f32>, world: &mut World) -> Entity {
    let config_dir = application_dir("assets/config").unwrap();
    let config = CameraConfig::load(config_dir.join("third_person_camera.ron")).unwrap();

    let (width, height) = world.exec(|screen_dims: ReadExpect<ScreenDimensions>| {
        (screen_dims.width(), screen_dims.height())
    });

    let camera_state = ThirdPersonCameraState::new(position, target);
    let input_processor = InputProcessor::new(config.input);
    let controller = CameraControllerComponent(FinalController::new(config.control));

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

#[derive(Deserialize, Serialize)]
pub struct CameraConfig {
    pub input: InputConfig,
    pub control: ThirdPersonControlConfig,
}

#[derive(Default)]
pub struct MainCameraTag;

impl Component for MainCameraTag {
    type Storage = NullStorage<Self>;
}

#[derive(Deserialize, Serialize)]
pub struct ThirdPersonControlConfig {
    pub min_radius: f32,
    pub max_radius: f32,
    pub smoothing_weight: f32,
    /// The distance from the camera feet to the target (along the Y axis).
    pub target_height_above_feet: f32,
    pub collision: CameraCollisionConfig,
}

pub struct CameraControllerComponent(pub FinalController);

impl Component for CameraControllerComponent {
    type Storage = HashMapStorage<Self>;
}

#[derive(SystemData)]
pub struct CameraControlData<'a, B>
where
    B: BindingTypes,
{
    controllers: WriteStorage<'a, CameraControllerComponent>,
    input_processors: WriteStorage<'a, InputProcessor>,
    tpc_states: WriteStorage<'a, ThirdPersonCameraState>,
    cameras: ReadStorage<'a, Camera>,
    transforms: WriteStorage<'a, Transform>,
    input_handler: Read<'a, InputHandler<B>>,
    voxel_map: ReadExpect<'a, VoxelMap>,
    cache_flusher: ReadExpect<'a, ChunkCacheFlusher>,
    voxel_bvt: ReadExpect<'a, VoxelBVT>,
    screen_dims: ReadExpect<'a, ScreenDimensions>,
}

impl<B> CameraControlData<'_, B>
where
    B: BindingTypes,
{
    fn update(&mut self, events: &[InputEvent<B>]) {
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
                &tpc_state.drag_plane(),
                &tpc_state.floor_plane(),
                cam,
                cam_tfm,
                &self.screen_dims,
            );
            let local_cache = LocalChunkCache3::new();
            let map_reader = self.voxel_map.voxels.reader(&local_cache);
            let lod0_reader = map_reader.lod_view(0);
            let voxel_infos =
                TransformMap::new(&lod0_reader, self.voxel_map.voxel_info_transform());
            let CameraControllerComponent(ctrlr) = ctrlr;
            let (new_cam_tfm, new_camera_state) =
                ctrlr.update(&tpc_state, &proc_input, &voxel_infos, &self.voxel_bvt);
            *tpc_state = new_camera_state;

            // Make sure not to overwrite the global matrix.
            *cam_tfm.translation_mut() = *new_cam_tfm.translation();
            *cam_tfm.rotation_mut() = *new_cam_tfm.rotation();

            self.cache_flusher.flush(local_cache);
        }
    }
}

pub struct CameraControlSystem<B>
where
    B: BindingTypes,
{
    reader_id: ReaderId<InputEvent<B>>,
}

#[derive(Default)]
pub struct CameraControlSystemDesc<B> {
    bindings: PhantomData<B>,
}

impl<'a, 'b, B> SystemDesc<'a, 'b, CameraControlSystem<B>> for CameraControlSystemDesc<B>
where
    B: BindingTypes,
{
    fn build(self, world: &mut World) -> CameraControlSystem<B> {
        <CameraControlSystem<B> as System<'_>>::SystemData::setup(world);

        let mut channel = world.write_resource::<EventChannel<InputEvent<B>>>();
        let reader_id = channel.register_reader();

        CameraControlSystem { reader_id }
    }
}

impl<'a, B> System<'a> for CameraControlSystem<B>
where
    B: BindingTypes,
{
    type SystemData = (
        CameraControlData<'a, B>,
        Read<'a, EventChannel<InputEvent<B>>>,
    );

    fn run(&mut self, (mut data, events): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("camera_control");

        let events: Vec<_> = events.read(&mut self.reader_id).cloned().collect();

        data.update(&events);
    }
}
