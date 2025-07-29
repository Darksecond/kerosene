use std::{
    fmt::Display,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timestamp {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Timestamp {
    pub fn now() -> Self {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        let mut secs = duration.as_secs();

        let mut year = 1970;
        loop {
            let days_in_year = if Self::is_leap(year) { 366 } else { 365 };
            let year_secs = days_in_year as u64 * 86_400;
            if secs < year_secs {
                break;
            }
            secs -= year_secs;
            year += 1;
        }

        let dim = Self::days_in_month(year);
        let mut month = 1;
        let mut day = 1;
        let mut days = secs / 86_400;
        secs %= 86_400;

        for (i, &d) in dim.iter().enumerate() {
            if days < d as u64 {
                month = (i + 1) as u32;
                day += days as u32;
                break;
            } else {
                days -= d as u64;
            }
        }

        let hour = secs / 3600;
        let minute = (secs % 3600) / 60;
        let second = secs % 60;

        Timestamp {
            year,
            month,
            day,
            hour: hour as u32,
            minute: minute as u32,
            second: second as u32,
        }
    }

    pub fn to_iso8601(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    fn is_leap(year: u32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    fn days_in_month(year: u32) -> [u32; 12] {
        let mut dim = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        if Self::is_leap(year) {
            dim[1] = 29;
        }
        dim
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Default Display uses ISO 8601 UTC format
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}
