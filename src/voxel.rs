pub mod loader;
pub mod map_file;
pub mod meshing;
pub mod setter;

use meshing::loader::VoxelMeshes;

use amethyst::{
    assets::{Handle, Prefab},
    core::math::{zero, Isometry3, Point3, Vector3},
    renderer::formats::mtl::MaterialPrefab,
};
use ilattice3 as lat;
use ilattice3::{closest_normal, ChunkedPaletteLatticeMap, GetPaletteAddress, IsEmpty};
use ilattice3_mesh::SurfaceNetsVoxel;
use ncollide3d::{bounding_volume::AABB, shape::Cuboid};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelFlags {
    /// Whether the voxel is considered for floor collisions (with the camera feet).
    pub is_floor: bool,
    /// Whether a bounding box (AABB) should be created for this voxel.
    pub is_empty: bool,
}

/// Fully describes a voxel model in a serializable format. Can be aliased by a `Voxel` for
/// instancing inside the map.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct VoxelInfo {
    pub flags: VoxelFlags,
    pub material_index: VoxelMaterialIndex,
}

impl IsEmpty for VoxelInfo {
    fn is_empty(&self) -> bool {
        self.flags.is_empty
    }
}

pub trait IsFloor {
    fn is_floor(&self) -> bool;
}

impl IsFloor for VoxelInfo {
    fn is_floor(&self) -> bool {
        self.flags.is_floor
    }
}

/// The data actually stored in each point of the voxel map.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Voxel {
    /// Points to some palette element.
    pub palette_address: u8,
    /// Quantized distance from the isosurface.
    pub distance: i8,
}

pub const EMPTY_VOXEL: Voxel = Voxel {
    palette_address: 0,
    distance: std::i8::MAX,
};

impl GetPaletteAddress for Voxel {
    fn get_palette_address(&self) -> usize {
        self.palette_address as usize
    }
}

// This is mostly just experimental. I don't have a good rationale for this value.
const SDF_QUANTIZE_FACTOR: f32 = 50.0;

pub fn encode_distance(distance: f32) -> i8 {
    (distance * SDF_QUANTIZE_FACTOR)
        .min(std::i8::MAX as f32)
        .max(std::i8::MIN as f32) as i8
}

/// The inverse of `encode_distance`.
pub fn decode_distance(encoded: i8) -> f32 {
    encoded as f32 / SDF_QUANTIZE_FACTOR
}

/// The data required for each voxel to generate a mesh.
pub struct VoxelGraphics {
    pub material_index: VoxelMaterialIndex,
    pub distance: f32,
}

impl SurfaceNetsVoxel<VoxelMaterialIndex> for VoxelGraphics {
    fn material(&self) -> VoxelMaterialIndex {
        self.material_index
    }

    fn distance(&self) -> f32 {
        self.distance
    }
}

pub struct VoxelMap {
    pub palette_assets: VoxelPaletteAssets,
    pub voxels: ChunkedPaletteLatticeMap<VoxelInfo, Voxel>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VoxelPaletteAssets {
    /// Material array prefab file identifiers.
    pub material_arrays: HashMap<usize, String>,
}

pub const VOXEL_CHUNK_SIZE: lat::Point = lat::Point {
    x: 16,
    y: 16,
    z: 16,
};

/// Identifier for one of the arrays of materials. Each mesh can only have one material array bound
/// for the draw call.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct VoxelMaterialArrayId(pub usize);

/// Index into the material array that's bound while drawing a voxel mesh. The vertex format will
/// contain a weighted vector of these indices.
#[derive(Clone, Copy, Debug, Deserialize, Hash, Eq, PartialEq, Serialize)]
pub struct VoxelMaterialIndex(pub VoxelMaterialIndexInt);

pub type VoxelMaterialIndexInt = u8;

#[derive(Default)]
pub struct VoxelAssets {
    /// Although these are just `Material`s, each `Texture` can have multiple layers for the purpose
    /// of splatting (blending between layers).
    pub material_arrays: HashMap<VoxelMaterialArrayId, Handle<Prefab<MaterialPrefab>>>,
    /// Generated at runtime, the asset handles are stored here.
    pub meshes: VoxelMeshes,
}

fn floor_float_vector_to_lattice_point(v: &Vector3<f32>) -> lat::Point {
    lat::Point::new(v.x.floor() as i32, v.y.floor() as i32, v.z.floor() as i32)
}

pub fn voxel_containing_point(p: &Point3<f32>) -> lat::Point {
    floor_float_vector_to_lattice_point(&p.coords)
}

pub struct MyPoint3(pub Point3<f32>);

impl From<lat::Point> for MyPoint3 {
    fn from(other: lat::Point) -> MyPoint3 {
        MyPoint3(<[f32; 3]>::from(other).into())
    }
}

/// Returns the AABB with corners (min, max + [1, 1, 1]).
pub fn extent_aabb(e: &lat::Extent) -> AABB<f32> {
    let MyPoint3(mins) = e.get_minimum().into();
    let MyPoint3(maxs) = (*e.get_world_supremum()).into();

    AABB::new(mins, maxs)
}

pub fn single_voxel_extent(point: lat::Point) -> lat::Extent {
    lat::Extent::from_min_and_local_supremum(point, [1, 1, 1].into())
}

pub fn voxel_aabb(p: &lat::Point) -> AABB<f32> {
    extent_aabb(&single_voxel_extent(*p))
}

pub fn voxel_center_offset() -> Vector3<f32> {
    Vector3::new(0.5, 0.5, 0.5)
}

pub fn voxel_center(p: &lat::Point) -> Point3<f32> {
    let MyPoint3(fpoint) = (*p).into();

    fpoint + voxel_center_offset()
}

fn half_extent(e: &lat::Extent) -> Vector3<f32> {
    let MyPoint3(sup) = (*e.get_local_supremum()).into();

    sup.coords / 2.0
}

pub fn extent_cuboid(e: &lat::Extent) -> Cuboid<f32> {
    Cuboid::new(half_extent(e))
}

pub fn extent_cuboid_transform(e: &lat::Extent) -> Isometry3<f32> {
    let MyPoint3(min) = e.get_minimum().into();
    let center = min.coords + half_extent(e);

    Isometry3::new(center, zero())
}

pub fn voxel_cuboid(p: &lat::Point) -> Cuboid<f32> {
    extent_cuboid(&single_voxel_extent(*p))
}

pub fn voxel_transform(p: &lat::Point) -> Isometry3<f32> {
    extent_cuboid_transform(&single_voxel_extent(*p))
}

/// Returns the normal vector of the face which `real_p` is closest to. `voxel_p` is the point of
/// the voxel.
pub fn closest_face(voxel_p: &lat::Point, real_p: &Point3<f32>) -> lat::Point {
    // Get a vector from the center of the voxel.
    let c = voxel_center(voxel_p);
    let real_v: [f32; 3] = (*real_p - c).into();

    closest_normal(&real_v)
}
