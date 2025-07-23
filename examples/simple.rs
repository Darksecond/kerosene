use kerosene::{
    actor::Exit,
    global::{send, spawn, spawn_linked, stop},
    main, receive,
};

main!(main_actor);

async fn main_actor() -> Exit {
    spawn(sender);

    Exit::Normal
}

async fn receiver() -> Exit {
    let mut count = 0;
    println!("Receiver started");

    loop {
        receive!({
            match String: _ => {
                count += 1;
                if count % 16 == 0 {
                    println!("Received {} messages", count);
                }
                if count == 128 {
                    stop();
                }
            }
        });
    }
}

async fn sender() -> Exit {
    let receiver = spawn_linked(receiver);

    for i in 0..128 {
        send(receiver, format!("Message {}", i));
    }

    Exit::Normal
}
