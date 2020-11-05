use crate::voxel::{
    chunk_cache_flusher::ChunkCacheFlusher, double_buffer::EditedChunksBackBuffer, VoxelDistance,
    VoxelMap, VoxelType, EMPTY_VOXEL,
};

use amethyst::{core::ecs::prelude::*, derive::SystemDesc, shrev::EventChannel};
use building_blocks::prelude::*;
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// TODO: delete entire chunks when they become empty

#[derive(Clone, Debug)]
pub struct EditVoxelsRequest {
    pub voxels: Vec<(Point3i, SetVoxel)>,
}

/// The data actually stored in each point of the voxel map.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SetVoxel {
    /// Points to some palette element.
    pub voxel_type: VoxelType,
    /// Distance from the isosurface.
    pub distance: f32,
}

/// Writes the `EditVoxelsRequest`s into the affected chunks out of place and puts the chunked edits
/// into the `EditedChunksBackBuffer` to be merged in at the end of the frame.
#[derive(SystemDesc)]
#[system_desc(name(VoxelEditorSystemDesc))]
pub struct VoxelEditorSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<EditVoxelsRequest>,
}

impl VoxelEditorSystem {
    pub fn new(reader_id: ReaderId<EditVoxelsRequest>) -> Self {
        VoxelEditorSystem { reader_id }
    }
}

impl<'a> System<'a> for VoxelEditorSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        ReadExpect<'a, VoxelMap>,
        Write<'a, Option<EditedChunksBackBuffer>>,
        Read<'a, EventChannel<EditVoxelsRequest>>,
        ReadExpect<'a, ChunkCacheFlusher>,
    );

    fn run(&mut self, (map, mut backbuffer, set_events, cache_flusher): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_editor");

        let local_chunk_cache = LocalChunkCache3::new();

        let mut edited_chunks = HashMap::new();
        for EditVoxelsRequest { voxels } in set_events.read(&mut self.reader_id) {
            for (
                p,
                SetVoxel {
                    voxel_type: new_type,
                    distance: new_dist,
                },
            ) in voxels.into_iter()
            {
                // Get the chunk containing the point. We only write out of place into the
                // backbuffer.
                let chunk_key = map.voxels.chunk_key(p);
                let chunk = edited_chunks.entry(chunk_key).or_insert_with(|| {
                    if let Some(c) = map.voxels.get_chunk(chunk_key, &local_chunk_cache) {
                        c.array.clone()
                    } else {
                        Array3::fill(map.voxels.extent_for_chunk_at_key(&chunk_key), EMPTY_VOXEL)
                    }
                });

                // Set the new voxel value.
                let voxel = chunk.get_mut(p);
                voxel.distance = VoxelDistance::encode(*new_dist);
                voxel.voxel_type = *new_type;
            }
        }

        // It's necessary to reload neighboring chunks when voxels change close to the boundaries.
        // We just always add the neighbors for simplicity.
        let mut neighbor_chunks = Vec::new();
        for chunk_key in edited_chunks.keys() {
            for offset in Point3i::von_neumann_offsets().iter() {
                neighbor_chunks.push(*chunk_key + *offset);
            }
        }

        assert!(backbuffer.is_none());
        *backbuffer = Some(EditedChunksBackBuffer {
            edited_chunks,
            neighbor_chunks,
        });

        cache_flusher.flush(local_chunk_cache);
    }
}
