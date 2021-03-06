use crate::voxel::{
    empty_array, empty_chunk_hash_map, Voxel, VoxelChunkHashMap, VoxelMap, VOXEL_CHUNK_SHAPE,
};

use amethyst::core::ecs::prelude::*;
use building_blocks::prelude::*;
use std::collections::HashSet;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// For the sake of pipelining, all voxels edits are first written out of place here. They get
/// merged into the `VoxelMap` by the `VoxelDoubleBufferingSystem` at the end of a frame.
pub struct EditedChunksBackBuffer {
    edited_voxels: VoxelChunkHashMap,
    // Includes the edited chunks as well as their neighbors, all of which need to be re-meshed.
    dirty_chunk_keys: HashSet<Point3i>,
}

impl EditedChunksBackBuffer {
    pub fn new() -> Self {
        Self {
            edited_voxels: empty_chunk_hash_map(),
            dirty_chunk_keys: Default::default(),
        }
    }

    /// This function does read-modify-write of the voxels in `extent`, reading from `reader` and
    /// writing into the backbuffer. This enables parallelism between voxel editors and the chunk
    /// processor. All edited chunks and their neighbors will be marked as dirty.
    pub fn edit_voxels_out_of_place(
        &mut self,
        reader: &CompressibleChunkMapReader3x1<Lz4, Voxel>,
        extent: &Extent3i,
        edit_func: impl Fn(Point3i, &mut Voxel),
    ) {
        // Copy any of the overlapping chunks that don't already exist in the backbuffer, i.e. those
        // chunks which haven't been modified by this function yet.
        for chunk_min in reader.indexer.chunk_mins_for_extent(extent) {
            let chunk_key = ChunkKey::new(0, chunk_min);
            self.edited_voxels
                .get_mut_chunk_or_insert_with(chunk_key, || {
                    reader.get_chunk(chunk_key).cloned().unwrap_or(empty_array(
                        reader.indexer.extent_for_chunk_with_min(chunk_min),
                    ))
                });
        }

        // Mark the chunks and their neighbors as dirty.
        let extent_with_neighbor_chunks = Extent3i::from_min_and_max(
            extent.minimum - VOXEL_CHUNK_SHAPE,
            extent.max() + VOXEL_CHUNK_SHAPE,
        );
        for chunk_key in reader
            .indexer
            .chunk_mins_for_extent(&extent_with_neighbor_chunks)
        {
            self.dirty_chunk_keys.insert(chunk_key);
        }

        // Edit the backbuffer.
        self.edited_voxels
            .lod_view_mut(0)
            .for_each_mut(extent, edit_func);
    }
}

#[derive(Default)]
pub struct DirtyChunks {
    pub chunks: HashSet<Point3i>,
}

/// The system responsible for merging the `EditedChunksBackBuffer` into the `VoxelMap`. This allows
/// the `VoxelChunkProcessorSystem` and systems that edit the `EditedChunksBackBuffer` to run in
/// parallel at the expense of a single frame of latency.
pub struct VoxelDoubleBufferingSystem;

impl<'a> System<'a> for VoxelDoubleBufferingSystem {
    type SystemData = (
        Write<'a, Option<DirtyChunks>>,
        WriteExpect<'a, EditedChunksBackBuffer>,
        WriteExpect<'a, VoxelMap>,
    );

    fn run(&mut self, (mut dirty_chunks, mut edits, mut map): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_double_buffering");

        // Create a new backbuffer.
        let EditedChunksBackBuffer {
            edited_voxels,
            dirty_chunk_keys,
        } = std::mem::replace(&mut *edits, EditedChunksBackBuffer::new());

        // Merge the edits into the map.
        for (chunk_key, chunk) in edited_voxels.take_storage().into_iter() {
            map.voxels.write_chunk(chunk_key, chunk);
        }

        // Update the set of dirty chunks so the `ChunkReloaderSystem` can see them on the next
        // frame.
        assert!(dirty_chunks.is_none());
        *dirty_chunks = Some(DirtyChunks {
            chunks: dirty_chunk_keys,
        });
    }
}
