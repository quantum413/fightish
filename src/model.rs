use cgmath::SquareMatrix;
use crate::buffer_structs::{ModelFrame, ModelSegment, ModelShard, ModelVertex};

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
}

pub mod check {
    use crate::buffer_structs::FrameInfo;
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
    };
}