mod histogram;
mod samples;
mod stats;

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use stats::*;

use crate::samples::SampleSet;

static mut TIME: Duration = Duration::ZERO;
static mut SCALE: usize = 1;

pub fn scale(scale: usize) {
    unsafe {
        SCALE = scale;
    }
}

pub fn measure(elapsed: Duration) {
    unsafe {
        TIME = elapsed;
    }
}

pub fn benchmark<F>(name: &str, mut f: F)
where
    F: FnMut(),
{
    scale(1);

    let n = black_box(find_iterations(Duration::from_secs(5), &mut f));
    let m = black_box(find_iterations(Duration::from_secs(1), &mut f));

    for _ in 0..m {
        black_box(f());
    }

    let scale = unsafe { SCALE };
    let mut samples = SampleSet::with_capacity(n);
    for _ in 0..n {
        black_box(f());
        samples.push(unsafe { TIME } / scale as u32);
    }

    let stats = samples.to_stats();

    println!("");
    println!("Benchmark '{}':", name);
    println!("  Results are scaled by {}", scale);
    println!("{:<22} {:>12}", "warmup iterations:", m);
    println!("{:<22} {:>12}", "iterations:", n);
    println!("");
    println!("{}", &stats);
    println!("");
    compare_stats(name, &stats);
    println!("");
    println!("{}", samples.histogram());

    stats.save(name);
}

fn find_iterations<F>(duration: Duration, f: &mut F) -> usize
where
    F: FnMut(),
{
    let now = Instant::now();
    f();
    let time = now.elapsed();

    let iterations = duration.div_duration_f64(time);
    iterations as _
}

fn compare_stats(name: &str, current: &Stats) {
    let Some(prev) = Stats::load(name) else {
        println!("No previous benchmark found.");
        return;
    };

    let comparison = Comparison::compare(&prev, current);

    println!("Comparison with previous run:");
    println!("{:<22} {:>12.3?}", "Previous mean:", prev.mean);
    println!("{:<22} {:>12.3?}", "Current mean:", current.mean);
    println!("{}", comparison);
}
