use std::num::NonZeroU64;
use crate::render::LayoutEnum;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    // note even though only really using 2+1D transformations, the alignments on vec3's are a real pain.
    pub clip_world_tf: [[f32; 4]; 4], // tf from world coordinates to clip coordinates (for bb purposes)
    pub frag_clip_tf: [[f32; 4]; 4], // tf from fragment coordinates to world coordinates.
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pub pos: [f32; 2]
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelSegment {
    pub idx: [i32; 4] // making this signed in case using negative values for special cases later
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelShard {
    pub bb: [f32; 4],
    pub color: [f32; 4],
    pub segment_range: [i32; 2],
    pub clip_depth: u32,
    pub filler: u32,
}


#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelFrame {
    pub shard_range: [i32; 2],
    pub segment_range: [i32; 2],
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FrameInfo {
    pub clip_size: u32,
    pub shard_size: u32,
    pub segment_size: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameObject {
    pub world_tex_tf: [[f32; 4]; 4],
    pub frame_index: i32,
    pub clip_offset: u32,
    pub shard_offset: i32,
    pub segment_offset: i32,
}

fn pad_to_copy_buffer_alignment(size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1; // 0b11 since copy buffer alignment is 4
    ((size + align_mask) & !align_mask) // round up to nearest aligned
        .max(wgpu::COPY_BUFFER_ALIGNMENT) // make sure it's non-empty
}

pub fn create_bind_group_layout_entry_buffer<T: LayoutEnum>(
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
pub enum UniformGroup {
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
pub enum ModelGroup {
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
pub enum SceneGroup {
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
pub enum FrameGroup {
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
