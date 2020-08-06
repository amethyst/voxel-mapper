pub mod loader;
pub mod manager;

use crate::{
    assets::{IndexedPosColorNormVertices, PosColorNormVertices},
    voxel::{decode_distance, Voxel, VoxelGraphics, VoxelInfo},
};

use amethyst::core::ecs::prelude::*;
use amethyst::renderer::rendy::mesh::{Color, Normal, Position};
use ilattice3 as lat;
use ilattice3::{prelude::*, GetPaletteAddress, HasIndexer, Indexer, LatticeVoxels, CUBE_CORNERS};
use ilattice3_mesh::{surface_nets, SurfaceNetsOutput};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

#[derive(Default)]
pub struct VoxelMeshEntities {
    pub chunk_entities: HashMap<lat::Point, Vec<Entity>>,
}

pub fn generate_mesh_vertices<I>(
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
            &LatticeVoxelsForMeshing(chunk_and_boundary),
            chunk_and_boundary.get_extent(),
        )
    };

    if indices.is_empty() {
        return None;
    }

    let vertex_material_weights = calculate_material_weights(
        &LatticeVoxelsForMeshing(chunk_and_boundary),
        &surface_points,
    );

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

pub struct LatticeVoxelsForMeshing<'a, I>(&'a LatticeVoxels<'a, VoxelInfo, Voxel, I>);

impl<'a, I> GetLinear for LatticeVoxelsForMeshing<'a, I>
where
    I: Indexer,
{
    type Data = VoxelGraphics;

    fn get_linear(&self, i: usize) -> Self::Data {
        let voxel = self.0.map.get_linear(i);

        VoxelGraphics {
            material_index: self.0.palette[voxel.get_palette_address()].material_index,
            distance: decode_distance(voxel.distance),
        }
    }
}

impl<'a, I> GetExtent for LatticeVoxelsForMeshing<'a, I> {
    fn get_extent(&self) -> &lat::Extent {
        self.0.get_extent()
    }
}

impl<'a, I> HasIndexer for LatticeVoxelsForMeshing<'a, I>
where
    I: Indexer,
{
    type Indexer = I;
}

/// Uses a kernel to average the adjacent materials for each surface point.
fn calculate_material_weights<V, I>(voxels: &V, surface_points: &[lat::Point]) -> Vec<[f32; 4]>
where
    V: GetExtent + GetLinear<Data = VoxelGraphics> + HasIndexer<Indexer = I>,
    I: Indexer,
{
    #[cfg(feature = "profiler")]
    profile_scope!("material_weights");

    // The current vertex format is limited to 4 numbers for material weights.
    const WEIGHT_TABLE: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let sup = voxels.get_extent().get_local_supremum();

    // Precompute the offsets for cube corners, like we do in surface nets.
    let mut linear_offsets = [0; 8];
    for (i, offset) in CUBE_CORNERS.iter().enumerate() {
        linear_offsets[i] = I::index_from_local_point(sup, offset);
    }

    let mut material_weights = vec![[0.0; 4]; surface_points.len()];

    for (i, p) in surface_points.iter().enumerate() {
        let p_linear = I::index_from_local_point(sup, p);
        let w = &mut material_weights[i];
        for offset in linear_offsets.iter() {
            let q_linear = p_linear + offset;
            let voxel = voxels.get_linear(q_linear);
            if voxel.distance < 0.0 {
                let material_w = WEIGHT_TABLE[voxel.material_index.0 as usize];
                w[0] += material_w[0];
                w[1] += material_w[1];
                w[2] += material_w[2];
                w[3] += material_w[3];
            }
        }
    }

    material_weights
}
