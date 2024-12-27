use cgmath::SquareMatrix;
use crate::scene::{SceneData};


// ideally one wouldn't waste memory on having a cpu copy of the model.
// so this is a simple stupid placeholder storage format
pub struct Model {
    pub vertices: Vec<ModelVertex>,
    pub segments: Vec<ModelSegment>,
    pub shards: Vec<ModelShard>,
    pub frames: Vec<ModelFrame>,
}

pub struct ModelInfo {
    pub num_vertices: usize,
    pub num_segments: usize,
    pub num_shards: usize,
    pub num_frames: usize,
    pub requested_frame_shards: usize,
    pub requested_frame_segments: usize,
    pub requested_scene_objects: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    // note even though only really using 2+1D transformations, the alignments on vec3's are a real pain.
    pub clip_world_tf: [[f32; 4]; 4], // tf from world coordinates to clip coordinates (for bb purposes)
    pub frag_clip_tf: [[f32; 4]; 4], // tf from fragment coordinates to world coordinates.
}

impl Uniforms {
    pub fn get(scene_data: &SceneData) -> Self {
        let frag_clip_tf = // frag coords scaled from vp_x/y to width + vp_x / height + vp_y;
                cgmath::Matrix4::from_translation(cgmath::vec3(
                    scene_data.vp_x as f32,
                    scene_data.vp_y as f32,
                    0f32,
                ))
            * // scaled from 0 to width/height
                cgmath::Matrix4::from_nonuniform_scale(
                    scene_data.vp_width as f32 / 2.0,
                    -(scene_data.vp_height as f32 / 2.0),
                    1f32,
                )
            * // scaled from 0 to +2 for x and -2 to 0 for y
            cgmath::Matrix4::from_translation(cgmath::vec3(1f32, -1f32, 0f32))
            ; // scaled -1 to +1 (clip coords)

        let world_clip_tf = scene_data.camera_tf;

        Self {
            clip_world_tf: world_clip_tf.invert().unwrap().into(),
            frag_clip_tf: frag_clip_tf.into(),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pos: [f32; 2]
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelSegment {
    pub idx: [i32; 4] // making this signed in case using negative values for special cases later
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelShard {
    pub bb: [f32; 4],
    pub color: [f32; 4],
    pub segment_range: [i32; 2],
    pub clip_depth: u32,
    pub filler: u32,
}


#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelFrame {
    pub shard_range: [i32; 2],
    pub segment_range: [i32; 2],
}

pub struct FrameInfo {
    pub clip_size: u32,
    pub shard_size: u32,
    pub segment_size: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameObject {
    pub world_tex_tf: [[f32; 4]; 4],
    pub frame_index: i32,
    pub clip_offset: u32,
    pub shard_offset: i32,
    pub segment_offset: i32,
}

pub mod check {
    use super::*;

    pub const VERTICES: &[ModelVertex] = &[
        ModelVertex { pos: [0.0, 0.0] },
        ModelVertex { pos: [0.5, 1.0] },
        ModelVertex { pos: [-0.5, 0.5] },
        ModelVertex { pos: [0.0, -0.5] },
        ModelVertex { pos: [0.2, 1.0] },
    ];
    pub const SEGMENTS: &[ModelSegment] = &[
        ModelSegment { idx: [0, 2, -1, -1] },
        ModelSegment { idx: [2, 3, 0, -1] },
        ModelSegment { idx: [3, 1, -1, -1] },
        ModelSegment { idx: [1, 0, -1, -1] },
        ModelSegment { idx: [0, 1, -1, -1] },
        ModelSegment { idx: [1, 4, -1, -1] },
        ModelSegment { idx: [4, 0, -1, -1] },
    ];
    // pub const SEGMENT_INDEX_RANGES: &[Range<i32>] = &[0..4, 4..7];
    pub const SHARDS: &[ModelShard] = &[
        ModelShard {
            bb: [-1.0f32, -1.0f32, 1.0f32, 1.0f32],
            color: [1.0, 0.0, 0.0, 1.0],
            segment_range: [0, 4],
            clip_depth: 0,
            filler: 0,
        },
        ModelShard {
            bb: [-0.2f32, 0.2f32, 1.3f32, 1.5f32],
            color: [0.0, 0.0, 1.0, 1.0],
            segment_range: [4, 7],
            clip_depth: 1,
            filler: 0,
        },];

    pub const FRAMES: &[ModelFrame] = &[
        ModelFrame {
            shard_range: [0, 2],
            segment_range: [0, 7],
        }
    ];

    pub const FRAME_INFO: &[FrameInfo] = &[
        FrameInfo {
            clip_size: 2,
            shard_size: 2,
            segment_size: 7,
        }
    ];

    pub fn model() -> Model { Model {
        vertices: Vec::from(VERTICES),
        segments: Vec::from(SEGMENTS),
        shards: Vec::from(SHARDS),
        frames: Vec::from(FRAMES),
    }}

    pub const MODEL_INFO: ModelInfo = ModelInfo {
        num_vertices: VERTICES.len(),
        num_segments: SEGMENTS.len(),
        num_shards: SHARDS.len(),
        num_frames: FRAMES.len(),
        requested_frame_shards: 15,
        requested_frame_segments: 20,
        requested_scene_objects: 5,
    };
}