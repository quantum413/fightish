struct Uniforms {
    // mat3x3's are EVIL
    @location(0)
    clip_world_tf: mat4x4<f32>,
    @location(1)
    frag_clip_tf: mat4x4<f32>,
}

struct Object {
    world_tex_tf: mat4x4<f32>,
    frame_index: i32,
    clip_offset: u32,
    shard_offset: i32,
    segment_offset: i32,
}

struct ShardVertex {
    pos: vec4<f32>,
    color: vec4<f32>,
    segment_range: vec2<i32>,
    clip_depth: u32,
}

struct FrameSegment {
    s: vec2<f32>,
    e: vec2<f32>,
    m: vec2<f32>,
    flags: u32,
}

struct Shard {
    bb: vec4<f32>,
    color: vec4<f32>,
    segment_range: vec2<i32>,
    clip_depth: u32,
}

struct Frame {
    shard_range: vec2<i32>,
    segment_range: vec2<i32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var<storage, read_write> frame_segment: array<FrameSegment>;
@group(1) @binding(1)
var<storage, read_write> frame_shards: array<ShardVertex>;

@group(2) @binding(0)
var<storage, read> model_vertex: array<vec2<f32>>;
@group(2) @binding(1)
var<storage, read> model_segments: array<vec4<i32>>;
@group(2) @binding(2)
var<storage, read> model_shards: array<Shard>;
@group(2) @binding(3)
var<storage, read> model_frames: array<Frame>;

@group(3) @binding(0)
var<storage, read> objects: array<Object>; // maybe convert to a uniform buffer

// stupidest possible algorithm...
@compute @workgroup_size(1) fn main(
    @builtin(global_invocation_id) id: vec3<u32>,
) {
    let object = objects[id.x];
    let frame = model_frames[object.frame_index];
    for (var i = 0; i < frame.shard_range.y - frame.shard_range.x; i++) {
        let shard = model_shards[i + frame.shard_range.x];
        let j = i + object.shard_offset;
        frame_shards[6 * j + 0] = get_shard_vert(object, shard, frame, vec2(shard.bb.x, shard.bb.y));
        frame_shards[6 * j + 1] = get_shard_vert(object, shard, frame, vec2(shard.bb.x, shard.bb.w));
        frame_shards[6 * j + 2] = get_shard_vert(object, shard, frame, vec2(shard.bb.z, shard.bb.y));
        frame_shards[6 * j + 3] = get_shard_vert(object, shard, frame, vec2(shard.bb.z, shard.bb.w));
        frame_shards[6 * j + 4] = get_shard_vert(object, shard, frame, vec2(shard.bb.z, shard.bb.y));
        frame_shards[6 * j + 5] = get_shard_vert(object, shard, frame, vec2(shard.bb.x, shard.bb.w));
    }
    let frag_tex_tf = uniforms.frag_clip_tf * uniforms.clip_world_tf * object.world_tex_tf;
    for (var i = frame.segment_range.x; i < frame.segment_range.y; i++) {
        var model_segment = model_segments[i];
        var segment: FrameSegment;
        segment.s = get_xy(frag_tex_tf * vec4(model_vertex[model_segment.x], 0.0, 1.0));
        segment.e = get_xy(frag_tex_tf * vec4(model_vertex[model_segment.y], 0.0, 1.0));
        segment.m = get_xy(frag_tex_tf * vec4(model_vertex[
            select(model_segment.z, model_segment.x, model_segment.z < 0)
        ], 0.0, 1.0));
        segment.flags = select(0u, 1u, model_segment.z < 0);
        frame_segment[i - frame.segment_range.x + object.segment_offset] = segment;
    }
}

fn get_xy(v: vec4<f32>) -> vec2<f32> { return v.xy / v.w; }

fn get_shard_vert(object: Object, shard: Shard, frame: Frame, bb_vert: vec2<f32>) -> ShardVertex {
    var out: ShardVertex;
    out.pos = uniforms.clip_world_tf * object.world_tex_tf * vec4(bb_vert, 0.0, 1.0);
    out.color = shard.color;
    out.segment_range = shard.segment_range - frame.segment_range.x + object.segment_offset;
    out.clip_depth = shard.clip_depth + object.clip_offset;
    return out;
}