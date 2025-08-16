use std::{
    alloc::Layout,
    ops::{Deref, DerefMut},
    slice,
};

const CHUNK_SIZE: usize = 0x1000;

pub struct Buffer {
    len: usize,
    capacity: usize,
    ptr: *mut u8,
}

unsafe impl Send for Buffer {}

impl Buffer {
    pub(crate) fn new() -> Self {
        let layout = Layout::array::<u8>(CHUNK_SIZE).expect("Failed to create buffer");

        Self {
            len: 0,
            capacity: CHUNK_SIZE,
            ptr: unsafe { std::alloc::alloc(layout) },
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub const fn remaining_capacity(&self) -> usize {
        self.capacity() - self.len()
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    pub unsafe fn set_len(&mut self, new_len: usize) {
        self.len = new_len;
    }

    pub fn resize(&mut self, new_len: usize) {
        let new_len = new_len.min(self.capacity);

        if new_len > self.len {
            let ptr = self.ptr.wrapping_add(self.len);
            let slice = unsafe { slice::from_raw_parts_mut(ptr, new_len - self.len) };
            slice.fill(0);
        }

        self.len = new_len;
    }

    /// Copy a slice into the buffer.
    /// This will only copy up to the available space in the buffer from `src`.
    ///
    /// # Panics
    ///
    /// Panics if the buffer is too small to copy the slice.
    pub fn copy_from_slice(&mut self, src: &[u8]) {
        assert!(
            self.remaining_capacity() >= src.len(),
            "Buffer is too small to copy the slice"
        );

        let ptr = self.ptr.wrapping_add(self.len);
        let slice = unsafe { slice::from_raw_parts_mut(ptr, src.len()) };
        slice.copy_from_slice(&src);
        self.len += src.len();
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let layout = Layout::array::<u8>(self.capacity).expect("Failed to create buffer");
        unsafe {
            std::alloc::dealloc(self.ptr, layout);
        }
    }
}

/// Acquire a buffer from the buffer pool.
///
/// # Parameters
///
/// * `size_hint`: Hints to the size of the resulting buffer, the buffer can be smaller or larger than the hint.
pub async fn reserve_buffer(size_hint: usize) -> Buffer {
    let _ = size_hint;
    Buffer::new()
}

// TODO: Consider free_buffer
