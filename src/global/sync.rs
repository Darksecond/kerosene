//! Module with synchronous operations.
//!
//! All of these are safe to use from any unmanaged thread.
//!
//! You can spawn a new unmanaged thread using [`crate::thread::spawn`].

use std::time::Duration;

use crate::{
    Exit, IntoAsyncActor, Pid,
    actor::{MAX_META_KV, Signal, ToPid},
    metadata::MetaKeyValue,
    utils::UnsortedSet,
};

/// Sends a signal to an actor.
///
/// If the actor is not found, the signal is dropped.
pub fn send_signal(to: impl ToPid, message: Signal) {
    let system = unsafe { crate::thread::borrow() };

    let pid = to.to_reference(&system.registry);

    if let Some(actor) = system.registry.lookup_pid(pid) {
        actor.send_signal(message);
        system.schedule(pid);
    }
}

/// Schedule a message to be delivered to an actor after a given delay.
///
/// If the actor is not found, the signal is dropped.
pub fn schedule<T>(to: impl ToPid, message: T, delay: Duration)
where
    T: Send + 'static,
{
    let system = unsafe { crate::thread::borrow() };

    let to = to.to_reference(&system.registry);
    system.timer.add(to, delay, message);
}

/// Send a message to an actor.
///
/// If the actor is not found, the message is dropped.
/// an actor can either be a `Pid` or a `NamedRef`.
pub fn send<M>(to: impl ToPid, message: M)
where
    M: Send + 'static,
{
    let message = Signal::Message(Box::new(message));
    send_signal(to, message);
}

/// Stops the system
pub fn stop() {
    let system = unsafe { crate::thread::borrow() };

    system.stop_all();
}

/// Gets all the metadata for the current actor.
///
/// If ran from an unmanaged thread without a valid context,
/// an empty `UnsortedSet` will be returned.
pub fn metadata() -> UnsortedSet<MetaKeyValue, MAX_META_KV> {
    if super::has_context() {
        super::context().actor.metadata().clone()
    } else {
        UnsortedSet::new()
    }
}

/// Returns the current actors' PID
///
/// If ran from an unmanaged thread without a valid context,
/// `Pid::invalid()` will be returned.
pub fn pid() -> Pid {
    if super::has_context() {
        super::context().pid()
    } else {
        Pid::invalid()
    }
}

/// Sends an exit signal to the chosen actor.
pub fn exit(to: impl ToPid, reason: Exit) {
    let system = unsafe { crate::thread::borrow() };

    let to = to.to_reference(&system.registry);
    send_signal(to, Signal::Exit(to, reason));
}

/// Spawns a new actor.
///
/// The spawned actor will not be linked to the current actor.
/// The Pid of the spawned actor is returned.
pub fn spawn<B>(behavior: B) -> Pid
where
    B: IntoAsyncActor,
{
    use crate::actor::{ActorControlBlock, HydratedActor};
    use std::sync::{Mutex, atomic::Ordering};

    let metadata = metadata();
    let system = unsafe { crate::thread::borrow() };

    let pid = system.registry.allocate_pid();

    let spawn_at = if super::has_context() {
        let context = super::context();
        context
            .actor
            .control_block()
            .worker_id
            .load(Ordering::Acquire) as _
    } else {
        // TODO: Better algorithm than just blindly pick worker 0.
        0
    };

    let mut control_block = ActorControlBlock::new(pid, spawn_at);
    control_block.metadata = Mutex::new(metadata);

    let actor = HydratedActor::new(control_block, behavior);

    system.registry.add(actor);
    system.schedule(pid);

    pid
}

/// Register a name for an actor
pub fn register(name: &'static str, actor: Pid) {
    let system = unsafe { crate::thread::borrow() };

    system.registry.register(name, actor);
}
