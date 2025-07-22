use std::{marker::PhantomData, sync::Arc};

use crate::{
    actor::{Pid, Signal},
    registry::Registry,
    scheduler::Scheduler,
};

pub enum Reason {
    Normal,
}

pub struct PortExit {
    pub port: PortPid,
    pub reason: Reason,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PortPid {
    pid: Pid,
    index: u16,
    generation: u16,
}

impl PortPid {
    pub const fn invalid() -> Self {
        Self {
            pid: Pid::invalid(),
            index: u16::MAX,
            generation: u16::MAX,
        }
    }
}

pub trait ErasedPort: Send {}
impl<T> ErasedPort for T where T: Port {}

pub trait Port: Send + 'static {
    type Message: Send + 'static;

    fn receive(&mut self, ctx: PortContext, message: Self::Message);

    fn start(&mut self, ctx: PortContext) {
        let _ = ctx;
    }

    fn stop(&mut self, ctx: PortContext) {
        let _ = ctx;
    }
}

pub struct PortContext {
    pub(crate) port: PortPid,
    pub(crate) registry: Arc<Registry>,
    pub(crate) scheduler: Arc<Scheduler>,
}

impl PortContext {
    pub fn send<M>(&self, message: M)
    where
        M: Send + 'static,
    {
        let message = Signal::Message(Box::new(message));

        if let Some(actor) = self.registry.lookup_pid(self.port.pid) {
            actor.send_signal(message);
            self.scheduler.schedule(self.port.pid);
        }
    }

    pub fn exit(&self, reason: Reason) {
        self.send(PortExit {
            port: self.port,
            reason,
        });
    }
}

#[derive(Debug, PartialEq)]
pub struct PortRef<A> {
    port: PortPid,
    _marker: PhantomData<fn() -> A>,
}

impl<A> PortRef<A> {
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

impl<A> Copy for PortRef<A> {}

impl<A> Clone for PortRef<A> {
    fn clone(&self) -> Self {
        *self
    }
}

struct PortEntry {
    generation: u16,
    port: Option<Box<dyn ErasedPort>>,
}

pub struct PortTable {
    pid: Pid,
    ports: Vec<PortEntry>,
}

impl PortTable {
    pub fn new(pid: Pid) -> Self {
        Self {
            pid,
            ports: Vec::new(),
        }
    }

    pub fn create<P>(&mut self, port: P) -> PortRef<P>
    where
        P: Port,
    {
        for (index, entry) in self.ports.iter_mut().enumerate() {
            if entry.port.is_none() {
                entry.generation = entry.generation.wrapping_add(1);
                entry.port = Some(Box::new(port));

                return PortRef {
                    port: PortPid {
                        pid: self.pid,
                        index: index as u16,
                        generation: entry.generation,
                    },
                    _marker: PhantomData,
                };
            }
        }

        let index = self.ports.len();

        self.ports.push(PortEntry {
            generation: 0,
            port: Some(Box::new(port)),
        });

        PortRef {
            port: PortPid {
                pid: self.pid,
                index: index as u16,
                generation: 0,
            },
            _marker: PhantomData,
        }
    }

    pub fn close<P>(&mut self, port: PortRef<P>) {
        if port.port.pid != self.pid {
            // This is not your port, you cannot close it.
            return;
        }

        if let Some(entry) = self.ports.get_mut(port.port.index as usize) {
            if entry.port.is_some() && entry.generation == port.port.generation {
                entry.port = None;
            }
        }
    }

    pub fn get_mut<P>(&mut self, port: PortRef<P>) -> Option<&mut P> {
        if port.port.pid != self.pid {
            // This is not your port, you cannot get it.
            return None;
        }

        if let Some(entry) = self.ports.get_mut(port.port.index as usize) {
            if entry.port.is_some() && entry.generation == port.port.generation {
                if let Some(port) = entry.port.as_mut() {
                    let port = port.as_mut() as *mut dyn ErasedPort as *mut P;
                    let port = unsafe { &mut *port };
                    return Some(port);
                }
            }
        }

        None
    }
}
