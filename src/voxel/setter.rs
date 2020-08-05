use crate::voxel::{
    encode_distance, meshing::chunk_reloader::VoxelChunkChangeSet, VoxelMap, EMPTY_VOXEL,
};

use amethyst::{core::ecs::prelude::*, derive::SystemDesc, shrev::EventChannel};
use ilattice3 as lat;
use ilattice3::FACE_ADJACENT_OFFSETS;
use std::collections::HashSet;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

// TODO: delete entire chunks when they become empty

#[derive(Clone)]
pub struct SetVoxelsEvent {
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
    reader_id: ReaderId<SetVoxelsEvent>,
}

impl VoxelSetterSystem {
    pub fn new(reader_id: ReaderId<SetVoxelsEvent>) -> Self {
        VoxelSetterSystem { reader_id }
    }
}

impl<'a> System<'a> for VoxelSetterSystem {
    #[allow(clippy::type_complexity)]
    type SystemData = (
        WriteExpect<'a, VoxelMap>,
        Write<'a, EventChannel<VoxelChunkChangeSet>>,
        Read<'a, EventChannel<SetVoxelsEvent>>,
    );

    fn run(&mut self, (mut voxel_map, mut chunk_changed_events, set_events): Self::SystemData) {
        #[cfg(feature = "profiler")]
        profile_scope!("voxel_setter");

        let mut chunks_changed = HashSet::new();
        {
            #[cfg(feature = "profiler")]
            profile_scope!("process_set_events");

            for SetVoxelsEvent { voxels } in set_events.read(&mut self.reader_id) {
                for (
                    p,
                    SetVoxel {
                        palette_address: new_address,
                        distance: new_dist,
                    },
                ) in voxels.into_iter()
                {
                    // Set the new voxel.
                    let (chunk_key, voxel) =
                        voxel_map.voxels.map.get_mut_or_create(&p, EMPTY_VOXEL);
                    voxel.distance = encode_distance(*new_dist);
                    voxel.palette_address = *new_address;
                    chunks_changed.insert(chunk_key);
                }
            }
        }

        let mut adjacent_chunks = Vec::new();
        for chunk in chunks_changed.iter() {
            for offset in FACE_ADJACENT_OFFSETS.iter() {
                adjacent_chunks.push(*chunk + *offset);
            }
        }
        for adj_chunk in adjacent_chunks.into_iter() {
            chunks_changed.insert(adj_chunk);
        }

        chunk_changed_events.single_write(VoxelChunkChangeSet {
            chunks: chunks_changed,
        });
    }
}
