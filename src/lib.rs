use std::{num::NonZero, sync::Arc, thread::JoinHandle, time::Duration};

use crate::{
    actor::{ActorControlBlock, HydratedActor},
    library::{
        logger::{info, logger_actor},
        supervisor::{RestartPolicy, Strategy, Supervisor},
    },
    system::System,
    worker::{ActiveWorker, Worker},
};

mod actor;
mod async_actor;
pub mod global;
pub mod library;
mod metadata;
mod migration;
mod registry;
mod scheduler;
mod system;
pub mod thread;
mod timer;
mod utils;
mod worker;

pub use actor::{Exit, Pid, TrapExitMessage};
pub use async_actor::IntoAsyncActor;

fn main_actor<A>(actor: A) -> impl IntoAsyncActor
where
    A: IntoAsyncActor,
{
    async move || {
        let mut actor = Some(actor);
        let supervisor = Supervisor::spawn_linked(Strategy::OneForOne);

        supervisor.supervise(RestartPolicy::Permanent, || logger_actor);
        supervisor.supervise(RestartPolicy::Permanent, || library::blocking::router);

        global::schedule(global::sync::pid(), (), Duration::from_millis(10)).await;

        loop {
            receive! {
                match () {
                    _ => {
                        info("System started").emit();

                        if let Some(actor) = actor.take() {
                            global::spawn_linked(actor);
                        }
                    }
                }
            }
        }
    }
}

fn start_worker(system: Arc<System>) -> JoinHandle<()> {
    let id = system.scheduler.allocate_slot();

    let worker = Arc::new(Worker::new(id));

    let handle = {
        let worker = worker.clone();

        crate::thread::spawn(move || {
            worker.run();
        })
    };

    let _ = system.scheduler.replace_slot(
        id,
        ActiveWorker {
            thread: handle.thread().clone(),
            worker,
        },
    );

    handle
}

pub fn run<A>(entry_point: A)
where
    A: IntoAsyncActor,
{
    let system = System::new();
    crate::thread::give(system.clone());

    let handles = {
        let cores = std::thread::available_parallelism()
            .map(NonZero::get)
            .unwrap_or(1);

        (0..cores)
            .map(|_| start_worker(system.clone()))
            .collect::<Vec<_>>()
    };

    {
        let pid = system.registry.allocate_pid();

        let control_block = ActorControlBlock::new(pid, 0);

        let actor = HydratedActor::new(control_block, main_actor(entry_point));

        system.registry.add(actor);

        system.schedule(pid);
    }

    let timer_handle = {
        crate::thread::spawn(move || {
            let system = unsafe { crate::thread::borrow() };
            system.timer.run();
        })
    };

    for handle in handles {
        handle.join().unwrap();
    }
    timer_handle.join().unwrap();

    drop(unsafe { crate::thread::get() });
}

#[macro_export]
macro_rules! main {
    ($actor:expr) => {
        fn main() {
            $crate::run($actor);
        }
    };
}
