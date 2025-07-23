mod control_block;
mod inbox;
mod message_queue;
mod references;
mod waker;

use std::{
    any::Any,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, atomic::Ordering},
};

use crate::{
    actor::waker::ActorWaker, async_actor::IntoAsyncActor, port::PortTable, scheduler::Scheduler,
    utils::UnsortedSet,
};

pub use control_block::{ActorControlBlock, MAX_LINKS};
pub use inbox::Inbox;
pub use message_queue::*;
pub use references::*;

pub trait HydratedActorBase: Send + Sync + 'static {
    fn send_signal(&self, message: Signal);

    fn control_block(&self) -> &ActorControlBlock;

    fn poll(self: Pin<&Self>) -> Option<Exit>;

    fn has_messages(&self) -> bool;

    fn ports(&self) -> MutexGuard<PortTable>;
    fn queue(&self) -> MutexGuard<MessageQueue>;
    fn links(&self) -> MutexGuard<UnsortedSet<Pid, MAX_LINKS>>;
}

pub struct TrapExitMessage {
    pub pid: Pid,
    pub reason: Exit,
}

impl<B> HydratedActorBase for HydratedActor<B>
where
    B: IntoAsyncActor,
{
    fn ports(&self) -> MutexGuard<PortTable> {
        self.ports.lock().expect("Failed to acquire lock")
    }

    fn queue(&self) -> MutexGuard<MessageQueue> {
        self.messages.lock().expect("Failed to acquire lock")
    }

    fn links(&self) -> MutexGuard<UnsortedSet<Pid, MAX_LINKS>> {
        self.control_block
            .links
            .lock()
            .expect("Failed to acquire lock")
    }

    fn send_signal(&self, message: Signal) {
        self.inbox.push(message)
    }

    fn control_block(&self) -> &ActorControlBlock {
        &self.control_block
    }

    fn poll(self: Pin<&Self>) -> Option<Exit> {
        if let Some(signal) = self.inbox.pop() {
            match signal {
                Signal::Exit(pid, reason) => {
                    // Remove the link if one existed.
                    self.links().remove(&pid);

                    if self.control_block.trap_exit.load(Ordering::Relaxed) {
                        self.messages
                            .lock()
                            .unwrap()
                            .push(Box::new(TrapExitMessage { pid, reason }));
                    } else if reason != Exit::Normal {
                        return Some(reason);
                    }
                }
                Signal::Kill => return Some(Exit::Killed),
                Signal::Link(pid) => {
                    let _ = self.control_block.add_link(pid);
                }
                Signal::Unlink(pid) => {
                    let _ = self.control_block.remove_link(pid);
                }
                Signal::TimerFired => {
                    // We don't need to do anything but run the future.
                }
                Signal::Message(msg) => {
                    self.messages.lock().unwrap().push(msg);
                }
            }
        }

        let mut actor = self.actor.lock().expect("Failed to acquire lock");
        actor.to_running();

        if let ActorState::Running(future) = &mut *actor {
            let waker = self.waker.clone().into();
            let mut cx = std::task::Context::from_waker(&waker);

            // SAFETY: This is OK because we are not moving the future out of the actor and the actor is pinned.
            let future = unsafe { Pin::new_unchecked(future) };

            let status = Future::poll(future, &mut cx);

            match status {
                std::task::Poll::Ready(exit) => Some(exit),
                std::task::Poll::Pending => None,
            }
        } else {
            None
        }
    }

    fn has_messages(&self) -> bool {
        !self.inbox.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Graceful shutdown (actor chose to exit normally)
    Normal,

    /// Actor panicked (e.g. converted via catch_unwind)
    Panic(String),

    /// Actor was shut down as part of system halt
    Shutdown,

    /// Actor was killed intentionally (e.g. supervisor or monitor)
    Killed,
}

pub enum Signal {
    Exit(Pid, Exit),
    Kill,
    Link(Pid),
    Unlink(Pid),
    TimerFired,
    Message(Box<dyn Any + Send>),
}

enum ActorState<A>
where
    A: IntoAsyncActor,
{
    Uninitialized,
    Waiting(A),
    Running(A::Actor),
}

impl<A> ActorState<A>
where
    A: IntoAsyncActor,
{
    fn to_running(&mut self) {
        if matches!(self, ActorState::Running(_)) {
            return;
        }

        let actor = std::mem::replace(self, ActorState::Uninitialized);
        *self = actor.to_running_inner();
    }

    fn to_running_inner(self) -> ActorState<A> {
        match self {
            ActorState::Waiting(actor) => ActorState::Running(actor.into_async_actor()),
            _ => self,
        }
    }
}

pub struct HydratedActor<A>
where
    A: IntoAsyncActor,
{
    pub(crate) control_block: ActorControlBlock,
    pub inbox: Inbox<Signal>,
    waker: Arc<ActorWaker>,
    messages: Mutex<MessageQueue>,
    ports: Mutex<PortTable>,
    actor: Mutex<ActorState<A>>,
}

impl<A> HydratedActor<A>
where
    A: IntoAsyncActor,
{
    pub(crate) fn new(
        scheduler: &Arc<Scheduler>,
        control_block: ActorControlBlock,
        actor: A,
    ) -> Self {
        let pid = control_block.pid;

        Self {
            control_block,
            inbox: Inbox::new(),
            waker: Arc::new(ActorWaker::new(scheduler.clone(), pid)),
            actor: Mutex::new(ActorState::Waiting(actor)),
            ports: Mutex::new(PortTable::new(pid)),
            messages: Mutex::new(MessageQueue::new()),
        }
    }
}
