// Copied from amethyst_rendy. All skinning and transparent stuff removed.

use super::material_set::{MaterialId, MaterialSub};

use amethyst::assets::{AssetStorage, Handle};
use amethyst::core::{
    ecs::{Join, Read, ReadExpect, ReadStorage, SystemData, World},
    transform::Transform,
};
use amethyst::renderer::{
    batch::{GroupIterator, TwoLevelBatch},
    mtl::{Material, StaticTextureSet},
    pipeline::{PipelineDescBuilder, PipelinesBuilder},
    pod::VertexArgs,
    resources::Tint,
    skinning::JointTransforms,
    submodules::{DynamicVertexBuffer, EnvironmentSub},
    types::{Backend, Mesh},
    util,
    visibility::Visibility,
};
use derivative::Derivative;
use rendy::{
    command::{QueueId, RenderPassEncoder},
    factory::Factory,
    graph::{
        render::{PrepareResult, RenderGroup, RenderGroupDesc},
        GraphContext, NodeBuffer, NodeImage,
    },
    hal::{self, device::Device, pso},
    mesh::{AsVertex, VertexFormat},
    shader::{Shader, SpirvShader},
};
use smallvec::SmallVec;
use std::marker::PhantomData;

macro_rules! profile_scope_impl {
    ($string:expr) => {
        #[cfg(feature = "profiler")]
        let _profile_scope = thread_profiler::ProfileScope::new(format!(
            "{} {}: {}",
            module_path!(),
            <T as Base3DPassDef>::NAME,
            $string
        ));
    };
}

/// Define drawing opaque 3d meshes with specified shaders and texture set
pub trait Base3DPassDef: 'static + std::fmt::Debug + Send + Sync {
    /// The human readable name of this pass
    const NAME: &'static str;

    /// The [mtl::StaticTextureSet] type implementation for this pass
    type TextureSet: for<'a> StaticTextureSet<'a>;

    /// Returns the vertex `SpirvShader` which will be used for this pass
    fn vertex_shader() -> &'static SpirvShader;

    /// Returns the fragment `SpirvShader` which will be used for this pass
    fn fragment_shader() -> &'static SpirvShader;

    /// Returns the `VertexFormat` of this pass
    fn base_format() -> Vec<VertexFormat>;
}

/// Draw opaque 3d meshes with specified shaders and texture set
#[derive(Clone, Derivative)]
#[derivative(Debug(bound = ""), Default(bound = ""))]
pub struct DrawBase3DDesc<B: Backend, T: Base3DPassDef> {
    marker: PhantomData<(B, T)>,
}

impl<B: Backend, T: Base3DPassDef> DrawBase3DDesc<B, T> {
    /// Create pass in default configuration
    pub fn new() -> Self {
        Default::default()
    }
}

impl<B: Backend, T: Base3DPassDef> RenderGroupDesc<B, World> for DrawBase3DDesc<B, T> {
    fn build(
        self,
        _ctx: &GraphContext<B>,
        factory: &mut Factory<B>,
        _queue: QueueId,
        _aux: &World,
        framebuffer_width: u32,
        framebuffer_height: u32,
        subpass: hal::pass::Subpass<'_, B>,
        _buffers: Vec<NodeBuffer>,
        _images: Vec<NodeImage>,
    ) -> Result<Box<dyn RenderGroup<B, World>>, failure::Error> {
        profile_scope_impl!("build");

        let env = EnvironmentSub::new(
            factory,
            [
                hal::pso::ShaderStageFlags::VERTEX,
                hal::pso::ShaderStageFlags::FRAGMENT,
            ],
        )?;
        let materials = MaterialSub::new(factory)?;

        let mut vertex_format_base = T::base_format();

        let (mut pipelines, pipeline_layout) = build_pipelines::<B, T>(
            factory,
            subpass,
            framebuffer_width,
            framebuffer_height,
            &vertex_format_base,
            vec![env.raw_layout(), materials.raw_layout()],
        )?;

        vertex_format_base.sort();

        Ok(Box::new(DrawBase3D::<B, T> {
            pipeline_basic: pipelines.remove(0),
            pipeline_layout,
            static_batches: Default::default(),
            vertex_format_base,
            env,
            materials,
            models: DynamicVertexBuffer::new(),
            marker: PhantomData,
        }))
    }
}

/// Base implementation of a 3D render pass which can be consumed by actual 3D render passes,
/// such as [pass::pbr::DrawPbr]
#[derive(Derivative)]
#[derivative(Debug(bound = ""))]
pub struct DrawBase3D<B: Backend, T: Base3DPassDef> {
    pipeline_basic: B::GraphicsPipeline,
    pipeline_layout: B::PipelineLayout,
    static_batches: TwoLevelBatch<MaterialId, u32, SmallVec<[VertexArgs; 4]>>,
    vertex_format_base: Vec<VertexFormat>,
    env: EnvironmentSub<B>,
    materials: MaterialSub<B, T::TextureSet>,
    models: DynamicVertexBuffer<B, VertexArgs>,
    marker: PhantomData<T>,
}

