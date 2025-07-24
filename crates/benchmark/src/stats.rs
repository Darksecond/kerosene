use std::{fmt::Display, time::Duration};

#[derive(Debug)]
pub struct Stats {
    pub total: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub stddev: Duration,
    pub min: Duration,
    pub max: Duration,
}

impl Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:<22} {:>12.3?}", "Total time:", self.total)?;
        writeln!(f, "{:<22} {:>12.3?}", "Min time:", self.min)?;
        writeln!(f, "{:<22} {:>12.3?}", "Max time:", self.max)?;
        writeln!(f, "{:<22} {:>12.3?}", "Mean time:", self.mean)?;
        writeln!(f, "{:<22} {:>12.3?}", "Median time:", self.median)?;
        writeln!(f, "{:<22} {:>12.3?}", "Stddev time:", self.stddev)?;

        Ok(())
    }
}

impl Stats {
    pub fn cv(&self) -> f64 {
        let mean = self.mean;
        let stddev = self.stddev;

        stddev.as_secs_f64() / mean.as_secs_f64() * 100.0
    }

    pub fn save(&self, name: &str) {
        let path = format!("target/benchmarks/{}.bench", sanitize(name));
        std::fs::create_dir_all("target/benchmarks").unwrap();
        let contents = format!(
            "mean_ns={}\nmedian_ns={}\nstddev_ns={}\nmin_ns={}\nmax_ns={}\ntotal_ns={}",
            self.mean.as_nanos(),
            self.median.as_nanos(),
            self.stddev.as_nanos(),
            self.min.as_nanos(),
            self.max.as_nanos(),
            self.total.as_nanos()
        );
        std::fs::write(path, contents).unwrap();
    }

    pub fn load(name: &str) -> Option<Stats> {
        let path = format!("target/benchmarks/{}.bench", sanitize(name));
        let contents = std::fs::read_to_string(path).ok()?;

        let mut mean_ns: Option<u64> = None;
        let mut median_ns: Option<u64> = None;
        let mut stddev_ns: Option<u64> = None;
        let mut min_ns: Option<u64> = None;
        let mut max_ns: Option<u64> = None;
        let mut total_ns: Option<u64> = None;

        for line in contents.lines() {
            let mut parts = line.split('=');
            let key = parts.next()?;
            let value = parts.next()?;
            match key.trim() {
                "mean_ns" => mean_ns = value.trim().parse().ok(),
                "median_ns" => median_ns = value.trim().parse().ok(),
                "stddev_ns" => stddev_ns = value.trim().parse().ok(),
                "min_ns" => min_ns = value.trim().parse().ok(),
                "max_ns" => max_ns = value.trim().parse().ok(),
                "total_ns" => total_ns = value.trim().parse().ok(),
                _ => {}
            }
        }

        Some(Stats {
            total: Duration::from_nanos(total_ns?),
            mean: Duration::from_nanos(mean_ns?),
            median: Duration::from_nanos(median_ns?),
            stddev: Duration::from_nanos(stddev_ns?),
            min: Duration::from_nanos(min_ns?),
            max: Duration::from_nanos(max_ns?),
        })
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

pub struct Comparison {
    delta: f64,
    percent: f64,
    cv: f64,
}

impl Comparison {
    pub fn compare(prev: &Stats, current: &Stats) -> Self {
        let prev_mean = prev.mean.as_nanos() as f64;
        let curr_mean = current.mean.as_nanos() as f64;
        let delta = curr_mean - prev_mean;
        let percent = delta / prev_mean * 100.0;
        let cv = current.cv();

        Self { delta, percent, cv }
    }

    fn is_noise(&self, margin: f64) -> bool {
        self.cv >= margin
    }
}

impl Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{:<22} {}{:10.3?} ({:+6.2}%)",
            "Change:",
            if self.delta < 0.0 { "-" } else { "+" },
            Duration::from_nanos(self.delta.abs() as _),
            self.percent
        )?;

        let margin = 5.0; // ±5%

        if self.is_noise(margin) {
            writeln!(
                f,
                "⚠️  High variation detected (CV >= {:.1}%), results likely noise",
                margin
            )?;
        } else {
            match self.percent {
                p if p.abs() <= margin => {
                    writeln!(
                        f,
                        "✅ Change is within ±{:.1}% margin (likely noise)",
                        margin
                    )?;
                }
                p if p < 0.0 => {
                    writeln!(f, "🟢 Performance improved")?;
                }
                _ => {
                    writeln!(f, "⚠️  Performance regression")?;
                }
            }
        }

        Ok(())
    }
}
