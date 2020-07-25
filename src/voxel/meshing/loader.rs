use crate::{
    assets::{BoundedMesh, IndexedPosColorNormVertices, MeshLoader, PosColorNormVertices},
    voxel::{decode_distance, Voxel, VoxelGraphics, VoxelInfo, VoxelMaterial},
};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    renderer::rendy::mesh::{Color, Normal, Position},
};
use ilattice3 as lat;
use ilattice3::{
    prelude::*, ChunkedPaletteLatticeMap, GetPaletteAddress, HasIndexer, Indexer, LatticeVoxels,
    CUBE_CORNERS,
};
use ilattice3_mesh::{surface_nets, SurfaceNetsOutput};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

/// Generates vertices for voxel meshes and loads them into `BoundedMesh` objects.
#[derive(SystemData)]
pub struct VoxelMeshLoader<'a> {
    mesh_loader: MeshLoader<'a>,
}

pub struct ChunkMeshes {
    pub meshes: Vec<(VoxelMaterial, BoundedMesh)>,
}

#[derive(Default)]
pub struct VoxelMeshes {
    pub chunk_meshes: HashMap<lat::Point, ChunkMeshes>,
}

impl<'a> VoxelMeshLoader<'a> {
    pub fn start_loading(
        &mut self,
        voxels: &ChunkedPaletteLatticeMap<VoxelInfo, Voxel>,
        progress: &mut ProgressCounter,
    ) -> VoxelMeshes {
        let chunk_meshes = voxels
            .iter_chunks_with_boundary()
            .map(|(chunk_key, chunk_and_boundary)| {
                (
                    *chunk_key,
                    self.start_loading_chunk(&chunk_and_boundary, progress),
                )
            })
            .collect();

        VoxelMeshes { chunk_meshes }
    }

    /// Generates vertices for the lattice chunk identified by `chunk_key` and starts loading
    /// them into `BoundedMesh` objects, which will be ready for use when `progress` is complete.
    pub fn start_loading_chunk<I>(
        &self,
        chunk_and_boundary: &LatticeVoxels<'_, VoxelInfo, Voxel, I>,
        progress: &mut ProgressCounter,
    ) -> ChunkMeshes
    where
        I: Indexer,
    {
        let SurfaceNetsOutput {
            positions,
            normals,
            indices_by_material,
            surface_points,
        } = {
            #[cfg(feature = "profiler")]
            profile_scope!("surface_nets");

            surface_nets(
                &LatticeVoxelsForMeshing(chunk_and_boundary),
                chunk_and_boundary.get_extent(),
            )
        };

        let vertex_material_weights = calculate_material_weights(
            &LatticeVoxelsForMeshing(chunk_and_boundary),
            &surface_points,
        );

        let meshes = indices_by_material
            .into_iter()
            .map(|(material, indices)| {
                // TODO: eventually it would be nice to have enough control so that the same vertex
                // buffer could be used with multiple index buffers
                //
                // Just use the same vertices for each mesh. Not memory efficient, but significantly
                // faster than trying to split the mesh.
                let positions = positions.clone().into_iter().map(|p| Position(p)).collect();
                let colors = vertex_material_weights
                    .clone()
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
                let ivs = IndexedPosColorNormVertices { vertices, indices };

                (
                    material,
                    self.mesh_loader
                        .start_loading_pos_norm_mesh(ivs, &mut *progress),
                )
            })
            .collect();

        ChunkMeshes { meshes }
    }
}

pub struct LatticeVoxelsForMeshing<'a, I>(&'a LatticeVoxels<'a, VoxelInfo, Voxel, I>);

impl<'a, I> GetLinear for LatticeVoxelsForMeshing<'a, I>
where
    I: Indexer,
{
    type Data = VoxelGraphics;

    fn get_linear(&self, i: usize) -> Self::Data {
        let voxel = self.0.map.get_linear(i);
        let graphics = VoxelGraphics {
            material: self.0.palette[voxel.get_palette_address()].material,
            distance: decode_distance(voxel.distance),
        };

        graphics
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

    // The vertex format is limited to 4 numbers for material weights.
    // TODO: make this table for the actual set of materials in the chunk
    // PERF: make this into a sparse Vec instead of a HashMap
    let weight_table: HashMap<VoxelMaterial, [f32; 4]> = [
        (VoxelMaterial(1), [1.0, 0.0, 0.0, 0.0]),
        (VoxelMaterial(2), [0.0, 1.0, 0.0, 0.0]),
        (VoxelMaterial(3), [0.0, 0.0, 1.0, 0.0]),
        (VoxelMaterial(4), [0.0, 0.0, 0.0, 1.0]),
    ]
    .iter()
    .cloned()
    .collect();

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
                let material_w = weight_table
                    .get(&voxel.material)
                    .expect("Negative voxel must have a material");
                w[0] += material_w[0];
                w[1] += material_w[1];
                w[2] += material_w[2];
                w[3] += material_w[3];
            }
        }
    }

    material_weights
}
