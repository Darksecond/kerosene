use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::utils::CachePadded;

pub struct Queue<const S: usize, T> {
    buffer: [UnsafeCell<MaybeUninit<T>>; S],
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    published: CachePadded<AtomicUsize>,
}

unsafe impl<const S: usize, T> Send for Queue<S, T> where T: Send {}
unsafe impl<const S: usize, T> Sync for Queue<S, T> where T: Send {}

impl<const S: usize, T> Queue<S, T> {
    pub fn new() -> Self {
        Self {
            buffer: unsafe { MaybeUninit::uninit().assume_init() },
            head: AtomicUsize::new(0).into(),
            tail: AtomicUsize::new(0).into(),
            published: AtomicUsize::new(0).into(),
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        S
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);

            if (head - tail) >= self.capacity() {
                // Queue full
                return Err(value);
            }

            if self
                .head
                .compare_exchange_weak(head, head + 1, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                let index = head % self.capacity();

                // Write the value into the buffer
                unsafe {
                    let slot = self
                        .buffer
                        .get_unchecked(index)
                        .get()
                        .as_mut()
                        .unwrap_unchecked();
                    slot.write(value);
                }

                // Publish the slot as ready
                // To handle multiple producers, we must publish in order
                // So we spin until we are next to publish
                loop {
                    let published = self.published.load(Ordering::Acquire);

                    if published == head {
                        self.published.store(head + 1, Ordering::Release);
                        break;
                    }
                    std::hint::spin_loop();
                }

                return Ok(());
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.load(Ordering::Acquire);
        let published = self.published.load(Ordering::Acquire);

        if tail >= published {
            // Queue empty or slots not ready yet
            return None;
        }

        let index = tail % self.capacity();

        let value = unsafe { self.buffer.get_unchecked(index).get().read().assume_init() };

        self.tail.store(tail + 1, Ordering::Release);

        Some(value)
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let published = self.published.load(Ordering::Acquire);

        tail >= published
    }

    /// Returns true if the queue is full.
    pub fn is_full(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.published.load(Ordering::Acquire);

        (head - tail) >= self.capacity()
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let published = self.published.load(Ordering::Acquire);

        published.saturating_add(tail)
    }
}

impl<const S: usize, T> Drop for Queue<S, T> {
    fn drop(&mut self) {
        while let Some(value) = self.pop() {
            drop(value);
        }
    }
}
