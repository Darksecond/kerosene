use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use crate::{
    registry::Registry,
    scheduler::{Scheduler, Slot},
    worker::WorkerId,
    worker::WorkerSnapshot,
};

struct MonitorEntry {
    snapshot: WorkerSnapshot,
}

pub struct Monitor {
    entries: Vec<Option<MonitorEntry>>,
    scheduler: Arc<Scheduler>,
    registry: Arc<Registry>,
}

impl Monitor {
    pub fn new(scheduler: Arc<Scheduler>, registry: Arc<Registry>) -> Self {
        Self {
            scheduler,
            registry,
            entries: Vec::new(),
        }
    }

    fn sync(&mut self) {
        let workers = &self.scheduler.workers;

        self.entries.resize_with(workers.len(), || None);

        for (worker_id, slot) in workers.iter().enumerate() {
            let slot = slot.read().expect("Failed to acquire lock");
            match &*slot {
                Slot::Active(active_worker) => {
                    let snapshot = active_worker.worker.snapshot();
                    if let Some(entry) = &mut self.entries[worker_id] {
                        entry.snapshot = snapshot;
                    } else {
                        self.entries[worker_id] = Some(MonitorEntry { snapshot });
                    }
                }
                _ => {
                    self.entries[worker_id] = None;
                }
            }
        }
    }

    pub fn run(&mut self) {
        loop {
            if self.scheduler.stopped.load(Ordering::Acquire) {
                return;
            }

            self.sync();

            let queue_length_median =
                median(self.entries.iter().filter_map(|entry| {
                    entry.as_ref().map(|entry| entry.snapshot.run_queue_length)
                }));

            for (worker_id, entry) in self.entries.iter_mut().enumerate() {
                let Some(entry) = entry else { continue };

                let mut snapshot = {
                    if let Some(worker) = self.scheduler.get_worker(worker_id) {
                        worker.snapshot()
                    } else {
                        continue;
                    }
                };

                if detect_overload(entry, &snapshot, queue_length_median) {
                    eprintln!("Rebalancing worker {}", worker_id);

                    rebalance_worker(
                        &self.scheduler,
                        &self.registry,
                        worker_id,
                        queue_length_median,
                    );

                    if let Some(worker) = self.scheduler.get_worker(worker_id) {
                        snapshot = worker.snapshot();
                    };
                }

                entry.snapshot = snapshot;
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn median(iter: impl Iterator<Item = usize>) -> usize {
    const MAX: usize = 1024;

    let mut histogram = [0; MAX + 1];
    let mut total = 0;

    for len in iter {
        let clamped = len.min(MAX);
        histogram[clamped] += 1;
        total += 1;
    }

    if total == 0 {
        return 0;
    }

    let median_pos = (total - 1) / 2;
    let mut cumulative = 0;

    for (len, count) in histogram.iter().enumerate() {
        cumulative += count;
        if cumulative > median_pos {
            return len;
        }
    }

    MAX
}

fn detect_overload(
    entry: &mut MonitorEntry,
    snapshot: &WorkerSnapshot,
    queue_length_median: usize,
) -> bool {
    const OVERLOAD_GROWTH_THRESHOLD: usize = 16;
    const OVERLOAD_IMBALANCE_THRESHOLD: usize = 4;

    let growth = snapshot
        .run_queue_length
        .saturating_sub(entry.snapshot.run_queue_length);
    let imbalance = snapshot.run_queue_length > queue_length_median * OVERLOAD_IMBALANCE_THRESHOLD;

    growth > OVERLOAD_GROWTH_THRESHOLD || imbalance
}

pub fn rebalance_worker(
    scheduler: &Arc<Scheduler>,
    registry: &Arc<Registry>,
    worker_id: WorkerId,
    queue_length_median: usize,
) {
    let Some(source_worker) = scheduler.get_worker(worker_id) else {
        return;
    };

    let queue_length = source_worker.run_queue_length();

    let Some(destination_id) = find_least_loaded_worker(scheduler, worker_id) else {
        return;
    };

    let Some(destination_worker) = scheduler.get_worker(destination_id) else {
        return;
    };

    let excess = queue_length.saturating_sub(queue_length_median);
    let tasks_to_move = (excess / 2).max(1);

    eprintln!(
        "Moving {} tasks from worker {} to worker {}",
        tasks_to_move, worker_id, destination_id
    );

    let source_queue = &source_worker.run_queue;
    let destination_queue = &destination_worker.run_queue;

    for _ in 0..tasks_to_move {
        if let Some(pid) = source_queue.try_pop() {
            if let Some(actor) = registry.lookup_pid(pid) {
                if actor.control_block().is_running.load(Ordering::Acquire) {
                    eprintln!("Trying to move running actor {}", pid.0);
                    // Skip actors that are running, just put it back where we found it.
                    source_queue.push(pid);
                } else {
                    actor
                        .control_block()
                        .worker_id
                        .store(destination_id as _, Ordering::Release);

                    destination_queue.push(pid);
                }
            }
        }
    }

    scheduler.wake_worker(destination_id);
}

fn find_least_loaded_worker(scheduler: &Arc<Scheduler>, worker_id: WorkerId) -> Option<WorkerId> {
    let workers = &scheduler.workers;

    workers
        .iter()
        .enumerate()
        .filter_map(|(id, slot)| {
            let slot = slot.read().expect("Failed to acquire lock");
            if let Slot::Active(worker) = &*slot {
                Some((id, worker.worker.run_queue_length()))
            } else {
                None
            }
        })
        .filter(|(id, _)| *id != worker_id)
        .min_by_key(|(_, run_queue_length)| *run_queue_length)
        .map(|(id, _)| id)
}
