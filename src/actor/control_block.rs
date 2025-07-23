use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::{
    actor::Pid,
    port::PortPid,
    utils::{CachePadded, UnsortedSet},
    worker::WorkerId,
};

pub const MAX_LINKS: usize = 32;

pub struct ActorControlBlock {
    pub pid: Pid,
    pub trap_exit: AtomicBool,
    pub is_scheduled: CachePadded<AtomicBool>,
    pub is_running: CachePadded<AtomicBool>,
    pub worker_id: AtomicU64,
    pub(crate) links: Mutex<UnsortedSet<Pid, MAX_LINKS>>,
    pub(crate) ports: Mutex<UnsortedSet<PortPid, MAX_LINKS>>,
}

impl ActorControlBlock {
    pub fn new(pid: Pid, worker_id: WorkerId) -> Self {
        Self {
            pid,
            trap_exit: AtomicBool::new(false),
            is_scheduled: CachePadded::new(AtomicBool::new(false)),
            is_running: CachePadded::new(AtomicBool::new(false)),
            worker_id: AtomicU64::new(worker_id as _),
            links: Mutex::new(UnsortedSet::new()),
            ports: Mutex::new(UnsortedSet::new()),
        }
    }

    pub fn try_schedule(&self) -> bool {
        self.is_scheduled.swap(true, Ordering::AcqRel) == false
    }

    pub fn add_link(&self, pid: Pid) -> Result<(), ()> {
        let mut links = self.links.lock().expect("Failed to acquire lock");
        if links.insert(pid) { Ok(()) } else { Err(()) }
    }

    pub fn remove_link(&self, pid: Pid) -> Result<(), ()> {
        let mut links = self.links.lock().expect("Failed to acquire lock");

        if links.remove(&pid) { Ok(()) } else { Err(()) }
    }
}
