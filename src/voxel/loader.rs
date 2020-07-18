use super::{
    meshing::loader::VoxelMeshLoader, VoxelAssets, VoxelMap, VoxelMaterial, VoxelMaterialInt,
};

use amethyst::{
    assets::{Handle, Prefab, PrefabLoader, ProgressCounter, RonFormat},
    core::ecs::prelude::*,
    renderer::formats::mtl::MaterialPrefab,
    utils::application_dir,
};
use std::collections::HashMap;

#[derive(SystemData)]
pub struct VoxelLoader<'a> {
    material_loader: PrefabLoader<'a, MaterialPrefab>,
    mesh_loader: VoxelMeshLoader<'a>,
}

impl<'a> VoxelLoader<'a> {
    pub fn start_loading(
        &mut self,
        map: VoxelMap,
        progress: &mut ProgressCounter,
    ) -> (VoxelAssets, VoxelMap) {
        let materials = self.start_loading_materials(&map.palette_assets.materials, &mut *progress);
        let meshes = self.mesh_loader.start_loading(&map.voxels, &mut *progress);

        let assets = VoxelAssets {
            materials,
            meshes,
            debug: false,
        };

        (assets, map)
    }

    fn start_loading_materials(
        &mut self,
        material_set: &HashMap<VoxelMaterialInt, String>,
        progress: &mut ProgressCounter,
    ) -> HashMap<VoxelMaterial, Handle<Prefab<MaterialPrefab>>> {
        let materials_dir =
            application_dir("assets/materials").expect("Failed to get materials dir.");

        material_set
            .iter()
            .map(|(m, mtl_name)| {
                (
                    VoxelMaterial(*m),
                    self.material_loader.load(
                        materials_dir
                            .join(mtl_name)
                            .join("prefab.ron")
                            .to_str()
                            .unwrap(),
                        RonFormat,
                        &mut *progress,
                    ),
                )
            })
            .collect()
    }
}
