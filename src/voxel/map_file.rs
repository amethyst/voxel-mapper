use crate::{
    assets::{read_bincode_file, write_bincode_file, BincodeFileError},
    voxel::{VoxelMap, VoxelPalette, EMPTY_VOXEL, VOXEL_CHUNK_SHAPE},
};

use amethyst::config::Config;
use building_blocks::{
    prelude::*,
    storage::{
        chunk_map::SerializableChunkMap3,
        compressible_map::{BincodeLz4, MaybeCompressed},
    },
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize)]
pub struct VoxelMapFile {
    palette: VoxelPalette,
    voxels_file_path: Option<(VoxelsFileType, String)>,
}

#[derive(Deserialize, Serialize)]
pub enum VoxelsFileType {
    Bincode,
    ProcGenDungeon,
}

pub fn load_voxel_map(path: impl AsRef<Path>) -> Result<VoxelMap, BincodeFileError> {
    // TODO: gosh I guess we should have another error type
    let spec: VoxelMapFile = Config::load(path).unwrap();

    let voxels = match spec.voxels_file_path {
        Some((VoxelsFileType::Bincode, path)) => {
            let serializable_map: SerializableChunkMap3<_> = read_bincode_file(path)?;

            let map = futures::executor::block_on(ChunkMap3::from_serializable(
                &serializable_map,
                FastLz4 { level: 10 },
            ));

            let mut sum_bytes = 0;
            for (_, chunk) in map.chunks.iter_maybe_compressed() {
                match chunk {
                    MaybeCompressed::Compressed(compressed_chunk) => {
                        sum_bytes += compressed_chunk.compressed_array.compressed_bytes.len();
                    }
                    _ => (),
                }
            }
            let num_chunks = serializable_map.compressed_chunks.len();
            log::debug!(
                "# chunks = {}; avg compressed size = {} bytes",
                num_chunks,
                sum_bytes / num_chunks
            );

            map
        }
        // TODO: return support for ProcGenDungeon map type; this was removed temporarily while
        // porting from ilattice3 to building-blocks, because ilattice3-procgen will take some more
        // effort to port
        // Some((VoxelsFileType::ProcGenDungeon, path)) => {
        //     // TODO: don't hardcode this
        //     let voxel_type_map = [0, 2];

        //     generate_dungeon(path, voxel_type_map).unwrap()
        // }
        _ => {
            let ambient_value = EMPTY_VOXEL;

            ChunkMap3::new(VOXEL_CHUNK_SHAPE, ambient_value, (), FastLz4 { level: 10 })
        }
    };

    Ok(VoxelMap {
        palette: spec.palette,
        voxels,
    })
}

pub fn save_voxel_map(path: impl AsRef<Path>, map: &VoxelMap) -> Result<(), BincodeFileError> {
    let serializable_map =
        futures::executor::block_on(map.voxels.to_serializable(BincodeLz4 { level: 16 }));

    let mut sum_bytes = 0;
    for chunk in serializable_map.compressed_chunks.values() {
        sum_bytes += chunk.compressed_bytes.len();
    }
    let num_chunks = serializable_map.compressed_chunks.len();
    log::debug!(
        "# chunks = {}; avg compressed size = {} bytes",
        num_chunks,
        sum_bytes / num_chunks
    );

    write_bincode_file(path, &serializable_map)
}
