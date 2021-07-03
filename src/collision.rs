use crate::voxel::{LocalVoxelCache, VoxelMap};

pub mod floor_translation;

use building_blocks::{prelude::*, search::OctreeDbvt, storage::OctreeSet};

pub type VoxelBVT = OctreeDbvt<Point3i>;

pub fn insert_all_chunk_bvts(
    bvt: &mut VoxelBVT,
    voxel_map: &VoxelMap,
    chunk_cache: &LocalVoxelCache,
) {
    let reader = voxel_map.voxels.reader(chunk_cache);
    for &chunk_key in voxel_map.voxels.storage().chunk_keys() {
        let chunk = reader.get_chunk(chunk_key).unwrap();
        let chunk_infos = TransformMap::new(chunk, voxel_map.voxel_info_transform());
        let octree = OctreeSet::from_array3(&chunk_infos, *chunk_infos.extent());
        if octree.is_empty() {
            bvt.remove(&chunk_key.minimum);
        } else {
            bvt.insert(chunk_key.minimum, octree);
        }
    }
}
