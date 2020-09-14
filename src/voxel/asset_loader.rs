use super::{
    meshing::loader::VoxelMeshLoader, ArrayMaterialId, LocalVoxelChunkCache, VoxelAssets, VoxelMap,
};

use amethyst::{
    assets::{Handle, Prefab, PrefabLoader, ProgressCounter, RonFormat},
    core::ecs::prelude::*,
    renderer::formats::mtl::MaterialPrefab,
    utils::application_dir,
};
use std::collections::HashMap;

#[derive(SystemData)]
pub struct VoxelAssetLoader<'a> {
    material_loader: PrefabLoader<'a, MaterialPrefab>,
    mesh_loader: VoxelMeshLoader<'a>,
}

impl<'a> VoxelAssetLoader<'a> {
    pub fn start_loading(
        &mut self,
        map: &VoxelMap,
        chunk_cache: &LocalVoxelChunkCache,
        progress: &mut ProgressCounter,
    ) -> VoxelAssets {
        let array_materials =
            self.start_loading_materials(&map.palette_assets.array_materials, &mut *progress);
        let meshes =
            self.mesh_loader
                .start_loading_all_chunks(&map.voxels, chunk_cache, &mut *progress);

        VoxelAssets {
            array_materials,
            meshes,
        }
    }

    fn start_loading_materials(
        &mut self,
        material_array_set: &HashMap<usize, String>,
        progress: &mut ProgressCounter,
    ) -> HashMap<ArrayMaterialId, Handle<Prefab<MaterialPrefab>>> {
        let array_materials_dir =
            application_dir("assets/array_materials").expect("Failed to get array_materials dir.");

        material_array_set
            .iter()
            .map(|(array_id, mtl_array_name)| {
                (
                    ArrayMaterialId(*array_id),
                    self.material_loader.load(
                        array_materials_dir
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
