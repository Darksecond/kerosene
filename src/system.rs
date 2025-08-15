use std::sync::{Arc, atomic::Ordering};

use crate::{
    Pid, actor::ToPid, migration::Parameters, registry::Registry, scheduler::Scheduler,
    timer::Timer, worker::WorkerId,
};

pub struct System {
    pub registry: Registry,
    pub scheduler: Scheduler,
    pub timer: Timer,
}

impl System {
    pub fn new() -> Arc<Self> {
        let registry = Registry::new();
        let scheduler = Scheduler::new();
        let timer = Timer::new();

        Arc::new(System {
            registry,
            scheduler,
            timer,
        })
    }

    pub fn stop_all(&self) {
        self.scheduler.stop_all();
        self.registry.remove_all();
        self.timer.stop();
    }

    pub fn schedule(&self, pid: impl ToPid) {
        let pid = pid.to_reference(&self.registry);

        let Some(actor) = self.registry.lookup_pid(pid) else {
            return;
        };

        self.scheduler.schedule_actor(actor);
    }

    // Try and steal from a worker, starting to the next working in the ring
    // and overflowing back around.
    pub fn try_steal(&self, worker_id: WorkerId) -> Option<Pid> {
        let n = self.scheduler.count();

        let mut i = (worker_id + 1) % n;

        while i != worker_id {
            let Some(worker) = self.scheduler.get_worker(i) else {
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

    pub fn try_pull(&self, target: WorkerId, parameters: Parameters) {
        let Some(target) = self.scheduler.get_worker(target) else {
            return;
        };

        let Some(source) = self.scheduler.get_worker(parameters.target) else {
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
        let Some(source) = self.scheduler.get_worker(source) else {
            return;
        };

        let Some(target) = self.scheduler.get_worker(parameters.target) else {
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
}
