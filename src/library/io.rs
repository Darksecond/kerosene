pub mod file;

use std::ops::{Deref, DerefMut};

const CHUNK_SIZE: usize = 0x1000;

pub struct Buffer {
    len: usize,
    buffer: Box<[u8]>,
}

impl Buffer {
    pub(crate) fn new() -> Self {
        Self {
            buffer: vec![0; CHUNK_SIZE].into_boxed_slice(),
            len: 0,
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    pub const fn remaining_capacity(&self) -> usize {
        self.capacity() - self.len()
    }

    pub fn resize(&mut self, new_len: usize) {
        let new_len = new_len.min(self.buffer.len());

        if new_len > self.len {
            self.buffer[self.len..new_len].fill(0);
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

        self.buffer[self.len..self.len + src.len()].copy_from_slice(&src);
        self.len += src.len();
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer[..self.len]
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer[..self.len]
    }
}

pub mod pool {
    use super::Buffer;

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
}
