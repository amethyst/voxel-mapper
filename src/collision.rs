use crate::voxel::{Voxel, VoxelMap};

pub mod floor_translation;

use building_blocks::{
    partition::{Octree, OctreeDBVT},
    prelude::*,
};

pub type VoxelBVT = OctreeDBVT<Point3i>;

pub fn insert_all_chunk_bvts(
    bvt: &mut VoxelBVT,
    voxel_map: &VoxelMap,
    chunk_cache: &LocalChunkCache3<Voxel>,
) {
    for chunk_key in voxel_map.voxels.chunk_keys() {
        let chunk = voxel_map.voxels.get_chunk(*chunk_key, chunk_cache).unwrap();
        let chunk_infos = TransformMap::new(&chunk.array, voxel_map.voxel_info_transform());
        let octree = Octree::from_array3(&chunk_infos, *chunk_infos.extent());
        if octree.is_empty() {
            bvt.remove(chunk_key);
        } else {
            bvt.insert(*chunk_key, octree);
        }
    }
}
