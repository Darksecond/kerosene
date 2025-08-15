//! Actor context
//!
//! This module provides functions that can be used within an actor.
mod receive;
pub mod sync;

use std::{
    any::Any,
    cell::Cell,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};

use crate::{
    actor::{ActorControlBlock, Exit, HydratedActor, HydratedActorBase, Pid, Signal, ToPid},
    async_actor::IntoAsyncActor,
    metadata::{MetaKeyValue, MetaValue},
};

thread_local! {
    static CONTEXT: Cell<*mut GlobalContext> = const { Cell::new(std::ptr::null_mut()) };
}

#[allow(dead_code)]
pub(crate) struct GlobalContext<'a> {
    pub(crate) budget: usize,
    pub(crate) actor: &'a Pin<Arc<dyn HydratedActorBase>>,

    pub(crate) _marker: PhantomData<*const ()>,
}

impl<'a> GlobalContext<'a> {
    #[inline]
    pub fn pid(&self) -> Pid {
        self.actor.control_block().pid
    }
}

#[doc(hidden)]
pub enum RecvError {
    Timeout,
}

fn context<'a>() -> &'static GlobalContext<'a> {
    CONTEXT.with(|ctx| unsafe { &*(ctx.get() as *const GlobalContext) })
}

fn context_mut<'a>() -> &'static mut GlobalContext<'a> {
    CONTEXT.with(|ctx| unsafe { &mut *(ctx.get() as *mut GlobalContext) })
}

pub(crate) fn set_context(context: *mut GlobalContext) {
    CONTEXT.with(|ctx| ctx.set(context as *mut _));
}

pub(crate) fn reset_context() {
    CONTEXT.with(|ctx| ctx.set(std::ptr::null_mut()));
}

pub(crate) fn has_context() -> bool {
    !CONTEXT.get().is_null()
}

/// Sends an exit signal to the chosen actor.
///
/// If the actor is the current actor, it will yield immediately.
/// Otherwise, it will add one to the budget.
pub async fn exit(to: impl ToPid, reason: Exit) {
    let system = unsafe { crate::thread::borrow() };
    let to = to.to_reference(&system.registry);

    sync::exit(to, reason);

    if to == sync::pid() {
        yield_immediate().await
    } else {
        yield_now(1).await;
    }
}

/// Traps the exit signal
///
/// Normally when an actor receives a exit signal from a linked actor, it will exit itself if the reason is not `Exit::Normal`.
/// However when exits are trapped they are unconditionally turned into a message.
///
/// The following messages can be received when trap_exit is set:
/// - `TrapExitMessage`: when a linked actor dies.
/// - `TrapPortExitMessage`: when a linked port dies.
pub fn trap_exit(should_trap: bool) {
    context()
        .actor
        .control_block()
        .trap_exit
        .store(should_trap, Ordering::Relaxed);
}

/// Sleeps for a given duration
///
/// This will spend 1 budget unit.
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    struct Sleep(Instant, Duration);

    impl Future for Sleep {
        type Output = ();

        fn poll(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            if self.0.elapsed() < self.1 {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        }
    }

    // We don't use yield_now here because we're already going to sleep.
    context_mut().budget += 1;
    let system = unsafe { crate::thread::borrow() };
    system.timer.wake_up(sync::pid(), duration);
    let now = Instant::now();

    Sleep(now, duration)
}

/// Sends a signal to an actor.
///
/// If the actor is not found, the signal is dropped.
pub async fn send_signal(to: impl ToPid, message: Signal) {
    yield_now(1).await;
    sync::send_signal(to, message);
}

/// Schedule a message to be delivered to an actor after a given delay.
///
/// If the actor is not found, the signal is dropped.
pub async fn schedule<T>(to: Pid, message: T, delay: Duration)
where
    T: Send + 'static,
{
    yield_now(1).await;
    sync::schedule(to, message, delay);
}

/// Send a message to an actor.
///
/// If the actor is not found, the message is dropped.
/// an actor can either be a `Pid` or a `NamedRef`.
pub async fn send<M>(to: impl ToPid, message: M)
where
    M: Send + 'static,
{
    yield_now(1).await;
    sync::send(to, message);
}

/// Spawns a new actor.
///
/// The spawned actor will not be linked to the current actor.
/// The Pid of the spawned actor is returned.
pub async fn spawn<B>(behavior: B) -> Pid
where
    B: IntoAsyncActor,
{
    yield_now(1).await;
    sync::spawn(behavior)
}

// TODO: Make async
/// Spawns a new actor and links it to the current actor.
///
/// The Pid of the spawned actor is returned.
pub fn spawn_linked<B>(behavior: B) -> Pid
where
    B: IntoAsyncActor,
{
    let system = unsafe { crate::thread::borrow() };
    let context = context();
    let pid = context.pid();
    let new_pid = system.registry.allocate_pid();

    let spawn_at = context
        .actor
        .control_block()
        .worker_id
        .load(Ordering::Acquire) as _;

    let mut control_block = ActorControlBlock::new(new_pid, spawn_at);
    control_block.metadata = Mutex::new(context.actor.metadata().clone());

    let _ = control_block.add_link(pid);

    let actor = HydratedActor::new(control_block, behavior);

    let _ = context.actor.control_block().add_link(new_pid);

    system.registry.add(actor);

    system.schedule(new_pid);

    new_pid
}

/// Yield the current actor if the budget is spent.
///
/// # Parameters
///
/// * `budget`: The amount of budget to spend.
///
/// This allows other actors to run.
/// If you use the `receive!` macro, that will automatically yield.
pub fn yield_now(budget: usize) -> impl Future<Output = ()> {
    const MAX_BUDGET: usize = 16;

    struct YieldNow;

    impl Future for YieldNow {
        type Output = ();

        fn poll(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let system = unsafe { crate::thread::borrow() };
            if context().budget >= MAX_BUDGET {
                context_mut().budget = 0;
                system.schedule(sync::pid());

                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        }
    }

    context_mut().budget += budget;

    YieldNow
}

// TODO: Implement this better.
pub async fn yield_immediate() {
    yield_now(16).await;
}

/// Insert or update metadata for the current actor.
pub fn insert_metadata(key: &'static str, value: impl Into<MetaValue>) {
    context().actor.metadata().insert(MetaKeyValue {
        key,
        value: value.into(),
    });
}

// TODO: We should consider tracking where we are in the message queue and resume from there, since obviously none of the previous messages matched.
#[doc(hidden)]
#[must_use]
pub async fn recv_matching<F>(
    timeout: Option<Duration>,
    matcher: F,
) -> Result<Box<dyn Any + Send>, RecvError>
where
    F: Fn(&Box<dyn Any + Send>) -> bool,
{
    let now = Instant::now();

    if let Some(timeout) = timeout {
        let system = unsafe { crate::thread::borrow() };
        system.timer.wake_up(sync::pid(), timeout);
    }

    yield_now(0).await;

    std::future::poll_fn(move |_cx| {
        if let Some(timeout) = timeout {
            // Handle timeouts
            if now.elapsed() >= timeout {
                return std::task::Poll::Ready(Err(RecvError::Timeout));
            }
        }

        if let Some(message) = context().actor.queue().remove_matching(&matcher) {
            context_mut().budget += 1;
            std::task::Poll::Ready(Ok(message))
        } else {
            std::task::Poll::Pending
        }
    })
    .await
}
