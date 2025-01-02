use std::iter;
use crate::buffer_structs::{FrameInfo, ModelFrame, ModelGroup, ModelSegment, ModelShard, ModelVertex};
use crate::render::{DeviceHandle, LayoutEnum};
use rand::prelude::*;
use log::*;

// ideally one wouldn't waste memory on having a cpu copy of the model.
// so this is a simple stupid placeholder storage format
#[derive(Debug)]
pub struct Model {
    pub vertices: Vec<ModelVertex>,
    pub segments: Vec<ModelSegment>,
    pub shards: Vec<ModelShard>,
    pub frames: Vec<ModelFrame>,
}

#[derive(Debug)]
pub struct SimpleLoader {
    model: Model,
    frame_info: Vec<FrameInfo>,
    bind_group: Option<wgpu::BindGroup>,
}

impl SimpleLoader {
    pub fn new(model: Model) -> Self {
        let frame_info = model
            .frames
            .iter()
            .map(|f| {
                if f.shard_range[0] == f.shard_range[1] {return FrameInfo{..Default::default()}}
                FrameInfo {
                    clip_size: (f.shard_range[0] .. f.shard_range[1])
                        .map(|i| model.shards[i as usize].clip_depth)
                        .max().unwrap() + 1,
                    shard_size: (f.shard_range[1] - f.shard_range[0]) as u32,
                    segment_size: (f.segment_range[1] - f.segment_range[0]) as u32,
                }
            }).collect();
        info!(
            "Model information:\n# Frames: {}\n# Shards: {}\n# Segments: {}\n# Vertices: {}",
            model.frames.len(),
            model.shards.len(),
            model.segments.len(),
            model.vertices.len(),
        );
        Self {
            model,
            frame_info,
            bind_group: None,
        }
    }

    pub fn frame_info(&self) -> &Vec<FrameInfo> {
        &self.frame_info
    }

    pub fn load(&mut self, device: &DeviceHandle) {

        let vertex_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Vertex, self.model.vertices.len() as u64);
        let segment_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Segment, self.model.segments.len() as u64);
        let shard_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Shard, self.model.shards.len() as u64);
        let frame_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Frame, self.model.frames.len() as u64);
        self.bind_group = Some(device
            .create_bind_group_with_enum_layout_map(
                &device.create_bind_group_layout::<ModelGroup>(Some("Model bind group layout")),
                Some("Model bind group"),
                |t| match t {
                    ModelGroup::Vertex => vertex_model_buffer.as_entire_binding(),
                    ModelGroup::Segment => segment_model_buffer.as_entire_binding(),
                    ModelGroup::Shard => shard_model_buffer.as_entire_binding(),
                    ModelGroup::Frame => frame_model_buffer.as_entire_binding(),
                }
            ));

        device
            .queue
            .write_buffer_with(
                &vertex_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Vertex.size() * self.model.vertices.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(self.model.vertices.as_slice()));
        device
            .queue
            .write_buffer_with(
                &segment_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Segment.size() * self.model.segments.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(self.model.segments.as_slice()));
        device
            .queue
            .write_buffer_with(
                &shard_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Shard.size() * self.model.shards.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(self.model.shards.as_slice()));
        device
            .queue
            .write_buffer_with(
                &frame_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Frame.size() * self.model.frames.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(self.model.frames.as_slice()));
    }

    pub fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref()
    }
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

    pub fn model() -> Model { Model {
        vertices: Vec::from(VERTICES),
        segments: Vec::from(SEGMENTS),
        shards: Vec::from(SHARDS),
        frames: Vec::from(FRAMES),
    }}
}

pub fn make_load_test(
    num_frames: u32,
    num_frame_shards: std::ops::Range<u32>,
    num_shard_segments: std::ops::Range<u32>,
) -> Model {
    let mut rng = StdRng::from_seed(b"hflkajafdsahlvbsdfhqueesaydailay".clone());
    let mut vertices:  Vec<ModelVertex> = Vec::new();
    let mut segments: Vec<ModelSegment> = Vec::new();
    let mut shards: Vec<ModelShard> = Vec::new();
    let mut frames: Vec<ModelFrame> = Vec::new();

    // let mut frame_segment_offset: i32 = 0;
    // let mut frame_shard_offset: i32 = 0;
    for frame in 0..num_frames {
        let frame_segment_offset = segments.len() as i32;
        let frame_shard_offset = shards.len() as i32;
        let num_shards = rng.gen_range(num_frame_shards.clone());
        info!("num shards for frame {}: {}", frame, num_shards);
        for shard in 0..num_shards {
            let shard_segment_offset = segments.len() as i32;
            let num_segments = rng.gen_range(num_shard_segments.clone());
            info!("num segments for shard {} in frame {} : {}", shard, frame, num_segments);
            if num_segments == 0 {
                shards.push(ModelShard{
                    bb: [0., 0., 0., 0.],
                    color: [0., 0., 0., 1.],
                    segment_range: [shard_segment_offset, shard_segment_offset],
                    clip_depth: shard,
                    filler: 0,
                });
                continue;
            }
            let mut control_vertices: Vec<ModelVertex> = (0..num_segments)
                .map(|_| ModelVertex {
                    pos: [rng.gen::<f32>() - 0.5, rng.gen::<f32>() - 0.5],
                })
                .collect();
            let mut corner_vertices: Vec<ModelVertex> = (0..(num_segments as usize - 1))
                .map(|i| ModelVertex{
                    pos: [
                        (control_vertices[i].pos[0] + control_vertices[i + 1].pos[0]) / 2.,
                        (control_vertices[i].pos[1] + control_vertices[i + 1].pos[1]) / 2.,
                    ]
                })
                .chain(iter::once(ModelVertex{
                    pos: [
                        (control_vertices[0].pos[0] + control_vertices[num_segments as usize - 1].pos[0]) / 2.,
                        (control_vertices[0].pos[1] + control_vertices[num_segments as usize - 1].pos[1]) / 2.,
                    ]
                }))
                .collect();
            let vertex_offset = vertices.len() as i32;
            vertices.append(&mut control_vertices);
            let corner_offset = vertices.len() as i32;
            vertices.append(&mut corner_vertices);
            let mut shard_segments : Vec<_> = iter::once(ModelSegment {
                    idx: [
                        corner_offset + num_segments as i32 - 1,
                        corner_offset,
                        vertex_offset,
                        -1,
                    ]
                })
                .chain((1..num_segments as i32).map(|i| ModelSegment {
                    idx: [
                        corner_offset + i - 1,
                        corner_offset + i,
                        vertex_offset + i,
                        -1,
                    ]
                }))
                .collect();
            segments.append(&mut shard_segments);
            shards.push(ModelShard {
                bb: [-0.5, -0.5, 0.5, 0.5],
                color: [rng.gen(), rng.gen(), rng.gen(), 1.0],
                segment_range: [shard_segment_offset, segments.len() as i32],
                clip_depth: shard,
                filler: 0,
            })
        }
        frames.push(ModelFrame {
            shard_range: [frame_shard_offset, shards.len() as i32],
            segment_range: [frame_segment_offset, segments.len() as i32],
        });
    }
    Model {
        vertices,
        segments,
        shards,
        frames,
    }
}
