use super::{encode_distance, Voxel, EMPTY_VOXEL, VOXEL_CHUNK_SIZE};

use amethyst::config::Config;
use ilattice3::{prelude::*, ChunkedLatticeMap};
use ilattice3_procgen::{
    map_types::dungeon::DungeonMapSpec, sampling::small_rng, Voxel as ProcVoxel, VoxelEncoder,
};
use std::path::Path;

/// `voxel_type_map` is used to convert from the dungeon voxel type indices to the corresponding
/// palette addresses.
pub fn generate_dungeon<P: AsRef<Path>>(
    path: P,
    voxel_type_map: [u8; 2],
) -> amethyst::Result<ChunkedLatticeMap<Voxel>> {
    let spec = DungeonMapSpec::load(path)?;

    let mut map = ChunkedLatticeMap::new(VOXEL_CHUNK_SIZE);
    let mut encoder = DungeonEncoder::new(&mut map, voxel_type_map);

    let mut rng = small_rng(spec.seed);
    spec.generate(&mut rng, &mut encoder);

    Ok(map)
}

pub struct DungeonEncoder<'a> {
    map: &'a mut ChunkedLatticeMap<Voxel>,
    voxel_type_map: [u8; 2],
}

impl<'a> DungeonEncoder<'a> {
    pub fn new(map: &'a mut ChunkedLatticeMap<Voxel>, voxel_type_map: [u8; 2]) -> Self {
        DungeonEncoder {
            map,
            voxel_type_map,
        }
    }
}

impl VoxelEncoder for DungeonEncoder<'_> {
    fn encode_voxel(&mut self, point: &Point, data: &ProcVoxel) {
        let (_, voxel) = self.map.get_mut_or_default(point, (), EMPTY_VOXEL);
        voxel.distance = encode_distance(data.distance);
        voxel.palette_address = self.voxel_type_map[data.voxel_type as usize];
    }
}
