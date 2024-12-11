struct Uniforms {
    // mat3x3's are EVIL
    @location(0)
    clip_world_tf: mat4x4<f32>,
    @location(1)
    world_frag_tf: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.color = model.color;

    out.clip_position = uniforms.clip_world_tf * vec4(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_pos = uniforms.world_frag_tf * in.clip_position;
    return select(vec4(1.0,1.0,1.0,1.0),
        in.color,
        (fract((tex_pos.x / tex_pos.w) * 4) < 0.5) == (fract((tex_pos.y / tex_pos.w) * 4) < 0.5),
    );
}
