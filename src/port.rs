use std::{
    any::Any,
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{
    actor::{Exit, Inbox, Pid, Signal},
    registry::Registry,
    scheduler::Scheduler,
    worker::WorkerId,
};

pub struct PortInbox<T>
where
    T: Port,
{
    port: PortRef<T>,
    inbox: Inbox<T::Message>,
    is_scheduled: AtomicBool,
}

impl<T> PortInbox<T>
where
    T: Port,
{
    pub fn new(port: PortRef<T>) -> Self {
        Self {
            port,
            inbox: Inbox::new(),
            is_scheduled: AtomicBool::new(false),
        }
    }

    pub fn push(&self, message: T::Message) {
        self.inbox.push(message);
    }

    pub fn schedule(&self, scheduler: &Scheduler) {
        if !self.is_scheduled.swap(true, Ordering::AcqRel) {
            scheduler.schedule_port(self.port.port);
        }
    }
}

pub struct HydratedPort<T>
where
    T: Port,
{
    inbox: Arc<PortInbox<T>>,
    context: Arc<PortContext>,
    port: T,
}

pub trait ErasedHydratedPort {
    fn poll(&mut self) -> Option<Exit>;
    fn start(&mut self);
    fn close(&mut self, reason: Exit);
}

impl<T> ErasedHydratedPort for HydratedPort<T>
where
    T: Port,
{
    fn poll(&mut self) -> Option<Exit> {
        self.inbox.is_scheduled.store(false, Ordering::Release);

        while let Some(message) = self.inbox.inbox.pop() {
            self.port.receive(&self.context, message);
        }

        if !self.inbox.inbox.is_empty() {
            self.context
                .scheduler
                .schedule_port(self.inbox.port.port_pid());
        }

        self.context
            .exit
            .lock()
            .expect("Failed to acquire lock")
            .take()
    }

    fn start(&mut self) {
        self.port.start(&self.context);
    }

    fn close(&mut self, reason: Exit) {
        self.port.stop(&self.context);

        self.context
            .send_signal(Signal::PortExit(self.inbox.port.port_pid(), reason));
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PortPid {
    worker: WorkerId,
    index: u16,
    generation: u16,
}

impl PortPid {
    pub const fn invalid() -> Self {
        Self {
            worker: WorkerId::MAX,
            index: u16::MAX,
            generation: u16::MAX,
        }
    }

    pub const fn worker(&self) -> WorkerId {
        self.worker
    }
}

pub trait Port: 'static {
    type Message: Send + 'static;

    fn receive(&mut self, ctx: &Arc<PortContext>, message: Self::Message);

    fn start(&mut self, ctx: &Arc<PortContext>) {
        let _ = ctx;
    }

    fn stop(&mut self, ctx: &Arc<PortContext>) {
        let _ = ctx;
    }
}

pub struct PortContext {
    owner: AtomicU64,
    registry: Arc<Registry>,
    scheduler: Arc<Scheduler>,
    exit: Mutex<Option<Exit>>,
}

impl PortContext {
    fn new(owner: Pid, registry: Arc<Registry>, scheduler: Arc<Scheduler>) -> Self {
        Self {
            owner: AtomicU64::new(owner.0),
            registry,
            scheduler,
            exit: Mutex::new(None),
        }
    }

    pub fn send_signal(&self, signal: Signal) {
        let owner = self.owner();

        if let Some(actor) = self.registry.lookup_pid(owner) {
            actor.send_signal(signal);
            self.scheduler.schedule(owner);
        }
    }

    pub fn send<M>(&self, message: M)
    where
        M: Send + 'static,
    {
        let message = Signal::Message(Box::new(message));
        self.send_signal(message);
    }

    pub fn exit(&self, reason: Exit) {
        *self.exit.lock().expect("Failed to acquire lock") = Some(reason);
    }

    pub fn owner(&self) -> Pid {
        Pid(self.owner.load(Ordering::Relaxed))
    }
}

#[derive(Debug, PartialEq)]
pub struct PortRef<A> {
    port: PortPid,
    _marker: PhantomData<fn() -> A>,
}

unsafe impl<A> Send for PortRef<A> {}
unsafe impl<A> Sync for PortRef<A> {}

impl<A> PortRef<A> {
    pub unsafe fn new_unchecked(port: PortPid) -> Self {
        Self {
            port,
            _marker: PhantomData,
        }
    }

    pub fn port_pid(&self) -> PortPid {
        self.port
    }

    pub fn invalid() -> Self {
        Self {
            port: PortPid::invalid(),
            _marker: PhantomData,
        }
    }
}

impl<A> From<PortRef<A>> for PortPid {
    fn from(value: PortRef<A>) -> Self {
        value.port_pid()
    }
}

impl<A> Copy for PortRef<A> {}

impl<A> Clone for PortRef<A> {
    fn clone(&self) -> Self {
        *self
    }
}

struct PortEntry {
    generation: u16,
    port: Option<Box<dyn ErasedHydratedPort>>,
}

pub struct PortTable {
    worker: WorkerId,
    ports: Vec<PortEntry>,
}

impl PortTable {
    pub fn new(worker: WorkerId) -> Self {
        Self {
            worker,
            ports: Vec::new(),
        }
    }

    fn allocate(&mut self) -> PortPid {
        for (index, entry) in self.ports.iter_mut().enumerate() {
            if entry.port.is_none() {
                entry.generation = entry.generation.wrapping_add(1);

                return PortPid {
                    worker: self.worker,
                    index: index as u16,
                    generation: entry.generation,
                };
            }
        }

        let index = self.ports.len();

        self.ports.push(PortEntry {
            generation: 0,
            port: None,
        });

        PortPid {
            worker: self.worker,
            index: index as u16,
            generation: 0,
        }
    }

    fn set(&mut self, port: PortPid, value: Box<dyn ErasedHydratedPort>) {
        if let Some(entry) = self.ports.get_mut(port.index as usize) {
            debug_assert_eq!(entry.generation, port.generation);
            entry.port = Some(value);
        }
    }

    pub fn create<P>(
        &mut self,
        scheduler: Arc<Scheduler>,
        registry: Arc<Registry>,
        owner: Pid,
        port: P,
    ) -> PortRef<P>
    where
        P: Port,
    {
        let port_pid = self.allocate();
        let port_ref = unsafe { PortRef::new_unchecked(port_pid) };

        let inbox = Arc::new(PortInbox::new(port_ref));
        let context = Arc::new(PortContext::new(owner, registry.clone(), scheduler));

        self.set(
            port_pid,
            Box::new(HydratedPort {
                port,
                inbox: inbox.clone(),
                context,
            }),
        );

        registry.ports.insert(port_ref, inbox);

        port_ref
    }

    pub fn close(&mut self, port: PortPid, reason: Exit) {
        if let Some(entry) = self.ports.get_mut(port.index as usize) {
            if entry.generation == port.generation {
                if let Some(mut port) = entry.port.take() {
                    port.close(reason);
                }
            }
        }
    }

    pub fn get_mut(&mut self, port: PortPid) -> Option<&mut dyn ErasedHydratedPort> {
        if let Some(entry) = self.ports.get_mut(port.index as usize) {
            if entry.port.is_some() && entry.generation == port.generation {
                if let Some(port) = entry.port.as_mut() {
                    return Some(port.as_mut());
                }
            }
        }

        None
    }
}

pub struct PortInboxTable {
    table: RwLock<HashMap<PortPid, Arc<dyn Any + Send + Sync + 'static>>>,
}

impl PortInboxTable {
    pub fn new() -> Self {
        Self {
            table: RwLock::new(HashMap::new()),
        }
    }

    pub fn clear(&self) {
        let mut table = self.table.write().expect("Failed to acquire lock");
        table.clear();
    }

    pub fn insert<T>(&self, port_ref: PortRef<T>, inbox: Arc<PortInbox<T>>)
    where
        T: Port,
    {
        let mut table = self.table.write().expect("Failed to acquire lock");
        table.insert(port_ref.port, inbox);
    }

    pub fn remove(&self, port_pid: PortPid) {
        let mut table = self.table.write().expect("Failed to acquire lock");
        table.remove(&port_pid);
    }

    pub fn send<T>(&self, scheduler: &Scheduler, port_ref: PortRef<T>, message: T::Message)
    where
        T: Port,
    {
        let table = self.table.read().expect("Failed to acquire lock");

        if let Some(inbox) = table.get(&port_ref.port) {
            let inbox = inbox
                .downcast_ref::<PortInbox<T>>()
                .expect("Downcast mismatch");

            inbox.push(message);
            inbox.schedule(scheduler);
        }
    }
}
