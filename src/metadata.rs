use std::fmt::Display;

use crate::{Pid, utils::Timestamp};

/// Various types that are supported as metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum MetaValue {
    OwnedString(String),
    StaticStr(&'static str),

    Unsigned(u64),
    Signed(i64),

    Pid(Pid),

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
            MetaValue::Timestamp(timestamp) => write!(f, "{}", timestamp),
        }
    }
}

impl From<&'static str> for MetaValue {
    fn from(value: &'static str) -> Self {
        MetaValue::StaticStr(value)
    }
}

impl From<u64> for MetaValue {
    fn from(value: u64) -> Self {
        MetaValue::Unsigned(value)
    }
}

impl From<i64> for MetaValue {
    fn from(value: i64) -> Self {
        MetaValue::Signed(value)
    }
}

impl From<u32> for MetaValue {
    fn from(value: u32) -> Self {
        MetaValue::Unsigned(value as u64)
    }
}

impl From<i32> for MetaValue {
    fn from(value: i32) -> Self {
        MetaValue::Signed(value as i64)
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
pub struct MetaKeyValue {
    pub key: &'static str,
    pub value: MetaValue,
}

impl PartialEq for MetaKeyValue {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
