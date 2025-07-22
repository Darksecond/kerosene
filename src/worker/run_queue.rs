use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, Sender},
};

use crate::actor::Pid;

pub struct RunQueue {
    length: AtomicUsize,
    sender: Sender<Pid>,
    receiver: Mutex<Receiver<Pid>>,
}

impl RunQueue {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            length: AtomicUsize::new(0),
            sender,
            receiver: Mutex::new(receiver),
        }
    }

    pub fn push(&self, pid: Pid) {
        self.sender.send(pid).expect("Failed to enqueue PID");
        self.length.fetch_add(1, Ordering::Relaxed);
    }

    pub fn try_pop(&self) -> Option<Pid> {
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
