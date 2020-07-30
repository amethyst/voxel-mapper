use crate::voxel::{
    encode_distance, meshing::chunk_reloader::VoxelChunkChangeSet, VoxelMap, EMPTY_VOXEL,
};

use amethyst::{core::ecs::prelude::*, derive::SystemDesc, shrev::EventChannel};
use ilattice3 as lat;
use std::collections::HashSet;

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
        let mut chunks_changed = HashSet::new();
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
                let (chunk_key, voxel) = voxel_map.voxels.map.get_mut_or_create(&p, EMPTY_VOXEL);
                voxel.distance = encode_distance(*new_dist);
                voxel.palette_address = *new_address;
                chunks_changed.insert(chunk_key);

                // If the point is close to a boundary, then we need to update the adjacent chunks.
                let chunk_extent = voxel_map.voxels.map.extent_for_chunk_key(&chunk_key);
                let boundaries = chunk_extent.point_is_on_boundary(&p);
                for (dir, is_on_boundary) in boundaries.iter() {
                    if *is_on_boundary {
                        let adjacent_chunk = chunk_key + lat::Point::from(dir);
                        chunks_changed.insert(adjacent_chunk);
                    }
                }
                let chunk_extent_within = chunk_extent.radial_grow(-1);
                let boundaries_within = chunk_extent_within.point_is_on_boundary(&p);
                for (dir, is_on_boundary) in boundaries_within.iter() {
                    if *is_on_boundary {
                        let adjacent_chunk = chunk_key + lat::Point::from(dir);
                        chunks_changed.insert(adjacent_chunk);
                    }
                }
            }
        }
        let chunks = chunks_changed
            .into_iter()
            .map(|chunk_key| {
                (
                    chunk_key,
                    voxel_map.voxels.get_chunk_and_boundary(&chunk_key).map,
                )
            })
            .collect();
        chunk_changed_events.single_write(VoxelChunkChangeSet { chunks });
    }
}
