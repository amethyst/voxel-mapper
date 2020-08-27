use crate::{
    assets::{read_bincode_file, write_bincode_file, BincodeFileError},
    voxel::{
        map_generators::generate_dungeon, VoxelInfo, VoxelMap, VoxelPaletteAssets, VOXEL_CHUNK_SIZE,
    },
};

use amethyst::config::Config;
use ilattice3::{ChunkedLatticeMap, ChunkedPaletteLatticeMap};
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
        Some((VoxelsFileType::Bincode, path)) => read_bincode_file(path)?,
        Some((VoxelsFileType::ProcGenDungeon, path)) => {
            // TODO: don't hardcode this
            let voxel_type_map = [0, 2];

            generate_dungeon(path, voxel_type_map).unwrap()
        }
        None => ChunkedLatticeMap::new(VOXEL_CHUNK_SIZE),
    };
    let voxels = ChunkedPaletteLatticeMap {
        map,
        palette: spec.palette_spec.infos,
    };

    Ok(VoxelMap {
        palette_assets: spec.palette_spec.assets,
        voxels,
    })
}

pub fn save_voxel_map(path: impl AsRef<Path>, map: &VoxelMap) -> Result<(), BincodeFileError> {
    write_bincode_file(path, &map.voxels.map)
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
