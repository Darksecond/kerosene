use std::time::Duration;

use kerosene::{
    Exit,
    global::{sleep, stop},
    library::logger::debug,
    main,
};

main!(main_actor);

async fn main_actor() -> Exit {
    debug("{time}: This is a log line from PID {pid} with {test} at [{file}:{line}]")
        .with("test", 123)
        .emit();

    sleep(Duration::from_millis(100)).await;
    stop();

    Exit::Normal
}
