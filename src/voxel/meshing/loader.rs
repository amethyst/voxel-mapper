use super::generate_mesh_vertices_with_surface_nets;

use crate::{
    assets::{BoundedMesh, IndexedPosColorNormVertices, MeshLoader},
    voxel::{ArrayMaterialId, Voxel, VoxelMap},
};

use amethyst::{assets::ProgressCounter, core::ecs::prelude::*};
use building_blocks::prelude::*;
use std::collections::HashMap;

/// Loads the vertices for chunks into `ChunkMesh` objects.
#[derive(SystemData)]
pub struct VoxelMeshLoader<'a> {
    pub mesh_loader: MeshLoader<'a>,
}

#[derive(Clone)]
pub struct ChunkMesh {
    pub material_array_id: ArrayMaterialId,
    pub mesh: BoundedMesh,
}

#[derive(Default)]
pub struct VoxelMeshes {
    pub chunk_meshes: HashMap<Point3i, ChunkMesh>,
}

impl<'a> VoxelMeshLoader<'a> {
    pub fn start_loading_all_chunks(
        &mut self,
        voxel_map: &VoxelMap,
        chunk_cache: &LocalChunkCache3<Voxel>,
        progress: &mut ProgressCounter,
    ) -> VoxelMeshes {
        let chunk_meshes = voxel_map
            .voxels
            .chunk_keys()
            .filter_map(|chunk_key| {
                let chunk_extent = voxel_map.voxels.extent_for_chunk_at_key(chunk_key);
                let vertices =
                    generate_mesh_vertices_with_surface_nets(voxel_map, &chunk_extent, chunk_cache);

                vertices.map(|v| (*chunk_key, self.start_loading_chunk(v, progress)))
            })
            .collect();

        VoxelMeshes { chunk_meshes }
    }

    pub fn start_loading_chunk(
        &self,
        vertices: IndexedPosColorNormVertices,
        progress: &mut ProgressCounter,
    ) -> ChunkMesh {
        let mesh = self
            .mesh_loader
            .start_loading_pos_color_norm_mesh(vertices, &mut *progress);

        ChunkMesh {
            // TODO: support multiple array materials
            material_array_id: ArrayMaterialId(1),
            mesh,
        }
    }
}
