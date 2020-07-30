use super::{meshing::loader::VoxelMeshLoader, VoxelArrayMaterialId, VoxelAssets, VoxelMap};

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
        let material_arrays =
            self.start_loading_materials(&map.palette_assets.material_arrays, &mut *progress);
        let meshes = self.mesh_loader.start_loading(&map.voxels, &mut *progress);

        (
            VoxelAssets {
                material_arrays,
                meshes,
            },
            map,
        )
    }

    fn start_loading_materials(
        &mut self,
        material_array_set: &HashMap<usize, String>,
        progress: &mut ProgressCounter,
    ) -> HashMap<VoxelArrayMaterialId, Handle<Prefab<MaterialPrefab>>> {
        let material_arrays_dir =
            application_dir("assets/material_arrays").expect("Failed to get material_arrays dir.");

        material_array_set
            .iter()
            .map(|(array_id, mtl_array_name)| {
                (
                    VoxelArrayMaterialId(*array_id),
                    self.material_loader.load(
                        material_arrays_dir
                            .join(mtl_array_name)
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
