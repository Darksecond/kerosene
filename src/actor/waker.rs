use std::sync::Arc;

use crate::{actor::Pid, scheduler::Scheduler};

pub struct ActorWaker {
    scheduler: Arc<Scheduler>,
    pid: Pid,
}

impl ActorWaker {
    pub fn new(scheduler: Arc<Scheduler>, pid: Pid) -> Self {
        ActorWaker { scheduler, pid }
    }
}

impl std::task::Wake for ActorWaker {
    fn wake(self: Arc<Self>) {
        self.scheduler.schedule(self.pid);
    }
}
