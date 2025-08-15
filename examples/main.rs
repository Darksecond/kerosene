use std::time::{Duration, Instant};

use kerosene::{
    Exit, global,
    library::{
        self,
        supervisor::{RestartPolicy, Strategy, Supervisor},
    },
    main, receive,
};

main!(main_actor);

async fn main_actor() -> Exit {
    println!("MainActor::started");
    println!(
        "MainActor has pid {:?} {:?}",
        global::sync::pid(),
        global::sync::pid()
    );

    let child = global::spawn(my_actor).await;
    global::send(child, String::from("Hello!")).await;
    global::schedule(global::sync::pid(), (), Duration::from_secs(1)).await;

    global::spawn(stop_actor).await;

    let contents = library::io::file::read_string("Cargo.toml")
        .await
        .ok()
        .unwrap_or(String::new());

    global::send(child, contents).await;

    global::spawn(sender).await;

    for _ in 0..128 {
        global::spawn(idle_loop_actor).await;
    }

    loop {
        receive! {
            match () {
                _ => {
                    println!("MainActor::handle");

                    global::send(child, String::from("Timer!")).await;
                    global::schedule(global::sync::pid(), (), Duration::from_secs(1)).await;
                }
            }
        }
    }
}

async fn my_actor() -> Exit {
    loop {
        receive! {
            match String {
                message => {
                    println!("Received message: {} at {:?}", message, Instant::now());
                }
            }
        }
    }
}

async fn blocking_actor() -> Exit {
    println!("BlockingActor::started at {}", global::sync::pid().0);
    global::sleep(Duration::from_secs(10)).await;
    println!("BlockingActor::started completed");

    Exit::Normal
}

async fn idle_loop_actor() -> Exit {
    global::send(global::sync::pid(), ()).await;
    loop {
        receive! {
            match () {
                _ => {
                    global::send(global::sync::pid(), ()).await;
                }
            }
        }
    }
}

async fn receiver() -> Exit {
    let mut count = 0;
    global::sleep(Duration::from_secs(3)).await;
    println!("Receiver started");

    loop {
        receive! {
            match String {
                _message => {
                    count += 1;
                    if count % 512 == 0 {
                        println!("Received {} messages", count);
                    }
                }
            }
        }
    }
}

async fn sender() -> Exit {
    let receiver = global::spawn_linked(receiver);

    for i in 0..2048 {
        global::send(receiver, format!("Message {}", i)).await;
    }

    Exit::Normal
}

async fn stop_actor() -> Exit {
    let supervisor = Supervisor::spawn_linked(Strategy::OneForOne);
    supervisor.supervise(RestartPolicy::Permanent, || blocking_actor);

    global::schedule(global::sync::pid(), (), Duration::from_secs(30)).await;

    receive! {
        match () {
            _ => {
                eprintln!("StopActor::handle");
            }
        }
    };

    Exit::Shutdown
}
