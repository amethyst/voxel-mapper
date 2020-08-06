use crate::voxel::{editor::EditVoxelsRequest, Voxel, VoxelMap};

use amethyst::{core::ecs::prelude::*, shrev::EventChannel};
use ilattice3 as lat;
use ilattice3::VecLatticeMap;
use std::collections::{HashMap, HashSet};

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// Used by systems that want to double buffer their `SetVoxelRequests`, allowing them to run in
/// concurrently with the `VoxelEditorSystem`. Any requests written here in frame T will be written
/// to the `VoxelMap` at the end of frame T+1.
#[derive(Default)]
pub struct EditVoxelsRequestBackBuffer {
    requests: Vec<EditVoxelsRequest>,
}

impl EditVoxelsRequestBackBuffer {
    pub fn push_request(&mut self, edit: EditVoxelsRequest) {
        self.requests.push(edit);
    }
}

/// For the sake of pipelining, all voxels edits are first written out of place by the
/// `VoxelEditorSystem`. They get merged into the `VoxelMap` by the `VoxelDoubleBufferingSystem` at
/// the end of a frame.
#[derive(Default)]
pub struct EditedChunksBackBuffer {
    pub edited_chunks: HashMap<lat::Point, VecLatticeMap<Voxel>>,
    pub neighbor_chunks: Vec<lat::Point>,
}

#[derive(Default)]
pub struct DirtyChunks {
    pub chunks: HashSet<lat::Point>,
}

/// The system responsible for merging the `EditedChunksBackBuffer` into the `VoxelMap`. This allows
/// the `VoxelChunkProcessorSystem` and `VoxelEditorSystem` to run in parallel at the expense of a
/// single frame of latency.
pub struct VoxelDoubleBufferingSystem;

impl<'a> System<'a> for VoxelDoubleBufferingSystem {
    type SystemData = (
        Write<'a, EditVoxelsRequestBackBuffer>,
        Write<'a, EventChannel<EditVoxelsRequest>>,
        Write<'a, Option<EditedChunksBackBuffer>>,
        Write<'a, Option<DirtyChunks>>,
        WriteExpect<'a, VoxelMap>,
    );

    fn run(
        &mut self,
        (
            mut edit_requests, mut set_voxels_channel, mut edits, mut dirty_chunks, mut map
        ): Self::SystemData,
    ) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_double_buffering");

        // Submit the requests to the setter.
        set_voxels_channel.drain_vec_write(&mut edit_requests.requests);

        // Merge the edits into the map.
        let EditedChunksBackBuffer {
            edited_chunks,
            neighbor_chunks,
        } = edits.take().unwrap();
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
