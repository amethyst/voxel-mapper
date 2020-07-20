use super::loader::ChunkMeshes;
use crate::{
    assets::BoundedMesh,
    collision::{VoxelBVT, VoxelBVTLeaves},
    voxel::{meshing::VoxelMeshEntities, voxel_aabb, Voxel, VoxelAssets, VoxelMap, VoxelMaterial},
};

use amethyst::{
    assets::{Handle, Prefab},
    core::{ecs::prelude::*, Transform},
    renderer::formats::mtl::MaterialPrefab,
};
use ilattice3 as lat;
use ilattice3::{find_surface_voxels, prelude::*, Indexer, IsEmpty, LatticeVoxels};
use ncollide3d::partitioning::DBVTLeaf;
use std::collections::HashMap;

#[derive(SystemData)]
pub struct VoxelMeshManager<'a> {
    entities: Entities<'a>,
    lazy: Read<'a, LazyUpdate>,
    mesh_entities: Write<'a, VoxelMeshEntities>,
    bvt: WriteExpect<'a, VoxelBVT>,
    bvt_leaves: Write<'a, VoxelBVTLeaves>,
}

impl<'a> VoxelMeshManager<'a> {
    /// Similar to VoxelChunkReloaderSystem::run, but it runs on every chunk loaded in the map, and
    /// expects that mesh assets are finished loading.
    pub fn make_all_chunk_mesh_entities(&mut self, assets: &mut VoxelAssets, map: &VoxelMap) {
        let VoxelAssets {
            materials, meshes, ..
        } = assets;

        for chunk_key in map.voxels.map.chunk_keys() {
            let chunk_meshes = meshes.chunk_meshes.get(chunk_key).unwrap();
            let chunk = map.voxels.get_chunk_and_boundary(chunk_key);
            self.update_chunk_mesh_entities(chunk_key, &chunk, &chunk_meshes, materials);
        }
    }

    pub fn update_chunk_mesh_entities<T, I>(
        &mut self,
        chunk_key: &lat::Point,
        chunk: &LatticeVoxels<'_, T, Voxel, I>,
        meshes: &ChunkMeshes,
        materials: &HashMap<VoxelMaterial, Handle<Prefab<MaterialPrefab>>>,
    ) -> u128
    where
        T: IsEmpty,
        I: Indexer,
    {
        // Make new entities.
        let mut new_entities = Vec::new();
        for (material, mesh) in meshes.meshes.iter().cloned() {
            let material = materials[&material].clone();
            let entity = self.make_voxel_mesh_entity(mesh, material);
            new_entities.push(entity);
        }

        // Replace the bounding volumes.
        let before_bvs = std::time::Instant::now();
        self.remove_bounding_volumes_for_chunk(chunk_key);
        self.create_bounding_volumes_for_voxels(chunk_key, chunk);
        let micros_to_create_bvs = before_bvs.elapsed().as_micros();

        // Replace the entities.
        let mesh_entities = self
            .mesh_entities
            .chunk_entities
            .entry(*chunk_key)
            .or_insert_with(Vec::new);
        for e in mesh_entities.drain(..) {
            self.entities.delete(e).unwrap();
        }
        *mesh_entities = new_entities;

        micros_to_create_bvs
    }

    /// Creates a new entity with the given mesh and material. Expects the mesh vertices to already
    /// be in world coordinates, so the model transform can be the identity.
    fn make_voxel_mesh_entity(
        &self,
        mesh: BoundedMesh,
        material: Handle<Prefab<MaterialPrefab>>,
    ) -> Entity {
        let BoundedMesh { mesh, sphere } = mesh;

        self.lazy
            .create_entity(&self.entities)
            .with(material)
            .with(mesh)
            .with(Transform::default())
            .with(sphere)
            .build()
    }

    fn remove_bounding_volumes_for_chunk(&mut self, chunk_key: &lat::Point) {
        if let Some(leaves) = self.bvt_leaves.leaves.remove(chunk_key) {
            for leaf in leaves.into_iter() {
                self.bvt.remove(leaf);
            }
        }
    }

    fn create_bounding_volumes_for_voxels<T, I>(
        &mut self,
        chunk_key: &lat::Point,
        chunk_voxels: &LatticeVoxels<'_, T, Voxel, I>,
    ) where
        T: IsEmpty,
        I: Indexer,
    {
        let solid_points: Vec<_> = find_surface_voxels(chunk_voxels, chunk_voxels.get_extent());
        let leaves = self
            .bvt_leaves
            .leaves
            .entry(*chunk_key)
            .or_insert_with(Vec::new);
        for p in solid_points.iter() {
            leaves.push(self.bvt.insert(DBVTLeaf::new(voxel_aabb(p), *p)));
        }
    }

    pub fn destroy(&mut self) {
        for (_chunk_key, entities) in self.mesh_entities.chunk_entities.drain() {
            for e in entities.into_iter() {
                self.entities.delete(e).unwrap();
            }
        }
        *self.bvt = VoxelBVT::new();
        *self.bvt_leaves = VoxelBVTLeaves::default();
    }
}
