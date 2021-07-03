use super::{
    chunk_cache_compressor::ChunkCacheCompressorSystem,
    chunk_cache_flusher::{ChunkCacheFlusher, ChunkCacheFlusherSystem, ChunkCacheReceiver},
    chunk_processor::{MeshMode, VoxelChunkProcessorSystem},
    double_buffer::{EditedChunksBackBuffer, VoxelDoubleBufferingSystem},
};

use amethyst::core::{ecs::prelude::*, SystemBundle};
use building_blocks::{core::Point3i, search::OctreeDbvt};

/// Includes the voxel systems necessary for making edits to the `VoxelMap` and generating the
/// corresponding entities in real time. Before dispatching, the `World` must contain a `VoxelMap`
/// and `VoxelAssets` resources. These can be created using the `load_voxel_map` function and
/// `VoxelAssetLoader`.
///
/// In order for edits to be considered by the pipeline of systems, they must be written to the
/// `EditedChunksBackBuffer`. Editing the `VoxelMap` directly will not work.
pub struct VoxelSystemBundle;

impl<'a, 'b> SystemBundle<'a, 'b> for VoxelSystemBundle {
    fn build(
        self,
        world: &mut World,
        dispatcher: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), amethyst::Error> {
        world.insert(OctreeDbvt::<Point3i>::default());
        world.insert(MeshMode::SurfaceNets);
        world.insert(EditedChunksBackBuffer::new());

        // Chunk cache maintenance.
        let (tx, rx) = crossbeam::channel::unbounded();
        world.insert(ChunkCacheFlusher::new(tx));
        world.insert(ChunkCacheReceiver::new(rx));
        dispatcher.add(ChunkCacheFlusherSystem, "chunk_cache_flusher", &[]);
        dispatcher.add(ChunkCacheCompressorSystem, "chunk_cache_compressor", &[]);

        // Voxel editing.
        dispatcher.add(VoxelChunkProcessorSystem, "voxel_chunk_processor", &[]);
        dispatcher.add(
            VoxelDoubleBufferingSystem,
            "voxel_double_buffering",
            &["voxel_chunk_processor"],
        );

        Ok(())
    }
}
