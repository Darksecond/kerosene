use std::time::{Duration, Instant};

use benchmark::{measure, scale};
use kerosene::{
    Exit, IntoAsyncActor, Pid,
    global::{send, sleep, spawn, stop},
    receive,
};

async fn receive_actor() -> Exit {
    let mut count = 0;
    let now = Instant::now();
    loop {
        receive!({
            match i32: _ => { count += 1;}
        });
        if count == 100000 {
            measure(now.elapsed());
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

fn main() {
    benchmark::benchmark("receive 100000 messages", || {
        scale(100000);

        kerosene::run(main_actor);
    });
}
