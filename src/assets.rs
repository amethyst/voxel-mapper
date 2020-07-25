use crate::geometry::ritter_sphere_bounding_positions;

use amethyst::{
    assets::{AssetLoaderSystemData, Handle, Progress},
    core::ecs::prelude::*,
    renderer::{
        rendy::mesh::{Color, MeshBuilder, Normal, Position},
        visibility::BoundingSphere,
        Mesh,
    },
};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Default)]
pub struct PosColorNormVertices {
    pub positions: Vec<Position>,
    pub colors: Vec<Color>,
    pub normals: Vec<Normal>,
}

pub struct IndexedPosColorNormVertices {
    pub indices: Vec<u32>,
    pub vertices: PosColorNormVertices,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundedMesh {
    pub mesh: Handle<Mesh>,
    pub sphere: BoundingSphere,
}

/// Loads vertices into `BoundedMesh` objects.
#[derive(SystemData)]
pub struct MeshLoader<'a> {
    loader: AssetLoaderSystemData<'a, Mesh>,
}

impl<'a> MeshLoader<'a> {
    pub fn start_loading_pos_norm_mesh<P: Progress>(
        &self,
        ivs: IndexedPosColorNormVertices,
        progress: P,
    ) -> BoundedMesh {
        // We can't load empty meshes.
        assert!(!ivs.indices.is_empty());

        let sphere = ritter_sphere_bounding_positions(&ivs.vertices.positions);
        let sphere = BoundingSphere::new(sphere.center, sphere.radius);

        let mesh = self.loader.load_from_data(
            MeshBuilder::new()
                .with_vertices(ivs.vertices.positions)
                .with_vertices(ivs.vertices.colors)
                .with_vertices(ivs.vertices.normals)
                .with_indices(ivs.indices)
                .into(),
            progress,
        );

        BoundedMesh { mesh, sphere }
    }
}

#[derive(Debug)]
pub enum BincodeFileError {
    BincodeError(bincode::Error),
    IoError(io::Error),
}

impl From<io::Error> for BincodeFileError {
    fn from(other: io::Error) -> Self {
        BincodeFileError::IoError(other)
    }
}

impl From<bincode::Error> for BincodeFileError {
    fn from(other: bincode::Error) -> Self {
        BincodeFileError::BincodeError(other)
    }
}

pub fn write_bincode_file<P: AsRef<Path>, T: Serialize>(
    path: P,
    data: T,
) -> Result<(), BincodeFileError> {
    let serial = bincode::serialize(&data)?;
    let mut f = File::create(path)?;
    f.write_all(&serial)?;

    Ok(())
}

pub fn read_bincode_file<P: AsRef<Path>, T: for<'a> Deserialize<'a>>(
    path: P,
) -> Result<T, BincodeFileError> {
    let mut f = File::open(path)?;
    let mut contents = Vec::new();
    f.read_to_end(&mut contents)?;
    let t = bincode::deserialize(&contents)?;

    Ok(t)
}
