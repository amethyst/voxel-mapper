use crate::control::hover_3d::ObjectsUnderCursor;

use voxel_mapper::voxel::voxel_containing_point;

use amethyst::{
    core::{ecs::prelude::*, math as na},
    renderer::{debug_drawing::DebugLinesComponent, palette::Srgba},
};
use building_blocks::core::prelude::*;

#[derive(Default)]
pub struct HoverHintTag;

impl Component for HoverHintTag {
    type Storage = NullStorage<Self>;
}

pub fn make_hover_hint_lines(world: &mut World) {
    world
        .create_entity()
        .with(HoverHintTag)
        .with(DebugLinesComponent::new())
        .build();
}

pub struct HoverHintSystem;

impl<'a> System<'a> for HoverHintSystem {
    type SystemData = (
        Read<'a, ObjectsUnderCursor>,
        ReadStorage<'a, HoverHintTag>,
        WriteStorage<'a, DebugLinesComponent>,
    );

    fn run(&mut self, (objects, is_hint, mut debug_lines): Self::SystemData) {
        for (_, lines) in (&is_hint, &mut debug_lines).join() {
            lines.clear();
            let box_p = if let Some(v) = &objects.voxel {
                v.hover_adjacent_point()
            } else if let Some(p) = objects.xz_plane {
                voxel_containing_point(p)
            } else {
                continue;
            };
            // TODO: amethyst is using an older version of nalgebra than building-blocks, so we
            // can't do the simplest conversion
            let box_min: na::Point3<f32> = Point3f::from(box_p).0.into();
            let box_max = box_min + na::Vector3::new(1.0, 1.0, 1.0);
            lines.add_box(box_min, box_max, Srgba::new(1.0, 0.0, 1.0, 1.0));
        }
    }
}
