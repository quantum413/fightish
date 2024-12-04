use std::sync::Arc;
use std::default::Default;
use std::ops::Deref;
use winit::{
    event::*,
    event_loop::{EventLoop,ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
    application::ApplicationHandler,
};
use anyhow::{anyhow, Result};
use log::{info, LevelFilter, warn};
use wgpu;
use pollster;

#[derive(Debug)]
struct App<'s> {
    target: Option<RenderTarget<'s>>,
    context: RenderContext,
}

impl App<'_> {
    fn new() -> Self {
        Self {
            target: None,
            context: RenderContext::new(),
        }
    }
}

#[derive(Debug)]
struct RenderContext {
    instance: wgpu::Instance,
    devices: Vec<DeviceHandle>,
}

impl RenderContext {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor{
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        Self {
            instance,
            devices: Vec::new(),
        }
    }

    async fn create_target<'a, 'b> (&'a mut self, window: Arc<Window>) -> Result<RenderTarget<'b>> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Err(anyhow!("Zero size window."))
        }
        let surface_target: wgpu::SurfaceTarget<'b> = window.clone().into();
        let surface: wgpu::Surface<'b> = self.instance.create_surface(surface_target)?;
        let device_id = self.device(Some(&surface)).await.ok_or(anyhow!("No compatible device."))?;

        let surface_caps = surface.get_capabilities(&self.devices[*device_id].adapter);

        let format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        // note surface_caps.formats only supposed to be empty when surface and adapter not compatible
        // so taking first should be ok.

        let config  = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        Ok(RenderTarget{
            surface,
            config,
            format,
            device_id,
            window,
            minimized: false,
        })
    }

    async fn device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<DeviceId> {
        let mut compatible_device = match compatible_surface {
            Some(s) => self
                .devices
                .iter()
                .enumerate()
                .find(|(_, d) | d.adapter.is_surface_supported(s))
                .map(|(index, _)| DeviceId(index)),
            None => (!self.devices.is_empty()).then_some(DeviceId(0,)),
        };
        if compatible_device.is_none() {
            compatible_device = self.new_device(compatible_surface).await;
        }
        compatible_device
    }

    async fn new_device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<DeviceId> {
        let adapter = self.instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface,
                force_fallback_adapter: false,
            }
        )
            .await?;

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(), // if web need to take into account limits
                label: None,
                memory_hints: Default::default(),
            },
            None,
        )
            .await.ok()?;
        let id = DeviceId(self.devices.len());
        self.devices.push(DeviceHandle{
            adapter,
            device,
            queue
        });
        Some(id)
    }
    fn resize_surface (&self, target: &mut RenderTarget, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            target.config.width = size.width;
            target.config.height = size.height;
            target.minimized = false;
            self.configure_surface(target);
        } else {
            target.minimized = true;
        }
    }

    fn configure_surface (&self, target: &mut RenderTarget) {
        let device = &self.devices[*target.device_id];
        target.surface.configure(&device.device, &target.config);
    }

}

#[derive(Debug)]
struct RenderTarget<'s> {
    // window must be dropped after surface
    surface: wgpu::Surface<'s>,
    config: wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,

    minimized: bool,
    device_id: DeviceId,

    window: Arc<Window>,
}

impl RenderTarget<'_> {
    fn is_live (&self) -> bool {
        return !self.minimized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceId(usize);

impl Deref for DeviceId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct DeviceHandle {
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}



impl App<'_> {
    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.target.as_mut().map(
            |t| self.context.resize_surface(t, size)
        );
    }

    fn render(&mut self) -> Result<()> {
        // if let Self::Ready {backend} = self{
        //     if backend.minimized { return Ok(()) }
        //     let output = backend.surface.get_current_texture();
        //
        //     Ok(())
        // }
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Window resumed/created, creating window");
        assert!(self.target.is_none(), "Suspending and resuming are not supported.");
        let window = event_loop.create_window(Window::default_attributes()).unwrap();
        let target = pollster::block_on(self.context.create_target(Arc::new(window))).unwrap();
        self.target = Some(target)
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
            _ => {}
        }
    }
}
fn main() -> Result<()>{
    env_logger::builder()
        .filter(Some("wgpu_hal"), LevelFilter::Warn)
        .filter(Some("wgpu_core"), LevelFilter::Warn)
        .init();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}