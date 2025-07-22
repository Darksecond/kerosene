use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    pin::Pin,
    sync::{Arc, RwLock},
};

use crate::actor::{HydratedActorBase, Pid};

const NUM_SHARDS: u64 = 64;

struct Shard {
    actors: RwLock<HashMap<Pid, Pin<Arc<dyn HydratedActorBase>>>>,
}

impl Shard {
    pub fn new() -> Self {
        Self {
            actors: RwLock::new(HashMap::new()),
        }
    }
}

pub struct Table {
    shards: [Shard; NUM_SHARDS as usize],
}

impl Table {
    pub fn new() -> Self {
        let shards = std::array::from_fn(|_| Shard::new());
        Self { shards }
    }

    pub fn lookup(&self, pid: Pid) -> Option<Pin<Arc<dyn HydratedActorBase>>> {
        let shard = self.shard(pid);

        shard
            .actors
            .read()
            .expect("Failed to acquire lock")
            .get(&pid)
            .cloned()
    }

    pub fn remove(&self, pid: Pid) {
        let shard = self.shard(pid);

        shard
            .actors
            .write()
            .expect("Failed to acquire lock")
            .remove(&pid);
    }

    pub fn add(&self, pid: Pid, actor: Pin<Arc<dyn HydratedActorBase>>) {
        let shard = self.shard(pid);

        shard
            .actors
            .write()
            .expect("Failed to acquire lock")
            .insert(pid, actor);
    }

    fn shard(&self, pid: Pid) -> &Shard {
        let mut hasher = DefaultHasher::new();
        pid.hash(&mut hasher);
        let shard_index = hasher.finish() % NUM_SHARDS;

        &self.shards[shard_index as usize]
    }
}
