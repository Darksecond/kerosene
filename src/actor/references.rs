use crate::registry::Registry;

pub trait ToPid {
    fn to_reference(&self, registry: &Registry) -> Pid;
}

impl ToPid for Pid {
    fn to_reference(&self, _registry: &Registry) -> Pid {
        *self
    }
}

impl ToPid for NamedRef {
    fn to_reference(&self, registry: &Registry) -> Pid {
        registry.lookup_name(*self).unwrap_or(Pid::invalid()).into()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NamedRef {
    name: &'static str,
}

impl NamedRef {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Pid(pub u64);

impl Pid {
    pub const fn invalid() -> Self {
        Pid(u64::MAX)
    }
}
