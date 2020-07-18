use crate::{
    assets::{BoundedMesh, IndexedPosNormVertices, MeshLoader, PosNormVertices},
    voxel::{decode_distance, Voxel, VoxelGraphics, VoxelInfo, VoxelMaterial},
};

use amethyst::{
    assets::ProgressCounter,
    core::ecs::prelude::*,
    renderer::rendy::mesh::{Normal, Position},
};
use ilattice3 as lat;
use ilattice3::{
    prelude::*, ChunkedPaletteLatticeMap, GetPaletteAddress, HasIndexer, Indexer, LatticeVoxels,
};
use ilattice3_mesh::{surface_nets, SurfaceNetsOutput};
use std::collections::HashMap;

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
                    self.start_loading_chunk(&chunk_and_boundary, progress).0,
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
    ) -> (ChunkMeshes, usize, u128)
    where
        I: Indexer,
    {
        let before_surface_nets = std::time::Instant::now();
        let SurfaceNetsOutput {
            positions,
            normals,
            indices_by_material,
        } = surface_nets(
            &LatticeVoxelsForMeshing(chunk_and_boundary),
            chunk_and_boundary.get_extent(),
        );
        let surface_nets_micros = before_surface_nets.elapsed().as_micros();

        let mut num_triangles = 0;
        let meshes = indices_by_material
            .into_iter()
            .map(|(material, indices)| {
                // TODO: eventually it would be nice to have enough control so that the same vertex
                // buffer could be used with multiple index buffers
                //
                // Just use the same vertices for each mesh. Not memory efficient, but significantly
                // faster than trying to split the mesh.
                let positions = positions.clone().into_iter().map(|p| Position(p)).collect();
                let normals = normals.clone().into_iter().map(|n| Normal(n)).collect();
                let vertices = PosNormVertices { positions, normals };
                let indices: Vec<_> = indices.into_iter().map(|i| i as u32).collect();
                num_triangles += indices.len() / 3;
                let ivs = IndexedPosNormVertices { vertices, indices };

                (
                    material,
                    self.mesh_loader
                        .start_loading_pos_norm_mesh(ivs, &mut *progress),
                )
            })
            .collect();

        (ChunkMeshes { meshes }, num_triangles, surface_nets_micros)
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

impl<'a, I> HasIndexer for LatticeVoxelsForMeshing<'a, I>
where
    I: Indexer,
{
    type Indexer = I;
}
