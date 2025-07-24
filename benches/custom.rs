use std::time::Instant;

use kerosene::{
    Exit,
    global::{spawn, stop},
    run,
};

fn main() {
    benchmark::benchmark("Print Hello, World!", || {
        run(async move || {
            let now = Instant::now();
            spawn(async move || {
                println!("Hello, world!");
                stop();
                Exit::Normal
            });
            benchmark::measure(now.elapsed());

            Exit::Normal
        });
    });
}
