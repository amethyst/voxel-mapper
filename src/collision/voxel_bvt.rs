use crate::voxel::{voxel_aabb, LocalVoxelChunkCache, Voxel, VoxelInfo};

use fnv::FnvHashMap;
use ilattice3 as lat;
use ilattice3::{algos::find_surface_voxels, prelude::*, Extent, PaletteLatticeMap};
use ncollide3d::{
    bounding_volume::AABB,
    partitioning::{DBVTLeaf, DBVTLeafId, DBVTNodeId, BVH, DBVT},
};

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// Acceleration structure for spatial queries with voxels.
///
/// The hypothesis is that it's cheapest to remove and regenerate the BVT for an entire chunk at a
/// time, since there can be many streaming voxel updates, and we can combine those into a single
/// chunk update, where we only have to find surface voxels, which is already how the meshes are
/// updated.
///
/// To that end, this is a BVH with two layers of DBVT:
///
///          DBVT of chunks
///          /    ...    \
///    chunk id       chunk id   ---> hash map -->   DBVT of voxels in chunk
///                                                 /         ...          \
///                                              voxel                    voxel
///
/// The "top BVT" leaf nodes should never be used externally, as they will be forwarded to the root
/// of a "chunk BVT."
pub struct VoxelBVT {
    top_bvt: LatticeBVT,
    chunk_bvts: FnvHashMap<lat::Point, (DBVTLeafId, LatticeBVT)>,
}

// It's faster to insert all points into a DBVT than to construct a new BVT, since the BVT will try
// to balance the tree.
pub type LatticeBVT = DBVT<f32, lat::Point, AABB<f32>>;

#[derive(Clone, Copy, Debug)]
pub enum VoxelBVTNode {
    Top(DBVTNodeId),
    Chunk(lat::Point, DBVTNodeId),
}

impl BVH<lat::Point, AABB<f32>> for VoxelBVT {
    type Node = VoxelBVTNode;

    fn root(&self) -> Option<Self::Node> {
        self.top_bvt.root().map(|r| VoxelBVTNode::Top(r))
    }

    fn child(&self, child_num: usize, node: Self::Node) -> Self::Node {
        match node {
            VoxelBVTNode::Top(DBVTNodeId::Internal(node_num)) => VoxelBVTNode::Top(
                self.top_bvt
                    .child(child_num, DBVTNodeId::Internal(node_num)),
            ),
            VoxelBVTNode::Top(DBVTNodeId::Leaf(node_num)) => {
                let (chunk_key, chunk_bvt) = self.forward_top_leaf(node_num);

                VoxelBVTNode::Chunk(
                    chunk_key,
                    chunk_bvt.child(child_num, chunk_bvt.root().unwrap()),
                )
            }
            VoxelBVTNode::Chunk(chunk_key, chunk_bvt_id) => VoxelBVTNode::Chunk(
                chunk_key,
                self.chunk_bvts[&chunk_key].1.child(child_num, chunk_bvt_id),
            ),
        }
    }

    fn num_children(&self, node: Self::Node) -> usize {
        match node {
            VoxelBVTNode::Top(DBVTNodeId::Internal(node_num)) => {
                self.top_bvt.num_children(DBVTNodeId::Internal(node_num))
            }
            VoxelBVTNode::Top(DBVTNodeId::Leaf(node_num)) => {
                let (_, chunk_bvt) = self.forward_top_leaf(node_num);

                chunk_bvt.num_children(chunk_bvt.root().unwrap())
            }
            VoxelBVTNode::Chunk(chunk_key, chunk_bvt_id) => {
                self.chunk_bvts[&chunk_key].1.num_children(chunk_bvt_id)
            }
        }
    }

    fn content(&self, node: Self::Node) -> (&AABB<f32>, Option<&lat::Point>) {
        match node {
            VoxelBVTNode::Top(DBVTNodeId::Internal(node_num)) => {
                self.top_bvt.content(DBVTNodeId::Internal(node_num))
            }
            VoxelBVTNode::Top(DBVTNodeId::Leaf(node_num)) => {
                let (_, chunk_bvt) = self.forward_top_leaf(node_num);

                chunk_bvt.content(chunk_bvt.root().unwrap())
            }
            VoxelBVTNode::Chunk(chunk_key, chunk_bvt_id) => {
                self.chunk_bvts[&chunk_key].1.content(chunk_bvt_id)
            }
        }
    }
}

impl VoxelBVT {
    pub fn new() -> Self {
        VoxelBVT {
            top_bvt: DBVT::new(),
            chunk_bvts: FnvHashMap::default(),
        }
    }

    pub fn insert_chunk(&mut self, chunk_key: lat::Point, new_bvt: LatticeBVT) {
        let bv = new_bvt.root_bounding_volume().unwrap().clone();
        let top_leaf_id = self.top_bvt.insert(DBVTLeaf::new(bv, chunk_key));
        if let Some((old_chunk_leaf, _)) = self.chunk_bvts.insert(chunk_key, (top_leaf_id, new_bvt))
        {
            self.top_bvt.remove(old_chunk_leaf);
        }
    }

    pub fn remove_chunk(&mut self, chunk_key: &lat::Point) -> Option<LatticeBVT> {
        self.chunk_bvts
            .remove(chunk_key)
            .map(|(leaf_id, chunk_bvt)| {
                self.top_bvt.remove(leaf_id);

                chunk_bvt
            })
    }

    // Go straight from the top-layer leaf node to a bottom-layer root node.
    fn forward_top_leaf(&self, leaf_node_num: usize) -> (lat::Point, &LatticeBVT) {
        let (_, chunk_key) = self.top_bvt.content(DBVTNodeId::Leaf(leaf_node_num));
        let chunk_key = chunk_key.unwrap();

        (*chunk_key, &self.chunk_bvts[chunk_key].1)
    }
}

pub fn insert_all_chunk_bvts(
    bvt: &mut VoxelBVT,
    map: &PaletteLatticeMap<VoxelInfo, Voxel>,
    chunk_cache: &LocalVoxelChunkCache,
) {
    for (key, chunk) in map.iter_chunks_with_boundary(chunk_cache) {
        if let Some(new_bvt) = generate_chunk_bvt(&chunk, chunk.get_extent()) {
            bvt.insert_chunk(*key, new_bvt);
        } else {
            bvt.remove_chunk(key);
        }
    }
}

pub fn generate_chunk_bvt<V, T, I>(voxels: &V, extent: &Extent) -> Option<LatticeBVT>
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
