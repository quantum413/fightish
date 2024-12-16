use anyhow::anyhow;
use cgmath::SquareMatrix;
use crate::render::{DeviceHandle, DeviceId, RenderContext, TargetTextureDongle};
use crate::scene::SceneData;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Quad {
    bb: [f32; 4],
    color: [f32; 4],
    clip_depth: u32,
    padding: [f32; 3],
}

const QUADS: &[Quad] = &[
    Quad {bb: [-1.0f32, -1.0f32, 1.0f32, 1.0f32], color: [1.0, 0.0, 0.0, 1.0], clip_depth: 1, padding: [0f32; 3]},
    Quad {bb: [0.2f32, 0.7f32, 1.3f32, 1.5f32], color: [0.0, 0.0, 1.0, 1.0], clip_depth: 0, padding: [0f32; 3]},
];

const QUAD_BUFFER_SIZE: u32 = 10;

fn pad_to_copy_buffer_alignment(size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1; // 0b11 since copy buffer alignment is 4
    ((size + align_mask) & !align_mask) // round up to nearest aligned
        .max(wgpu::COPY_BUFFER_ALIGNMENT) // make sure it's non-empty
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    // note even though only really using 2+1D transformations, the alignments on vec3's are a real pain.
    clip_world_tf: [[f32; 4]; 4], // tf from world coordinates to clip coordinates (for bb purposes)
    world_frag_tf: [[f32; 4]; 4], // tf from fragment coordinates to world coordinates.
}

impl Uniforms {
    fn get(scene_data: &SceneData) -> Self {
        let clip_frag_tf = // scaled -1 to +1 (clip coords)
            cgmath::Matrix4::from_translation(cgmath::vec3(-1f32, 1f32, 0f32))
                * // scaled from 0 to +2 for x and -2 to 0 for y
                cgmath::Matrix4::from_nonuniform_scale(
                    2f32 / scene_data.vp_width as f32,
                    -2f32 / scene_data.vp_height as f32,
                    1f32,
                )
                * // scaled from 0 to width/height
                cgmath::Matrix4::from_translation(cgmath::vec3(
                    -scene_data.vp_x as f32,
                    -scene_data.vp_y as f32,
                    0f32,
                )); // scaled from vp_x/y to width + vp_x / height + vp_y

        let world_clip_tf = cgmath::Matrix4::from_nonuniform_scale(
            scene_data.vp_width as f32 / scene_data.vp_height as f32 * scene_data.camera_scale,
            scene_data.camera_scale,
            1f32,
        );

        Self {
            clip_world_tf: world_clip_tf.invert().unwrap().into(),
            world_frag_tf: (world_clip_tf * clip_frag_tf).into(),
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

#[derive(Debug)]
pub struct RenderEngine {
    render_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    quad_bind_group: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
}

impl RenderEngine {
    pub fn new(context: &RenderContext, device_id: DeviceId, format: &wgpu::TextureFormat) -> RenderEngine {
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
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }
                ],
                label: Some("uniform_bind_group_layout"),
            });
        let quad_bind_group_layout = device
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("quad bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage {
                                read_only: true,
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }
                ],
            });
        let render_pipeline_layout = device
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &uniform_bind_group_layout,
                    &quad_bind_group_layout,
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
        let uniform_buffer = device
            .device
            .create_buffer(
                &wgpu::BufferDescriptor {
                    label: Some("uniform_buffer"),
                    size: pad_to_copy_buffer_alignment(size_of::<Uniforms>() as u64),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }
            );
        let uniform_bind_group = device
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &uniform_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    }
                ],
                label: Some("uniform_bind_group"),
            });
        let quad_buffer = device
            .device
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("quad buffer"),
                size: pad_to_copy_buffer_alignment(size_of::<Quad>() as u64) * QUAD_BUFFER_SIZE as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let quad_bind_group = device
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor{
                label: Some("quad bind group"),
                layout: &quad_bind_group_layout,
                entries: &[wgpu::BindGroupEntry{
                    binding: 0,
                    resource: quad_buffer.as_entire_binding(),
                }]
            });
        RenderEngine {
            render_pipeline,
            uniform_buffer,
            uniform_bind_group,
            quad_bind_group,
            quad_buffer,
        }
    }
    pub fn render(&self, device: &DeviceHandle,
                         target_surface_view: &wgpu::TextureView,
                         target_texture_views: &Vec<wgpu::TextureView>,
                         scene_data: &SceneData,
    ) -> anyhow::Result<()> {
        let mut encoder = device
            .device
            .create_command_encoder(
                &wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                }
            );

        device.queue.write_buffer_with(
            &self.quad_buffer,
            0,
            wgpu::BufferSize::new((size_of::<Quad>() * QUADS.len()) as u64).unwrap(),
        )
            .ok_or(anyhow!("Unable to get quad buffer view"))?
            .copy_from_slice(bytemuck::cast_slice(QUADS));

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
        render_pass.draw(0..(QUADS.len() * 6) as u32, 0..1);
        drop(render_pass);

        let mut view = device
            .queue
            .write_buffer_with(
                &self.uniform_buffer,
                0,
                wgpu::BufferSize::new(size_of::<Uniforms>() as wgpu::BufferAddress).unwrap(),
            )
            .ok_or(anyhow!("Could not write to uniforms buffer"))?;
        view.copy_from_slice(bytemuck::cast_slice(
            &[Uniforms::get(scene_data)]
        ));
        drop(view);
        device.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}
