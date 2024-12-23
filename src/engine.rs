use std::num::NonZeroU64;
use anyhow::{anyhow, Result};
use crate::model::*;
use crate::render::{DeviceHandle, DeviceId, LayoutEnum, RenderContext, TargetTextureDongle};
use crate::scene::{SceneData, Shard};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

fn pad_to_copy_buffer_alignment(size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1; // 0b11 since copy buffer alignment is 4
    ((size + align_mask) & !align_mask) // round up to nearest aligned
        .max(wgpu::COPY_BUFFER_ALIGNMENT) // make sure it's non-empty
}

#[derive(Debug)]
pub struct RenderEngine {
    render_pipeline: wgpu::RenderPipeline,
    world_uniforms_buffer: wgpu::Buffer,
    origin_uniforms_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    quad_bind_group: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    segment_buffer: wgpu::Buffer,
    vert_bind_group: wgpu::BindGroup,
}

impl RenderEngine {
    pub fn new(context: &RenderContext, device_id: DeviceId, format: &wgpu::TextureFormat) -> RenderEngine {
        let info = check::MODEL_INFO;
        let device = context.get_device_by_id(device_id);
        let shader = device
            .device
            .create_shader_module(
                wgpu::ShaderModuleDescriptor {
                    label: Some("Shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
                }
            );


        let uniform_bind_group_layout = device
            .create_bind_group_layout::<UniformGroup>(Some("Uniform bind group layout"));
        let quad_bind_group_layout = device
            .create_bind_group_layout::<QuadGroup>(Some("Quad bind group layout"));
        let vert_bind_group_layout = device
            .create_bind_group_layout::<VertexGroup>(Some("Vertex bind group layout"));
        let render_pipeline_layout = device
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &quad_bind_group_layout,
                    &vert_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });


