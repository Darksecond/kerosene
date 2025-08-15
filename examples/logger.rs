use std::time::Duration;

use kerosene::{
    Exit,
    global::{insert_metadata, sleep, spawn, sync::stop},
    library::logger::debug,
    main,
};

main!(main_actor);

async fn main_actor() -> Exit {
    insert_metadata("actor_key", 123);

    debug("{time}: This is a log line from PID {pid} with {test} at [{file}:{line}]")
        .with("test", 123)
        .emit();

    spawn(async move || {
        debug("{actor_key} at [{file}:{line}]").emit();
        Exit::Normal
    })
    .await;

    sleep(Duration::from_millis(100)).await;
    stop();

    Exit::Normal
}
