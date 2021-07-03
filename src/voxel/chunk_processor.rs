use crate::{
    assets::IndexedPosColorNormVertices,
    voxel::{
        chunk_cache_flusher::ChunkCacheFlusher,
        double_buffer::DirtyChunks,
        meshing::{
            generate_mesh_vertices_with_greedy_quads, generate_mesh_vertices_with_surface_nets,
            loader::VoxelMeshLoader, manager::VoxelMeshManager,
        },
        VoxelAssets, VoxelMap,
    },
};

use amethyst::{assets::ProgressCounter, core::ecs::prelude::*};
use building_blocks::{prelude::*, search::OctreeDbvt, storage::OctreeSet};
use rayon::prelude::*;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub enum MeshMode {
    SurfaceNets,
    GreedyQuads,
}

pub struct VoxelChunkProcessorSystem;

impl<'a> System<'a> for VoxelChunkProcessorSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        ReadExpect<'a, VoxelMap>,
        ReadExpect<'a, MeshMode>,
        ReadExpect<'a, ChunkCacheFlusher>,
        Write<'a, Option<DirtyChunks>>,
        WriteExpect<'a, VoxelAssets>,
        WriteExpect<'a, OctreeDbvt<Point3i>>,
        VoxelMeshLoader<'a>,
        VoxelMeshManager<'a>,
    );

    fn run(
        &mut self,
        (
            voxel_map,
            mesh_mode,
            cache_flusher,
            mut dirty_chunks,
            mut voxel_assets,
            mut voxel_bvt,
            loader,
            mut manager,
        ): Self::SystemData,
    ) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_chunk_processor");

        let chunks_to_generate = match dirty_chunks.take() {
            Some(c) => c.chunks,
            None => return,
        };

        let VoxelAssets {
            array_materials,
            meshes,
            ..
        } = &mut *voxel_assets;

        // Do parallel processing of dirty chunks.
        let generated_chunks: Vec<(Point3i, OctreeSet, Option<IndexedPosColorNormVertices>)> =
            chunks_to_generate
                .into_par_iter()
                .filter_map(|chunk_min| {
                    let chunk_key = ChunkKey::new(0, chunk_min);

                    let local_chunk_cache = LocalChunkCache3::new();
                    let reader = voxel_map.voxels.reader(&local_chunk_cache);

                    let chunk_extent = reader.indexer.extent_for_chunk_with_min(chunk_min);

                    let vertices = match *mesh_mode {
                        MeshMode::SurfaceNets => generate_mesh_vertices_with_surface_nets(
                            &voxel_map,
                            &chunk_extent,
                            &local_chunk_cache,
                        ),
                        MeshMode::GreedyQuads => generate_mesh_vertices_with_greedy_quads(
                            &voxel_map,
                            &chunk_extent,
                            &local_chunk_cache,
                        ),
                    };

                    let maybe_processed_chunk = reader.get_chunk(chunk_key).map(|chunk| {
                        let is_empty_map =
                            TransformMap::new(chunk, voxel_map.voxel_info_transform());
                        let new_octree = OctreeSet::from_array3(&is_empty_map, chunk_extent);

                        (chunk_min, new_octree, vertices)
                    });

                    cache_flusher.flush(local_chunk_cache);

                    maybe_processed_chunk
                })
                .collect();

        // Collect the generated results.
        for (chunk_min, octree, vertices) in generated_chunks.into_iter() {
            // Load the mesh.
            let mesh = {
                #[cfg(feature = "profiler")]
                profile_scope!("load_chunk_mesh");

                let mut _unused_progress = ProgressCounter::new();
                vertices.map(|v| loader.start_loading_chunk(v, &mut _unused_progress))
            };

            // Replace the chunk BVT.
            if octree.is_empty() {
                voxel_bvt.remove(&chunk_min);
            } else {
                voxel_bvt.insert(chunk_min, octree);
            }

            // Update entities and drop old assets.
            manager.update_chunk_mesh_entities(chunk_min, mesh.clone(), array_materials);
            if let Some(new_mesh) = mesh {
                let _drop_old_chunk_meshes = meshes.chunk_meshes.insert(chunk_min, new_mesh);
            } else {
                meshes.chunk_meshes.remove(&chunk_min);
            }
        }
    }
}
