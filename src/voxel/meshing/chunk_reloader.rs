use super::{loader::VoxelMeshLoader, manager::VoxelMeshManager};
use crate::{
    collision::VoxelBVT,
    voxel::{meshing::loader::ChunkMeshes, Voxel, VoxelAssets, VoxelMap},
};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    derive::SystemDesc,
    shrev::{EventChannel, ReaderId},
};
use ilattice3 as lat;
use ilattice3::{find_surface_voxels, prelude::*, LatticeVoxels, VecLatticeMap};
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
    meshes: ChunkMeshes,
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
            for complete_chunk in complete_set.chunks.into_iter() {
                // Update entities and drop old assets.
                manager.update_chunk_mesh_entities(
                    &complete_chunk.key,
                    &complete_chunk.meshes,
                    material_arrays,
                );
                let _drop_old_chunk_meshes = meshes
                    .chunk_meshes
                    .insert(complete_chunk.key, complete_chunk.meshes);
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
        let max_chunks_generated_per_frame = 32;
        let mut num_chunks_generated = 0;
        while num_chunks_generated < max_chunks_generated_per_frame {
            let mut gen_set = match reload_queue.pop_generating() {
                Some(set) => set,
                None => break,
            };

            while let Some((chunk_key, chunk_voxels)) = gen_set.chunks_to_mesh.pop() {
                let chunk_voxels = LatticeVoxels {
                    map: chunk_voxels,
                    palette: &map.voxels.palette,
                };

                // Replace the chunk BVT.
                {
                    #[cfg(feature = "profiler")]
                    profile_scope!("update_bvt_chunk");

                    // This is subtly different from the set of surface points returned from the
                    // surface nets meshing algorithm, since we use the IsEmpty check instead of the
                    // signed distance. This allows us to have parts of the mesh that don't collide.
                    let solid_points: Vec<_> =
                        find_surface_voxels(&chunk_voxels, chunk_voxels.get_extent());
                    voxel_bvt.insert_chunk(chunk_key, &solid_points);
                }

                // Regenerate the mesh.
                let loading_chunk_meshes =
                    loader.start_loading_chunk(&chunk_voxels, &mut gen_set.progress);
                gen_set.chunks_loading.push(LoadingChunk {
                    key: chunk_key,
                    meshes: loading_chunk_meshes,
                });

                num_chunks_generated += 1;
                if num_chunks_generated == max_chunks_generated_per_frame {
                    break;
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
