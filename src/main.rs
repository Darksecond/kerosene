use std::time::{Duration, Instant};

use beam::{
    actor::Exit,
    global, main, receive,
    supervisor::{RestartPolicy, Strategy, Supervisor},
};

main!(main_actor);

async fn main_actor() -> Exit {
    println!("MainActor::started");
    println!("MainActor has pid {:?} {:?}", global::pid(), global::pid());

    let child = global::spawn(my_actor);
    let _ = global::send(child, String::from("Hello!"));
    global::schedule(global::pid(), (), Duration::from_secs(1));

    global::spawn(stop_actor);

    let contents = beam::file::read_string("Cargo.toml")
        .await
        .ok()
        .unwrap_or(String::new());

    global::send(child, contents);

    global::spawn(sender);

    for _ in 0..128 {
        global::spawn(idle_loop_actor);
    }

    loop {
        receive!({
            match (): _ => {
                println!("MainActor::handle");

                let _ = beam::global::send(child, String::from("Timer!"));
                global::schedule(global::pid(), (), Duration::from_secs(1));
            }
        });
    }
}

async fn my_actor() -> Exit {
    loop {
        receive!({
            match String: message => {
                println!("Received message: {} at {:?}", message, Instant::now());
            }
        });
    }
}

async fn blocking_actor() -> Exit {
    println!("BlockingActor::started at {}", global::pid().0);
    global::sleep(Duration::from_secs(10)).await;
    println!("BlockingActor::started completed");

    Exit::Normal
}

async fn idle_loop_actor() -> Exit {
    let _ = global::send(global::pid(), ());
    loop {
        receive!({
            match (): _ => {
                let _ = global::send(global::pid(), ());
            }
        });
    }
}

async fn receiver() -> Exit {
    let mut count = 0;
    global::sleep(Duration::from_secs(3)).await;
    println!("Receiver started");

    loop {
        receive!({
            match String: _ => {
                count += 1;
                if count % 512 == 0 {
                    println!("Received {} messages", count);
                }
            }
        });
    }
}

async fn sender() -> Exit {
    let receiver = global::spawn_linked(receiver);

    for i in 0..2048 {
        global::send(receiver, format!("Message {}", i));
    }

    Exit::Normal
}

async fn stop_actor() -> Exit {
    let supervisor = Supervisor::spawn_linked(Strategy::OneForOne);
    supervisor.supervise(RestartPolicy::Permanent, || blocking_actor);

    global::schedule(global::pid(), (), Duration::from_secs(30));

    receive!({
        match (): _ => {
            eprintln!("StopActor::handle");
        }
    });

    Exit::Shutdown
}
