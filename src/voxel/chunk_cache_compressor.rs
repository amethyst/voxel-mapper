use crate::voxel::VoxelMap;

use amethyst::core::ecs::prelude::*;

/// A system that evicts and compresses the least recently used voxel chunks when the cache gets too
/// big.
#[derive(Default)]
pub struct ChunkCacheCompressorSystem;

/// This constant should be correlated with the size of a chunk, which is currently 16^3 * 2 bytes.
/// We'll reserve a little under a gigabyte for the cache.
const MAX_CACHED_CHUNKS: usize = 1000000;

impl<'a> System<'a> for ChunkCacheCompressorSystem {
    type SystemData = WriteExpect<'a, VoxelMap>;

    fn run(&mut self, mut voxel_map: Self::SystemData) {
        let overgrowth = voxel_map.voxels.map.chunks.len_cached() as i64 - MAX_CACHED_CHUNKS as i64;
        for _ in 0..overgrowth.max(0) {
            voxel_map.voxels.map.chunks.compress_lru();
        }
    }
}
