use crate::voxel::{double_buffer::VoxelEditsBackBuffer, encode_distance, VoxelMap, EMPTY_VOXEL};

use amethyst::{core::ecs::prelude::*, derive::SystemDesc, shrev::EventChannel};
use ilattice3 as lat;
use ilattice3::{prelude::*, VecLatticeMap, FACE_ADJACENT_OFFSETS};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// TODO: delete entire chunks when they become empty

#[derive(Clone)]
pub struct SetVoxelsRequest {
    pub voxels: Vec<(lat::Point, SetVoxel)>,
}

/// The data actually stored in each point of the voxel map.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct SetVoxel {
    /// Points to some palette element.
    pub palette_address: u8,
    /// Distance from the isosurface.
    pub distance: f32,
}

/// Updates voxels when it receives `SetVoxel` events and sends `ChunkChanged` events for all chunks
/// that are affected.
#[derive(SystemDesc)]
#[system_desc(name(VoxelSetterSystemDesc))]
pub struct VoxelSetterSystem {
    #[system_desc(event_channel_reader)]
    reader_id: ReaderId<SetVoxelsRequest>,
}

impl VoxelSetterSystem {
    pub fn new(reader_id: ReaderId<SetVoxelsRequest>) -> Self {
        VoxelSetterSystem { reader_id }
    }
}

impl<'a> System<'a> for VoxelSetterSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        ReadExpect<'a, VoxelMap>,
        Write<'a, Option<VoxelEditsBackBuffer>>,
        Read<'a, EventChannel<SetVoxelsRequest>>,
    );

    fn run(&mut self, (map, mut backbuffer, set_events): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_setter");

        let mut edited_chunks = HashMap::new();
        for SetVoxelsRequest { voxels } in set_events.read(&mut self.reader_id) {
            for (
                p,
                SetVoxel {
                    palette_address: new_address,
                    distance: new_dist,
                },
            ) in voxels.into_iter()
            {
                // Get the chunk containing the point. We only write out of place into the
                // backbuffer.
                let chunk_key = map.voxels.map.chunk_key(p);
                let chunk = edited_chunks.entry(chunk_key).or_insert_with(|| {
                    if let Some(c) = map.voxels.map.get_chunk(&chunk_key) {
                        c.clone()
                    } else {
                        VecLatticeMap::fill(
                            map.voxels.map.extent_for_chunk_key(&chunk_key),
                            EMPTY_VOXEL,
                        )
                    }
                });

                // Set the new voxel value.
                let voxel = chunk.get_world_ref_mut(p);
                voxel.distance = encode_distance(*new_dist);
                voxel.palette_address = *new_address;
            }
        }

        // It's necessary to reload neighboring chunks when voxels change close to the boundaries.
        // We just always add the neighbors for simplicity.
        let mut neighbor_chunks = Vec::new();
        for chunk_key in edited_chunks.keys() {
            for offset in FACE_ADJACENT_OFFSETS.iter() {
                neighbor_chunks.push(*chunk_key + *offset);
            }
        }

        assert!(backbuffer.is_none());
        *backbuffer = Some(VoxelEditsBackBuffer {
            edited_chunks,
            neighbor_chunks,
        });
    }
}
