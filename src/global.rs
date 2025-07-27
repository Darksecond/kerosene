//! Actor context
//!
//! This module provides functions that can be used within an actor.
mod receive;

use std::{
    any::Any,
    cell::Cell,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use crate::{
    actor::{ActorControlBlock, Exit, HydratedActor, HydratedActorBase, Pid, Signal, ToPid},
    async_actor::IntoAsyncActor,
    port::{Port, PortPid, PortRef, PortTable},
    registry::Registry,
    scheduler::Scheduler,
    timer::Timer,
};

thread_local! {
    static CONTEXT: Cell<*mut GlobalContext> = const { Cell::new(std::ptr::null_mut()) };
}

#[allow(dead_code)]
pub(crate) struct GlobalContext<'a> {
    pub(crate) budget: usize,
    pub(crate) actor: &'a Pin<Arc<dyn HydratedActorBase>>,
    pub(crate) registry: &'a Arc<Registry>,
    pub(crate) scheduler: &'a Arc<Scheduler>,
    pub(crate) timer: &'a Arc<Timer>,
    pub(crate) ports: *mut PortTable,

    pub(crate) _marker: PhantomData<*const ()>,
}

impl<'a> GlobalContext<'a> {
    #[inline]
    pub fn pid(&self) -> Pid {
        self.actor.control_block().pid
    }

    #[inline]
    pub fn ports(&mut self) -> &mut PortTable {
        unsafe { &mut *self.ports }
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

/// Returns the current actors' PID
pub fn pid() -> Pid {
    context().pid()
}

/// Stops the system
pub fn stop() {
    context().scheduler.stop_all();
}

/// Register a name for an actor
pub fn register(name: &'static str, actor: Pid) {
    context().registry.register(name, actor);
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
    context().timer.wake_up(pid(), duration);
    let now = Instant::now();

    Sleep(now, duration)
}

/// Sends a signal to an actor.
///
/// If the actor is not found, the signal is dropped.
pub fn send_signal(to: Pid, message: Signal) {
    let context = context();

    if let Some(actor) = context.registry.lookup_pid(to) {
        actor.send_signal(message);
        context.scheduler.schedule(to);
    }
}

/// Schedule a message to be delivered to an actor after a given delay.
///
/// If the actor is not found, the signal is dropped.
pub fn schedule<T>(to: Pid, message: T, delay: Duration)
where
    T: Send + 'static,
{
    context().timer.add(to, delay, message);
}

/// Send a message to an actor.
///
/// If the actor is not found, the message is dropped.
/// an actor can either be a `Pid` or a `NamedRef`.
pub fn send<M>(to: impl ToPid, message: M)
where
    M: Send + 'static,
{
    let context = context();
    let message = Signal::Message(Box::new(message));

    let pid = to.to_reference(&context.registry);

    if let Some(actor) = context.registry.lookup_pid(pid) {
        actor.send_signal(message);
        context.scheduler.schedule(pid);
    }
}

/// Spawns a new actor.
///
/// The spawned actor will not be linked to the current actor.
/// The Pid of the spawned actor is returned.
pub fn spawn<B>(behavior: B) -> Pid
where
    B: IntoAsyncActor,
{
    let context = context();
    let pid = context.registry.allocate_pid();

    let spawn_at = context
        .actor
        .control_block()
        .worker_id
        .load(Ordering::Acquire) as _;

    let control_block = ActorControlBlock::new(pid, spawn_at);

    let actor = HydratedActor::new(context.scheduler, control_block, behavior);

    context.registry.add(actor);

    context.scheduler.schedule(pid);

    pid
}

/// Spawns a new actor and links it to the current actor.
///
/// The Pid of the spawned actor is returned.
pub fn spawn_linked<B>(behavior: B) -> Pid
where
    B: IntoAsyncActor,
{
    let context = context();
    let pid = context.pid();
    let new_pid = context.registry.allocate_pid();

    let spawn_at = context
        .actor
        .control_block()
        .worker_id
        .load(Ordering::Acquire) as _;

    let control_block = ActorControlBlock::new(new_pid, spawn_at);

    let _ = control_block.add_link(pid);

    let actor = HydratedActor::new(context.scheduler, control_block, behavior);

    let _ = context.actor.control_block().add_link(new_pid);

    context.registry.add(actor);

    context.scheduler.schedule(new_pid);

    new_pid
}

pub fn create_port<P>(port: P) -> PortRef<P>
where
    P: Port,
{
    let pid = pid();
    let port = context_mut().ports().create(
        context().scheduler.clone(),
        context().registry.clone(),
        pid,
        port,
    );

    // TODO: Introduce port signals
    context_mut()
        .ports()
        .get_mut(port.port_pid())
        .unwrap()
        .start();

    context().actor.ports().insert(port.port_pid());

    port
}

pub fn close_port(port: impl Into<PortPid>) {
    let port = port.into();
    context_mut().ports().close(port, Exit::Normal);
    context().actor.ports().remove(&port);
}

pub fn send_port<P>(port: PortRef<P>, message: P::Message)
where
    P: Port,
{
    let context = context();

    context
        .registry
        .ports
        .send(context.scheduler, port, message);
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
            if context().budget >= MAX_BUDGET {
                context_mut().budget = 0;
                context().scheduler.schedule(pid());

                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        }
    }

    context_mut().budget += budget;

    YieldNow
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
        context().timer.wake_up(pid(), timeout);
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
