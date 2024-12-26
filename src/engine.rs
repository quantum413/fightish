use std::num::NonZeroU64;
use anyhow::{anyhow, Result};
use crate::model::*;
use crate::render::{DeviceHandle, DeviceId, LayoutEnum, RenderContext, TargetTextureDongle};
use crate::scene::{SceneData};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

fn pad_to_copy_buffer_alignment(size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1; // 0b11 since copy buffer alignment is 4
    ((size + align_mask) & !align_mask) // round up to nearest aligned
        .max(wgpu::COPY_BUFFER_ALIGNMENT) // make sure it's non-empty
}

#[derive(Debug)]
pub struct RenderEngine {
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,

    world_uniforms_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    shard_vertex_frame_buffer: wgpu::Buffer,
    segment_frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    frame_read_bind_group: wgpu::BindGroup,

    vertex_model_buffer: wgpu::Buffer,
    segment_model_buffer: wgpu::Buffer,
    shard_model_buffer: wgpu::Buffer,
    frame_model_buffer: wgpu::Buffer,
    model_bind_group: wgpu::BindGroup,

    object_scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
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

        let compute_shader = device
            .device
            .create_shader_module(
                wgpu::ShaderModuleDescriptor {
                    label: Some("Frame preprocessing compute shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("frame_preprocess.wgsl").into())
                }
            );


        let uniform_bind_group_layout = device
            .create_bind_group_layout::<UniformGroup>(Some("Uniform bind group layout"));
        let frame_bind_group_layout = device
            .create_bind_group_layout::<FrameGroup>(Some("Frame bind group layout"));
        let frame_read_bind_group_layout = device
            .device
            .create_bind_group_layout( &wgpu::BindGroupLayoutDescriptor {
                entries:
                &[
                    create_bind_group_layout_entry_buffer(
                        &FrameGroup::Segment,
                        wgpu::ShaderStages::FRAGMENT,
                        wgpu::BufferBindingType::Storage {read_only: true,}
                    ),
                    create_bind_group_layout_entry_buffer(
                        &FrameGroup::ShardVertex,
                        wgpu::ShaderStages::VERTEX,
                        wgpu::BufferBindingType::Storage {read_only: true,}
                    ),
                ],
                label: Some("Frame read bind group layout")
            });
        let model_bind_group_layout = device
            .create_bind_group_layout::<ModelGroup>(Some("Model bind group layout"));
        let scene_bind_group_layout = device
            .create_bind_group_layout::<SceneGroup>(Some("Object bind group layout"));
        let render_pipeline_layout = device
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &frame_read_bind_group_layout,
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

        let compute_pipeline_layout = device
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{
                label: Some("Compute pipeline layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &frame_bind_group_layout,
                    &model_bind_group_layout,
                    &scene_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor{
                label: Some("Compute pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &compute_shader,
                entry_point: "main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        let world_uniforms_buffer = device
            .create_buffer_with_layout_enum(&UniformGroup::World, 1);
        let uniform_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &uniform_bind_group_layout,
                Some("Uniform bind group"),
                |t| match t {
                    UniformGroup::World => world_uniforms_buffer.as_entire_binding(),
                }
            );

        let segment_frame_buffer = device
            .create_buffer_with_layout_enum(&FrameGroup::Segment, info.requested_frame_segments as u64);
        let shard_vertex_frame_buffer = device
            .create_buffer_with_layout_enum(&FrameGroup::ShardVertex, info.requested_frame_shards as u64 * 6);
        let frame_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &frame_bind_group_layout,
                Some("Frame bind group"),
                |t| match t {
                    FrameGroup::Segment => segment_frame_buffer.as_entire_binding(),
                    FrameGroup::ShardVertex => shard_vertex_frame_buffer.as_entire_binding(),
                }
            );
        let frame_read_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &frame_read_bind_group_layout,
                Some("Frame read bind group"),
                |t| match t {
                    FrameGroup::Segment => segment_frame_buffer.as_entire_binding(),
                    FrameGroup::ShardVertex => shard_vertex_frame_buffer.as_entire_binding(),
                }
            );

        let vertex_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Vertex, info.num_vertices as u64);
        let segment_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Segment, info.num_segments as u64);
        let shard_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Shard, info.num_shards as u64);
        let frame_model_buffer = device
            .create_buffer_with_layout_enum(&ModelGroup::Frame, info.num_frames as u64);
        let model_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &model_bind_group_layout,
                Some("Model bind group"),
                |t| match t {
                    ModelGroup::Vertex => vertex_model_buffer.as_entire_binding(),
                    ModelGroup::Segment => segment_model_buffer.as_entire_binding(),
                    ModelGroup::Shard => shard_model_buffer.as_entire_binding(),
                    ModelGroup::Frame => frame_model_buffer.as_entire_binding(),
                }
            );

        let object_scene_buffer = device
            .create_buffer_with_layout_enum(&SceneGroup::Object, info.requested_scene_objects as u64);
        let scene_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &scene_bind_group_layout,
                Some("Scene bind group"),
                |t| match t {
                    SceneGroup::Object => object_scene_buffer.as_entire_binding(),
                }
            );

        let model = check::model();
        device
            .queue
            .write_buffer_with(
                &vertex_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Vertex.size() * model.vertices.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(model.vertices.as_slice()));
        device
            .queue
            .write_buffer_with(
                &segment_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Segment.size() * model.segments.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(model.segments.as_slice()));
        device
            .queue
            .write_buffer_with(
                &shard_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Shard.size() * model.shards.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(model.shards.as_slice()));
        device
            .queue
            .write_buffer_with(
                &frame_model_buffer,
                0,
                wgpu::BufferSize::new(ModelGroup::Frame.size() * model.frames.len() as u64).unwrap()
            )
            .unwrap()
            .copy_from_slice(bytemuck::cast_slice(model.frames.as_slice()));

        RenderEngine {
            render_pipeline,
            compute_pipeline,

            world_uniforms_buffer,
            uniform_bind_group,

            shard_vertex_frame_buffer,
            segment_frame_buffer,
            frame_bind_group,
            frame_read_bind_group,

            vertex_model_buffer,
            segment_model_buffer,
            shard_model_buffer,
            frame_model_buffer,
            model_bind_group,

            object_scene_buffer,
            scene_bind_group,
        }
    }
    pub fn render(&self, device: &DeviceHandle,
                         target_surface_view: &wgpu::TextureView,
                         target_texture_views: &Vec<wgpu::TextureView>,
                         scene_data: &SceneData,
    ) -> Result<()> {
        let info = check::MODEL_INFO;
        let frame_info = check::FRAME_INFO;
        if scene_data.objects.len() > info.requested_scene_objects {
            return Err(anyhow!("Number of objects exceeds buffer size."));
        }
        let shard_extent: u32 = scene_data
            .objects
            .iter()
            .map(|o| check::FRAME_INFO[o.frame_index as usize].shard_size)
            .sum();

        let segment_extent: u32 = scene_data
            .objects
            .iter()
            .map(|o| check::FRAME_INFO[o.frame_index as usize].shard_size)
            .sum();

        if shard_extent > info.requested_frame_shards as u32 {
            return Err(anyhow!("Number of frame shards exceeds buffer size."));
        }
        if segment_extent > info.requested_frame_segments as u32 {
            return Err(anyhow!("Number of frame segments exceeds buffer size."));
        }

        let mut encoder = device
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                }
            );

        let mut view = device.queue.write_buffer_with(
            &self.object_scene_buffer,
            0,
            wgpu::BufferSize::new(SceneGroup::Object.size() * scene_data.objects.len() as u64).unwrap(),
        )
            .ok_or(anyhow!("Unable to get object buffer view"))?;
        let mut clip_offset: u32 = 0;
        let mut shard_offset: i32 = 0;
        let mut segment_offset: i32 = 0;

        for i in 0..scene_data.objects.len() {
            let o = &scene_data.objects[i];
            bytemuck::cast_slice_mut(&mut *view)[i] = FrameObject {
                world_tex_tf: o.world_local_tf.into(),
                frame_index: o.frame_index,
                clip_offset,
                shard_offset,
                segment_offset,
            };
            let frame: &FrameInfo = &frame_info[o.frame_index as usize];
            clip_offset += frame.clip_size;
            shard_offset += frame.shard_size as i32;
            segment_offset += frame.segment_size as i32;
        }
        drop(view);

        let mut view = device
            .queue
            .write_buffer_with(
                &self.world_uniforms_buffer,
                0,
                wgpu::BufferSize::new(UniformGroup::World.size()).unwrap(),
            )
            .ok_or(anyhow!("Could not write to world uniforms buffer"))?;
        view.copy_from_slice(bytemuck::cast_slice(
            &[Uniforms::get(scene_data)]
        ));
        drop(view);

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{
            label: Some("Frame Preprocessing Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        compute_pass.set_bind_group(1, &self.frame_bind_group, &[]);
        compute_pass.set_bind_group(2, &self.model_bind_group, &[]);
        compute_pass.set_bind_group(3, &self.scene_bind_group, &[]);
        compute_pass.dispatch_workgroups(scene_data.objects.len() as u32, 1, 1);
        drop(compute_pass);

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
        render_pass.set_bind_group(1, &self.frame_read_bind_group, &[]);
        render_pass.draw(0..(shard_extent * 6), 0..1);
        drop(render_pass);

        device.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
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
    World,
}

impl LayoutEnum for UniformGroup {
    type Iter = <[Self; 1] as IntoIterator>::IntoIter;
    fn entry_iter() -> Self::Iter {
        [Self::World].into_iter()
    }
    fn size(&self) -> u64 {
        pad_to_copy_buffer_alignment(match self {
            Self::World => size_of::<Uniforms>() as u64,
        })
    }
    fn binding(&self) -> u32 {
        match self {
            Self::World => 0,
        }
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        match self {
            Self::World => create_bind_group_layout_entry_buffer(
                self,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                wgpu::BufferBindingType::Uniform,
            ),
        }
    }

    fn buffer_descriptor(&self, _count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some(match self {
                Self::World => "World uniform buffer",
            }),
            size: match self {
                Self::World => self.size(),
            },
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ModelGroup {
    Vertex,
    Segment,
    Shard,
    Frame,
}

impl LayoutEnum for ModelGroup {
    type Iter = <[Self; 4] as IntoIterator>::IntoIter;

    fn entry_iter() -> Self::Iter {
        [Self::Vertex, Self::Segment, Self::Shard, Self::Frame].into_iter()
    }

    fn size(&self) -> u64 {
        match self {
            ModelGroup::Vertex => 8,
            ModelGroup::Segment => 16,
            ModelGroup::Shard => size_of::<ModelShard>() as u64,
            ModelGroup::Frame => size_of::<ModelFrame>() as u64,
        }
    }

    fn binding(&self) -> u32 {
        match self {
            ModelGroup::Vertex => 0,
            ModelGroup::Segment => 1,
            ModelGroup::Shard => 2,
            ModelGroup::Frame => 3,
        }
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        create_bind_group_layout_entry_buffer(
            self,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage {read_only: true}
        )
    }

    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some(match self {
                ModelGroup::Vertex => "Model vertex buffer",
                ModelGroup::Segment => "Model segment buffer",
                ModelGroup::Shard => "Model shard buffer",
                ModelGroup::Frame => "Model frame buffer",
            }),
            size: self.size() * count,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum SceneGroup {
    Object,
}

impl LayoutEnum for SceneGroup {
    type Iter = <[Self; 1] as IntoIterator>::IntoIter;

    fn entry_iter() -> Self::Iter {
        [Self::Object].into_iter()
    }

    fn size(&self) -> u64 {
        match self {
            Self::Object => size_of::<FrameObject>() as u64
        }
    }

    fn binding(&self) -> u32 {
        match self {
            Self::Object => 0,
        }
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        create_bind_group_layout_entry_buffer(
            self,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage {read_only: true}
        )
    }

    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor {
            label: Some("Scene objects buffer"),
            size: self.size() * count,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FrameGroup {
    Segment,
    ShardVertex,
}

impl LayoutEnum for FrameGroup {
    type Iter = <[Self; 2] as IntoIterator>::IntoIter;

    fn entry_iter() -> Self::Iter {
        [Self::Segment, Self::ShardVertex].into_iter()
    }

    fn size(&self) -> u64 {
        match self {
            Self::Segment => 32,
            Self::ShardVertex => 48,
        }
    }

    fn binding(&self) -> u32 {
        match self {
            Self::Segment => 0,
            Self::ShardVertex => 1,
        }
    }

    fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        create_bind_group_layout_entry_buffer(
            self,
            wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage {read_only: false,}
        )
    }

    fn buffer_descriptor(&self, count: u64) -> wgpu::BufferDescriptor<'static> {
        wgpu::BufferDescriptor{
            label: Some(match self {
                Self::Segment => "Frame segments buffer",
                Self::ShardVertex => "Frame shards vertex buffer",
            }),
            size: self.size() * count,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        }
    }
}
