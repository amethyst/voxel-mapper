use super::{loader::VoxelMeshLoader, manager::VoxelMeshManager};
use crate::{
    assets::IndexedPosColorNormVertices,
    collision::voxel_bvt::{generate_chunk_bvt, ChunkBVT, VoxelBVT},
    voxel::{
        meshing::{generate_mesh_vertices, loader::ChunkMesh},
        Voxel, VoxelAssets, VoxelMap,
    },
};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    derive::SystemDesc,
    shrev::{EventChannel, ReaderId},
};
use ilattice3 as lat;
use ilattice3::{prelude::*, LatticeVoxels, VecLatticeMap};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// An event to notify the VoxelChunkReloaderSystem that it should reload the meshes all of
/// the chunks *atomically* (we don't want to see some chunks updated out of sync with others).
#[derive(Clone, Default)]
pub struct VoxelChunkChangeSet {
    pub chunks: HashMap<lat::Point, VecLatticeMap<Voxel>>,
}

impl VoxelChunkChangeSet {
    fn start_meshing(self) -> MeshingChunkSet {
        MeshingChunkSet {
            chunks_to_mesh: self.chunks.into_iter().collect(),
            ..Default::default()
        }
    }

    fn try_combine(&mut self, other: Self, limit: usize) -> Option<Self> {
        let union_size = other
            .chunks
            .keys()
            .collect::<HashSet<_>>()
            .union(&other.chunks.keys().collect())
            .count();
        if union_size > limit {
            return Some(other);
        }

        self.chunks.extend(other.chunks);

        None
    }
}

#[derive(Default)]
struct MeshingChunkSet {
    chunks_to_mesh: Vec<(lat::Point, VecLatticeMap<Voxel>)>,
    chunks_loading: Vec<LoadingChunk>,
    progress: ProgressCounter,
}

impl MeshingChunkSet {
    fn finish_meshing(self) -> LoadingChunkSet {
        assert!(self.chunks_to_mesh.is_empty());

        LoadingChunkSet {
            chunks: self.chunks_loading,
            progress: self.progress,
        }
    }
}

#[derive(Default)]
struct LoadingChunkSet {
    chunks: Vec<LoadingChunk>,
    progress: ProgressCounter,
}

struct LoadingChunk {
    key: lat::Point,
    mesh: Option<ChunkMesh>,
}

/// The sequence of chunk change sets to be reloaded. Supports combining change sets up to a maximum
/// size.
#[derive(Default)]
pub struct ChunkReloadQueue {
    queue: VecDeque<VoxelChunkChangeSet>,
    // A chunk set that still has chunks that need new meshes and bounding volumes generated. Kept
    // separate from the `queue` because it's not eligible for combining.
    generating_slot: Option<MeshingChunkSet>,
    // A chunk set that's only waiting for the meshes to be loaded onto the GPU.
    loading_slot: Option<LoadingChunkSet>,
}

impl ChunkReloadQueue {
    fn push(&mut self, change_set: VoxelChunkChangeSet, combine_limit: usize) {
        // Try to combine.
        if let Some(back) = self.queue.back_mut() {
            if let Some(change_set) = back.try_combine(change_set, combine_limit) {
                self.queue.push_back(change_set);
            }
        } else {
            self.queue.push_back(change_set);
        }
    }

    /// Returns whatever is in the loading slot, filling it from the queue if necessary.
    fn pop_generating(&mut self) -> Option<MeshingChunkSet> {
        self.generating_slot
            .take()
            .or_else(|| self.queue.pop_front().map(|s| s.start_meshing()))
    }

    fn put_generating(&mut self, set: MeshingChunkSet) {
        assert!(self.generating_slot.is_none());
        self.generating_slot.replace(set);
    }

    fn has_loading(&self) -> bool {
        self.loading_slot.is_some()
    }

    fn put_loading(&mut self, set: LoadingChunkSet) {
        assert!(!self.has_loading());
        self.loading_slot.replace(set);
    }

    fn pop_complete(&mut self) -> Option<LoadingChunkSet> {
        let is_complete = self
            .loading_slot
            .as_ref()
            .map_or(false, |set| set.progress.is_complete());

        if is_complete {
            self.loading_slot.take()
        } else {
            None
        }
    }
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
        Write<'a, ChunkReloadQueue>,
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
            mut reload_queue,
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

        // Create mesh entities once change sets have finished loading.
        if let Some(complete_set) = reload_queue.pop_complete() {
            for LoadingChunk { key, mesh } in complete_set.chunks.into_iter() {
                // Update entities and drop old assets.
                manager.update_chunk_mesh_entities(&key, mesh.clone(), material_arrays);
                if let Some(new_mesh) = mesh {
                    let _drop_old_chunk_meshes = meshes.chunk_meshes.insert(key, new_mesh);
                } else {
                    meshes.chunk_meshes.remove(&key);
                }
            }
        }

        {
            #[cfg(feature = "profiler")]
            profile_scope!("reload_enqueue");

            // Feed the pipeline with new change sets.
            let combine_limit = 1024;
            for change_set in chunk_changes.read(&mut self.reader_id).cloned() {
                reload_queue.push(change_set, combine_limit);
            }
        }

        // Keep an upper bound on the latency incurred from the meshing algorithm and updates to the
        // voxel BVH.
        let mut chunk_budget_remaining = 32;
        while chunk_budget_remaining > 0 {
            let mut gen_set = match reload_queue.pop_generating() {
                Some(set) => set,
                None => break,
            };

            let new_len = gen_set
                .chunks_to_mesh
                .len()
                .saturating_sub(chunk_budget_remaining);
            let work_chunks: Vec<(lat::Point, VecLatticeMap<Voxel>)> =
                gen_set.chunks_to_mesh.drain(new_len..).collect();
            chunk_budget_remaining -= work_chunks.len();

            // Do parallel isosurface generation.
            let generated_chunks: Vec<(
                lat::Point,
                Option<ChunkBVT>,
                Option<IndexedPosColorNormVertices>,
            )> = work_chunks
                .into_par_iter()
                .map(|(chunk_key, chunk_voxels)| {
                    let chunk_voxels = LatticeVoxels {
                        map: chunk_voxels,
                        palette: &map.voxels.palette,
                    };
                    let vertices = generate_mesh_vertices(&chunk_voxels);
                    let new_bvt = generate_chunk_bvt(&chunk_voxels, chunk_voxels.get_extent());

                    (chunk_key, new_bvt, vertices)
                })
                .collect();

            // Collect the generated results.
            for (chunk_key, bvt, vertices) in generated_chunks.into_iter() {
                // Load the mesh.
                {
                    #[cfg(feature = "profiler")]
                    profile_scope!("load_chunk_mesh");

                    let loading_chunk = LoadingChunk {
                        key: chunk_key,
                        mesh: vertices
                            .map(|v| loader.start_loading_chunk(v, &mut gen_set.progress)),
                    };
                    gen_set.chunks_loading.push(loading_chunk);
                }
                // Replace the chunk BVT.
                if let Some(bvt) = bvt {
                    voxel_bvt.insert_chunk(chunk_key, bvt);
                } else {
                    voxel_bvt.remove_chunk(&chunk_key);
                }
            }

            if gen_set.chunks_to_mesh.is_empty() && !reload_queue.has_loading() {
                reload_queue.put_loading(gen_set.finish_meshing());
            } else {
                // Need to wait.
                reload_queue.put_generating(gen_set);
                break;
            }
        }
    }
}
