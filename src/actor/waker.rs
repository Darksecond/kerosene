use std::sync::{Arc, Weak};

use crate::{actor::Pid, system::System};

pub struct ActorWaker {
    system: Weak<System>,
    pid: Pid,
}

impl ActorWaker {
    pub fn new(system: &Arc<System>, pid: Pid) -> Self {
        ActorWaker {
            system: Arc::downgrade(system),
            pid,
        }
    }
}

impl std::task::Wake for ActorWaker {
    fn wake(self: Arc<Self>) {
        if let Some(system) = Weak::upgrade(&self.system) {
            system.schedule(self.pid);
        }
    }
}
