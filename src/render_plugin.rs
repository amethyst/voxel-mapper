// Copied from amethyst_rendy. All skinning and transparent stuff removed.

use crate::base_3d::{Base3DPassDef, DrawBase3DDesc};

use amethyst::core::ecs::{DispatcherBuilder, World};
use amethyst::error::Error;
use amethyst::renderer::{
    bundle::{RenderOrder, RenderPlan, RenderPlugin, Target},
    visibility::VisibilitySortingSystem,
    Backend, Factory,
};
use rendy::graph::render::RenderGroupDesc;

/// A `RenderPlugin` for forward rendering of 3d objects.
/// Generic over 3d pass rendering method.
#[derive(derivative::Derivative)]
#[derivative(Default(bound = ""), Debug(bound = ""))]
pub struct RenderBase3D<D: Base3DPassDef> {
    target: Target,
    marker: std::marker::PhantomData<D>,
}

impl<D: Base3DPassDef> RenderBase3D<D> {
    /// Set target to which 3d meshes will be rendered.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = target;
        self
    }
}

impl<B: Backend, D: Base3DPassDef> RenderPlugin<B> for RenderBase3D<D> {
    fn on_build<'a, 'b>(
        &mut self,
        _world: &mut World,
        builder: &mut DispatcherBuilder<'a, 'b>,
    ) -> Result<(), Error> {
        builder.add(VisibilitySortingSystem::new(), "visibility_system", &[]);
        Ok(())
    }

    fn on_plan(
        &mut self,
        plan: &mut RenderPlan<B>,
        _factory: &mut Factory<B>,
        _world: &World,
    ) -> Result<(), Error> {
        plan.extend_target(self.target, move |ctx| {
            ctx.add(RenderOrder::Opaque, DrawBase3DDesc::<B, D>::new().builder())?;
            Ok(())
        });

        Ok(())
    }
}
