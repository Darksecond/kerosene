use std::time::{Duration, Instant};

use beam::{
    actor::{Exit, Pid},
    async_actor::IntoAsyncActor,
    global::{send, sleep, spawn, stop},
    receive,
};
use criterion::{Criterion, criterion_group, criterion_main};

static mut TIME: Duration = Duration::ZERO;

async fn receive_actor() -> Exit {
    let mut count = 0;
    let now = Instant::now();
    loop {
        receive!({
            match i32: _ => { count += 1;}
        });
        if count == 100000 {
            unsafe { TIME = now.elapsed() };
            stop();
        }
    }
}

fn sender_actor(receiver: Pid) -> impl IntoAsyncActor {
    async move || {
        for i in 0..100000 {
            let message = i;
            send(receiver, message);
        }

        Exit::Normal
    }
}

async fn main_actor() -> Exit {
    let receive = spawn(receive_actor);
    spawn(sender_actor(receive));

    // Just idle.
    sleep(Duration::from_secs(100)).await;

    Exit::Normal
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("receive 100000 messages", |b| {
        b.iter_custom(|iters| {
            for _ in 0..iters {
                beam::run(main_actor);
            }
            unsafe { TIME }
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
