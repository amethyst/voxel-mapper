#version 450

// Copied from amethyst_rendy, augmented for triplanar mapping

const float PI = 3.14159265359;

struct UvOffset {
    vec2 u_offset;
    vec2 v_offset;
};

float tex_coord(float coord, vec2 offset) {
    return offset.x + coord * (offset.y - offset.x);
}

vec2 tex_coords(vec2 coord, UvOffset offset) {
    return vec2(tex_coord(coord.x, offset.u_offset), tex_coord(coord.y, offset.v_offset));
}

vec3 schlick_fresnel(float HdotV, vec3 fresnel_base) {
    return fresnel_base + (1.0 - fresnel_base) * pow(1.0 - HdotV, 5.0);
}

float ggx_normal_distribution(vec3 N, vec3 H, float a) {
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH*NdotH;

    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    denom = PI * denom * denom;

    return (a2 + 0.0000001) / denom;
}

float ggx_geometry(float NdotV, float NdotL, float r2) {
    float a1 = r2 + 1.0;
    float k = a1 * a1 / 8.0;
    float denom = NdotV * (1.0 - k) + k;
    float ggx1 = NdotV / denom;
    denom = NdotL * (1.0 - k) + k;
    float ggx2 = NdotL / denom;
    return ggx1 * ggx2;
}

float s_curve (float x) {
		x = x * 2.0 - 1.0;
		return -x * abs(x) * 0.5 + x + 0.5;
}

struct PointLight {
    vec3 position;
    vec3 color;
    float intensity;
};

struct DirectionalLight {
    vec3 color;
    float intensity;
    vec3 direction;
};

struct SpotLight {
    vec3 position;
    vec3 color;
    vec3 direction;
    float angle;
    float intensity;
    float range;
    float smoothness;
};

layout(std140, set = 0, binding = 1) uniform Environment {
    vec3 ambient_color;
    vec3 camera_position;
    int point_light_count;
    int directional_light_count;
    int spot_light_count;
};

layout(std140, set = 0, binding = 2) uniform PointLights {
    PointLight plight[128];
};

layout(std140, set = 0, binding = 3) uniform DirectionalLights {
    DirectionalLight dlight[16];
};

layout(std140, set = 0, binding = 4) uniform SpotLights {
    SpotLight slight[128];
};

layout(std140, set = 1, binding = 0) uniform Material {
    UvOffset uv_offset;
    float alpha_cutoff;
};

layout(set = 1, binding = 1) uniform sampler2D albedo;
layout(set = 1, binding = 2) uniform sampler2D emission;
layout(set = 1, binding = 3) uniform sampler2D normal;
layout(set = 1, binding = 4) uniform sampler2D metallic_roughness;
layout(set = 1, binding = 5) uniform sampler2D ambient_occlusion;
layout(set = 1, binding = 6) uniform sampler2D cavity;

layout(location = 0) in VertexData {
    vec3 position;
    vec3 normal;
    vec4 color;
} vertex;

layout(location = 0) out vec4 out_color;


vec3 fresnel(float HdotV, vec3 fresnel_base) {
    return fresnel_base + (1.0 - fresnel_base) * pow(1.0 - HdotV, 5.0);
}

vec3 compute_light(vec3 attenuation,
                   vec3 light_color,
                   vec3 view_direction,
                   vec3 light_direction,
                   vec3 albedo,
                   vec3 normal,
                   float roughness2,
                   float metallic,
                   vec3 fresnel_base) {

    vec3 halfway = normalize(view_direction + light_direction);
    float normal_distribution = ggx_normal_distribution(normal, halfway, roughness2);

    float NdotV = max(dot(normal, view_direction), 0.0);
    float NdotL = max(dot(normal, light_direction), 0.0);
    float HdotV = max(dot(halfway, view_direction), 0.0);
    float geometry = ggx_geometry(NdotV, NdotL, roughness2);


    vec3 fresnel = fresnel(HdotV, fresnel_base);
    vec3 diffuse = vec3(1.0) - fresnel;
    diffuse *= 1.0 - metallic;

    vec3 nominator = normal_distribution * geometry * fresnel;
    float denominator = 4 * NdotV * NdotL + 0.0001;
    vec3 specular = nominator / denominator;

    vec3 resulting_light = (diffuse * albedo / PI + specular) * light_color * attenuation * NdotL;
    return resulting_light;
}

vec4 triplanar_texture(sampler2D tex, vec3 blend, vec2 uv_x, vec2 uv_y, vec2 uv_z) {
    vec4 x = texture(tex, uv_x);
    vec4 y = texture(tex, uv_y);
    vec4 z = texture(tex, uv_z);
    return blend.x * x + blend.y * y + blend.z * z;
}

vec3 triplanar_normal_to_world(sampler2D tex, vec3 blend, vec2 uv_x, vec2 uv_y, vec2 uv_z, vec3 surf_normal) {
    // Important that the texture is loaded as Unorm.
    vec3 tnormalx = 2.0 * texture(tex, uv_x).rgb - 1.0;
    vec3 tnormaly = 2.0 * texture(tex, uv_y).rgb - 1.0;
    vec3 tnormalz = 2.0 * texture(tex, uv_z).rgb - 1.0;

    // Use swizzle method to convert normal into world space.
    // Get the sign (-1 or 1) of the surface normal
    vec3 axis_sign = sign(surf_normal);
    // Flip tangent normal z to account for surface normal facing
    tnormalx.z *= axis_sign.x;
    tnormaly.z *= axis_sign.y;
    tnormalz.z *= axis_sign.z;
    // Swizzle tangent normals to match world orientation and triblend
    return normalize(
        tnormalx.zyx * blend.x +
        tnormaly.xzy * blend.y +
        tnormalz.xyz * blend.z
    );
}

