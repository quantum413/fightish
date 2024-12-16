
pub struct SceneData {
    pub vp_x: i32,
    pub vp_y: i32,
    pub vp_width: u32,
    pub vp_height: u32,

    pub camera_scale: f32, // might want to replace this with a view matrix.

    // pub shards: Vec<Shard>,
}

pub struct Shard {
    pub world_tex_tf: cgmath::Matrix4<f32>,
    pub tex_bb: cgmath::Vector4<f32>, // x, y, x, y
    pub color: cgmath::Vector4<f32>,
    pub depth: f32,
}