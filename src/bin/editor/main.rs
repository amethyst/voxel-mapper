mod bindings;
mod debug_feet;
mod edit_voxel;
mod flashlight;
mod hover_hint;
mod only_state;

use bindings::GameBindings;
use debug_feet::DrawCameraFeetSystem;
use edit_voxel::EditVoxelSystemDesc;
use flashlight::FlashlightSystem;
use hover_hint::HoverHintSystem;
use only_state::OnlyState;

use voxel_mapper::{
    control::{camera::CameraControlSystemDesc, hover_3d::HoverObjectSystem},
    rendering::triplanar_pass::RenderTriplanarPbr,
    voxel::{meshing::chunk_reloader::VoxelChunkReloaderSystemDesc, setter::VoxelSetterSystemDesc},
};

use amethyst::{
    assets::PrefabLoaderSystemDesc,
    config::Config,
    core::TransformBundle,
    input::InputBundle,
    prelude::*,
    renderer::{
        formats::mtl::MaterialPrefab, palette::Srgb, types::DefaultBackend, RenderDebugLines,
        RenderSkybox, RenderToWindow, RenderingBundle,
    },
    utils::application_dir,
    LoggerConfig,
};
use std::path::PathBuf;
use structopt::StructOpt;

fn run_app(map_file: PathBuf) -> amethyst::Result<()> {
    let assets_dir = application_dir("assets")?;

    let config_dir = assets_dir.join("config");
    let logger_config_path = config_dir.join("logger.ron");
    let display_config_path = config_dir.join("display_config.ron");
    let input_config_path = config_dir.join("map_editor_bindings.ron");

    amethyst::Logger::from_config(LoggerConfig::load(&logger_config_path)?).start();

    let game_data = GameDataBuilder::new()
        .with_system_desc(
            PrefabLoaderSystemDesc::<MaterialPrefab>::default(),
            "material_prefab_loader",
            &[],
        )
        .with_bundle(TransformBundle::new())?
        .with_bundle(
            InputBundle::<GameBindings>::new().with_bindings_from_file(&input_config_path)?,
        )?
        .with_system_desc(
            CameraControlSystemDesc::<GameBindings>::default(),
            "camera_control",
            &[],
        )
        .with(DrawCameraFeetSystem, "draw_camera_feet", &[])
        .with(FlashlightSystem, "flashlight_system", &[])
        .with(
            HoverObjectSystem::<GameBindings>::default(),
            "hover_object",
            &[],
        )
        .with(HoverHintSystem, "hover_hint", &["hover_object"])
        .with_system_desc(EditVoxelSystemDesc, "edit_voxel", &["hover_object"])
        .with_system_desc(VoxelSetterSystemDesc, "voxel_setter", &[])
        .with_system_desc(VoxelChunkReloaderSystemDesc, "chunk_reloader", &[])
        .with_bundle(
            RenderingBundle::<DefaultBackend>::new()
                .with_plugin(
                    RenderToWindow::from_config_path(display_config_path)?
                        .with_clear([0.0, 0.0, 0.0, 1.0]),
                )
                .with_plugin(RenderTriplanarPbr::default())
                .with_plugin(RenderSkybox::with_colors(
                    Srgb::new(0.82, 0.51, 0.50),
                    Srgb::new(0.18, 0.11, 0.85),
                ))
                .with_plugin(RenderDebugLines::default()),
        )?;
    let mut game = Application::new(&assets_dir, OnlyState::new(map_file), game_data)?;
    game.run();

    Ok(())
}

#[derive(StructOpt, Debug)]
#[structopt(name = "voxel-mapper-editor")]
struct Opt {
    #[structopt(parse(from_os_str))]
    map_file: PathBuf,
}

fn main() -> amethyst::Result<()> {
    let opt = Opt::from_args();
    run_app(opt.map_file)
}
