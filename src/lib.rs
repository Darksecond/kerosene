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
mod port;
mod registry;
mod scheduler;
mod system;
mod timer;
mod utils;
mod worker;

pub use actor::{Exit, Pid, TrapExitMessage, TrapPortExitMessage};
pub use async_actor::IntoAsyncActor;
pub use port::{Port, PortPid, PortRef};

fn main_actor<A>(actor: A) -> impl IntoAsyncActor
where
    A: IntoAsyncActor,
{
    async move || {
        let mut actor = Some(actor);
        let supervisor = Supervisor::spawn_linked(Strategy::OneForOne);

        supervisor.supervise(RestartPolicy::Permanent, || logger_actor);

        global::schedule(global::pid(), (), Duration::from_millis(10));

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

        let system = system.clone();
        std::thread::spawn(move || {
            worker.run(system.clone());
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

        let actor = HydratedActor::new(&system.scheduler, control_block, main_actor(entry_point));

        system.registry.add(actor);

        system.scheduler.schedule(pid);
    }

    let timer_handle = {
        let timer = system.timer.clone();
        std::thread::spawn(move || {
            timer.run(system.clone());
        })
    };

    for handle in handles {
        handle.join().unwrap();
    }
    timer_handle.join().unwrap();
}

#[macro_export]
macro_rules! main {
    ($actor:expr) => {
        fn main() {
            $crate::run($actor);
        }
    };
}
