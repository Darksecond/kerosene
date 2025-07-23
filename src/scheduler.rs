use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use crate::{
    actor::Pid,
    port::PortPid,
    registry::Registry,
    worker::{ActiveWorker, Worker, WorkerId},
};

pub(crate) enum Slot {
    Active(ActiveWorker),
    Reserved,
    Empty,
}

pub struct Scheduler {
    count: AtomicUsize,
    pub(crate) workers: [RwLock<Slot>; 128],
    registry: Arc<Registry>,
    pub epoch: Instant,
    pub(crate) stopped: AtomicBool,
}

impl Scheduler {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            count: AtomicUsize::new(0),
            workers: std::array::from_fn(|_| RwLock::new(Slot::Empty)),
            registry,
            epoch: Instant::now(),
            stopped: AtomicBool::new(false),
        }
    }

    pub fn allocate_slot(&self) -> WorkerId {
        let index = self.count.fetch_add(1, Ordering::Relaxed);
        let slot = &mut self.workers[index].write().expect("Failed to acquire lock");
        **slot = Slot::Reserved;

        index
    }

    pub fn replace_slot(&self, id: WorkerId, worker: ActiveWorker) -> Option<ActiveWorker> {
        let slot = &mut self.workers[id].write().expect("Failed to acquire lock");

        let slot = std::mem::replace(&mut **slot, Slot::Active(worker));

        match slot {
            Slot::Active(worker) => Some(worker),
            _ => None,
        }
    }

    pub fn get_worker(&self, id: WorkerId) -> Option<Arc<Worker>> {
        let slot = &self.workers[id].read().expect("Failed to acquire lock");

        match &**slot {
            Slot::Active(worker) => Some(worker.worker.clone()),
            _ => None,
        }
    }

    pub fn wake_worker(&self, worker_id: WorkerId) {
        let slot = &self.workers[worker_id]
            .read()
            .expect("Failed to acquire lock");

        if let Slot::Active(active_worker) = &**slot {
            active_worker.handle.thread().unpark();
        }
    }

    pub fn stop_all(&self) {
        let count = self.count.load(Ordering::Acquire);
        for worker_id in 0..count {
            self.stop(worker_id);
        }

        self.registry.remove_all();

        self.stopped.store(true, Ordering::Release);

        // TODO Stop the timer.
    }

    fn stop(&self, worker_id: WorkerId) {
        eprintln!("Stopping worker {}", worker_id);

        let slot = &mut self.workers[worker_id]
            .write()
            .expect("Failed to acquire lock");

        match &**slot {
            Slot::Active(active_worker) => {
                active_worker.worker.running.store(false, Ordering::Release);
                active_worker.handle.thread().unpark();
            }
            _ => (),
        }

        **slot = Slot::Empty;
    }

    pub fn schedule(&self, pid: Pid) {
        let Some(actor) = self.registry.lookup_pid(pid) else {
            return;
        };

        let control_block = actor.control_block();

        let worker_id = control_block.worker_id.load(Ordering::Acquire) as usize;

        {
            if control_block.try_schedule() {
                let Some(worker) = self.get_worker(worker_id) else {
                    eprintln!("Worker is assigned to invalid worker {}", worker_id);
                    return;
                };

                worker.run_queue.push(pid);

                self.wake_worker(worker_id);
            }
        }
    }

    pub fn schedule_port(&self, port: PortPid) {
        if let Some(worker) = self.get_worker(port.worker()) {
            worker.port_run_queue.push(port);
            self.wake_worker(port.worker());
        }
    }
}
