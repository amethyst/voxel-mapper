use super::{loader::VoxelMeshLoader, manager::VoxelMeshManager};
use crate::{
    assets::IndexedPosColorNormVertices,
    collision::voxel_bvt::{generate_chunk_bvt, ChunkBVT, VoxelBVT},
    voxel::{meshing::generate_mesh_vertices, VoxelAssets, VoxelMap},
};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    derive::SystemDesc,
    shrev::{EventChannel, ReaderId},
};
use ilattice3 as lat;
use ilattice3::prelude::*;
use rayon::prelude::*;
use std::collections::HashSet;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

#[derive(Clone, Default)]
pub struct VoxelChunkChangeSet {
    pub chunks: HashSet<lat::Point>,
}

#[derive(SystemDesc)]
#[system_desc(name(VoxelChunkReloaderSystemDesc))]
pub struct VoxelChunkReloaderSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<VoxelChunkChangeSet>,
}

impl VoxelChunkReloaderSystem {
    pub fn new(reader_id: ReaderId<VoxelChunkChangeSet>) -> Self {
        VoxelChunkReloaderSystem { reader_id }
    }
}

impl<'a> System<'a> for VoxelChunkReloaderSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        Read<'a, EventChannel<VoxelChunkChangeSet>>,
        ReadExpect<'a, VoxelMap>,
        WriteExpect<'a, VoxelAssets>,
        WriteExpect<'a, VoxelBVT>,
        VoxelMeshLoader<'a>,
        VoxelMeshManager<'a>,
    );

    fn run(
        &mut self,
        (
            chunk_changes,
            map,
            mut voxel_assets,
            mut voxel_bvt,
            loader,
            mut manager,
        ): Self::SystemData,
    ) {
        #[cfg(feature = "profiler")]
        profile_scope!("chunk_reloader");

        let VoxelAssets {
            material_arrays,
            meshes,
            ..
        } = &mut *voxel_assets;

        for change_set in chunk_changes.read(&mut self.reader_id) {
            // Do parallel isosurface generation.
            let generated_chunks: Vec<(
                lat::Point,
                Option<ChunkBVT>,
                Option<IndexedPosColorNormVertices>,
            )> = change_set
                .chunks
                .clone()
                .into_par_iter()
                .map(|chunk_key| {
                    // TODO: figure out how to avoid copying like this; it's pretty slow
                    let chunk_voxels = map.voxels.get_chunk_and_boundary(&chunk_key);

                    let vertices = generate_mesh_vertices(&chunk_voxels);
                    let new_bvt = generate_chunk_bvt(&chunk_voxels, chunk_voxels.get_extent());

                    (chunk_key, new_bvt, vertices)
                })
                .collect();

            // Collect the generated results.
            for (chunk_key, bvt, vertices) in generated_chunks.into_iter() {
                // Load the mesh.
                let mesh = {
                    #[cfg(feature = "profiler")]
                    profile_scope!("load_chunk_mesh");

                    let mut _unused_progress = ProgressCounter::new();
                    vertices.map(|v| loader.start_loading_chunk(v, &mut _unused_progress))
                };

                // Replace the chunk BVT.
                if let Some(bvt) = bvt {
                    voxel_bvt.insert_chunk(chunk_key, bvt);
                } else {
                    voxel_bvt.remove_chunk(&chunk_key);
                }

                // Update entities and drop old assets.
                manager.update_chunk_mesh_entities(&chunk_key, mesh.clone(), material_arrays);
                if let Some(new_mesh) = mesh {
                    let _drop_old_chunk_meshes = meshes.chunk_meshes.insert(chunk_key, new_mesh);
                } else {
                    meshes.chunk_meshes.remove(&chunk_key);
                }
            }
        }
    }
}
