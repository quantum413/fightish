struct Uniforms {
    // mat3x3's are EVIL
    @location(0)
    clip_world_tf: mat4x4<f32>,
    @location(1)
    world_frag_tf: mat4x4<f32>,
}
struct Origin {
    @location(0)
    world_tex_tf: mat4x4<f32>,
}
const MAX_NUM_ORIGINS = 5;
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;
@group(0) @binding(1)
var<uniform> origins: array<Origin, MAX_NUM_ORIGINS>;

struct Quad {
    bb: vec4<f32>,
    color: vec4<f32>,
    segment_index_range: vec2<i32>,
    clip_depth: u32, // really only have 24 bits guaranteed
    origin_index: u32,
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
    @location(2) @interpolate(flat) origin_index: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let quad = quads[index/6];
    let m = index % 6;
    out.clip_position = uniforms.clip_world_tf * origins[quad.origin_index].world_tex_tf * vec4(
        select(quad.bb.x, quad.bb.z, (m & 1) != 0),
        select(quad.bb.y, quad.bb.w, m > 1 && m < 5),
        0.0,
        1.0,
    );
    out.clip_position = vec4(out.clip_position.xy / out.clip_position.w, f32(quad.clip_depth) / 16777216.0 , 1.0);
    out.color = quad.color;
    out.segment_range = quad.segment_index_range;
    out.origin_index = quad.origin_index;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_pos = uniforms.world_frag_tf * in.clip_position;
    let tf = origins[in.origin_index].world_tex_tf;
    let v0 = tex_pos.xy;
    var winding: i32 = 0;

    for (var segment_index: i32 = in.segment_range.x; segment_index < in.segment_range.y; segment_index++) {
        let segment = segments[segment_index];
        let v1 = tf * vec4(verts[segment.idx.x].pos, 0.0, 1.0);
        let v2 = tf * vec4(verts[segment.idx.y].pos, 0.0, 1.0);
        if (segment.idx.z < 0){
            winding += winding_line(v0, v1.xy / v1.w, v2.xy / v2.w);
        }
        else {
            let v3 = tf * vec4(verts[segment.idx.z].pos, 0.0, 1.0);
            winding += winding_quad(v0, v1.xy / v1.w, v2.xy / v2.w, v3.xy / v3.w);
        }
    }
    if winding == 0 { discard; }
    return in.color;
}

fn winding_line(v0: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>) -> i32 {
    let code: u32 = (u32(v0.y < v1.y) << 3)
                + (u32(v0.y < v2.y) << 2)
                + (u32((v2.x - v0.x) * ((v0.y - v1.y) / (v2.y - v1.y)) + (v1.x - v0.x) * ((v0.y - v2.y) / (v1.y - v2.y)) > 0) << 1);
    return i32((0x5195u >> code) & 3) - 1;
}

fn winding_quad(v0: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>, v3: vec2<f32>) -> i32 {
    // v0 the point to be tested, v1, then v2, then v3 in the end, control, end order.
    // quadratics specifications. Not mandating symmetry of the two directions,
    // so internal boundaries must be lines.
    // this is using Lengyel's algorithm, modified a bit.
    let code: u32 = (
        0x2E74u >>
        (
            select(0x0u, 0x2u, v1.y > v0.y) +
            select(0x0u, 0x4u, v2.y > v0.y) +
            select(0x0u, 0x8u, v3.y > v0.y)
        )
    ) & 0x3u;

    // they used a t^2 - 2b t + c polynomial format.
    // they make a branch to skip this if the code vanished, need to analye whether you get
    // workgroup divergence problems.
    let ax = (v1.x + v3.x) - 2 * v2.x;
    let ay = (v1.y + v3.y) - 2 * v2.y;
    let bx = v1.x - v2.x;
    let by = v1.y - v2.y;
    let cy = v1.y - v0.y;
    let ra = 1.0f / ay;

    let d = sqrt(max(by * by - ay * cy, 0.0));
    // when code is 0x1u, this is the case where root 1 (the minus one) is forced in the range
    // but the other root could be big.
    // specifically, v1.y > v3.y, when ay is small, by > 0
    // so (by - d) * ra is O(1) due to a cancellation, which is bad, so use a different formula.
    // this is more accurate than their formula with an explicit epsilon, but maybe slower?
    let t1 = select((by - d) * ra, cy / (by + d), code == 0x1u);
    // same logic, but flipped.
    let t2 = select((by + d) * ra, cy / (by - d), code == 0x2u);
    // so now the original values of t1 and t2 only actually get used when the code is 0x3u.
    // note need t1 == t2 in the case when there are no roots, but this is only in case 0x3u, so o.k.

    let b1 = (ax * t1 - 2 * bx) * t1 + v1.x > v0.x;
    let b2 = (ax * t2 - 2 * bx) * t2 + v1.x > v0.x;

    return i32((code > 1) && b2) - i32 (((code & 1) != 0) && b1);
}