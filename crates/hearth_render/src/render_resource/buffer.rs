use crate::renderer::wgpu_wrapper;
use core::ops::{Deref, RangeBounds};
use hearth_utils::define_atomic_id;

define_atomic_id!(BufferId);

wgpu_wrapper! {
    #[derive(Clone, Debug)]
    struct WgpuBuffer(wgpu::Buffer);
}

#[derive(Clone, Debug)]
pub struct Buffer {
    id: BufferId,
    value: WgpuBuffer,
}

impl Buffer {
    #[inline]
    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn slice(&self, bounds: impl RangeBounds<wgpu::BufferAddress>) -> BufferSlice<'_> {
        BufferSlice {
            id: self.id,
            value: self.value.slice(bounds),
        }
    }

    #[inline]
    pub fn unmap(&self) {
        self.value.unmap();
    }
}

impl From<wgpu::Buffer> for Buffer {
    fn from(value: wgpu::Buffer) -> Self {
        Buffer {
            id: BufferId::new(),
            value: WgpuBuffer::new(value),
        }
    }
}

impl Deref for Buffer {
    type Target = wgpu::Buffer;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[derive(Clone, Debug)]
pub struct BufferSlice<'a> {
    id: BufferId,
    value: wgpu::BufferSlice<'a>,
}

impl<'a> BufferSlice<'a> {
    #[inline]
    pub fn id(&self) -> BufferId {
        self.id
    }
}

impl<'a> Deref for BufferSlice<'a> {
    type Target = wgpu::BufferSlice<'a>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
