use std::{num::NonZero, sync::Arc, time::Duration};

use crate::{
    actor::{ActorControlBlock, HydratedActor},
    library::{
        logger::{Logger, logger_actor},
        supervisor::{RestartPolicy, Strategy, Supervisor},
    },
    registry::Registry,
    scheduler::Scheduler,
    timer::Timer,
    worker::{ActiveWorker, Worker},
};

mod actor;
mod async_actor;
pub mod global;
pub mod library;
mod migration;
mod port;
mod registry;
mod scheduler;
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

        supervisor.supervise_named("global_logger", RestartPolicy::Permanent, || logger_actor);

        global::schedule(global::pid(), (), Duration::from_millis(10));

        loop {
            receive!({
                match (): _ => {
                    Logger::global().debug("System started");

                    if let Some(actor) = actor.take() {
                        global::spawn_linked(actor);
                    }
                }
            });
        }
    }
}

fn start_worker(registry: Arc<Registry>, scheduler: Arc<Scheduler>, timer: Arc<Timer>) {
    let id = scheduler.allocate_slot();

    let worker = Arc::new(Worker::new(id));

    let handle = {
        let worker = worker.clone();
        let scheduler = scheduler.clone();

        std::thread::spawn(move || {
            worker.run(registry, scheduler, timer);
        })
    };

    let _ = scheduler.replace_slot(
        id,
        ActiveWorker {
            thread: handle.thread().clone(),
            handle: Some(handle),
            worker,
        },
    );
}

pub fn run<A>(entry_point: A)
where
    A: IntoAsyncActor,
{
    let registry = Arc::new(Registry::new());
    let scheduler = Arc::new(Scheduler::new(registry.clone()));
    let timer = Arc::new(Timer::new(scheduler.clone(), registry.clone()));

    {
        let cores = std::thread::available_parallelism()
            .map(NonZero::get)
            .unwrap_or(1);

        for _ in 0..cores {
            start_worker(registry.clone(), scheduler.clone(), timer.clone());
        }
    }

    {
        let pid = registry.allocate_pid();

        let control_block = ActorControlBlock::new(pid, 0);

        let actor = HydratedActor::new(&scheduler, control_block, main_actor(entry_point));

        registry.add(actor);

        scheduler.schedule(pid);
    }

    {
        let timer = timer.clone();
        std::thread::spawn(move || {
            timer.run();
        });
    }

    scheduler.wait_all();
}

#[macro_export]
macro_rules! main {
    ($actor:expr) => {
        fn main() {
            $crate::run($actor);
        }
    };
}
