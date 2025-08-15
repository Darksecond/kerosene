use std::time::Duration;

use kerosene::{
    Exit,
    global::{spawn, sync::pid, sync::stop},
    library::blocking::block_on,
    main,
};

main!(main_actor);

async fn main_actor() -> Exit {
    spawn(async move || {
        println!("I'm actor {:?}", pid());

        block_on(move || {
            panic!("I'm panicking!");
        })
        .await;

        Exit::Normal
    })
    .await;

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
