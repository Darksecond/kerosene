use std::ops::Deref;

pub struct FilledBuffer {
    len: usize,
    buffer: Box<[u8]>,
}

impl FilledBuffer {
    pub(crate) fn new(buffer: Box<[u8]>, len: usize) -> Self {
        Self { buffer, len }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl Deref for FilledBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.buffer[..self.len]
    }
}
