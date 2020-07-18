pub mod chunk_reloader;
pub mod loader;
pub mod manager;

use amethyst::core::ecs::prelude::*;
use ilattice3 as lat;
use std::collections::HashMap;

#[derive(Default)]
pub struct VoxelMeshEntities {
    pub chunk_entities: HashMap<lat::Point, Vec<Entity>>,
}
