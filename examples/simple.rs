use std::time::Duration;

use kerosene::{
    Exit,
    global::{send, sleep, spawn, spawn_linked, stop},
    library::file::read_string,
    main, receive,
};

main!(main_actor);

async fn main_actor() -> Exit {
    spawn(async move || {
        let _ = read_string("Cargo.toml").await;

        Exit::Normal
    });

    sleep(Duration::from_secs(1)).await;
    spawn(sender);

    Exit::Normal
}

async fn receiver() -> Exit {
    let mut count = 0;
    println!("Receiver started");

    loop {
        receive! {
            match String {
                _ => {
                    count += 1;
                    if count % 16 == 0 {
                        println!("Received {} messages", count);
                    }
                    if count == 128 {
                        stop();
                    }
                }
            }
        }
    }
}

async fn sender() -> Exit {
    let receiver = spawn_linked(receiver);

    for i in 0..128 {
        send(receiver, format!("Message {}", i));
    }

    Exit::Normal
}
