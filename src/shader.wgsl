struct Uniforms {
    // mat3x3's are EVIL
    @location(0)
    clip_world_tf: mat4x4<f32>,
    @location(1)
    world_frag_tf: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Quad {
    bb: vec4<f32>,
    color: vec4<f32>,
    clip_depth: u32, // really only have 24 bits guaranteed
}


@group(1) @binding(0)
var<storage, read> quads: array<Quad>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let quad = quads[index/6];
    let m = index % 6;
    out.clip_position = uniforms.clip_world_tf * vec4(
        select(quad.bb.x, quad.bb.z, (m & 1) != 0),
        select(quad.bb.y, quad.bb.w, m > 1 && m < 5),
        0.0,
        1.0,
    );
    out.clip_position = vec4(out.clip_position.xy / out.clip_position.w, f32(quad.clip_depth) / 16777216.0 , 1.0);
    out.color = quad.color;
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