        let render_pipeline = device
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main", // name of the main function of the vertex shader
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: format.clone(),
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });


        let world_uniforms_buffer = device
            .create_buffer_with_layout_enum(&UniformGroup::WORLD, 1);
        let origin_uniforms_buffer = device
            .create_buffer_with_layout_enum(&UniformGroup::ORIGIN, ORIGIN_BUFFER_SIZE as u64);
        let uniform_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &uniform_bind_group_layout,
                Some("Uniform bind group"),
                |t| match t {
                    UniformGroup::WORLD => world_uniforms_buffer.as_entire_binding(),
                    UniformGroup::ORIGIN => origin_uniforms_buffer.as_entire_binding(),
                }
            );

        let quad_buffer = device
            .create_buffer_with_layout_enum(&QuadGroup::QUADS, info.requested_quads as u64);
        let quad_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &quad_bind_group_layout,
                Some("Quad bind group"),
                |t| match t {
                    QuadGroup::QUADS => quad_buffer.as_entire_binding(),
                }
            );

        let vertex_buffer = device
            .create_buffer_with_layout_enum(&VertexGroup::VERTEX, info.num_vertices as u64);
        let segment_buffer = device
            .create_buffer_with_layout_enum(&VertexGroup::SEGMENT, info.num_segments as u64);

        let vert_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &vert_bind_group_layout,
                Some("Vertex bind group"),
                |t| match t {
                    VertexGroup::VERTEX => vertex_buffer.as_entire_binding(),
                    VertexGroup::SEGMENT => segment_buffer.as_entire_binding(),
                }
            );
        let model = check::model();
        device
            .queue
            .write_buffer_with(
                &vertex_buffer,
                0,
                wgpu::BufferSize::new((size_of::<Vertex>() * model.vertices.len()) as u64).unwrap(),
            )
            .unwrap()// eventually will move this to loading code, can handle errors after that
            .copy_from_slice(bytemuck::cast_slice(model.vertices.as_slice()));

        device
            .queue
            .write_buffer_with(
                &segment_buffer,
                0,
                wgpu::BufferSize::new((size_of::<Segment>() * model.segments.len()) as u64).unwrap(),
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(model.segments.as_slice()));


        RenderEngine {
            render_pipeline,
            world_uniforms_buffer,
            origin_uniforms_buffer,
            uniform_bind_group,
            quad_bind_group,
            quad_buffer,
            vertex_buffer,
            segment_buffer,
            vert_bind_group,
        }
    }
    pub fn render(&self, device: &DeviceHandle,
                         target_surface_view: &wgpu::TextureView,
                         target_texture_views: &Vec<wgpu::TextureView>,
                         scene_data: &SceneData,
    ) -> Result<()> {
        let info = check::MODEL_INFO;
        if scene_data.objects.len() >= ORIGIN_BUFFER_SIZE as usize {
            return Err(anyhow!("Number of shards exceeds buffer size."));
        }

        let shards: Vec<Shard> = self.get_shards(scene_data);

        if shards.len() >= info.requested_quads {
            return Err(anyhow!("Number of shards exceeds buffer size."));
        }

        let mut encoder = device
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                }
            );
        let ranges = check::model().segment_ranges;
        let mut view = device.queue.write_buffer_with(
            &self.quad_buffer,
            0,
            wgpu::BufferSize::new(QuadGroup::QUADS.size() * shards.len() as u64).unwrap(),
        )
            .ok_or(anyhow!("Unable to get quad buffer view"))?;
        shards
            .iter()
            .enumerate()
            .for_each(|(i, e)| {
                bytemuck::cast_slice_mut(&mut *view)[i] = Quad {
                    bb: e.tex_bb.into(),
                    color: e.color.into(),
                    segment_index_range: [
                        ranges[e.tex_id].start,
                        ranges[e.tex_id].end,
                    ],
                    clip_depth: e.clip_depth,
                    origin_index: e.origin_index as u32,
                }
            });
        drop(view);

        let mut view = device
            .queue
            .write_buffer_with(
                &self.world_uniforms_buffer,
                0,
                wgpu::BufferSize::new(UniformGroup::WORLD.size()).unwrap(),
            )
            .ok_or(anyhow!("Could not write to world uniforms buffer"))?;
        view.copy_from_slice(bytemuck::cast_slice(
            &[Uniforms::get(scene_data)]
        ));
        drop(view);

        let mut view = device
            .queue
            .write_buffer_with(
                &self.origin_uniforms_buffer,
                0,
                wgpu::BufferSize::new(
                    UniformGroup::ORIGIN.size() * scene_data.objects.len() as u64
                ).unwrap()
            ).ok_or(anyhow!("Could not write to origin uniform buffer"))?;
        scene_data
            .objects
            .iter()
            .enumerate()
            .for_each(|(i, o)|
                bytemuck::cast_slice_mut(&mut *view)[i] = Origin {
                    world_tex_tf: o.world_local_tf.into(),
                }
            );
        drop(view);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target_texture_views[0],
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &self.quad_bind_group, &[]);
        render_pass.set_bind_group(2, &self.vert_bind_group, &[]);
        render_pass.draw(0..(shards.len() * 6) as u32, 0..1);
        drop(render_pass);

        device.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }

    fn get_shards(&self, scene_data: &SceneData) -> Vec<Shard> {
        scene_data
            .objects
            .iter()
            .enumerate()
            .map(|(i, _)| [
                Shard {
                    tex_bb: cgmath::Vector4::new(-1.0f32, -1.0f32, 1.0f32, 1.0f32),
                    color: cgmath::Vector4::new(1.0, 0.0, 0.0, 1.0),
                    clip_depth: 1,
                    tex_id: 0,
                    origin_index: i,
                },
                Shard {
                    tex_bb: cgmath::Vector4::new(-0.2f32, 0.2f32, 1.3f32, 1.5f32),
                    color: cgmath::Vector4::new(0.0, 0.0, 1.0, 1.0),
                    clip_depth: 2,
                    tex_id: 1,
                    origin_index: i,
                },
            ])
            .flatten()
            .collect()
    }
}

