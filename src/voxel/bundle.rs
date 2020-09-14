use super::{
    chunk_cache_flusher::{ChunkCacheFlusher, ChunkCacheFlusherSystem, ChunkCacheReceiver},
    chunk_processor::{MeshMode, VoxelChunkProcessorSystem},
    double_buffer::VoxelDoubleBufferingSystem,
    editor::VoxelEditorSystemDesc,
};
use crate::collision::VoxelBVT;

use amethyst::core::{ecs::prelude::*, SystemBundle, SystemDesc};

/// Includes the voxel systems necessary for making edits to the `VoxelMap` and generating the
/// corresponding entities in real time. Before dispatching, the `World` must contain a `VoxelMap`
/// and `VoxelAssets` resources. These can be created using the `load_voxel_map` function and
/// `VoxelAssetLoader`.
///
/// In order for edits to be considered by the pipeline of systems, they must be submitted either to
/// the `EventChannel<EditVoxelsRequest>` or the `EditVoxelsRequestBackBuffer` resource. Editing the
/// `VoxelMap` directly will not work.
pub struct VoxelSystemBundle;

impl<'a, 'b> SystemBundle<'a, 'b> for VoxelSystemBundle {
    fn build(
        self,
        world: &mut World,
        dispatcher: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), amethyst::Error> {
        world.insert(VoxelBVT::new());
        world.insert(MeshMode::SurfaceNets);

        let (tx, rx) = crossbeam::channel::unbounded();
        world.insert(ChunkCacheFlusher::new(tx));
        world.insert(ChunkCacheReceiver::new(rx));
        dispatcher.add(ChunkCacheFlusherSystem, "chunk_cache_flusher", &[]);

        dispatcher.add(VoxelEditorSystemDesc.build(world), "voxel_editor", &[]);
        dispatcher.add(VoxelChunkProcessorSystem, "voxel_chunk_processor", &[]);
        dispatcher.add(
            VoxelDoubleBufferingSystem,
            "voxel_double_buffering",
            &["voxel_editor", "voxel_chunk_processor"],
        );

        Ok(())
    }
}
