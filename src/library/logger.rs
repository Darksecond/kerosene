//! The logger system is modeled on erlang's logger system.
//!
//! See <https://www.erlang.org/doc/apps/kernel/logger.html>.
//!
//! You can start a log message by using one of the helper methods, or by using `Logbuilder::new` directly.
//! Then you can append metadata to the log message and format it using '{key}' in the log message.
//!
//! ```no_run
//! use kerosene::library::betterlogger::debug;
//! debug("Hello, {world}")
//!     .with("world", "Earth")
//!     .emit();
//! ```
//!
//! There is system level metadata always availble, see `LogBuilder::emit` for details.

use std::{fmt::Display, panic::Location};

use crate::{
    Exit,
    global::{metadata, pid, register, send},
    metadata::{MetaKeyValue, MetaValue},
    receive,
    utils::{Timestamp, UnsortedSet},
};

enum LogMessage {
    Log(Record),
}

/// The severity of the log message.
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

#[derive(Clone, Debug)]
struct Record {
    level: Level,
    message: &'static str, // TODO: Should probably be CoW
    values: UnsortedSet<MetaKeyValue, 16>,
}

/// Allows building a log message with metadata.
#[must_use]
pub struct LogBuilder {
    logger: &'static str,
    level: Level,
    message: &'static str,
    values: UnsortedSet<MetaKeyValue, 16>,
    location: &'static Location<'static>,
}

impl LogBuilder {
    #[track_caller]
    pub fn new(level: Level, message: &'static str) -> Self {
        Self::with_location(Location::caller(), level, message)
    }

    pub fn with_location(
        location: &'static Location<'static>,
        level: Level,
        message: &'static str,
    ) -> Self {
        let mut values = UnsortedSet::new();

        // Insert metadata from the actor
        values.merge_with(metadata());

        LogBuilder {
            logger: "logger",
            level,
            message,
            values,
            location,
        }
    }

    /// Add metadata to the log message
    pub fn with(mut self, key: &'static str, value: impl Into<MetaValue>) -> Self {
        let meta = MetaKeyValue {
            key,
            value: value.into(),
        };
        self.values.insert(meta);
        self
    }

    /// Emit the log message
    ///
    /// This will insert the following metadata:
    /// - time: The current timestamp
    /// - pid: The process ID
    /// - file: The file name where the log was emitted
    /// - line: The line number where the log was emitted
    pub fn emit(self) {
        let location = self.location;
        let mut values = self.values;

        values.insert(MetaKeyValue {
            key: "time",
            value: Timestamp::now().into(),
        });

        values.insert(MetaKeyValue {
            key: "pid",
            value: pid().into(),
        });

        values.insert(MetaKeyValue {
            key: "file",
            value: location.file().into(),
        });

        values.insert(MetaKeyValue {
            key: "line",
            value: location.line().into(),
        });

        let log = Record {
            level: self.level,
            message: self.message,
            values,
        };

        send(self.logger, LogMessage::Log(log));
    }
}

/// Create a new log builder with the 'debug' level.
#[track_caller]
pub fn debug(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Debug, message)
}

/// Create a new log builder with the 'info' level.
#[track_caller]
pub fn info(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Info, message)
}

/// Create a new log builder with the 'notice' level.
#[track_caller]
pub fn notice(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Notice, message)
}

/// Create a new log builder with the 'warning' level.
#[track_caller]
pub fn warning(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Warning, message)
}

/// Create a new log builder with the 'error' level.
#[track_caller]
pub fn error(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Error, message)
}

/// Create a new log builder with the 'critical' level.
#[track_caller]
pub fn critical(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Critical, message)
}

/// Create a new log builder with the 'alert' level.
#[track_caller]
pub fn alert(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Alert, message)
}

/// Create a new log builder with the 'emergency' level.
#[track_caller]
pub fn emergency(message: &'static str) -> LogBuilder {
    LogBuilder::with_location(Location::caller(), Level::Emergency, message)
}

/// The Logger actor.
///
/// This should be registered as 'betterlogger'.
pub(crate) async fn logger_actor() -> Exit {
    register("logger", pid());

    loop {
        let message = receive! {
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
    values: &'a UnsortedSet<MetaKeyValue, N>,
) -> Option<&'a MetaKeyValue> {
    values.iter().find(|meta| meta.key == key)
}

// TODO: Rewrite this to be less spaghetti
fn parse<const N: usize>(msg: &'static str, values: &UnsortedSet<MetaKeyValue, N>) -> String {
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
    use crate::utils::UnsortedSet;

    use super::{MetaKeyValue, parse};

    #[test]
    fn test_parse() {
        let msg = "Hello {name} {last_name} {name}!";

        let mut values = UnsortedSet::<MetaKeyValue, 16>::new();
        values.insert(MetaKeyValue {
            key: "name",
            value: "John".into(),
        });

        let parsed = parse(msg, &values);
        assert_eq!(parsed, "Hello John!");
    }
}
