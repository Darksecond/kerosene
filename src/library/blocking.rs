use std::{
    any::Any,
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc::channel,
};

use crate::{
    Exit, IntoAsyncActor, Pid,
    global::{
        exit, send, spawn_linked,
        sync::{self, pid, register},
    },
    receive,
};

const NAME: &str = "blocking_pool";

/// Run a blocking closure.
///
/// This will run on a dedicated thread pool.
pub async fn block_on<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let pid = pid();

    let closure = move || {
        // TODO: Capture backtrace
        let result = match catch_unwind(AssertUnwindSafe(|| f())) {
            Ok(res) => JobResult::Success(res),
            Err(err) => JobResult::Panic(panic_to_string(err)),
        };

        sync::send(pid, result);
    };

    send(
        NAME,
        Job {
            closure: Box::new(closure),
        },
    )
    .await;

    receive! {
        match JobResult<R> {
            JobResult::Success(res) => res,
            JobResult::Panic(err) => {
                exit(pid, Exit::Panic(err)).await;
                unreachable!()
            }
        }
    }
}

#[allow(dead_code)]
enum JobResult<R> {
    Success(R),
    Panic(String),
}

struct Job {
    closure: Box<dyn FnOnce() + Send + 'static>,
}

struct Idle(Pid);

pub(crate) async fn router() -> Exit {
    register(NAME, pid());

    // TODO: Make this configurable
    const HANDLERS: usize = 4;

    let mut idle = (0..HANDLERS)
        .map(|_| spawn_linked(handler(pid())))
        .collect::<VecDeque<_>>();

    loop {
        if idle.is_empty() {
            receive! {
                match Idle {
                    Idle(pid) => {
                        idle.push_back(pid);
                    }
                }
            }
        } else {
            receive! {
                match Job {
                    job => {
                        let pid = idle.pop_front().expect("Idle queue should not be empty");
                        send(pid, job).await;
                    }
                }
                match Idle {
                    Idle(pid) => {
                        idle.push_back(pid);
                    }
                }
            }
        }
    }
}

fn handler(router: Pid) -> impl IntoAsyncActor {
    async move || {
        let pid = pid();
        let (tx, rx) = channel::<Job>();

        crate::thread::spawn(move || {
            for job in rx {
                // TODO: Handle panics
                (job.closure)();

                // Mark ourselves as idle
                sync::send(router, Idle(pid));
            }
        });

        loop {
            receive! {
                match Job {
                    job => {
                        let _ = tx.send(job);
                    }
                }
            }
        }
    }
}

fn panic_to_string(err: Box<dyn Any + Send>) -> String {
    if let Some(str) = err.downcast_ref::<String>() {
        str.to_string()
    } else if let Some(err) = err.downcast_ref::<&'static str>() {
        err.to_string()
    } else {
        "Unknown panic".to_string()
    }
}
