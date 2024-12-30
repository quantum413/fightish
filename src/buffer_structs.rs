use cgmath::SquareMatrix;
use crate::scene::SceneData;

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
    pub pos: [f32; 2]
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
