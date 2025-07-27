use std::time::Duration;

use kerosene::{
    Exit,
    global::{register, sleep, spawn_linked, stop},
    library::betterlogger::{Level, LogBuilder, logger_actor},
    main,
};

main!(main_actor);

async fn main_actor() -> Exit {
    let logger_pid = spawn_linked(logger_actor);
    register("betterlogger", logger_pid);

    LogBuilder::new(
        Level::Debug,
        "{time}: This is a log line from PID {pid} with {test} at [{file}:{line}]",
    )
    .with("test", 123)
    .emit();

    sleep(Duration::from_millis(100)).await;
    stop();

    Exit::Normal
}
