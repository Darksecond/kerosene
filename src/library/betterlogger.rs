use std::{fmt::Display, panic::Location};

use crate::{
    Exit, Pid, PortPid,
    global::{pid, send},
    receive_new,
    utils::{Timestamp, UnsortedSet},
};

enum LogMessage {
    Log(Log),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Level {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

impl Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Emergency => write!(f, "EMERGENCY"),
            Level::Alert => write!(f, "ALERT"),
            Level::Critical => write!(f, "CRITICAL"),
            Level::Error => write!(f, "ERROR"),
            Level::Warning => write!(f, "WARNING"),
            Level::Notice => write!(f, "NOTICE"),
            Level::Info => write!(f, "INFO"),
            Level::Debug => write!(f, "DEBUG"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetaValue {
    OwnedString(String),
    StaticStr(&'static str),

    Unsigned(u64),
    Signed(i64),

    Pid(Pid),
    Port(PortPid),

    Timestamp(Timestamp),
}

impl Display for MetaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetaValue::OwnedString(str) => write!(f, "{}", str),
            MetaValue::StaticStr(str) => write!(f, "{}", str),
            MetaValue::Unsigned(num) => write!(f, "{}", num),
            MetaValue::Signed(num) => write!(f, "{}", num),
            MetaValue::Pid(pid) => write!(f, "{}", pid.0),
            MetaValue::Port(port_pid) => write!(f, "{:?}", port_pid),
            MetaValue::Timestamp(timestamp) => write!(f, "{}", timestamp),
        }
    }
}

impl From<&'static str> for MetaValue {
    fn from(value: &'static str) -> Self {
        MetaValue::StaticStr(value)
    }
}

impl From<u32> for MetaValue {
    fn from(value: u32) -> Self {
        MetaValue::Unsigned(value as u64)
    }
}

impl From<Pid> for MetaValue {
    fn from(value: Pid) -> Self {
        MetaValue::Pid(value)
    }
}

impl From<Timestamp> for MetaValue {
    fn from(value: Timestamp) -> Self {
        MetaValue::Timestamp(value)
    }
}

#[derive(Clone, Debug)]
pub struct MetaData {
    key: &'static str,
    value: MetaValue,
}

impl PartialEq for MetaData {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

#[derive(Clone, Debug)]
pub struct Log {
    level: Level,
    message: &'static str, // TODO: Should probably be CoW
    values: UnsortedSet<MetaData, 16>,
}

#[must_use]
pub struct LogBuilder {
    logger: &'static str,
    level: Level,
    message: &'static str,
    values: UnsortedSet<MetaData, 16>,
}

impl LogBuilder {
    pub fn new(level: Level, message: &'static str) -> Self {
        LogBuilder {
            logger: "betterlogger",
            level,
            message,
            values: UnsortedSet::new(),
        }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<MetaValue>) -> Self {
        let meta = MetaData {
            key,
            value: value.into(),
        };
        self.values.insert(meta);
        self
    }

    #[track_caller]
    pub fn emit(self) {
        let location = Location::caller();

        let mut values = self.values;

        values.insert(MetaData {
            key: "time",
            value: Timestamp::now().into(),
        });

        values.insert(MetaData {
            key: "pid",
            value: pid().into(),
        });

        values.insert(MetaData {
            key: "file",
            value: location.file().into(),
        });

        values.insert(MetaData {
            key: "line",
            value: location.line().into(),
        });

        let log = Log {
            level: self.level,
            message: self.message,
            values,
        };

        send(self.logger, LogMessage::Log(log));
    }
}

pub async fn logger_actor() -> Exit {
    loop {
        let message = receive_new! {
            match LogMessage {
                m => m,
            }
        };

        match message {
            LogMessage::Log(log) => {
                let message = parse(log.message, &log.values);
                println!("[{}] {}", log.level, message);
            }
        }
    }
}

fn find_key<'a, const N: usize>(
    key: &str,
    values: &'a UnsortedSet<MetaData, N>,
) -> Option<&'a MetaData> {
    values.iter().find(|meta| meta.key == key)
}

// TODO: Rewrite this to be less spaghetti
fn parse<const N: usize>(msg: &'static str, values: &UnsortedSet<MetaData, N>) -> String {
    let mut result = String::with_capacity(msg.len());
    let mut chars = msg.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '{' {
            if let Some(&(_, next)) = chars.peek() {
                if next == '{' {
                    result.push('{');
                    chars.next();
                    continue;
                }
            }

            let start = i + 1;
            let mut end = None;
            while let Some((j, d)) = chars.next() {
                if d == '}' {
                    // handle escaped '}}'
                    if let Some(&(_, next)) = chars.peek() {
                        if next == '}' {
                            chars.next(); // skip one '}'
                        }
                    }

                    end = Some(j);
                    break;
                }
            }

            if let Some(end) = end {
                let key = &msg[start..end];
                if let Some(meta) = find_key(key, values) {
                    result.push_str(&meta.value.to_string());
                } else {
                    result.push('{');
                    result.push_str(key);
                    result.push('}');
                }
            }
        } else if c == '}' {
            if let Some(&(_, next)) = chars.peek() {
                if next == '}' {
                    result.push('}');
                    chars.next();
                    continue;
                }
            }

            result.push('}'); // unmatched `}`, maybe invalid, but keep
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{MetaData, parse};
    use crate::utils::UnsortedSet;

    #[test]
    fn test_parse() {
        let msg = "Hello {name} {last_name} {name}!";

        let mut values = UnsortedSet::<_, 16>::new();
        values.insert(MetaData {
            key: "name",
            value: "John".into(),
        });

        let parsed = parse(msg, &values);
        assert_eq!(parsed, "Hello John!");
    }
}
