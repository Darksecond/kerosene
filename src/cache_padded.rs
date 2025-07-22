use std::{
    fmt::{self},
    ops::{Deref, DerefMut},
};

// TODO We should probably use different alignments for different platforms.
/// Cache-aligned padding for a type to prevent false sharing.
/// Basically https://docs.rs/crossbeam-utils/0.8.21/src/crossbeam_utils/cache_padded.rs.html.
#[repr(align(128))]
pub struct CachePadded<T> {
    inner: T,
}

impl<T> CachePadded<T> {
    pub fn new(inner: T) -> Self {
        CachePadded { inner }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> From<T> for CachePadded<T> {
    fn from(inner: T) -> Self {
        CachePadded { inner }
    }
}

unsafe impl<T> Sync for CachePadded<T> where T: Sync {}
unsafe impl<T> Send for CachePadded<T> where T: Send {}

impl<T> fmt::Display for CachePadded<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}
