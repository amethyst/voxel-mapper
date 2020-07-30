use crate::{
    debug_feet::make_camera_feet_lines, edit_voxel::PaintBrush, hover_hint::make_hover_hint_lines,
};

use voxel_mapper::{
    collision::VoxelBVT,
    control::camera::make_camera,
    voxel::{
        loader::VoxelLoader,
        map_file::{load_voxel_map, save_voxel_map},
        meshing::manager::VoxelMeshManager,
        VoxelMap,
    },
};

use amethyst::{
    assets::ProgressCounter,
    core::{
        ecs::prelude::*,
        math::{Point3, Vector3},
        Transform,
    },
    input::{is_key_down, VirtualKeyCode},
    prelude::*,
    renderer::{
        debug_drawing::DebugLinesComponent,
        light::{Light, PointLight},
        palette::{rgb::Rgb, Srgba},
    },
};
use std::path::PathBuf;

pub struct OnlyState {
    map_file: PathBuf,
}

impl OnlyState {
    pub fn new(map_file: PathBuf) -> Self {
        OnlyState { map_file }
    }
}

impl SimpleState for OnlyState {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        let StateData { world, .. } = data;

        world.insert(VoxelBVT::new());
        world.insert(PaintBrush {
            radius: 8,
            voxel_address: 1,
            dist_from_camera: None,
        });

        let map = load_voxel_map(&self.map_file).expect("Failed to load voxel map");

        let (mut assets, map) = world.exec(|mut loader: VoxelLoader| {
            let mut unused_progress = ProgressCounter::new();

            loader.start_loading(map, &mut unused_progress)
        });
        world.exec(|mut manager: VoxelMeshManager| {
            manager.make_all_chunk_mesh_entities(&mut assets, &map);
        });
        world.insert(assets);
        world.insert(map);

        make_hover_hint_lines(world);
        make_gridlines(100, world);
        make_sunlight(10.0, world);
        make_camera(
            Point3::new(0.0, 10.0, 0.0),
            Point3::new(0.0, 0.0, 0.0),
            world,
        );
        make_camera_feet_lines(world);
    }

    fn handle_event(
        &mut self,
        _data: StateData<'_, GameData<'_, '_>>,
        event: StateEvent,
    ) -> SimpleTrans {
        if let StateEvent::Window(event) = &event {
            if is_key_down(&event, VirtualKeyCode::Escape) {
                return Trans::Quit;
            }
        }

        Trans::None
    }

    fn on_stop(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        data.world.exec(
            |(mut manager, map): (VoxelMeshManager, ReadExpect<VoxelMap>)| {
                manager.destroy();

                save_voxel_map("saved_voxels.bin", &map).expect("Failed to save voxels");
            },
        );
    }
}

fn make_gridlines(num_grid_lines: usize, world: &mut World) {
    let mut lines = DebugLinesComponent::new();
    let num_grid_lines = num_grid_lines as i32 / 2;
    for i in -num_grid_lines..num_grid_lines {
        let color = if i == 0 {
            Srgba::new(1.0, 0.0, 0.0, 1.0)
        } else {
            Srgba::new(0.0, 1.0, 0.0, 1.0)
        };
        lines.add_direction(
            Point3::new(i as f32, 0.0, -(num_grid_lines as f32)),
            Vector3::new(0.0, 0.0, (2 * num_grid_lines) as f32),
            color,
        );
        lines.add_direction(
            Point3::new(-(num_grid_lines as f32), 0.0, i as f32),
            Vector3::new((2 * num_grid_lines) as f32, 0.0, 0.0),
            color,
        );
    }

    world.create_entity().with(lines).build();
}

fn make_sunlight(intensity: f32, world: &mut World) {
    let light: Light = PointLight {
        intensity,
        color: Rgb::new(1.0, 1.0, 1.0),
        ..PointLight::default()
    }
    .into();
    let mut tfm = Transform::default();
    *tfm.translation_mut() = Vector3::new(0.0, 100.0, 0.0);

    world.create_entity().with(light).with(tfm).build();
}
