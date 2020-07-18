use amethyst::renderer::{mtl::FullTextureSet, pass::Base3DPassDef, RenderBase3D};
use rendy::{
    mesh::{AsVertex, Normal, Position, VertexFormat},
    shader::{ShaderKind, SourceLanguage, SourceShaderInfo, SpirvShader},
};

lazy_static::lazy_static! {
    static ref POS_NORM_VERTEX: SpirvShader = SourceShaderInfo::new(
        include_str!("shaders/pos_norm.vert"),
        "shaders/pos_norm.vert",
        ShaderKind::Vertex,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();
    static ref TRIPLANAR_PBR_FRAGMENT: SpirvShader = SourceShaderInfo::new(
        include_str!("shaders/triplanar_pbr.frag"),
        "shaders/triplanar_pbr.frag",
        ShaderKind::Fragment,
        SourceLanguage::GLSL,
        "main",
    ).precompile().unwrap();
}

#[derive(Debug)]
pub struct TriplanarPassDef;
impl Base3DPassDef for TriplanarPassDef {
    const NAME: &'static str = "TriplanarPbr";
    type TextureSet = FullTextureSet;
    fn vertex_shader() -> &'static SpirvShader {
        &POS_NORM_VERTEX
    }
    fn vertex_skinned_shader() -> &'static SpirvShader {
        unimplemented!()
    }
    fn fragment_shader() -> &'static SpirvShader {
        &TRIPLANAR_PBR_FRAGMENT
    }
    fn base_format() -> Vec<VertexFormat> {
        vec![Position::vertex(), Normal::vertex()]
    }
    fn skinned_format() -> Vec<VertexFormat> {
        vec![]
    }
}

pub type RenderTriplanarPbr = RenderBase3D<TriplanarPassDef>;
