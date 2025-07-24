use std::{fmt::Display, time::Duration};

pub struct Histogram {
    min: f64,
    range: f64,
    counts: [usize; Self::BINS],
}

impl Histogram {
    const BINS: usize = 20;

    pub fn new(durations: &[Duration]) -> Self {
        let min = durations.iter().min().unwrap().as_nanos() as f64;
        let max = durations.iter().max().unwrap().as_nanos() as f64;
        let range = (max - min).max(1.0);

        let mut counts = [0; Self::BINS];
        for d in durations {
            let v = d.as_nanos() as f64;
            let idx = (((v - min) / range) * (Self::BINS as f64 - 1.0)).round() as usize;
            let idx = idx.min(Self::BINS - 1); // clamp to max index
            counts[idx] += 1;
        }

        Self { min, range, counts }
    }
}

impl Display for Histogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Histogram ({} bins):", Self::BINS)?;

        for (i, count) in self.counts.iter().enumerate() {
            let bar = "*".repeat(*count);

            let lower = self.min + i as f64 * (self.range / Self::BINS as f64);
            let upper = self.min + (i + 1) as f64 * (self.range / Self::BINS as f64);

            let lower = Duration::from_nanos(lower as u64);
            let upper = Duration::from_nanos(upper as u64);

            writeln!(f, "{:<8.0?} - {:>8.0?} | {}", lower, upper, bar)?;
        }

        Ok(())
    }
}
