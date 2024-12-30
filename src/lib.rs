use anyhow::anyhow;
use std::sync::Arc;
use winit::window::{Window, WindowId};
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
use winit::event::{KeyEvent, WindowEvent};
use log::{info, warn};

use cgmath::SquareMatrix;

mod scene;
mod render;
mod engine;
mod model;
mod buffer_structs;

use scene::SceneData;
use render::{
    RenderContext,
    RenderTarget,
    TargetData,
};
use engine::{RenderEngine, RenderDongle};
#[derive(Debug)]
struct AppState {
    scale: f32,
    pos: cgmath::Vector2<f32>,
    rot: f32,
}

impl AppState {
    fn new() -> Self {
        Self {
            scale: 1.0f32,
            pos: cgmath::Zero::zero(),
            rot: 0.0,
        }
    }

    fn create_scene_data(&self, target_data: &TargetData) -> SceneData {
        let camera_tf = // world
            cgmath::Matrix4::from_translation(
                cgmath::Vector3::new(self.pos.x, self.pos.y, 0.0)
            )
            * // scaled, untranslated
            cgmath::Matrix4::from_nonuniform_scale(
                target_data.vp_width as f32 / target_data.vp_height as f32 * self.scale,
                self.scale,
                1f32,
            ); // clip coords
        let object_tf = cgmath::Matrix4::from_angle_z(cgmath::Rad(self.rot));
        SceneData {
            vp_x: target_data.vp_x,
            vp_y: target_data.vp_y,
            vp_width: target_data.vp_width,
            vp_height: target_data.vp_height,

            camera_tf,

            objects: vec![
                scene::Object{world_local_tf: object_tf, frame_index: 0},
                scene::Object{world_local_tf: object_tf.invert().unwrap(), frame_index: 0},
            ]
        }
    }

    fn handle_input(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                    ..
                },
                ..
            } => {
                match keycode {
                    winit::keyboard::KeyCode::KeyQ => { self.scale *= 1.1 },
                    winit::keyboard::KeyCode::KeyE => { self.scale *= 0.9 },
                    winit::keyboard::KeyCode::KeyW => { self.pos.y += self.scale * 0.1 },
                    winit::keyboard::KeyCode::KeyA => { self.pos.x -= self.scale * 0.1 },
                    winit::keyboard::KeyCode::KeyS => { self.pos.y -= self.scale * 0.1 },
                    winit::keyboard::KeyCode::KeyD => { self.pos.x += self.scale * 0.1 },
                    winit::keyboard::KeyCode::KeyZ => { self.rot += 0.1 },
                    winit::keyboard::KeyCode::KeyC => { self.rot -= 0.1 },
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct App<'s> {
    target: Option<RenderTarget<'s, RenderDongle>>,
    context: RenderContext,
    engine: Option<RenderEngine>,
    state: AppState,
}

impl App<'_> {
    pub fn new() -> Self {
        Self {
            target: None,
            context: RenderContext::new(),
            engine: None,
            state: AppState::new(),
        }
    }
    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.target.as_mut().map(
            |t| t.resize(&self.context, size)
        );
    }

    fn render(&mut self) -> anyhow::Result<()> {
        if let Some(target) = self.target.as_ref() {
            if !target.is_live() { return Ok(()); }
            let output = target.surface().get_current_texture()?;
            let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.engine.as_mut().ok_or(anyhow!("Cannot render: engine missing."))?.render(
                target.device(&self.context),
                &view,
                &target.texture_views(),
                &self.state.create_scene_data(&target.get_data())
            )?;
            output.present();

            target.window().request_redraw();
        }
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Window resumed/created, creating window");
        assert!(self.target.is_none(), "Suspending and resuming are not supported.");
        let window = event_loop.create_window(Window::default_attributes()).unwrap();
        let target = pollster::block_on(RenderTarget::create(&mut self.context, Arc::new(window), RenderDongle::new())).unwrap();
        self.engine = Some(RenderEngine::new(&self.context, target.device_id(), target.surface_format()));
        self.target = Some(target);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, shutting down.");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.render().err().map(|e| warn!("{e}"));
            }
            WindowEvent::Resized(size) => {
                self.resize(size);
            }
            _ => {self.state.handle_input(event);}
        }
    }
}
