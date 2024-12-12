pub struct SceneData {
    pub vp_x: i32,
    pub vp_y: i32,
    pub vp_width: u32,
    pub vp_height: u32,

    pub camera_scale: f32, // might want to replace this with a view matrix.
}
