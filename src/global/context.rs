use std::sync::Arc;

use crate::{
    Exit, Pid,
    actor::{Signal, ToPid},
    registry::Registry,
    scheduler::Scheduler,
};

// TODO: Consider using Weak.
pub struct Context {
    pid: Pid,
    registry: Arc<Registry>,
    scheduler: Arc<Scheduler>,
}

impl Context {
    /// Constructs a new `Context`.
    ///
    /// This can only be constructed within an actor,
    /// but it can be send and used on non-managed threads.
    pub fn new() -> Self {
        let context = super::context();

        Self {
            pid: context.pid(),
            registry: context.system.registry.clone(),
            scheduler: context.system.scheduler.clone(),
        }
    }

    /// Returns the PID of the actor that created this `Context`.
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Sends a signal to an actor.
    ///
    /// If the actor is not found, the signal is dropped.
    pub fn send_signal(&self, to: impl ToPid, message: Signal) {
        let pid = to.to_reference(&self.registry);

        if let Some(actor) = self.registry.lookup_pid(pid) {
            actor.send_signal(message);
            self.scheduler.schedule(pid);
        }
    }

    /// Send a message to an actor.
    ///
    /// If the actor is not found, the message is dropped.
    /// an actor can either be a `Pid` or a `NamedRef`.
    pub fn send<M>(&self, to: impl ToPid, message: M)
    where
        M: Send + 'static,
    {
        let message = Signal::Message(Box::new(message));
        self.send_signal(to, message);
    }

    /// Sends an exit signal to the actor that created this `Context`.
    pub fn exit(&self, exit: Exit) {
        self.send_signal(self.pid, Signal::Exit(self.pid, exit));
    }
}
