use crate::voxel::{voxel_aabb, Voxel, VoxelInfo};

use ilattice3 as lat;
use ilattice3::{algos::find_surface_voxels, prelude::*, ChunkedPaletteLatticeMap, Extent};
use ncollide3d::{
    bounding_volume::{BoundingVolume, AABB},
    partitioning::{DBVTLeaf, DBVTNodeId, BVH, DBVT},
};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// Acceleration structure for spatial queries with voxels.
///
/// The hypothesis is that it's cheapest to remove and regenerate the BVT for an entire chunk at a
/// time, since there can be many streaming voxel updates, and we can combine those into a single
/// chunk update, where we only have to find surface voxels, which is already how the meshes are
/// updated.
///
/// To that end, this structure is basically just a hash map from chunk ID to BVT, and it keeps the
/// root AABB up to date. You can treat it like a regular BVH though.
pub struct VoxelBVT {
    root_aabb: Option<AABB<f32>>,
    root_children_keys: Vec<[i32; 3]>, // sorted, unique
    chunk_bvts: HashMap<lat::Point, ChunkBVT>,
}

// For some reason, it's faster to insert all point into a DBVT than to construct a new BVT.
pub type ChunkBVT = DBVT<f32, lat::Point, AABB<f32>>;

#[derive(Clone, Copy, Debug)]
pub enum VoxelBVTNode {
    Root,
    Chunk(lat::Point, DBVTNodeId),
}

impl BVH<lat::Point, AABB<f32>> for VoxelBVT {
    type Node = VoxelBVTNode;

    fn root(&self) -> Option<Self::Node> {
        self.root_aabb.as_ref().map(|_| VoxelBVTNode::Root)
    }

    fn child(&self, i: usize, node: Self::Node) -> Self::Node {
        match node {
            VoxelBVTNode::Root => {
                let chunk_key: lat::Point = self.root_children_keys[i].into();

                VoxelBVTNode::Chunk(chunk_key, self.chunk_bvts[&chunk_key].root().unwrap())
            }
            VoxelBVTNode::Chunk(chunk_key, bvt_id) => {
                VoxelBVTNode::Chunk(chunk_key, self.chunk_bvts[&chunk_key].child(i, bvt_id))
            }
        }
    }

    fn num_children(&self, node: Self::Node) -> usize {
        match node {
            VoxelBVTNode::Root => self.root_children_keys.len(),
            VoxelBVTNode::Chunk(chunk_key, bvt_id) => {
                self.chunk_bvts[&chunk_key].num_children(bvt_id)
            }
        }
    }

    fn content(&self, node: Self::Node) -> (&AABB<f32>, Option<&lat::Point>) {
        match node {
            VoxelBVTNode::Root => (self.root_aabb.as_ref().unwrap(), None),
            VoxelBVTNode::Chunk(chunk_key, bvt_id) => self.chunk_bvts[&chunk_key].content(bvt_id),
        }
    }
}

impl VoxelBVT {
    pub fn new() -> Self {
        VoxelBVT {
            root_aabb: None,
            root_children_keys: Vec::new(),
            chunk_bvts: HashMap::new(),
        }
    }

    pub fn insert_chunk(&mut self, chunk_key: lat::Point, new_bvt: ChunkBVT) {
        // Grow the root AABB.
        let new_bvt_root_aabb = new_bvt.root_bounding_volume().unwrap();
        self.root_aabb = self
            .root_aabb
            .as_ref()
            .map(|bv| bv.merged(new_bvt_root_aabb))
            .or(Some(new_bvt_root_aabb.clone()));

        self.chunk_bvts.insert(chunk_key, new_bvt);
        if let Err(insert_pos) = self.root_children_keys.binary_search(&chunk_key.into()) {
            self.root_children_keys.insert(insert_pos, chunk_key.into());
        }
    }

    pub fn remove_chunk(&mut self, chunk_key: &lat::Point) -> Option<ChunkBVT> {
        if let Some(removed) = self.chunk_bvts.remove(chunk_key) {
            if let Ok(remove_pos) = self.root_children_keys.binary_search(&(*chunk_key).into()) {
                self.root_children_keys.remove(remove_pos);
            }

            // Re-merge the remaining chunk AABBs to shrink the root AABB.
            self.root_aabb = self.merge_all_chunk_aabbs();

            Some(removed)
        } else {
            None
        }
    }

    fn merge_all_chunk_aabbs(&self) -> Option<AABB<f32>> {
        let mut merged: Option<AABB<f32>> = None;
        for bvt in self.chunk_bvts.values() {
            let bvt_aabb = bvt.root_bounding_volume().unwrap();
            merged = merged
                .map(|m| m.merged(bvt_aabb))
                .or(Some(bvt_aabb.clone()));
        }

        merged
    }
}

pub fn insert_all_chunk_bvts(bvt: &mut VoxelBVT, map: &ChunkedPaletteLatticeMap<VoxelInfo, Voxel>) {
    for (key, chunk) in map.iter_chunks_with_boundary() {
        if let Some(new_bvt) = generate_chunk_bvt(&chunk, chunk.get_extent()) {
            bvt.insert_chunk(*key, new_bvt);
        } else {
            bvt.remove_chunk(key);
        }
    }
}

pub fn generate_chunk_bvt<V, T, I>(voxels: &V, extent: &Extent) -> Option<ChunkBVT>
where
    V: GetLinearRef<Data = T> + HasIndexer<Indexer = I>,
    T: IsEmpty,
    I: Indexer,
{
    #[cfg(feature = "profiler")]
    profile_scope!("generate_chunk_bvt");

    // This is subtly different from the set of surface points returned from the surface nets
    // meshing algorithm, since we use the IsEmpty check instead of the signed distance. This allows
    // us to have parts of the mesh that don't collide.
    let solid_points: Vec<_> = find_surface_voxels(voxels, extent);

    if solid_points.is_empty() {
        None
    } else {
        let mut new_bvt = DBVT::new();
        for p in solid_points.iter() {
            new_bvt.insert(DBVTLeaf::new(voxel_aabb(&p), *p));
        }

        Some(new_bvt)
    }
}
