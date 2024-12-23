use cgmath::SquareMatrix;
use crate::scene::{SceneData, Shard};
use std::ops::Range;

pub const ORIGIN_BUFFER_SIZE: u32 = 5;


// ideally one wouldn't waste memory on having a cpu copy of the model.
// so this is a simple stupid placeholder storage format
pub struct Model {
    pub vertices: Vec<Vertex>,
    pub segments: Vec<Segment>,
    pub segment_ranges: Vec<Range<i32>>,
    pub shards: Vec<Shard>,
    pub frames: Vec<Range<usize>>,
}

pub struct ModelInfo {
    pub num_vertices: usize,
    pub num_segments: usize,
    pub requested_quads: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    // note even though only really using 2+1D transformations, the alignments on vec3's are a real pain.
    pub clip_world_tf: [[f32; 4]; 4], // tf from world coordinates to clip coordinates (for bb purposes)
    pub world_frag_tf: [[f32; 4]; 4], // tf from fragment coordinates to world coordinates.
}

impl Uniforms {
    pub fn get(scene_data: &SceneData) -> Self {
        let clip_frag_tf = // scaled -1 to +1 (clip coords)
            cgmath::Matrix4::from_translation(cgmath::vec3(-1f32, 1f32, 0f32))
                * // scaled from 0 to +2 for x and -2 to 0 for y
                cgmath::Matrix4::from_nonuniform_scale(
                    2f32 / scene_data.vp_width as f32,
                    -2f32 / scene_data.vp_height as f32,
                    1f32,
                )
                * // scaled from 0 to width/height
                cgmath::Matrix4::from_translation(cgmath::vec3(
                    -scene_data.vp_x as f32,
                    -scene_data.vp_y as f32,
                    0f32,
                )); // scaled from vp_x/y to width + vp_x / height + vp_y

        let world_clip_tf = scene_data.camera_tf;

        Self {
            clip_world_tf: world_clip_tf.invert().unwrap().into(),
            world_frag_tf: (world_clip_tf * clip_frag_tf).into(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Origin {
    pub world_tex_tf: [[f32; 4]; 4]
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    pub bb: [f32; 4],
    pub color: [f32; 4],
    pub segment_index_range: [i32; 2],
    pub clip_depth: u32,
    pub origin_index: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pos: [f32; 2]
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Segment {
    pub idx: [i32; 4] // making this signed in case using negative values for special cases later
}
pub mod check {
    use super::*;

    pub const VERTICES: &[Vertex] = &[
        Vertex { pos: [0.0, 0.0] },
        Vertex { pos: [0.5, 1.0] },
        Vertex { pos: [-0.5, 0.5] },
        Vertex { pos: [0.0, -0.5] },
        Vertex { pos: [0.2, 1.0] },
    ];
    pub const SEGMENTS: &[Segment] = &[
        Segment { idx: [0, 2, -1, -1] },
        Segment { idx: [2, 0, 3, -1] },
        Segment { idx: [3, 1, -1, -1] },
        Segment { idx: [1, 0, -1, -1] },
        Segment { idx: [0, 1, -1, -1] },
        Segment { idx: [1, 4, -1, -1] },
        Segment { idx: [4, 0, -1, -1] },
    ];
    pub const SEGMENT_INDEX_RANGES: &[Range<i32>] = &[0..4, 4..7];

    pub fn model() -> Model { Model {
        vertices: Vec::from(VERTICES),
        segments: Vec::from(SEGMENTS),
        segment_ranges: Vec::from(SEGMENT_INDEX_RANGES),
        shards: vec![
            Shard {
                tex_bb: cgmath::Vector4::new(-1.0f32, -1.0f32, 1.0f32, 1.0f32),
                color: cgmath::Vector4::new(1.0, 0.0, 0.0, 1.0),
                clip_depth: 1,
                tex_id: 0,
                origin_index: 0,
            },
            Shard {
                tex_bb: cgmath::Vector4::new(-0.2f32, 0.2f32, 1.3f32, 1.5f32),
                color: cgmath::Vector4::new(0.0, 0.0, 1.0, 1.0),
                clip_depth: 2,
                tex_id: 1,
                origin_index: 0,
            },],
        frames: vec![0..2],
    }}

    pub const MODEL_INFO: ModelInfo = ModelInfo {
        num_vertices: VERTICES.len(),
        num_segments: SEGMENTS.len(),
        requested_quads: 15,
    };
}