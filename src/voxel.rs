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
    core::math as na,
    renderer::formats::mtl::MaterialPrefab,
};
use building_blocks::{
    mesh::{MaterialVoxel, SignedDistance},
    partition::ncollide3d as nc_new,
    prelude::*,
};
use ncollide3d as nc_old;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The global source of truth for voxels in the current map.
pub struct VoxelMap {
    pub voxels: ChunkMap3<Voxel>,
    pub palette: VoxelPalette,
}

impl VoxelMap {
    pub fn voxel_info_transform<'a>(&'a self) -> impl Fn(Voxel) -> &'a VoxelInfo {
        move |v: Voxel| self.palette.get_voxel_type_info(v.voxel_type)
    }
}

/// The data actually stored in each point of the voxel map.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Voxel {
    pub voxel_type: VoxelType,
    pub distance: VoxelDistance,
}

/// Points to some palette element.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelType(pub u8);

/// Quantized distance from an isosurface.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelDistance(pub i8);

impl VoxelDistance {
    // This is mostly just experimental. I don't have a good rationale for this value.
    const SDF_QUANTIZE_FACTOR: f32 = 50.0;

    pub fn encode(distance: f32) -> Self {
        Self(
            (distance * Self::SDF_QUANTIZE_FACTOR)
                .min(std::i8::MAX as f32)
                .max(std::i8::MIN as f32) as i8,
        )
    }

    /// The inverse of `encode`.
    pub fn decode(self: Self) -> f32 {
        self.0 as f32 / Self::SDF_QUANTIZE_FACTOR
    }
}

impl SignedDistance for Voxel {
    fn distance(&self) -> f32 {
        self.distance.decode()
    }
}

pub const EMPTY_VOXEL: Voxel = Voxel {
    voxel_type: VoxelType(0),
    distance: VoxelDistance(50),
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

impl MaterialVoxel for &VoxelInfo {
    type Material = ArrayMaterialIndex;

    fn material(&self) -> Self::Material {
        self.material_index
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
    // TODO: amethyst is using an older version of nalgebra than building-blocks, so we can't do the
    // simplest conversion
    na::Point3::<f32>::from(Point3f::from(p).0) + voxel_center_offset()
}

pub fn voxel_containing_point(p: &na::Point3<f32>) -> Point3i {
    // TODO: amethyst is using an older version of nalgebra than building-blocks, so we can't do the
    // simplest conversion
    PointN([p.x, p.y, p.z]).in_voxel()
}

pub fn centered_extent(center: Point3i, radius: u32) -> Extent3i {
    let r = radius as i32;
    let min = center - PointN([r; 3]);
    let shape = PointN([2 * r + 1; 3]);

    Extent3i::from_min_and_shape(min, shape)
}

// TODO: amethyst is using an older version of nalgebra than building-blocks, so we need to upgrade
// the old ncollide types to new ones when using them with building-blocks

pub fn upgrade_ray(old_ray: nc_old::query::Ray<f32>) -> nc_new::query::Ray<f32> {
    nc_new::query::Ray::new(upgrade_point(old_ray.origin), upgrade_vector(old_ray.dir))
}

pub fn upgrade_point(old_p: na::Point3<f32>) -> nc_new::na::Point3<f32> {
    nc_new::na::Point3::<f32>::new(old_p.x, old_p.y, old_p.z)
}

pub fn upgrade_vector(old_v: na::Vector3<f32>) -> nc_new::na::Vector3<f32> {
    nc_new::na::Vector3::<f32>::new(old_v.x, old_v.y, old_v.z)
}

pub fn empty_chunk_map() -> ChunkMap3<Voxel> {
    let ambient_value = EMPTY_VOXEL;

    ChunkMap3::new(VOXEL_CHUNK_SHAPE, ambient_value, (), FastLz4 { level: 10 })
}

pub fn empty_array(extent: Extent3i) -> Array3<Voxel> {
    Array3::fill(extent, EMPTY_VOXEL)
}