#[derive(Debug)]
pub struct RenderDongle ();
impl RenderDongle {
    pub fn new() -> Self {Self ()}
}
impl TargetTextureDongle for RenderDongle {
    fn num_textures(&self) -> usize { 1 }

    fn texture_desc(&self, _index: usize, width: u32, height: u32) -> wgpu::TextureDescriptor {
        let depth_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        wgpu::TextureDescriptor {
            label: Some("Depth buffer"),
            size: depth_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }
    }
}

fn create_bind_group_layout_entry_buffer<T: LayoutEnum>(
    this: &T,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: this.binding(),
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(this.size()),
        },
        count: None,
    }
}

#[derive(Debug, Clone, Copy)]
enum UniformGroup {
    WORLD,
    ORIGIN,
}

impl LayoutEnum for UniformGroup {
    type Iter = <[Self; 2] as IntoIterator>::IntoIter;
    fn entry_iter() -> Self::Iter {
        [Self::WORLD, Self::ORIGIN].into_iter()
    }
    fn size(&self) -> u64 {
        pad_to_copy_buffer_alignment(match self {
            Self::WORLD => size_of::<Uniforms>() as u64,
            Self::ORIGIN => size_of::<Origin>() as u64,
        })
    }
    fn binding(&self) -> u32 {
        match self {
            Self::WORLD => 0,
            Self::ORIGIN => 1,
        }
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        match self {
            Self::WORLD => create_bind_group_layout_entry_buffer(
                self,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            ),
            Self::ORIGIN => {
                let mut entry = create_bind_group_layout_entry_buffer(
                    self,
                    wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    wgpu::BufferBindingType::Uniform,
                );
                entry.ty = wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    min_binding_size: wgpu::BufferSize::new(self.size() * ORIGIN_BUFFER_SIZE as u64),
                    has_dynamic_offset: false,
                };
                entry
            },
        }
    }

    fn buffer_descriptor(&self, _count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some(match self {
                Self::WORLD => "World uniform buffer",
                Self::ORIGIN => "Origin uniform buffer",
            }),
            size: match self {
                Self::WORLD => self.size(),
                Self::ORIGIN => self.size() * ORIGIN_BUFFER_SIZE as u64,
            },
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum QuadGroup {
    QUADS,
}
impl LayoutEnum for QuadGroup {
    type Iter = <[Self; 1] as IntoIterator>::IntoIter;

    fn entry_iter() -> Self::Iter {
        [Self::QUADS].into_iter()
    }
    fn size(&self) -> u64 {
        pad_to_copy_buffer_alignment(size_of::<Quad>() as u64)
    }

    fn binding(&self) -> u32 {
        0
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        create_bind_group_layout_entry_buffer(
            self,
            wgpu::ShaderStages::VERTEX,
            wgpu::BufferBindingType::Storage { read_only: true }
        )
    }

    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some("quad buffer"),
            size: self.size() * count,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum VertexGroup {
    VERTEX,
    SEGMENT,
}

impl LayoutEnum for VertexGroup {
    type Iter = <[Self; 2] as IntoIterator>::IntoIter;

    fn entry_iter() -> Self::Iter {
        [Self::VERTEX, Self::SEGMENT].into_iter()
    }
    fn size(&self) -> u64 {
        pad_to_copy_buffer_alignment(match self {
            Self::VERTEX => size_of::<Vertex>() as u64,
            Self::SEGMENT => size_of::<Segment>() as u64,
        })
    }
    fn binding(&self) -> u32 {
        match self {
            Self::VERTEX => 0,
            Self::SEGMENT => 1,
        }
    }
    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        match self {
            Self::VERTEX => create_bind_group_layout_entry_buffer(
                self,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Storage { read_only: true }
            ),
            Self::SEGMENT => create_bind_group_layout_entry_buffer(
                self,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Storage { read_only: true }
            ),
        }
    }

    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some(match self {
                Self::VERTEX => "Vertex storage buffer",
                Self::SEGMENT => "Segment storage buffer",
            }),
            size: self.size() * count,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}
