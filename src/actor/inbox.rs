use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::queue::Queue;

const QUEUE_SIZE: usize = 1024;

pub struct Inbox<T> {
    queue: Queue<QUEUE_SIZE, T>,
    overflow_count: AtomicUsize,
    overflow: Mutex<VecDeque<T>>,
}

impl<T> Inbox<T> {
    pub fn new() -> Self {
        Self {
            queue: Queue::new(),
            overflow_count: AtomicUsize::new(0),
            overflow: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, message: T) {
        if let Err(message) = self.queue.push(message) {
            let mut overflow = self.overflow.lock().expect("Failed to acquire lock");
            overflow.push_back(message);
            self.overflow_count.fetch_add(1, Ordering::Release);
        }
    }

    pub fn pop(&self) -> Option<T> {
        if let Some(message) = self.queue.pop() {
            return Some(message);
        }

        if self.overflow_count.load(Ordering::Acquire) == 0 {
            return None;
        }

        {
            let mut overflow = self.overflow.lock().expect("Failed to acquire lock");
            while let Some(item) = overflow.pop_front() {
                self.overflow_count.fetch_sub(1, Ordering::Release);

                if let Err(item) = self.queue.push(item) {
                    overflow.push_front(item);
                    self.overflow_count.fetch_add(1, Ordering::Release);
                    break;
                }
            }
        }

        self.queue.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.overflow_count.load(Ordering::Acquire) == 0
    }
}
