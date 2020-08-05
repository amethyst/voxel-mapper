use crate::voxel::{Voxel, VoxelMap};

use amethyst::core::ecs::prelude::*;
use ilattice3 as lat;
use ilattice3::VecLatticeMap;
use std::collections::{HashMap, HashSet};

/// For the sake of pipelining, all voxels edits are first written out of place. They get merged
/// into the `VoxelMap` by the `VoxelDoubleBufferingSystem` at the end of a frame.
#[derive(Default)]
pub struct VoxelBackBuffer {
    pub edited_chunks: HashMap<lat::Point, VecLatticeMap<Voxel>>,
    pub neighbor_chunks: Vec<lat::Point>,
}

#[derive(Default)]
pub struct DirtyChunks {
    pub chunks: HashSet<lat::Point>,
}

/// The system responsible for merging the `VoxelBackBuffer` into the `VoxelMap`. This allows the
/// `VoxelChunkReloaderSystem` and `VoxelSetterSystem` to run in parallel.
pub struct VoxelDoubleBufferingSystem;

impl<'a> System<'a> for VoxelDoubleBufferingSystem {
    type SystemData = (
        Write<'a, Option<VoxelBackBuffer>>,
        Write<'a, Option<DirtyChunks>>,
        WriteExpect<'a, VoxelMap>,
    );

    fn run(&mut self, (mut backbuffer, mut dirty_chunks, mut map): Self::SystemData) {
        // Merge the edits into the map.
        let VoxelBackBuffer {
            edited_chunks,
            neighbor_chunks,
        } = backbuffer.take().unwrap();
        let mut new_dirty_chunks = HashSet::new();
        for (chunk_key, chunk_voxels) in edited_chunks.into_iter() {
            map.voxels.map.insert_chunk(chunk_key, chunk_voxels);
            new_dirty_chunks.insert(chunk_key);
        }
        new_dirty_chunks.extend(neighbor_chunks.into_iter());

        // Update the set of dirty chunks so the `ChunkReloaderSystem` can see them on the next
        // frame.
        assert!(dirty_chunks.is_none());
        *dirty_chunks = Some(DirtyChunks {
            chunks: new_dirty_chunks,
        });
    }
}
