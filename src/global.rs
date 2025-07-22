use std::{
    any::Any,
    cell::Cell,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use crate::{
    actor::{ActorControlBlock, HydratedActor, HydratedActorBase, NamedRef, Pid, Signal, ToPid},
    async_actor::IntoAsyncActor,
    pending_once::pending_once,
    port::{Port, PortContext, PortRef},
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

    pub(crate) _marker: PhantomData<*const ()>,
}

impl<'a> GlobalContext<'a> {
    #[inline]
    pub fn pid(&self) -> Pid {
        self.actor.control_block().pid
    }
}

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

pub fn pid() -> Pid {
    context().pid()
}

pub fn stop() {
    context().scheduler.stop_all();
}

pub fn register(name: NamedRef, actor: Pid) {
    context().registry.register(name, actor);
}

pub fn trap_exit(should_trap: bool) {
    context()
        .actor
        .control_block()
        .trap_exit
        .store(should_trap, Ordering::Relaxed);
}

pub async fn sleep(duration: Duration) {
    context().timer.wake_up(pid(), duration);
    let now = Instant::now();

    while now.elapsed() < duration {
        pending_once().await;
    }
}

pub fn send_signal(to: Pid, message: Signal) {
    let context = context();

    if let Some(actor) = context.registry.lookup_pid(to) {
        actor.send_signal(message);
        context.scheduler.schedule(to);
    }
}

pub fn schedule<T>(to: Pid, message: T, delay: Duration)
where
    T: Send + 'static,
{
    context().timer.add(to, delay, Box::new(message));
}

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
    let context = context();
    let port = context.actor.ports().create(port);

    let port_context = PortContext {
        port: port.port_pid(),
        scheduler: context.scheduler.clone(),
        registry: context.registry.clone(),
    };

    if let Some(port) = context.actor.ports().get_mut(port) {
        port.start(port_context);
    }

    port
}

pub fn close_port<P>(port: PortRef<P>)
where
    P: Port,
{
    let context = context();
    let port_context = PortContext {
        port: port.port_pid(),
        scheduler: context.scheduler.clone(),
        registry: context.registry.clone(),
    };

    if let Some(port) = context.actor.ports().get_mut(port) {
        port.stop(port_context);
    }
    context.actor.ports().close(port);
}

pub fn send_port<P>(port: PortRef<P>, message: P::Message)
where
    P: Port,
{
    let context = context();
    let pid = port.port_pid();
    if let Some(port) = context.actor.ports().get_mut(port) {
        let port_context = PortContext {
            port: pid,
            scheduler: context.scheduler.clone(),
            registry: context.registry.clone(),
        };

        port.receive(port_context, message);
    }
}

#[must_use]
pub async fn recv_matching<F>(
    timeout: Option<Duration>,
    matcher: F,
) -> Result<Box<dyn Any + Send>, RecvError>
where
    F: Fn(&Box<dyn Any + Send>) -> bool,
{
    const MAX_BUDGET: usize = 16;

    let now = Instant::now();

    if let Some(timeout) = timeout {
        context().timer.wake_up(pid(), timeout);
    }

    if context().budget >= MAX_BUDGET {
        context_mut().budget = 0;
        context().scheduler.schedule(pid());
        pending_once().await;
    }

    std::future::poll_fn(move |_cx| {
        if let Some(timeout) = timeout {
            // Handle timeouts
            if now.elapsed() >= timeout {
                return std::task::Poll::Ready(Err(RecvError::Timeout));
            }
        }

        if let Some(message) = context().actor.pop_matching(&matcher) {
            context_mut().budget += 1;
            std::task::Poll::Ready(Ok(message))
        } else {
            std::task::Poll::Pending
        }
    })
    .await
}

#[macro_export]
macro_rules! receive {
    ({
        $(
            match $ty:ty: $pat:pat_param  $( if $guard:expr )? => $block:block
        ),+ $(,)?
        $( after $timeout:expr => $timeout_block:block )? $(,)?
    }) => {{
        #[allow(unused_variables)]
        let timeout: Option<std::time::Duration> = None;
        $( let timeout = Some($timeout); )?

        let msg = $crate::global::recv_matching(timeout, |msg| {
                $(
                  if let Some(msg) = msg.downcast_ref::<$ty>() {
                      #[allow(unused_variables)]
                      #[allow(irrefutable_let_patterns)]
                      if let $pat = msg {
                          $(if !$guard { return false;} )?
                          return true;
                      }
                  }
                )*

                false
            })
            .await;

        match msg {
            Ok(msg) => {
                'inner: loop {
                $(
                    if let Some(msg_ref) = msg.downcast_ref::<$ty>() {
                        #[allow(unused_variables)]
                        #[allow(irrefutable_let_patterns)]
                        let is_match = if let $pat = msg_ref {
                            true
                        } else { false };

                        if is_match {
                            if let Some($pat) = msg.downcast::<$ty>().ok().map(|msg| *msg) {
                                $block
                            }

                            break 'inner;
                        }
                    }
                )*

                    unreachable!("recv_matching returned a message that did not match any arm");

                }
            },
            Err(_) => {
                $( $timeout_block )?
            },
        }

    }};
}
