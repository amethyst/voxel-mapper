pub mod loader;
pub mod manager;

use crate::{
    assets::{IndexedPosColorNormVertices, PosColorNormVertices},
    rendering::splatted_triplanar_pbr_pass::ArrayMaterialIndex,
    voxel::{LocalVoxelCache, Voxel, VoxelMap, EMPTY_VOXEL},
};

use amethyst::core::ecs::prelude::*;
use amethyst::renderer::rendy::mesh::{Color, Normal, Position};
use building_blocks::{mesh::*, prelude::*};
use std::collections::HashMap;

#[cfg(feature = "profiler")]
use thread_profiler::profile_scope;

pub enum MeshMode {
    SurfaceNets,
    GreedyQuads,
}

#[derive(Default)]
pub struct VoxelMeshEntities {
    pub chunk_entities: HashMap<Point3i, Vec<Entity>>,
}

pub fn generate_mesh_vertices_with_surface_nets(
    voxel_map: &VoxelMap,
    chunk_extent: &Extent3i,
    local_chunk_cache: &LocalVoxelCache,
) -> Option<IndexedPosColorNormVertices> {
    #[cfg(feature = "profiler")]
    profile_scope!("generate_mesh_vertices");

    let mesh_extent = padded_surface_nets_chunk_extent(chunk_extent);
    // PERF: reuse these buffers between frames
    let mut buffer = SurfaceNetsBuffer::default();
    let mut mesh_voxels = Array3x1::fill(mesh_extent, EMPTY_VOXEL);
    let reader = voxel_map.voxels.reader(local_chunk_cache);
    copy_extent(&mesh_extent, &reader.lod_view(0), &mut mesh_voxels);

    {
        #[cfg(feature = "profiler")]
        profile_scope!("surface_nets");

        surface_nets(&mesh_voxels, &mesh_extent, 1.0, &mut buffer);
    }

    if buffer.mesh.is_empty() {
        return None;
    }

    let SurfaceNetsBuffer {
        mesh:
            PosNormMesh {
                positions,
                normals,
                indices,
            },
        surface_strides,
        ..
    } = buffer;

    let transform_voxel = |v: Voxel| {
        let info = voxel_map.palette.get_voxel_type_info(v.voxel_type);

        MaterialWeightsVoxel {
            material_index: info.material_index,
            distance: v.distance.0,
        }
    };
    let material_voxels = TransformMap::new(&mesh_voxels, &transform_voxel);
    let vertex_material_weights = {
        #[cfg(feature = "profiler")]
        profile_scope!("material_weights");

        material_weights(&material_voxels, &surface_strides)
    };

    let positions = positions.into_iter().map(|p| Position(p)).collect();
    let colors = vertex_material_weights
        .into_iter()
        .map(|w| Color(w))
        .collect();
    let normals = normals.into_iter().map(|n| Normal(n)).collect();
    let vertices = PosColorNormVertices {
        positions,
        colors,
        normals,
    };
    let indices: Vec<_> = indices.into_iter().map(|i| i as u32).collect();

    Some(IndexedPosColorNormVertices { vertices, indices })
}

pub fn generate_mesh_vertices_with_greedy_quads(
    voxel_map: &VoxelMap,
    chunk_extent: &Extent3i,
    local_chunk_cache: &LocalVoxelCache,
) -> Option<IndexedPosColorNormVertices> {
    #[cfg(feature = "profiler")]
    profile_scope!("generate_mesh_vertices");

    let mesh_extent = padded_greedy_quads_chunk_extent(chunk_extent);
    // PERF: reuse these buffers between frames
    let mut buffer = GreedyQuadsBuffer::new(mesh_extent, RIGHT_HANDED_Y_UP_CONFIG.quad_groups());
    let mut mesh_voxels = Array3x1::fill(mesh_extent, EMPTY_VOXEL);
    let reader = voxel_map.voxels.reader(local_chunk_cache);
    copy_extent(&mesh_extent, &reader.lod_view(0), &mut mesh_voxels);
    let voxel_infos = TransformMap::new(&mesh_voxels, voxel_map.voxel_info_transform());

    {
        #[cfg(feature = "profiler")]
        profile_scope!("greedy_quads");

        greedy_quads(&voxel_infos, &mesh_extent, &mut buffer);
    }

    if buffer.num_quads() == 0 {
        return None;
    }

    // PERF: reuse these buffers between frames
    let mut mesh = PosNormMesh::default();
    let mut colors = Vec::with_capacity(4 * buffer.num_quads());
    for group in buffer.quad_groups.iter() {
        for quad in group.quads.iter() {
            let material = voxel_infos.get(quad.minimum).material_index;
            colors.extend(&[Color(MATERIAL_WEIGHT_TABLE[material.0 as usize]); 4]);
            group.face.add_quad_to_pos_norm_mesh(quad, 1.0, &mut mesh);
        }
    }

    let positions = mesh.positions.into_iter().map(|p| Position(p)).collect();
    let normals = mesh.normals.into_iter().map(|n| Normal(n)).collect();
    let vertices = PosColorNormVertices {
        positions,
        colors,
        normals,
    };
    let indices = mesh.indices.into_iter().map(|i| i as u32).collect();

    Some(IndexedPosColorNormVertices { vertices, indices })
}

/// Returns the material weights for each of the points in `surface_strides`.
///
/// Uses a 2x2x2 kernel (the same shape as the Surface Nets kernel) to average the adjacent
/// materials for each surface point. `voxels` should at least contain the extent that was used with
/// `surface_nets` in order to generate `surface_strides`. Currently limited to 4 materials per
/// surface chunk.
fn material_weights<V>(voxels: &V, surface_strides: &[Stride]) -> Vec<[f32; 4]>
where
    V: IndexedArray<[i32; 3]> + Get<Stride, Item = MaterialWeightsVoxel>,
{
    // Precompute the offsets for cube corners.
    let mut corner_offset_strides = [Stride(0); 8];
    let corner_offsets = Local::localize_points_slice(&Point3i::corner_offsets());
    voxels.strides_from_local_points(&corner_offsets, &mut corner_offset_strides);

    let mut material_weights = vec![[0.0; 4]; surface_strides.len()];

    for (i, p_stride) in surface_strides.iter().enumerate() {
        let w = &mut material_weights[i];
        for offset_stride in corner_offset_strides.iter() {
            let q_stride = *p_stride + *offset_stride;
            let voxel = voxels.get(q_stride);
            if voxel.distance < 0 {
                let material_w = MATERIAL_WEIGHT_TABLE[voxel.material_index.0 as usize];
                w[0] += material_w[0];
                w[1] += material_w[1];
                w[2] += material_w[2];
                w[3] += material_w[3];
            }
        }
    }

    material_weights
}

// Currently limited to 4 numbers for material weights.
const MATERIAL_WEIGHT_TABLE: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

struct MaterialWeightsVoxel {
    material_index: ArrayMaterialIndex,
    distance: i8,
}
