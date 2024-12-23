use std::sync::Arc;
use std::ops::Deref;
use anyhow::anyhow;
use winit::window::Window;

#[derive(Debug)]
pub struct RenderContext {
    instance: wgpu::Instance,
    devices: Vec<DeviceHandle>,
}

impl RenderContext {
    pub fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        Self {
            instance,
            devices: Vec::new(),
        }
    }

    pub fn get_device_by_id(&self, id: DeviceId) -> &DeviceHandle {
        &self.devices[*id]
    }

    async fn device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<DeviceId> {
        let mut compatible_device = match compatible_surface {
            Some(s) => self
                .devices
                .iter()
                .enumerate()
                .find(|(_, d)| d.adapter.is_surface_supported(s))
                .map(|(index, _)| DeviceId(index)),
            None => (!self.devices.is_empty()).then_some(DeviceId(0, )),
        };
        if compatible_device.is_none() {
            compatible_device = self.new_device(compatible_surface).await;
        }
        compatible_device
    }

    async fn new_device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<DeviceId> {
        let adapter = self.instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
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
        self.devices.push(DeviceHandle {
            adapter,
            device,
            queue
        });
        Some(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceId(usize);

impl Deref for DeviceId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct DeviceHandle {
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl DeviceHandle {
    pub fn create_bind_group_layout<T: LayoutEnum> (&self, label: wgpu::Label<'_>) -> wgpu::BindGroupLayout {
        let entries : Vec<_> = T::entry_iter()
            .map(|t| T::layout_entry(&t))
            .collect();
        self
            .device
            .create_bind_group_layout( &wgpu::BindGroupLayoutDescriptor {
                entries: entries.as_slice(),
                label,
            })
    }

    pub fn create_buffer_with_layout_enum<T: LayoutEnum> (&self, ty: &T, count: u64) -> wgpu::Buffer {
        self
            .device
            .create_buffer(&ty.buffer_descriptor(count))
    }

    /// Creates a bind group using a wgpu layout and a map sending enums to binding resources.
    pub fn create_bind_group_with_enum_layout_map< 'l, 'a, T: LayoutEnum, F>
    (
        &self,
        layout: &wgpu::BindGroupLayout,
        label: wgpu::Label<'l>,
        map: F,
    ) -> wgpu::BindGroup where F: Fn(&T) -> wgpu::BindingResource<'a> {
        let entries: Vec<wgpu::BindGroupEntry> = T::entry_iter()
            .map(|t| wgpu::BindGroupEntry{
                binding: t.binding(),
                resource: map(&t),
            })
            .collect();
        self.device
            .create_bind_group(&wgpu::BindGroupDescriptor{
                label,
                layout,
                entries: entries.as_slice(),
            })
    }
}

#[derive(Debug)]
pub struct RenderTarget<'s, D: TargetTextureDongle> {
    // window must be dropped after surface
    surface: wgpu::Surface<'s>,
    config: wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,

    minimized: bool,
    device_id: DeviceId,

    window: Arc<Window>,

    texture_handler: TargetTextureHandler<D>
}

impl<D: TargetTextureDongle> RenderTarget<'_, D> {
    pub fn surface(&self) -> &wgpu::Surface<'_> { &self.surface }
    pub fn surface_format(&self) -> &wgpu::TextureFormat { &self.format }
    pub fn device_id(&self) -> DeviceId { self.device_id }
    pub fn is_live(&self) -> bool {
        return !self.minimized
    }

    pub fn get_data(&self) -> TargetData {
        TargetData {
            vp_x: 0,
            vp_y: 0,
            vp_width: self.config.width,
            vp_height: self.config.height,
        }
    }

    pub fn window(&self) -> &Window {
        self.window.as_ref()
    }

    pub fn device<'a>(&self, context: &'a RenderContext) -> &'a DeviceHandle {
        context.get_device_by_id(self.device_id)
    }

    pub async fn create<'a, 'b> (context: &'a mut RenderContext, window: Arc<Window>, dongle: D) -> anyhow::Result<RenderTarget<'b, D>> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Err(anyhow!("Cannot create zero size window."))
        }
        let surface_target: wgpu::SurfaceTarget<'b> = window.clone().into();
        let surface: wgpu::Surface<'b> = context.instance.create_surface(surface_target)?;
        let device_id = context.device(Some(&surface)).await.ok_or(anyhow!("No compatible device."))?;

        let surface_caps = surface
            .get_capabilities(&context.get_device_by_id(device_id).adapter);

        let format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        // note surface_caps.formats only supposed to be empty when surface and adapter not compatible
        // so taking first should be ok.

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        Ok(RenderTarget {
            surface,
            config,
            format,
            device_id,
            window,
            minimized: false,
            texture_handler: TargetTextureHandler::new(
                context, dongle, device_id, size.width, size.height
            ),
        })
    }

    pub fn resize(&mut self, context: &RenderContext, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.minimized = false;
            self.configure(context);
            self.texture_handler.refresh(context, self.device_id, size.width, size.height);
        } else {
            self.minimized = true;
        }
    }

    fn configure(&mut self, context: &RenderContext) {
        let device = self.device(context);
        self.surface.configure(&device.device, &self.config);
    }

    pub fn texture_views(&self) -> &Vec<wgpu::TextureView> {
        self.texture_handler.views()
    }
}

#[derive(Debug)]
struct TargetTextureHandler<D: TargetTextureDongle> {
    textures: Vec<wgpu::Texture>,
    views: Vec<wgpu::TextureView>,
    dongle: D,
}

impl<D: TargetTextureDongle> TargetTextureHandler<D> {
    pub fn new(context: &RenderContext, dongle: D, device_id: DeviceId, width: u32, height: u32) -> Self {
        let mut this = Self {
            textures: Vec::new(),
            views: Vec::new(),
            dongle,
        };
        this.refresh(context, device_id, width, height);
        this
    }

    pub fn refresh(&mut self, context: &RenderContext, device_id: DeviceId, width: u32, height: u32) {
        // Trying to drop the old textures first
        self.views = Vec::new();
        self.textures = Vec::new();

        self.textures = (0 .. self.dongle.num_textures())
            .map(|i|
                context
                    .get_device_by_id(device_id)
                    .device
                    .create_texture(&self.dongle.texture_desc(i, width, height))
            )
            .collect();
        self.views = (0 .. self.dongle.num_views())
            .map(|i|
                self
                    .textures[self.dongle.view_index(i)]
                    .create_view(&self.dongle.view_desc(i))
            )
            .collect();
    }

    pub fn views(&self) -> &Vec<wgpu::TextureView> { &self.views }

}

pub trait TargetTextureDongle {
    fn num_textures(&self) -> usize;

    fn num_views(&self) -> usize { self.num_textures() }

    fn texture_desc(&self, index: usize, width: u32, height: u32) -> wgpu::TextureDescriptor;

    /// The texture index associated with a given view.
    fn view_index(&self, index: usize) -> usize { index }

    #[allow(unused_variables)]
    fn view_desc(&self, index: usize) -> wgpu::TextureViewDescriptor { wgpu::TextureViewDescriptor::default() }
}

#[derive(Debug)]
pub struct TargetData {
    pub vp_x: i32,
    pub vp_y: i32,
    pub vp_width: u32,
    pub vp_height: u32,
}

pub trait LayoutEnum {
    type Iter : Iterator<Item = Self>;
    fn entry_iter() -> Self::Iter;
    fn size(&self) -> u64;
    fn binding(&self) -> u32;
    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry;
    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static>;
}
