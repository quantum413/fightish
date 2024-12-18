pub struct SceneData {
    pub vp_x: i32,
    pub vp_y: i32,
    pub vp_width: u32,
    pub vp_height: u32,

    pub camera_tf: cgmath::Matrix4<f32>,

    pub objects: Vec<Object>
}

pub struct Object {
    pub world_local_tf: cgmath::Matrix4<f32>,
    pub model_id: usize,
}

pub struct Shard {
    pub tex_bb: cgmath::Vector4<f32>, // x, y, x, y
    pub color: cgmath::Vector4<f32>,
    pub clip_depth: u32,
    pub tex_id: usize,
    pub origin_index: usize,
}