void main() {
    // Do triplanar mapping (world space -> UVs).
    float texture_scale = 2.0;
    vec3 blend = pow(abs(vertex.normal), vec3(3));
    blend = blend / (blend.x + blend.y + blend.z);
    vec2 uv_x = tex_coords(vertex.position.zy / texture_scale, uv_offset);
    vec2 uv_y = tex_coords(vertex.position.xz / texture_scale, uv_offset);
    vec2 uv_z = tex_coords(vertex.position.xy / texture_scale, uv_offset);

    vec4 albedo_alpha       = triplanar_texture(albedo, blend, uv_x, uv_y, uv_z);
    float alpha             = albedo_alpha.a;
    if(alpha < alpha_cutoff) discard;

    vec3 albedo             = albedo_alpha.rgb;
    vec3 emission           = triplanar_texture(emission, blend, uv_x, uv_y, uv_z).rgb;
    vec3 normal             = triplanar_normal_to_world(normal, blend, uv_x, uv_y, uv_z, vertex.normal);
    vec2 metallic_roughness = triplanar_texture(metallic_roughness, blend, uv_x, uv_y, uv_z).bg;
    float ambient_occlusion = triplanar_texture(ambient_occlusion, blend, uv_x, uv_y, uv_z).r;
    // TODO: Use cavity
    // float cavity            = texture(cavity, tex_coords(vertex.tex_coord, final_tex_coords).r;
    float metallic          = metallic_roughness.r;
    float roughness         = metallic_roughness.g;

    float roughness2 = roughness * roughness;
    vec3 fresnel_base = mix(vec3(0.04), albedo, metallic);

    vec3 view_direction = normalize(camera_position - vertex.position);
    vec3 lighted = vec3(0.0);
    for (int i = 0; i < point_light_count; i++) {
        vec3 light_direction = normalize(plight[i].position - vertex.position);
        float attenuation = plight[i].intensity / dot(light_direction, light_direction);

        vec3 light = compute_light(vec3(attenuation),
                                   plight[i].color,
                                   view_direction,
                                   light_direction,
                                   albedo,
                                   normal,
                                   roughness2,
                                   metallic,
                                   fresnel_base);

        lighted += light;
    }

    for (int i = 0; i < directional_light_count; i++) {
        vec3 light_direction = -normalize(dlight[i].direction);
        float attenuation = dlight[i].intensity;

        vec3 light = compute_light(vec3(attenuation),
                                   dlight[i].color,
                                   view_direction,
                                   light_direction,
                                   albedo,
                                   normal,
                                   roughness2,
                                   metallic,
                                   fresnel_base);

        lighted += light;
    }

    for (int i = 0; i < spot_light_count; i++) {
        vec3 light_vec = slight[i].position - vertex.position;
        vec3 normalized_light_vec = normalize(light_vec);

        // The distance between the current fragment and the "core" of the light
        float light_length = length(light_vec);

        // The allowed "length", everything after this won't be lit.
        // Later on we are dividing by this range, so it can't be 0
        float range = max(slight[i].range, 0.00001);

        // get normalized range, so everything 0..1 could be lit, everything else can't.
        float normalized_range = light_length / max(0.00001, range);

        // The attenuation for the "range". If we would only consider this, we'd have a
        // point light instead, so we need to also check for the spot angle and direction.
        float range_attenuation = max(0.0, 1.0 - normalized_range);

        // this is actually the cosine of the angle, so it can be compared with the
        // "dotted" frag_angle below a lot cheaper.
        float spot_angle = max(slight[i].angle, 0.00001);
        vec3 spot_direction = normalize(slight[i].direction);
        float smoothness = 1.0 - slight[i].smoothness;

        // Here we check if the current fragment is within the "ring" of the spotlight.
        float frag_angle = dot(spot_direction, -normalized_light_vec);

        // so that the ring_attenuation won't be > 1
        frag_angle = max(frag_angle, spot_angle);

        // How much is this outside of the ring? (let's call it "rim")
        // Also smooth this out.
        float rim_attenuation = pow(max((1.0 - frag_angle) / (1.0 - spot_angle), 0.00001), smoothness);

        // How much is this inside the "ring"?
        float ring_attenuation = 1.0 - rim_attenuation;

        // combine the attenuations and intensity
        float attenuation = range_attenuation * ring_attenuation * slight[i].intensity;

        vec3 light = compute_light(vec3(attenuation),
                                   slight[i].color,
                                   view_direction,
                                   normalize(light_vec),
                                   albedo,
                                   normal,
                                   roughness2,
                                   metallic,
                                   fresnel_base);
        lighted += light;
    }

    vec3 ambient = ambient_color * albedo * ambient_occlusion;
    vec3 color = ambient + lighted + emission;

    out_color = vec4(color, alpha) * vertex.color;
}
