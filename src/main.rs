use std::sync::Arc;
use std::default::Default;
use winit::{
    event::*,
    event_loop::{EventLoop,ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
    application::ApplicationHandler,
};
use anyhow::{anyhow, Result};
use log::info;
use wgpu;
use pollster;

#[derive(Debug, Default)]
enum App {
    #[default]
    Uninitialized,
    Ready {backend: Backend},
    Destroying,
}
#[derive(Debug)]
struct Backend {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    graphics_queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    size: winit::dpi::PhysicalSize<u32>,
}

impl Backend {
    async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Err(anyhow!("Zero size window. Minimization not supported yet?"))
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor{
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface::<'static>(window.clone())?;
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }
        )
            .await
            .ok_or(anyhow!("Couldn't find compatible adapter"))?;

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(), // if web need to take into account limits
                label: None,
                memory_hints: Default::default(),
            },
            None,
        )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        // note surface_caps.formats only supposed to be empty when surface and adapter not compatible

        let config  = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        Ok(Self {
            window,
            surface,
            device,
            graphics_queue: queue,
            config,
            size,
        })
    }

}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Window resumed/created, creating window");
        if let Self::Uninitialized = self {
            let window = event_loop.create_window(Window::default_attributes()).unwrap();
            let backend = pollster::block_on(Backend::new(Arc::new(window))).unwrap();
            *self = Self::Ready {
                backend
            };
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, shutting down.");
                *self = Self::Destroying;
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
}
fn main() -> Result<()>{
    env_logger::init();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}