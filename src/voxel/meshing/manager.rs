use super::loader::ChunkMesh;
use crate::{
    assets::BoundedMesh,
    voxel::{meshing::VoxelMeshEntities, ArrayMaterialId, VoxelAssets, VoxelMap},
};

use amethyst::{
    assets::{Handle, Prefab},
    core::{ecs::prelude::*, Transform},
    renderer::formats::mtl::MaterialPrefab,
};
use ilattice3 as lat;
use std::collections::HashMap;

#[derive(SystemData)]
pub struct VoxelMeshManager<'a> {
    entities: Entities<'a>,
    lazy: Read<'a, LazyUpdate>,
    mesh_entities: Write<'a, VoxelMeshEntities>,
}

impl<'a> VoxelMeshManager<'a> {
    /// Similar to VoxelChunkProcessorSystem::run, but it runs on every chunk loaded in the map, and
    /// expects that mesh assets are finished loading.
    pub fn make_all_chunk_mesh_entities(&mut self, assets: &mut VoxelAssets, map: &VoxelMap) {
        let VoxelAssets {
            array_materials,
            meshes,
            ..
        } = assets;

        for chunk_key in map.voxels.map.chunk_keys() {
            if let Some(chunk_mesh) = meshes.chunk_meshes.get(chunk_key) {
                self.update_chunk_mesh_entities(
                    chunk_key,
                    Some(chunk_mesh.clone()),
                    array_materials,
                );
            }
        }
    }

    pub fn update_chunk_mesh_entities(
        &mut self,
        chunk_key: &lat::Point,
        mesh: Option<ChunkMesh>,
        array_materials: &HashMap<ArrayMaterialId, Handle<Prefab<MaterialPrefab>>>,
    ) {
        // Make new entities.
        let mut new_entities = Vec::new();

        if let Some(ChunkMesh {
            material_array_id,
            mesh,
        }) = mesh
        {
            let material_array = array_materials[&material_array_id].clone();
            let entity = self.make_voxel_mesh_entity(mesh, material_array);
            new_entities.push(entity);
        }

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
    }

    /// Creates a new entity with the given mesh and material. Expects the mesh vertices to already
    /// be in world coordinates, so the model transform can be the identity.
    fn make_voxel_mesh_entity(
        &self,
        mesh: BoundedMesh,
        material_array: Handle<Prefab<MaterialPrefab>>,
    ) -> Entity {
        let BoundedMesh { mesh, sphere } = mesh;

        self.lazy
            .create_entity(&self.entities)
            .with(material_array)
            .with(mesh)
            .with(Transform::default())
            .with(sphere)
            .build()
    }

    pub fn destroy(&mut self) {
        for (_chunk_key, entities) in self.mesh_entities.chunk_entities.drain() {
            for e in entities.into_iter() {
                self.entities.delete(e).unwrap();
            }
        }
    }
}
