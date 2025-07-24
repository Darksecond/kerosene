use std::time::Duration;

use crate::{histogram::Histogram, stats::Stats};

pub struct SampleSet {
    total: Duration,
    samples: Vec<Duration>,
}

impl From<Vec<Duration>> for SampleSet {
    fn from(samples: Vec<Duration>) -> Self {
        SampleSet {
            total: samples.iter().sum(),
            samples,
        }
    }
}

impl SampleSet {
    pub fn with_capacity(capacity: usize) -> Self {
        SampleSet {
            total: Duration::ZERO,
            samples: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, sample: Duration) {
        self.total += sample;
        self.samples.push(sample);
    }

    pub fn total(&self) -> Duration {
        self.total
    }

    pub fn min(&self) -> Duration {
        self.samples.iter().min().copied().unwrap_or(Duration::ZERO)
    }

    pub fn max(&self) -> Duration {
        self.samples.iter().max().copied().unwrap_or(Duration::ZERO)
    }

    pub fn mean(&self) -> Duration {
        self.total() / self.samples.len() as u32
    }

    pub fn median(&self) -> Duration {
        let mut durations = self.samples.clone();
        durations.sort();

        let mid = durations.len() / 2;
        if durations.len() % 2 == 0 {
            durations[mid - 1] + (durations[mid] - durations[mid - 1]) / 2
        } else {
            durations[mid]
        }
    }

    pub fn stddev(&self) -> Duration {
        let mean = self.mean();

        let mean_ns = mean.as_nanos() as f64;
        let variance = self
            .samples
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - mean_ns;
                diff * diff
            })
            .sum::<f64>()
            / (self.samples.len() as f64);
        Duration::from_nanos(variance.sqrt() as u64)
    }

    pub fn histogram(&self) -> Histogram {
        Histogram::new(&self.samples)
    }

    pub fn to_stats(&self) -> Stats {
        Stats {
            total: self.total(),
            mean: self.mean(),
            median: self.median(),
            stddev: self.stddev(),
            min: self.min(),
            max: self.max(),
        }
    }
}