impl<B: Backend, T: Base3DPassDef> RenderGroup<B, World> for DrawBase3D<B, T> {
    fn prepare(
        &mut self,
        factory: &Factory<B>,
        _queue: QueueId,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        resources: &World,
    ) -> PrepareResult {
        profile_scope_impl!("prepare opaque");

        let (mesh_storage, visibility, meshes, materials, transforms, joints, tints) =
            <(
                Read<'_, AssetStorage<Mesh>>,
                ReadExpect<'_, Visibility>,
                ReadStorage<'_, Handle<Mesh>>,
                ReadStorage<'_, Handle<Material>>,
                ReadStorage<'_, Transform>,
                ReadStorage<'_, JointTransforms>,
                ReadStorage<'_, Tint>,
            )>::fetch(resources);

        // Prepare environment
        self.env.process(factory, index, resources);
        self.materials.maintain();

        self.static_batches.clear_inner();

        let materials_ref = &mut self.materials;
        let statics_ref = &mut self.static_batches;

        let static_input = || ((&materials, &meshes, &transforms, tints.maybe()), !&joints);
        {
            profile_scope_impl!("prepare");
            (static_input(), &visibility.visible_unordered)
                .join()
                .map(|(((mat, mesh, tform, tint), _), _)| {
                    ((mat, mesh.id()), VertexArgs::from_object_data(tform, tint))
                })
                .for_each_group(|(mat, mesh_id), data| {
                    if mesh_storage.contains_id(mesh_id) {
                        if let Some((mat, _)) = materials_ref.insert(factory, resources, mat) {
                            statics_ref.insert(mat, mesh_id, data.drain(..));
                        }
                    }
                });
        }

        {
            profile_scope_impl!("write");

            self.static_batches.prune();

            self.models.write(
                factory,
                index,
                self.static_batches.count() as u64,
                self.static_batches.data(),
            );
        }

        PrepareResult::DrawRecord
    }

    fn draw_inline(
        &mut self,
        mut encoder: RenderPassEncoder<'_, B>,
        index: usize,
        _subpass: hal::pass::Subpass<'_, B>,
        resources: &World,
    ) {
        profile_scope_impl!("draw opaque");

        let mesh_storage = <Read<'_, AssetStorage<Mesh>>>::fetch(resources);
        let models_loc = self.vertex_format_base.len() as u32;

        encoder.bind_graphics_pipeline(&self.pipeline_basic);
        self.env.bind(index, &self.pipeline_layout, 0, &mut encoder);

        if self.models.bind(index, models_loc, 0, &mut encoder) {
            let mut instances_drawn = 0;
            for (&mat_id, batches) in self.static_batches.iter() {
                if self.materials.loaded(mat_id) {
                    self.materials
                        .bind(&self.pipeline_layout, 1, mat_id, &mut encoder);
                    for (mesh_id, batch_data) in batches {
                        debug_assert!(mesh_storage.contains_id(*mesh_id));
                        if let Some(mesh) =
                            B::unwrap_mesh(unsafe { mesh_storage.get_by_id_unchecked(*mesh_id) })
                        {
                            mesh.bind_and_draw(
                                0,
                                &self.vertex_format_base,
                                instances_drawn..instances_drawn + batch_data.len() as u32,
                                &mut encoder,
                            )
                            .unwrap();
                        }
                        instances_drawn += batch_data.len() as u32;
                    }
                }
            }
        }
    }

    fn dispose(self: Box<Self>, factory: &mut Factory<B>, _aux: &World) {
        profile_scope_impl!("dispose");
        unsafe {
            factory
                .device()
                .destroy_graphics_pipeline(self.pipeline_basic);
            factory
                .device()
                .destroy_pipeline_layout(self.pipeline_layout);
        }
    }
}

fn build_pipelines<B: Backend, T: Base3DPassDef>(
    factory: &Factory<B>,
    subpass: hal::pass::Subpass<'_, B>,
    framebuffer_width: u32,
    framebuffer_height: u32,
    vertex_format_base: &[VertexFormat],
    layouts: Vec<&B::DescriptorSetLayout>,
) -> Result<(Vec<B::GraphicsPipeline>, B::PipelineLayout), failure::Error> {
    let pipeline_layout = unsafe {
        factory
            .device()
            .create_pipeline_layout(layouts, None as Option<(_, _)>)
    }?;

    let vertex_desc = vertex_format_base
        .iter()
        .map(|f| (f.clone(), pso::VertexInputRate::Vertex))
        .chain(Some((
            VertexArgs::vertex(),
            pso::VertexInputRate::Instance(1),
        )))
        .collect::<Vec<_>>();

    let shader_vertex_basic = unsafe { T::vertex_shader().module(factory).unwrap() };
    let shader_fragment = unsafe { T::fragment_shader().module(factory).unwrap() };
    let pipe_desc = PipelineDescBuilder::new()
        .with_vertex_desc(&vertex_desc)
        .with_shaders(util::simple_shader_set(
            &shader_vertex_basic,
            Some(&shader_fragment),
        ))
        .with_layout(&pipeline_layout)
        .with_subpass(subpass)
        .with_framebuffer_size(framebuffer_width, framebuffer_height)
        .with_face_culling(pso::Face::BACK)
        .with_depth_test(pso::DepthTest {
            fun: pso::Comparison::Less,
            write: true,
        })
        .with_blend_targets(vec![pso::ColorBlendDesc {
            mask: pso::ColorMask::ALL,
            blend: None,
        }]);

    let pipelines = PipelinesBuilder::new()
        .with_pipeline(pipe_desc)
        .build(factory, None);

    unsafe {
        factory.destroy_shader_module(shader_vertex_basic);
        factory.destroy_shader_module(shader_fragment);
    }

    match pipelines {
        Err(e) => {
            unsafe {
                factory.device().destroy_pipeline_layout(pipeline_layout);
            }
            Err(e)
        }
        Ok(pipelines) => Ok((pipelines, pipeline_layout)),
    }
}
