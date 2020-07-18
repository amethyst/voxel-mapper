use super::{loader::VoxelMeshLoader, manager::VoxelMeshManager};
use crate::voxel::{meshing::loader::ChunkMeshes, Voxel, VoxelAssets, VoxelMap};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    derive::SystemDesc,
    shrev::{EventChannel, ReaderId},
};
use ilattice3 as lat;
use ilattice3::{LatticeVoxels, VecLatticeMap};
use std::collections::{HashMap, HashSet, VecDeque};

/// An event to notify the VoxelChunkReloaderSystem that it should reload the meshes all of
/// `chunk_keys` *atomically* (we don't want to see some chunks updated out of sync with others).
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
    voxels: VecLatticeMap<Voxel>,
    meshes: ChunkMeshes,
}

/// The sequence of chunk change sets to be reloaded. Supports combining change sets up to a maximum
/// size.
#[derive(Default)]
pub struct ChunkReloadQueue {
    queue: VecDeque<VoxelChunkChangeSet>,
    // A chunk set that still has chunks that need new meshes generated. Kept separate from the
    // `queue` because it's not eligible for combining.
    meshing_slot: Option<MeshingChunkSet>,
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
    fn pop_meshing(&mut self) -> Option<MeshingChunkSet> {
        self.meshing_slot
            .take()
            .or_else(|| self.queue.pop_front().map(|s| s.start_meshing()))
    }

    fn put_meshing(&mut self, set: MeshingChunkSet) {
        assert!(self.meshing_slot.is_none());
        self.meshing_slot.replace(set);
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
            loader,
            mut manager,
        ): Self::SystemData,
    ) {
        let start = std::time::Instant::now();

        let VoxelAssets {
            materials, meshes, ..
        } = &mut *voxel_assets;

        // Create mesh entities once change sets have finished loading.
        if let Some(complete_set) = reload_queue.pop_complete() {
            for complete_chunk in complete_set.chunks.into_iter() {
                let voxels = LatticeVoxels {
                    map: complete_chunk.voxels,
                    palette: &map.voxels.palette,
                };
                // Update entities and drop old assets.
                manager.update_chunk_mesh_entities(
                    &complete_chunk.key,
                    &voxels,
                    &complete_chunk.meshes,
                    materials,
                );
                let _drop_old_chunk_meshes = meshes
                    .chunk_meshes
                    .insert(complete_chunk.key, complete_chunk.meshes);
            }
        }

        // Feed the pipeline with new change sets.
        let combine_limit = 256;
        for change_set in chunk_changes.read(&mut self.reader_id).cloned() {
            reload_queue.push(change_set, combine_limit);
        }

        // Keep an upper bound on the latency incurred from the meshing algorithm. This is probably
        // way higher than it needs to be, and the real bottleneck is often GPU bandwidth.
        let max_meshes_per_frame = 100;
        // Generate meshes.
        let mut num_chunks_meshed = 0;
        let mut num_triangles_generated = 0;
        let mut total_surface_nets_micros = 0;
        while num_chunks_meshed < max_meshes_per_frame {
            let mut meshing_set = match reload_queue.pop_meshing() {
                Some(set) => set,
                None => break,
            };

            while let Some((mesh_chunk_key, chunk_voxels)) = meshing_set.chunks_to_mesh.pop() {
                let chunk_voxels = LatticeVoxels {
                    map: chunk_voxels,
                    palette: &map.voxels.palette,
                };
                let (loading_chunk_meshes, num_triangles, surface_nets_micros) =
                    loader.start_loading_chunk(&chunk_voxels, &mut meshing_set.progress);
                total_surface_nets_micros += surface_nets_micros;
                num_triangles_generated += num_triangles;
                meshing_set.chunks_loading.push(LoadingChunk {
                    key: mesh_chunk_key,
                    meshes: loading_chunk_meshes,
                    voxels: chunk_voxels.map,
                });

                num_chunks_meshed += 1;
                if num_chunks_meshed == max_meshes_per_frame {
                    break;
                }
            }

            if meshing_set.chunks_to_mesh.is_empty() && !reload_queue.has_loading() {
                reload_queue.put_loading(meshing_set.finish_meshing());
            } else {
                // Need to wait.
                reload_queue.put_meshing(meshing_set);
                break;
            }
        }

        if num_chunks_meshed > 0 {
            log::debug!(
                "chunk_reloader took {} millis (surface nets = {} micros) to reload {} chunks, with {} triangles",
                start.elapsed().as_millis(),
                total_surface_nets_micros,
                num_chunks_meshed,
                num_triangles_generated
            );
        }
        if !reload_queue.queue.is_empty() {
            log::debug!("Reload queue len = {}", reload_queue.queue.len());
        }
    }
}
