#version 450

// Copied from amethyst_rendy

layout(std140, set = 0, binding = 0) uniform Projview {
    mat4 proj;
    mat4 view;
    mat4 proj_view;
};

layout(location = 0) in vec3 position;
layout(location = 1) in vec4 material_weights;
layout(location = 2) in vec3 normal;
layout(location = 3) in mat4 model; // instance rate
layout(location = 7) in vec4 tint; // instance rate

layout(location = 0) out VertexData {
    vec3 position;
    vec3 normal;
    vec4 color;
    vec4 material_weights;
} vertex;

void main() {
    vec4 vertex_position = model * vec4(position, 1.0);
    vertex.position = vertex_position.xyz;
    vertex.normal = mat3(model) * normal;
    vertex.color = tint;
    vertex.material_weights = normalize(material_weights);
    gl_Position = proj_view * vertex_position;
}
