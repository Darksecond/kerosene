mod table;

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::{
    actor::{HydratedActor, HydratedActorBase, NamedRef, Pid},
    async_actor::IntoAsyncActor,
};

use table::Table;

pub struct Registry {
    next_pid: AtomicU64,
    actors: Table,
    names: RwLock<HashMap<&'static str, Pid>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            next_pid: AtomicU64::new(0),
            actors: Table::new(),
            names: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, named: NamedRef, actor: Pid) {
        let mut names = self.names.write().expect("Failed to acquire lock");

        let pid = actor;

        names.insert(named.name(), pid);
    }

    pub fn lookup_name(&self, name: NamedRef) -> Option<Pid> {
        let names = self.names.read().expect("Failed to acquire lock");
        names.get(name.name()).copied()
    }

    pub fn allocate_pid(&self) -> Pid {
        let pid = self.next_pid.fetch_add(1, Ordering::Relaxed);
        Pid(pid)
    }

    pub fn lookup_pid(&self, pid: Pid) -> Option<Pin<Arc<dyn HydratedActorBase>>> {
        self.actors.lookup(pid)
    }

    pub fn remove(&self, pid: Pid) {
        self.actors.remove(pid);
    }

    pub fn add<A>(&self, actor: HydratedActor<A>)
    where
        A: IntoAsyncActor,
    {
        let pid = actor.control_block.pid;
        self.actors.add(pid, Arc::pin(actor));
    }
}
