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
    segment_index_range: vec2<i32>,
    clip_depth: u32, // really only have 24 bits guaranteed
}

@group(1) @binding(0)
var<storage, read> quads: array<Quad>;

struct Vertex{ pos: vec2<f32>, }
struct Segment{ idx: vec4<i32>, }

@group(2) @binding(0) var<storage, read> verts: array<Vertex>;
@group(2) @binding(1) var<storage, read> segments: array<Segment>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
    @location(1) @interpolate(flat) segment_range: vec2<i32>,
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
    out.segment_range = quad.segment_index_range;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_pos = uniforms.world_frag_tf * in.clip_position;
    let v0 = tex_pos.xy;
    var winding: i32 = 0;

    for (var segment_index: i32 = in.segment_range.x; segment_index < in.segment_range.y; segment_index++) {
        let segment = segments[segment_index];
        let v1 = verts[segment.idx.x].pos;
        let v2 = verts[segment.idx.y].pos;
        let t: u32 = (u32(v0.y < v1.y) << 3)
            + (u32(v0.y < v2.y) << 2)
            + (u32((v2.x - v0.x) * ((v0.y - v1.y) / (v2.y - v1.y)) + (v1.x - v0.x) * ((v0.y - v2.y) / (v1.y - v2.y)) > 0) << 1);
        winding += i32((0x5195u >> t ) & 3) - 1; // lookup table, effectively
    }
//    return select(vec4(1.0,1.0,1.0,1.0),
//        in.color,
//        winding != 0,
////        (fract((tex_pos.x / tex_pos.w) * 4) < 0.5) == (fract((tex_pos.y / tex_pos.w) * 4) < 0.5),
//    );
    if winding == 0 { discard; }
    return in.color;
}
