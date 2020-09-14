use crate::voxel::{VoxelChunkCache, VoxelMap};

use amethyst::core::ecs::prelude::*;
use crossbeam::{Receiver, Sender};

pub struct ChunkCacheFlusher {
    tx: Sender<VoxelChunkCache>,
}

impl ChunkCacheFlusher {
    pub fn new(tx: Sender<VoxelChunkCache>) -> Self {
        Self { tx }
    }

    pub fn flush(&self, cache: VoxelChunkCache) {
        self.tx.send(cache).unwrap();
    }
}

pub struct ChunkCacheReceiver {
    rx: crossbeam::Receiver<VoxelChunkCache>,
}

impl ChunkCacheReceiver {
    pub fn new(rx: Receiver<VoxelChunkCache>) -> Self {
        Self { rx }
    }
}

/// A system that flushes system-local `VoxelChunkCache`s. Just send your cache using the
/// `ChunkCacheFlusher`.
#[derive(Default)]
pub struct ChunkCacheFlusherSystem;

impl<'a> System<'a> for ChunkCacheFlusherSystem {
    type SystemData = (
        ReadExpect<'a, ChunkCacheReceiver>,
        WriteExpect<'a, VoxelMap>,
    );

    fn run(&mut self, (cache_rx, mut voxel_map): Self::SystemData) {
        for cache in cache_rx.rx.try_iter() {
            voxel_map.voxels.map.chunks.flush_local_cache(cache);
        }
    }
}
