use crate::{
    control::hover_3d::ObjectsUnderCursor,
    voxel::{voxel_containing_point, MyPoint3},
};

use amethyst::{
    core::ecs::prelude::*,
    renderer::{debug_drawing::DebugLinesComponent, palette::Srgba},
};
use ilattice3 as lat;

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
                *v.point()
            } else if let Some(p) = objects.xz_plane {
                voxel_containing_point(&p)
            } else {
                continue;
            };
            let MyPoint3(box_min) = box_p.into();
            let MyPoint3(box_max) = (box_p + lat::Point::new(1, 1, 1)).into();
            lines.add_box(box_min, box_max, Srgba::new(1.0, 0.0, 1.0, 1.0));
        }
    }
}
