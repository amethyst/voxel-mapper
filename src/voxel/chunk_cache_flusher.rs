use crate::voxel::{Voxel, VoxelMap};

use amethyst::core::ecs::prelude::*;
use building_blocks::prelude::*;
use crossbeam::{Receiver, Sender};

pub struct ChunkCacheFlusher {
    tx: Sender<LocalChunkCache3<Voxel>>,
}

impl ChunkCacheFlusher {
    pub fn new(tx: Sender<LocalChunkCache3<Voxel>>) -> Self {
        Self { tx }
    }

    pub fn flush(&self, cache: LocalChunkCache3<Voxel>) {
        self.tx.send(cache).unwrap();
    }
}

pub struct ChunkCacheReceiver {
    rx: crossbeam::Receiver<LocalChunkCache3<Voxel>>,
}

impl ChunkCacheReceiver {
    pub fn new(rx: Receiver<LocalChunkCache3<Voxel>>) -> Self {
        Self { rx }
    }
}

// TODO: avoid flushing older data on top of newer compressed data in this scenario:
// 1. read uncached data into local cache
// 2. write new data
// 3. compress data out of cache
// 4. flush local cache
//
// Right now this is just unlikely because of the size of the cache and rate of compression

/// A system that flushes system-local `LocalChunkCache3<Voxel>`s. Just send your cache using the
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
            voxel_map.voxels.chunks.flush_local_cache(cache);
        }
    }
}
