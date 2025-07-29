use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::{
    actor::Pid,
    migration::{Mode, Parameters},
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
    pub(crate) stopped: AtomicBool,
    is_balancing: AtomicBool,
}

impl Scheduler {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            count: AtomicUsize::new(0),
            workers: std::array::from_fn(|_| RwLock::new(Slot::Empty)),
            registry,
            stopped: AtomicBool::new(false),
            is_balancing: AtomicBool::new(false),
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
            active_worker.thread.unpark();
        }
    }

    pub fn stop_all(&self) {
        let count = self.count.load(Ordering::Acquire);
        for worker_id in 0..count {
            self.stop(worker_id);
        }

        self.stopped.store(true, Ordering::Release);
    }

    fn stop(&self, worker_id: WorkerId) {
        eprintln!("Stopping worker {}", worker_id);

        let slot = &mut self.workers[worker_id]
            .write()
            .expect("Failed to acquire lock");

        match &**slot {
            Slot::Active(active_worker) => {
                active_worker.worker.running.store(false, Ordering::Release);
                active_worker.thread.unpark();
            }
            _ => (),
        }

        **slot = Slot::Empty;
        self.count.fetch_sub(1, Ordering::Release);
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

    // Try and steal from a worker, starting to the next working in the ring
    // and overflowing back around.
    pub fn try_steal(&self, worker_id: WorkerId) -> Option<Pid> {
        let n = self.count.load(Ordering::Relaxed);
        let mut i = (worker_id + 1) % n;

        while i != worker_id {
            let Some(worker) = self.get_worker(i) else {
                i = (i + 1) % n;
                continue;
            };

            if let Some(pid) = worker.run_queue.try_pop() {
                // Reassign actor to it's new worker.
                let Some(actor) = self.registry.lookup_pid(pid) else {
                    // actor must have been removed from the registry
                    return None;
                };

                let control_block = actor.control_block();
                if !control_block.is_running.load(Ordering::Acquire) {
                    control_block
                        .worker_id
                        .store(worker_id as _, Ordering::Release);

                    return Some(pid);
                } else {
                    eprintln!("Trying to steal running actor {}", pid.0);
                    worker.run_queue.push(pid);
                }
            }

            i = (i + 1) % n;
        }

        None
    }

    pub fn try_balance(&self, worker: WorkerId) -> bool {
        if self
            .is_balancing
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            println!("Balancing on worker {}", worker);
            self.balance();
            self.is_balancing.store(false, Ordering::Release);

            true
        } else {
            false
        }
    }

    pub fn try_pull(&self, target: WorkerId, parameters: Parameters) {
        let Some(target) = self.get_worker(target) else {
            return;
        };

        let Some(source) = self.get_worker(parameters.target) else {
            return;
        };

        let can_pull = source.run_queue_length() > parameters.balance
            && target.run_queue_length() < parameters.balance;

        if can_pull {
            let Some(pid) = source.run_queue.try_pop() else {
                return;
            };

            let Some(actor) = self.registry.lookup_pid(pid) else {
                return;
            };

            // println!(
            //     "Worker {} should pull {} from {}",
            //     target.spawn_at, pid.0, source.spawn_at
            // );

            if !actor.control_block().is_running.load(Ordering::Acquire) {
                actor
                    .control_block()
                    .worker_id
                    .store(target.spawn_at as _, Ordering::Release);

                target.run_queue.push(pid);
            } else {
                // We tried to push an actor that is currently running
                source.run_queue.push(pid);
            }
        }
    }

    pub fn try_push(&self, source: WorkerId, parameters: Parameters) {
        let Some(source) = self.get_worker(source) else {
            return;
        };

        let Some(target) = self.get_worker(parameters.target) else {
            return;
        };

        let can_push = source.run_queue_length() > parameters.balance
            && target.run_queue_length() < parameters.balance;

        if can_push {
            let Some(pid) = source.run_queue.try_pop() else {
                return;
            };

            let Some(actor) = self.registry.lookup_pid(pid) else {
                return;
            };

            // println!(
            //     "Worker {} should push {} to {}",
            //     source.spawn_at, pid.0, target.spawn_at
            // );

            if !actor.control_block().is_running.load(Ordering::Acquire) {
                actor
                    .control_block()
                    .worker_id
                    .store(target.spawn_at as _, Ordering::Release);

                target.run_queue.push(pid);
            } else {
                // We tried to push an actor that is currently running
                source.run_queue.push(pid);
            }
        }
    }

    fn balance(&self) {
        let worker_count = self.count.load(Ordering::Relaxed);

        let mut max_queue_lengths = Vec::with_capacity(worker_count);
        for i in 0..worker_count {
            if let Some(worker) = self.get_worker(i) {
                max_queue_lengths.push(worker.max_queue_length.load(Ordering::Relaxed));
            }
        }

        let average_queue_length = max_queue_lengths.iter().sum::<usize>() / worker_count;
        let average_queue_length = average_queue_length + 4; // Add some margin

        // println!("Average queue length: {}", average_queue_length);

        let mut max_queue_lengths = max_queue_lengths
            .iter()
            .copied()
            .enumerate()
            .collect::<Vec<_>>();
        max_queue_lengths.sort_by_key(|&(_, length)| length);

        // println!("{:?}", max_queue_lengths);

        let mut parameters = vec![Parameters::none(); worker_count];

        let mut i = 0;
        let mut j = worker_count - 1;
        while max_queue_lengths[i].1 < average_queue_length {
            let index = max_queue_lengths[i].0;
            let target = max_queue_lengths[j].0;

            parameters[index] = Parameters {
                target,
                mode: Mode::Pull,
                balance: average_queue_length,
            };

            i += 1;
            j -= 1;
            if max_queue_lengths[j].1 <= average_queue_length {
                j = worker_count - 1;
            }
        }

        let mut i = 0;
        let mut j = worker_count - 1;
        while max_queue_lengths[j].1 > average_queue_length {
            let index = max_queue_lengths[j].0;
            let target = max_queue_lengths[i].0;
            parameters[index] = Parameters {
                target,
                mode: Mode::Push,
                balance: average_queue_length,
            };

            j -= 1;
            i += 1;
            if max_queue_lengths[i].1 >= average_queue_length {
                i = 0;
            }
        }

        // println!("{:?}", parameters);

        for i in 0..worker_count {
            let active_worker = &self.workers[i];
            if let Slot::Active(slot) = &*active_worker.read().expect("Failed to acquire lock") {
                slot.worker.reductions.store(2000 * 1000, Ordering::Relaxed);
                slot.worker.max_queue_length.store(0, Ordering::Relaxed);
                slot.worker.migration.store(parameters[i]);

                slot.thread.unpark();
            }
        }
    }
}
