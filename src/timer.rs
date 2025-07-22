use std::{
    any::Any,
    collections::BinaryHeap,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use crate::{
    actor::{Pid, Signal},
    registry::Registry,
    scheduler::Scheduler,
};

pub struct Timer {
    entries: Mutex<BinaryHeap<Entry>>,
    cond: Condvar,
    scheduler: Arc<Scheduler>,
    registry: Arc<Registry>,
}

struct Entry {
    pid: Pid,
    expire_at: Instant,
    message: Signal,
}

impl Eq for Entry {}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.expire_at == other.expire_at && self.pid == other.pid
    }
}

// We want a min-heap, so reverse ordering:
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse to get min-heap by expire_at
        other.expire_at.cmp(&self.expire_at)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Timer {
    pub fn new(scheduler: Arc<Scheduler>, registry: Arc<Registry>) -> Self {
        Timer {
            entries: Mutex::new(BinaryHeap::new()),
            cond: Condvar::new(),
            scheduler,
            registry,
        }
    }

    pub fn wake_up(&self, pid: Pid, duration: Duration) {
        let expire_at = Instant::now() + duration;
        let mut entries = self.entries.lock().expect("Failed to acquire lock");
        entries.push(Entry {
            pid,
            expire_at,
            message: Signal::TimerFired,
        });
        self.cond.notify_one(); // Wake timer thread if sleeping
    }

    pub fn add(&self, pid: Pid, duration: Duration, message: Box<dyn Any + Send>) {
        let expire_at = Instant::now() + duration;
        let mut entries = self.entries.lock().expect("Failed to acquire lock");
        entries.push(Entry {
            pid,
            expire_at,
            message: Signal::Message(message),
        });
        self.cond.notify_one(); // Wake timer thread if sleeping
    }

    pub fn run(&self) {
        let mut entries = self.entries.lock().expect("Failed to acquire lock");
        loop {
            while let Some(entry) = entries.peek() {
                let now = Instant::now();

                if entry.expire_at <= now {
                    let entry = entries.pop().unwrap();
                    self.scheduler.schedule(entry.pid);
                    if let Some(actor) = self.registry.lookup_pid(entry.pid) {
                        let _ = actor.send_signal(entry.message);
                        self.scheduler.schedule(entry.pid);
                    }

                    continue;
                } else {
                    let wait_duration = entry.expire_at - now;
                    entries = self
                        .cond
                        .wait_timeout(entries, wait_duration)
                        .expect("Failed to acquire lock")
                        .0;
                }
            }

            // No timers; wait indefinitely until new timers are added
            entries = self.cond.wait(entries).expect("Failed to acquire lock");
        }
    }
}
