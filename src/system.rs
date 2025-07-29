use std::sync::Arc;

use crate::{registry::Registry, scheduler::Scheduler, timer::Timer};

pub struct System {
    pub registry: Arc<Registry>,
    pub scheduler: Arc<Scheduler>,
    pub timer: Arc<Timer>,
}

impl System {
    pub fn new() -> Arc<Self> {
        let registry = Arc::new(Registry::new());
        let scheduler = Arc::new(Scheduler::new(registry.clone()));
        let timer = Arc::new(Timer::new());

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
}
