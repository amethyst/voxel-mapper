use crate::rendering::splatted_triplanar_pbr_pass::{ArrayMaterialId, ArrayMaterialIndex};

pub mod asset_loader;
pub mod bundle;
pub mod chunk_cache_compressor;
pub mod chunk_cache_flusher;
pub mod chunk_processor;
pub mod double_buffer;
pub mod map_file;
//pub mod map_generators;
pub mod meshing;
pub mod search;

use meshing::loader::VoxelMeshes;

use amethyst::{
    assets::{Handle, Prefab},
    renderer::formats::mtl::MaterialPrefab,
};
use building_blocks::{
    core::bytemuck::{Pod, Zeroable},
    mesh::{IsOpaque, MergeVoxel},
    prelude::*,
};
use nalgebra as na;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The global source of truth for voxels in the current map.
pub struct VoxelMap {
    pub voxels: VoxelChunkMap,
    pub palette: VoxelPalette,
}

impl VoxelMap {
    pub fn new(palette: VoxelPalette) -> Self {
        Self {
            voxels: empty_compressible_chunk_map(),
            palette,
        }
    }

    pub fn voxel_info_transform<'a>(&'a self) -> impl Fn(Voxel) -> &'a VoxelInfo {
        move |v: Voxel| self.palette.get_voxel_type_info(v.voxel_type)
    }
}

/// The data actually stored in each point of the voxel map.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Voxel {
    pub voxel_type: VoxelType,
    pub distance: Sd8,
}

unsafe impl Zeroable for Voxel {}
unsafe impl Pod for Voxel {}

/// Points to some palette element.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelType(pub u8);

impl SignedDistance for Voxel {
    fn is_negative(&self) -> bool {
        self.distance.0 < 0
    }
}

impl From<Voxel> for f32 {
    fn from(v: Voxel) -> f32 {
        v.distance.into()
    }
}

pub const EMPTY_VOXEL: Voxel = Voxel {
    voxel_type: VoxelType(0),
    distance: Sd8(50),
};

/// A full static description of the `VoxelInfo`s to be loaded for one map.
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct VoxelPalette {
    /// File locations of any voxel assets (e.g. materials).
    pub assets: VoxelPaletteAssets,
    /// The palette of voxels that can be used in the lattice. Indexed by integer that is used as
    /// the address part of the `VoxelInfoPtr`.
    pub infos: Vec<VoxelInfo>,
}

impl VoxelPalette {
    pub fn get_voxel_type_info(&self, voxel_type: VoxelType) -> &VoxelInfo {
        &self.infos[voxel_type.0 as usize]
    }
}

/// Fully describes a voxel model in a serializable format. Can be aliased by a `Voxel` for
/// instancing inside the map.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelInfo {
    pub flags: VoxelFlags,
    pub material_index: ArrayMaterialIndex,
}

impl IsEmpty for &VoxelInfo {
    fn is_empty(&self) -> bool {
        self.flags.is_empty
    }
}

impl IsFloor for &VoxelInfo {
    fn is_floor(&self) -> bool {
        self.flags.is_floor
    }
}

impl MergeVoxel for &VoxelInfo {
    type VoxelValue = ArrayMaterialIndex;

    fn voxel_merge_value(&self) -> Self::VoxelValue {
        self.material_index
    }
}

impl IsOpaque for &VoxelInfo {
    fn is_opaque(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelFlags {
    /// Whether the voxel is considered for floor collisions (with the camera feet).
    pub is_floor: bool,
    /// Whether a bounding box (AABB) should be created for this voxel.
    pub is_empty: bool,
}

pub trait IsFloor {
    fn is_floor(&self) -> bool;
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VoxelPaletteAssets {
    /// Array material prefab file identifiers.
    pub array_materials: HashMap<usize, String>,
}

pub const VOXEL_CHUNK_SHAPE: Point3i = PointN([16; 3]);

#[derive(Default)]
pub struct VoxelAssets {
    /// Although these are just `Material`s, each `Texture` can have multiple layers for the purpose
    /// of splatting (blending between layers).
    pub array_materials: HashMap<ArrayMaterialId, Handle<Prefab<MaterialPrefab>>>,
    /// Generated at runtime, the asset handles are stored here.
    pub meshes: VoxelMeshes,
}

pub fn voxel_center_offset() -> na::Vector3<f32> {
    na::Vector3::new(0.5, 0.5, 0.5)
}

pub fn voxel_center(p: Point3i) -> na::Point3<f32> {
    na::Point3::<f32>::from(mint::Point3::<f32>::from(Point3f::from(p))) + voxel_center_offset()
}

pub fn voxel_containing_point(p: na::Point3<f32>) -> Point3i {
    let p: mint::Point3<f32> = p.into();

    Point3f::from(p).in_voxel()
}

pub fn centered_extent(center: Point3i, radius: u32) -> Extent3i {
    let r = radius as i32;
    let min = center - PointN([r; 3]);
    let shape = PointN([2 * r + 1; 3]);

    Extent3i::from_min_and_shape(min, shape)
}

pub fn empty_compressible_chunk_map() -> VoxelChunkMap {
    let builder = ChunkMapBuilder3x1::new(VOXEL_CHUNK_SHAPE, EMPTY_VOXEL);

    builder.build_with_write_storage(FastCompressibleChunkStorageNx1::with_bytes_compression(
        Lz4 { level: 10 },
    ))
}

pub fn empty_chunk_hash_map() -> VoxelChunkHashMap {
    ChunkMapBuilder3x1::new(VOXEL_CHUNK_SHAPE, EMPTY_VOXEL).build_with_hash_map_storage()
}

pub fn empty_array(extent: Extent3i) -> Array3x1<Voxel> {
    Array3x1::fill(extent, EMPTY_VOXEL)
}

pub type VoxelChunkMap = CompressibleChunkMap3x1<Lz4, Voxel>;
pub type VoxelChunkHashMap = ChunkHashMap3x1<Voxel>;

pub type LocalVoxelCache = LocalChunkCache3<Array3x1<Voxel>>;
pub type VoxelChunkReader<'a> = CompressibleChunkMapReader3x1<'a, Lz4, Voxel>;
