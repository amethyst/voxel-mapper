use crate::{
    assets::{read_bincode_file, write_bincode_file, BincodeFileError},
    voxel::{
        map_generators::generate_dungeon, VoxelInfo, VoxelMap, VoxelPaletteAssets, VOXEL_CHUNK_SIZE,
    },
};

use amethyst::config::Config;
use ilattice3::{
    chunked_lattice_map::SerializableChunkedLatticeMap,
    compressible_map::{BincodeLz4, MaybeCompressed},
    vec_lattice_map::FastLz4,
    ChunkedLatticeMap, PaletteLatticeMap,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize)]
pub struct VoxelMapFile {
    palette_spec: VoxelPaletteSpec,
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

    let map = match spec.voxels_file_path {
        Some((VoxelsFileType::Bincode, path)) => {
            let serializable_map: SerializableChunkedLatticeMap<_, _, _> = read_bincode_file(path)?;

            let map =
                ChunkedLatticeMap::from_serializable(&serializable_map, FastLz4 { level: 10 });

            let mut sum_bytes = 0;
            for (_, chunk) in map.chunks.iter_maybe_compressed() {
                match chunk {
                    MaybeCompressed::Compressed(compressed_chunk) => {
                        sum_bytes += compressed_chunk.compressed_map.compressed_bytes.len();
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
        Some((VoxelsFileType::ProcGenDungeon, path)) => {
            // TODO: don't hardcode this
            let voxel_type_map = [0, 2];

            generate_dungeon(path, voxel_type_map).unwrap()
        }
        None => ChunkedLatticeMap::new(VOXEL_CHUNK_SIZE),
    };
    let voxels = PaletteLatticeMap {
        map,
        palette: spec.palette_spec.infos,
    };

    Ok(VoxelMap {
        palette_assets: spec.palette_spec.assets,
        voxels,
    })
}

pub fn save_voxel_map(path: impl AsRef<Path>, map: &VoxelMap) -> Result<(), BincodeFileError> {
    let serializable_map = map.voxels.map.to_serializable(BincodeLz4 { level: 16 });

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

/// A full static description of the `VoxelInfo`s to be loaded for one map.
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct VoxelPaletteSpec {
    /// File locations of any voxel assets (e.g. materials).
    pub assets: VoxelPaletteAssets,
    /// The palette of voxels that can be used in the lattice. Indexed by integer that is used as
    /// the address part of the `VoxelInfoPtr`.
    pub infos: Vec<VoxelInfo>,
}
