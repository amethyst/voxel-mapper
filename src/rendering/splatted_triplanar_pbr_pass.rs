use amethyst::renderer::{mtl::FullTextureSet, pass::Base3DPassDef, RenderBase3D};
use rendy::{
    mesh::{AsVertex, VertexFormat},
    shader::{ShaderKind, SourceLanguage, SourceShaderInfo, SpirvShader},
    util::types::vertex::{Color, Normal, Position},
};

lazy_static::lazy_static! {
    static ref POS_COLOR_NORM_VERTEX: SpirvShader = SourceShaderInfo::new(
        include_str!("shaders/pos_color_norm.vert"),
        "shaders/pos_color_norm.vert",
        ShaderKind::Vertex,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();
    static ref SPLATTED_TRIPLANAR_PBR_FRAGMENT: SpirvShader = SourceShaderInfo::new(
        include_str!("shaders/splatted_triplanar_pbr.frag"),
        "shaders/splatted_triplanar_pbr.frag",
        ShaderKind::Fragment,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();
}

#[derive(Debug)]
pub struct SplattedTriplanarPbrPassDef;

impl Base3DPassDef for SplattedTriplanarPbrPassDef {
    const NAME: &'static str = "TriplanarPbr";
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
