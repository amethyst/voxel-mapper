pub mod loader;
pub mod manager;

use crate::{
    assets::{IndexedPosColorNormVertices, PosColorNormVertices},
    rendering::splatted_triplanar_pbr_pass::ArrayMaterialIndex,
    voxel::{decode_distance, Voxel, VoxelInfo},
};

use amethyst::core::ecs::prelude::*;
use amethyst::renderer::rendy::mesh::{Color, Normal, Position};
use ilattice3 as lat;
use ilattice3::{prelude::*, GetPaletteAddress, LatticeVoxels};
use ilattice3_mesh::{
    calculate_material_weights, greedy_quads, make_pos_norm_material_mesh_from_quads, surface_nets,
    MaterialVoxel, PosNormMaterialMesh, SignedDistanceVoxel, SurfaceNetsOutput,
    MATERIAL_WEIGHT_TABLE,
};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub enum MeshMode {
    SurfaceNets,
    GreedyQuads,
}

#[derive(Default)]
pub struct VoxelMeshEntities {
    pub chunk_entities: HashMap<lat::Point, Vec<Entity>>,
}

pub fn generate_mesh_vertices_with_surface_nets<I>(
    chunk_and_boundary: &LatticeVoxels<'_, VoxelInfo, Voxel, I>,
) -> Option<IndexedPosColorNormVertices>
where
    I: Indexer,
{
    #[cfg(feature = "profiler")]
    profile_scope!("generate_mesh_vertices");

    let SurfaceNetsOutput {
        positions,
        normals,
        indices,
        surface_points,
    } = {
        #[cfg(feature = "profiler")]
        profile_scope!("surface_nets");

        surface_nets(
            &LatticeVoxelsDistance(chunk_and_boundary),
            chunk_and_boundary.get_extent(),
        )
    };

    if indices.is_empty() {
        return None;
    }

    let vertex_material_weights = {
        #[cfg(feature = "profiler")]
        profile_scope!("material_weights");

        calculate_material_weights(&LatticeVoxelsMeshInfo(chunk_and_boundary), &surface_points)
    };

    let positions = positions.clone().into_iter().map(|p| Position(p)).collect();
    let colors = vertex_material_weights
        .into_iter()
        .map(|w| Color(w))
        .collect();
    let normals = normals.clone().into_iter().map(|n| Normal(n)).collect();
    let vertices = PosColorNormVertices {
        positions,
        colors,
        normals,
    };
    let indices: Vec<_> = indices.into_iter().map(|i| i as u32).collect();

    Some(IndexedPosColorNormVertices { vertices, indices })
}

struct LatticeVoxelsDistance<'a, I>(&'a LatticeVoxels<'a, VoxelInfo, Voxel, I>);

impl<'a, I> GetLinear for LatticeVoxelsDistance<'a, I>
where
    I: Indexer,
{
    type Data = SignedDistance;

    fn get_linear(&self, i: usize) -> Self::Data {
        let voxel = self.0.map.get_linear(i);

        SignedDistance(decode_distance(voxel.distance))
    }
}

impl<'a, I> HasIndexer for LatticeVoxelsDistance<'a, I>
where
    I: Indexer,
{
    type Indexer = I;
}

struct SignedDistance(f32);

impl SignedDistanceVoxel for SignedDistance {
    fn distance(&self) -> f32 {
        self.0
    }
}

struct LatticeVoxelsMeshInfo<'a, I>(&'a LatticeVoxels<'a, VoxelInfo, Voxel, I>);

impl<'a, I> GetLinear for LatticeVoxelsMeshInfo<'a, I>
where
    I: Indexer,
{
    type Data = VoxelMeshInfo;

    fn get_linear(&self, i: usize) -> Self::Data {
        let voxel = self.0.map.get_linear(i);
        let material_index = self.0.palette[voxel.get_palette_address()].material_index;
        let distance = decode_distance(voxel.distance);

        VoxelMeshInfo {
            distance,
            material_index,
        }
    }
}

impl<'a, I> HasIndexer for LatticeVoxelsMeshInfo<'a, I>
where
    I: Indexer,
{
    type Indexer = I;
}

impl<'a, I> GetExtent for LatticeVoxelsMeshInfo<'a, I> {
    fn get_extent(&self) -> &lat::Extent {
        self.0.get_extent()
    }
}

struct VoxelMeshInfo {
    distance: f32,
    material_index: ArrayMaterialIndex,
}

impl SignedDistanceVoxel for VoxelMeshInfo {
    fn distance(&self) -> f32 {
        self.distance
    }
}

impl MaterialVoxel for VoxelMeshInfo {
    fn material_index(&self) -> usize {
        self.material_index.0 as usize
    }
}

pub fn generate_mesh_vertices_with_greedy_quads<V, I>(
    chunk: &V,
) -> Option<IndexedPosColorNormVertices>
where
    V: GetExtent + GetWorldRef<Data = VoxelInfo> + HasIndexer<Indexer = I> + Send + Sync,
    I: Indexer + Send + Sync,
{
    #[cfg(feature = "profiler")]
    profile_scope!("generate_mesh_vertices");

    let PosNormMaterialMesh {
        positions,
        normals,
        materials,
        indices,
    } = {
        #[cfg(feature = "profiler")]
        profile_scope!("greedy_quads");

        let quads = greedy_quads(chunk, *chunk.get_extent());

        make_pos_norm_material_mesh_from_quads(&quads)
    };

    if indices.is_empty() {
        return None;
    }

    let vertex_material_weights = materials
        .into_iter()
        .map(|m: ArrayMaterialIndex| MATERIAL_WEIGHT_TABLE[m.0 as usize]);

    let positions = positions.clone().into_iter().map(|p| Position(p)).collect();
    let colors = vertex_material_weights.map(|w| Color(w)).collect();
    let normals = normals.clone().into_iter().map(|n| Normal(n)).collect();
    let vertices = PosColorNormVertices {
        positions,
        colors,
        normals,
    };
    let indices: Vec<_> = indices.into_iter().map(|i| i as u32).collect();

    Some(IndexedPosColorNormVertices { vertices, indices })
}
