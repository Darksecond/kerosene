mod run_queue;

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub use run_queue::RunQueue;

use crate::{actor::Signal, registry::Registry, scheduler::Scheduler, timer::Timer};

pub type WorkerId = usize;

pub struct ActiveWorker {
    pub worker: Arc<Worker>,
    pub handle: JoinHandle<()>,
}

impl ActiveWorker {
    pub fn new(worker: Arc<Worker>, handle: JoinHandle<()>) -> Self {
        Self { worker, handle }
    }
}

pub struct Worker {
    pub spawn_at: WorkerId,
    pub run_queue: RunQueue,
    pub running: AtomicBool,
}

pub struct WorkerSnapshot {
    pub run_queue_length: usize,
}

impl Worker {
    pub fn new(spawn_at: WorkerId) -> Self {
        Self {
            spawn_at,
            run_queue: RunQueue::new(),
            running: AtomicBool::new(true),
        }
    }

    pub fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot {
            run_queue_length: self.run_queue.len(),
        }
    }

    pub fn run_queue_length(&self) -> usize {
        self.run_queue.len()
    }

    pub fn run(&self, registry: Arc<Registry>, scheduler: Arc<Scheduler>, timer: Arc<Timer>) {
        while self.running.load(Ordering::Relaxed) {
            let pid = {
                match self.run_queue.try_pop() {
                    Some(pid) => pid,

                    None => {
                        std::thread::park();
                        continue;
                    }
                }
            };

            let actor = match registry.lookup_pid(pid) {
                Some(actor) => actor,
                None => continue,
            };

            let control_block = actor.control_block();

            control_block.is_scheduled.store(false, Ordering::Release);
            control_block.is_running.store(true, Ordering::Release);

            let global_context = UnsafeCell::new(crate::global::GlobalContext {
                budget: 0,
                actor: &actor,
                registry: &registry,
                scheduler: &scheduler,
                timer: &timer,
                _marker: PhantomData,
            });

            crate::global::set_context(global_context.get());

            match actor.as_ref().poll() {
                None => {
                    if actor.has_messages() {
                        // scheduler.wake(pid);
                        if control_block.try_schedule() {
                            // Re-queue actor because it still has messages to process.
                            // TODO Consider a bounded inner loop for more efficiency.
                            self.run_queue.push(pid);
                        }
                    }
                }
                Some(exit) => {
                    eprintln!("Actor {} exited with reason {:?}", pid.0, exit);
                    let links = actor.links();

                    registry.remove(pid);

                    for linked in links.iter().copied() {
                        if let Some(child) = registry.lookup_pid(linked) {
                            child.send_signal(Signal::Exit(pid, exit.clone()));

                            scheduler.schedule(linked);
                        }
                    }
                }
            }

            crate::global::reset_context();

            control_block.is_running.store(false, Ordering::Release);
        }
    }
}
