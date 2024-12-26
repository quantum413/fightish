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
    pub frame_index: i32,
}