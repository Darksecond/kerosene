use std::time::Duration;

use kerosene::{Exit, global::stop, library::blocking::block_on, main};

main!(main_actor);

async fn main_actor() -> Exit {
    block_on(move || {
        std::thread::sleep(Duration::from_secs(1));
        println!("Hello, world!");
        std::thread::sleep(Duration::from_secs(1));
        println!("Bye, world!");
    })
    .await;

    stop();

    Exit::Normal
}
