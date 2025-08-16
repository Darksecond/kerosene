use std::time::Duration;

use kerosene::{
    Exit,
    global::{sleep, spawn_linked, sync::stop},
    library::io::{
        buffer_pool::reserve_buffer,
        io_pump::{close_descriptor, open_file, read},
    },
    main,
};

main!(main_actor);

async fn main_actor() -> Exit {
    spawn_linked(kerosene::library::io::io_pump::pump);
    sleep(Duration::from_millis(1)).await; // Wait for pump to register itself, will be fixed in the future.

    println!("Opening file");
    let file = open_file("Cargo.toml").await;

    println!("Reserving buffer");
    let buffer = reserve_buffer(0x1000).await;

    println!("Reading file");
    let filled_buffer = read(file, 0, buffer).await;
    let buf = std::str::from_utf8(&filled_buffer).unwrap();
    println!("{}", buf);

    println!("Closing file");
    close_descriptor(file);

    stop();
    Exit::Normal
}
