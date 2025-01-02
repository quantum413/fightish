use anyhow::{anyhow, Result};
use cgmath::SquareMatrix;
use log::*;
use crate::buffer_structs::*;
use crate::model::SimpleLoader;
use crate::render::{DeviceHandle, DeviceId, LayoutEnum, RenderContext, TargetTextureDongle};
use crate::scene::SceneData;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

#[derive(Debug)]
pub struct RenderEngine {
    render_pipeline: wgpu::RenderPipeline,
    compute_pipeline: wgpu::ComputePipeline,

    world_uniforms_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    shard_vertex_frame_capacity: u64,
    shard_vertex_frame_buffer: wgpu::Buffer,
    segment_frame_capacity: u64,
    frame_bind_group_layout: wgpu::BindGroupLayout,
    frame_read_bind_group_layout: wgpu::BindGroupLayout,
    segment_frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    frame_read_bind_group: wgpu::BindGroup,

    loader: SimpleLoader,

    object_scene_capacity: u64,
    object_scene_buffer: wgpu::Buffer,
    scene_bind_group_layout: wgpu::BindGroupLayout,
    scene_bind_group: wgpu::BindGroup,
}

impl RenderEngine {
    pub fn new(context: &RenderContext, device_id: DeviceId, format: &wgpu::TextureFormat, mut loader: SimpleLoader) -> RenderEngine {
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

        // this is jank, but everything else you could do is also jank given the setup.
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

        let segment_frame_capacity = 1u64;
        let shard_vertex_frame_capacity = 1u64;
        let segment_frame_buffer = device
            .create_buffer_with_layout_enum(&FrameGroup::Segment, segment_frame_capacity);
        let shard_vertex_frame_buffer = device
            .create_buffer_with_layout_enum(&FrameGroup::ShardVertex, shard_vertex_frame_capacity);
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

        let object_scene_capacity = 1u64;
        let object_scene_buffer = device
            .create_buffer_with_layout_enum(&SceneGroup::Object, object_scene_capacity);
        let scene_bind_group = device
            .create_bind_group_with_enum_layout_map(
                &scene_bind_group_layout,
                Some("Scene bind group"),
                |t| match t {
                    SceneGroup::Object => object_scene_buffer.as_entire_binding(),
                }
            );

        loader.load(device);

        RenderEngine {
            render_pipeline,
            compute_pipeline,

            world_uniforms_buffer,
            uniform_bind_group,

            shard_vertex_frame_capacity,
            segment_frame_capacity,
            shard_vertex_frame_buffer,
            segment_frame_buffer,
            frame_bind_group_layout,
            frame_read_bind_group_layout,
            frame_bind_group,
            frame_read_bind_group,

            loader,
            // vertex_model_buffer,
            // segment_model_buffer,
            // shard_model_buffer,
            // frame_model_buffer,
            // model_bind_group,

            object_scene_capacity,
            object_scene_buffer,
            scene_bind_group_layout,
            scene_bind_group,
        }
    }
    pub fn render(&mut self, device: &DeviceHandle,
                         target_surface_view: &wgpu::TextureView,
                         target_texture_views: &Vec<wgpu::TextureView>,
                         scene_data: &SceneData,
    ) -> Result<()> {
        let frame_info = self.loader.frame_info();
        if scene_data.objects.len() as u64 > self.object_scene_capacity {
            let old_capacity = self.object_scene_capacity;
            while self.object_scene_capacity < scene_data.objects.len() as u64 {
                self.object_scene_capacity *= 2;
            }
            info!(
                "Scene objects {} exceeds buffer capacity {}, resizing to capacity {}.",
                scene_data.objects.len(),
                old_capacity,
                self.object_scene_capacity,
            );
            self.object_scene_buffer.destroy();
            self.object_scene_buffer = device
                .create_buffer_with_layout_enum(
                    &SceneGroup::Object,
                    self.object_scene_capacity
                );
            self.scene_bind_group = device
                .create_bind_group_with_enum_layout_map(
                    &self.scene_bind_group_layout,
                    Some("Scene bind group"),
                    |t| match t {
                        SceneGroup::Object => self.object_scene_buffer.as_entire_binding(),
                    }
                );
        }
        let shard_extent: u32 = scene_data
            .objects
            .iter()
            .map(|o| frame_info[o.frame_index as usize].shard_size)
            .sum();

        let segment_extent: u32 = scene_data
            .objects
            .iter()
            .map(|o| frame_info[o.frame_index as usize].segment_size)
            .sum();

        let model_group = self.loader.bind_group().unwrap();

        let mut frame_bind_group_dirty = false;
        let shard_vertex_extent = shard_extent as u64 * 6;
        if shard_vertex_extent > self.shard_vertex_frame_capacity {
            frame_bind_group_dirty = true;
            let old_capacity = self.shard_vertex_frame_capacity;
            while self.shard_vertex_frame_capacity < shard_vertex_extent {
                self.shard_vertex_frame_capacity *= 2;
            }
            info!(
                "Frame shard vertices requested {} exceeds capacity {}, resizing buffer to capacity {}.",
                shard_vertex_extent,
                old_capacity,
                self.shard_vertex_frame_capacity,
            );
            self.shard_vertex_frame_buffer.destroy();
            self.shard_vertex_frame_buffer = device
                .create_buffer_with_layout_enum(
                    &FrameGroup::ShardVertex,
                    self.shard_vertex_frame_capacity
                );
        }
        if segment_extent as u64 > self.segment_frame_capacity {
            frame_bind_group_dirty = true;
            let old_capacity = self.segment_frame_capacity;
            while self.segment_frame_capacity < segment_extent as u64 {
                self.segment_frame_capacity *= 2;
            }
            info!(
                "Frame segments requested {} exceeds capacity {}, resizing buffer to capacity {}.",
                segment_extent,
                old_capacity,
                self.segment_frame_capacity,
            );
            self.segment_frame_buffer.destroy();
            self.segment_frame_buffer = device
                .create_buffer_with_layout_enum(
                    &FrameGroup::Segment,
                    self.segment_frame_capacity);
        }
        if frame_bind_group_dirty {
            info!("Rebuilding dirty bind groups.");
            self.frame_bind_group = device
                .create_bind_group_with_enum_layout_map(
                    &self.frame_bind_group_layout,
                    Some("Frame bind group"),
                    |t| match t {
                        FrameGroup::Segment => self.segment_frame_buffer.as_entire_binding(),
                        FrameGroup::ShardVertex => self.shard_vertex_frame_buffer.as_entire_binding(),
                    }
                );
            self.frame_read_bind_group = device
                .create_bind_group_with_enum_layout_map(
                    &self.frame_read_bind_group_layout,
                    Some("Frame read bind group"),
                    |t| match t {
                        FrameGroup::Segment => self.segment_frame_buffer.as_entire_binding(),
                        FrameGroup::ShardVertex => self.shard_vertex_frame_buffer.as_entire_binding(),
                    }
                );
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
            &[Self::get_uniforms(scene_data)]
        ));
        drop(view);

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{
            label: Some("Frame Preprocessing Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        compute_pass.set_bind_group(1, &self.frame_bind_group, &[]);
        compute_pass.set_bind_group(2, model_group, &[]);
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

    fn get_uniforms(scene_data: &SceneData) -> Uniforms{
        let frag_clip_tf = // frag coords scaled from vp_x/y to width + vp_x / height + vp_y;
            cgmath::Matrix4::from_translation(cgmath::vec3(
                scene_data.vp_x as f32,
                scene_data.vp_y as f32,
                0f32,
            ))
                * // scaled from 0 to width/height
                cgmath::Matrix4::from_nonuniform_scale(
                    scene_data.vp_width as f32 / 2.0,
                    -(scene_data.vp_height as f32 / 2.0),
                    1f32,
                )
                * // scaled from 0 to +2 for x and -2 to 0 for y
                cgmath::Matrix4::from_translation(cgmath::vec3(1f32, -1f32, 0f32))
            ; // scaled -1 to +1 (clip coords)

        let world_clip_tf = scene_data.camera_tf;

        Uniforms {
            clip_world_tf: world_clip_tf.invert().unwrap().into(),
            frag_clip_tf: frag_clip_tf.into(),
        }
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
