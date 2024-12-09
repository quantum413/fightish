use std::sync::Arc;
use std::default::Default;
use std::error::Error;
use std::fmt::{Display, Formatter};
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
use wgpu::util::DeviceExt;
use pollster;
use bytemuck;

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
            return Err(anyhow!("Cannot create zero size window."))
        }
        let surface_target: wgpu::SurfaceTarget<'b> = window.clone().into();
        let surface: wgpu::Surface<'b> = self.instance.create_surface(surface_target)?;
        let device_id = self.device(Some(&surface)).await.ok_or(anyhow!("No compatible device."))?;

        let surface_caps = surface
            .get_capabilities(&self.get_device_by_id(device_id).adapter);

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

    fn get_device_by_id(&self, id: DeviceId) -> &DeviceHandle {
        &self.devices[*id]
    }

    fn get_target_device(&self, target: &RenderTarget) -> &DeviceHandle {
        self.get_device_by_id(target.device_id)
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
        let device = self.get_device_by_id(target.device_id);
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBS: &'static [wgpu::VertexAttribute; 2] = &[
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: size_of::<[f32; 3]>() as wgpu::BufferAddress,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x4,
        }
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: Self::ATTRIBS,
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex { position: [0.0, 0.5, 0.0], color: [1.0, 0.0, 0.0, 1.0] },
    Vertex { position: [-0.5, -0.5, 0.0], color: [0.0, 1.0, 0.0, 1.0] },
    Vertex { position: [0.5, -0.5, 0.0], color: [0.0, 0.0, 1.0, 1.0] },
];
#[derive(Debug)]
struct RenderEngine {
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
}

// trash man version of !
#[derive(Debug)]
enum Never {}

impl Display for Never {
    fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
        unreachable!()
    }
}

impl Error for Never {

}

impl RenderEngine {
    fn new(context: &RenderContext, device_id: DeviceId, format: wgpu::TextureFormat) -> RenderEngine {
        let device = context.get_device_by_id(device_id);
        let shader = device
            .device
            .create_shader_module(
                wgpu::ShaderModuleDescriptor {
                    label: Some("Shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
                }
            );
        let render_pipeline_layout = device
            .device
            .create_pipeline_layout( &wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
        let render_pipeline = device
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor{
                label: Some("Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main", // name of the main function of the vertex shader
                    buffers: &[
                        Vertex::desc(),
                    ],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        let vertex_buffer = device.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        RenderEngine {
            render_pipeline,
            vertex_buffer,
        }
    }
    fn render(&self, device: &DeviceHandle, target_view: &wgpu::TextureView) -> Result<(), Never> {
        let mut encoder = device
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                }
            );

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.2,
                        g: 0.2,
                        b: 0.2,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..3, 0..1);
        drop(render_pass);

        device.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
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

#[derive(Debug)]
struct App<'s> {
    target: Option<RenderTarget<'s>>,
    context: RenderContext,
    engine: Option<RenderEngine>,
}

impl App<'_> {
    fn new() -> Self {
        Self {
            target: None,
            context: RenderContext::new(),
            engine: None,
        }
}
    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.target.as_mut().map(
            |t| self.context.resize_surface(t, size)
        );
    }

    fn render(&mut self) -> Result<()> {
        if let Some(target) = self.target.as_ref() {
            target.window.request_redraw();
            if !target.is_live() { return Ok(()); }
            let output = target.surface.get_current_texture()?;
            let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.engine.as_ref().ok_or(anyhow!("Cannot render: engine missing."))?.render(
                self.context.get_target_device(target),
                &view,
            )?;

            output.present();
        }
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Window resumed/created, creating window");
        assert!(self.target.is_none(), "Suspending and resuming are not supported.");
        let window = event_loop.create_window(Window::default_attributes()).unwrap();
        let target = pollster::block_on(self.context.create_target(Arc::new(window))).unwrap();
        self.engine = Some(RenderEngine::new(&self.context, target.device_id, target.format));
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