use std::{num::NonZero, sync::Arc, time::Duration};

use crate::{
    actor::{ActorControlBlock, HydratedActor, NamedRef},
    async_actor::IntoAsyncActor,
    logger::{Logger, logger_actor},
    monitor::Monitor,
    registry::Registry,
    scheduler::Scheduler,
    supervisor::{RestartPolicy, Strategy, Supervisor},
    timer::Timer,
    worker::{ActiveWorker, Worker},
};

pub mod actor;
pub mod async_actor;
pub mod cache_padded;
pub mod file;
pub mod global;
pub mod logger;
pub mod monitor;
pub mod pending_once;
pub mod port;
pub mod queue;
pub mod registry;
pub mod scheduler;
pub mod supervisor;
pub mod timer;
pub mod worker;

fn main_actor<A>(actor: A) -> impl IntoAsyncActor
where
    A: IntoAsyncActor,
{
    async move || {
        let mut actor = Some(actor);
        let supervisor = Supervisor::spawn_linked(Strategy::OneForOne);

        supervisor.supervise_named(
            NamedRef::new("global_logger"),
            RestartPolicy::Permanent,
            || logger_actor,
        );

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

    let _ = scheduler.replace_slot(id, ActiveWorker { handle, worker });
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

    Monitor::new(scheduler, registry).run();
}

#[macro_export]
macro_rules! main {
    ($actor:expr) => {
        fn main() {
            $crate::run($actor);
        }
    };
}
