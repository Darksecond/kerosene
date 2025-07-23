use crate::registry::Registry;

pub trait ToPid {
    fn to_reference(&self, registry: &Registry) -> Pid;
}

impl ToPid for Pid {
    fn to_reference(&self, _registry: &Registry) -> Pid {
        *self
    }
}

impl ToPid for &'static str {
    fn to_reference(&self, registry: &Registry) -> Pid {
        registry.lookup_name(*self).unwrap_or(Pid::invalid()).into()
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Pid(pub u64);

impl Pid {
    pub const fn invalid() -> Self {
        Pid(u64::MAX)
    }
}
