use std::{collections::VecDeque, sync::mpsc::channel};

use crate::{
    Exit, IntoAsyncActor, Pid,
    global::{Context, exit, pid, register, send, spawn_linked},
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

    let closure = move |ctx: &Context| {
        // TODO: Use catch_unwind to handle panics
        let result = f();

        ctx.send(pid, JobResult::Success(result));
    };

    send(
        NAME,
        Job {
            closure: Box::new(closure),
        },
    );

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
    closure: Box<dyn FnOnce(&Context) + Send + 'static>,
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
                        send(pid, job);
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
        let context = Context::new();
        let (tx, rx) = channel::<Job>();

        std::thread::spawn(move || {
            for job in rx {
                // TODO: Handle panics
                (job.closure)(&context);

                // Mark ourselves as idle
                context.send(router, Idle(context.pid()));
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
