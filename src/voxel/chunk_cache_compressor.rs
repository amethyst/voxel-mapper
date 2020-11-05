use crate::voxel::VoxelMap;

use amethyst::core::ecs::prelude::*;

/// A system that evicts and compresses the least recently used voxel chunks when the cache gets too
/// big.
#[derive(Default)]
pub struct ChunkCacheCompressorSystem;

// These constants should be correlated with the size of a chunk, which is currently 16^3 * 2 bytes.

/// We'll reserve a little under a gigabyte for the cache.
const MAX_CACHED_CHUNKS: usize = 1000000;
// Avoid high latency from compressing too many chunks in one frame. 8192-byte chunk compression
// latency is around 0.1 ms.
const MAX_COMPRESSED_PER_FRAME_PER_CORE: usize = 50;

impl<'a> System<'a> for ChunkCacheCompressorSystem {
    type SystemData = WriteExpect<'a, VoxelMap>;

    fn run(&mut self, mut voxel_map: Self::SystemData) {
        // PERF: compression could happen in parallel, but we'd need to add some CompressibleMap
        // APIs

        let overgrowth = voxel_map.voxels.chunks.len_cached() as i64 - MAX_CACHED_CHUNKS as i64;
        for _ in 0..overgrowth
            .max(0)
            .min(MAX_COMPRESSED_PER_FRAME_PER_CORE as i64)
        {
            voxel_map.voxels.chunks.compress_lru();
        }
    }
}
