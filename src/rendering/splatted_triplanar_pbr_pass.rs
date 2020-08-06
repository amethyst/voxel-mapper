use amethyst::renderer::{mtl::FullTextureSet, pass::Base3DPassDef, RenderBase3D};
use rendy::{
    hal::pso::ShaderStageFlags,
    mesh::{AsVertex, VertexFormat},
    shader::SpirvShader,
    util::types::vertex::{Color, Normal, Position},
};
use serde::{Deserialize, Serialize};

lazy_static::lazy_static! {
    static ref POS_COLOR_NORM_VERTEX: SpirvShader = SpirvShader::from_bytes(
        include_bytes!("shaders/pos_color_norm.spv"),
        ShaderStageFlags::VERTEX,
        "main",
    ).unwrap();
    static ref SPLATTED_TRIPLANAR_PBR_FRAGMENT: SpirvShader = SpirvShader::from_bytes(
        include_bytes!("shaders/splatted_triplanar_pbr.spv"),
        ShaderStageFlags::FRAGMENT,
        "main",
    ).unwrap();
}

#[derive(Debug)]
pub struct SplattedTriplanarPbrPassDef;

impl Base3DPassDef for SplattedTriplanarPbrPassDef {
    const NAME: &'static str = "SplattedTriplanarPbr";
    type TextureSet = FullTextureSet;
    fn vertex_shader() -> &'static SpirvShader {
        &POS_COLOR_NORM_VERTEX
    }
    fn vertex_skinned_shader() -> &'static SpirvShader {
        unimplemented!("Don't need skinning for this pass")
    }
    fn fragment_shader() -> &'static SpirvShader {
        &SPLATTED_TRIPLANAR_PBR_FRAGMENT
    }
    fn base_format() -> Vec<VertexFormat> {
        vec![Position::vertex(), Color::vertex(), Normal::vertex()]
    }
    fn skinned_format() -> Vec<VertexFormat> {
        vec![]
    }
}

/// A render pass that does triplanar texturing and splatting of PBR materials. Requires a vertex
/// format of (vec3 position, vec4 color, vec3 normal). The "color" attribute is really a vector of
/// 4 material weights, summing to one, determining how to blend the 4 materials present in the
/// bound array texture. This means at most 4 materials can be blended in one draw call.
pub type RenderSplattedTriplanarPbr = RenderBase3D<SplattedTriplanarPbrPassDef>;

/// Identifier for one of the arrays of materials. Each mesh can only have one material array bound
/// for the draw call.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ArrayMaterialId(pub usize);
