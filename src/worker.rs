mod run_queue;

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread::Thread,
};

pub use run_queue::RunQueue;

use crate::{
    actor::{Pid, Signal},
    migration::Migration,
};

pub type WorkerId = usize;

pub struct ActiveWorker {
    pub worker: Arc<Worker>,
    pub thread: Thread,
}

pub struct Worker {
    pub spawn_at: WorkerId,
    pub run_queue: RunQueue<Pid>,
    pub running: AtomicBool,
    pub reductions: AtomicU64,
    pub max_queue_length: AtomicUsize,
    pub migration: Migration,
}

impl Worker {
    pub fn new(spawn_at: WorkerId) -> Self {
        Self {
            spawn_at,
            run_queue: RunQueue::new(),
            running: AtomicBool::new(true),
            reductions: AtomicU64::new(2000 * 1000),
            max_queue_length: AtomicUsize::new(0),
            migration: Migration::new(),
        }
    }

    pub fn run_queue_length(&self) -> usize {
        self.run_queue.len()
    }

    pub fn run(&self) {
        let system = unsafe { crate::thread::borrow() };

        while self.running.load(Ordering::Relaxed) {
            self.max_queue_length
                .fetch_max(self.run_queue.len(), Ordering::Relaxed);

            // Try and balance the workers
            if self.reductions.fetch_sub(1, Ordering::Relaxed) == 0 {
                // Should balance
                if !system.scheduler.try_balance(self.spawn_at) {
                    self.reductions.store(u64::MAX, Ordering::Relaxed);
                }
            }

            // Try and push an actor according to the migration parameters
            {
                let parameters = self.migration.load_for_push();
                if parameters.mode == crate::migration::Mode::Push {
                    system.try_push(self.spawn_at, parameters);
                } else if parameters.mode == crate::migration::Mode::Pull {
                    system.try_pull(self.spawn_at, parameters);
                }
            }

            if let Some(pid) = self.run_queue.try_pop() {
                self.run_actor(pid);
            } else if let Some(pid) = system.try_steal(self.spawn_at) {
                eprintln!("Worker {} stealing pid {}", self.spawn_at, pid.0);
                self.run_actor(pid);
            } else {
                std::thread::park();
            }
        }
    }

    fn run_actor(&self, pid: Pid) {
        let system = unsafe { crate::thread::borrow() };

        let actor = match system.registry.lookup_pid(pid) {
            Some(actor) => actor,
            None => return,
        };

        let control_block = actor.control_block();

        control_block.is_scheduled.store(false, Ordering::Release);
        control_block.is_running.store(true, Ordering::Release);

        let global_context = UnsafeCell::new(crate::global::GlobalContext {
            budget: 0,
            actor: &actor,
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

                // TODO: Set the inner actor to Uninitialized; so we *know* we drop the future in context.

                system.registry.remove(pid);

                for linked in links.iter().copied() {
                    if let Some(child) = system.registry.lookup_pid(linked) {
                        child.send_signal(Signal::Exit(pid, exit.clone()));

                        system.schedule(linked);
                    }
                }
            }
        }

        crate::global::reset_context();

        control_block.is_running.store(false, Ordering::Release);
    }
}
