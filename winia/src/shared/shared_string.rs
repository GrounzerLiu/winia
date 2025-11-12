use crate::shared::{SharedDerived, SharedSource};

pub type SharedString = SharedSource<String>;
pub type SharedDerivedString = SharedDerived<String>;

impl From<&str> for SharedString {
    fn from(value: &str) -> Self {
        SharedSource::new(value.to_string())
    }
}

impl From<&str> for SharedDerivedString {
    fn from(value: &str) -> Self {
        SharedDerived::new_derived(value.to_string())
    }
}