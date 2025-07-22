use std::sync::{
    RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::{actor::Pid, worker::WorkerId};

pub struct ActorControlBlock {
    pub pid: Pid,
    pub trap_exit: AtomicBool,
    pub is_scheduled: AtomicBool,
    pub is_running: AtomicBool,
    pub worker_id: AtomicU64,
    pub(crate) links: RwLock<[Pid; Self::MAX_LINKS]>,
}

impl ActorControlBlock {
    pub const MAX_LINKS: usize = 32;

    pub fn new(pid: Pid, worker_id: WorkerId) -> Self {
        Self {
            pid,
            trap_exit: AtomicBool::new(false),
            is_scheduled: AtomicBool::new(false),
            is_running: AtomicBool::new(false),
            worker_id: AtomicU64::new(worker_id as _),
            links: RwLock::new([Pid::invalid(); Self::MAX_LINKS]),
        }
    }

    pub fn try_schedule(&self) -> bool {
        self.is_scheduled.swap(true, Ordering::AcqRel) == false
    }

    pub fn add_link(&self, pid: Pid) -> Result<(), ()> {
        let mut links = self.links.write().expect("Failed to acquire lock");
        for slot in links.iter_mut() {
            if *slot == Pid::invalid() {
                *slot = pid;
                return Ok(());
            }
        }

        Err(())
    }

    pub fn remove_link(&self, pid: Pid) -> Result<(), ()> {
        let mut links = self.links.write().expect("Failed to acquire lock");
        for slot in links.iter_mut() {
            if *slot == pid {
                *slot = Pid::invalid();
                return Ok(());
            }
        }

        Err(())
    }
}
