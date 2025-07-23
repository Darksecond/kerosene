use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, Sender},
};

pub struct RunQueue<T> {
    length: AtomicUsize,
    sender: Sender<T>,
    receiver: Mutex<Receiver<T>>,
}

impl<T> RunQueue<T> {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            length: AtomicUsize::new(0),
            sender,
            receiver: Mutex::new(receiver),
        }
    }

    pub fn push(&self, item: T) {
        self.sender.send(item).expect("Failed to enqueue item");
        self.length.fetch_add(1, Ordering::Relaxed);
    }

    pub fn try_pop(&self) -> Option<T> {
        let item = self
            .receiver
            .lock()
            .expect("Failed to acquire lock")
            .try_recv()
            .ok();

        if item.is_some() {
            self.length.fetch_sub(1, Ordering::Relaxed);
        }

        item
    }

    pub fn len(&self) -> usize {
        self.length.load(Ordering::Relaxed)
    }
}
