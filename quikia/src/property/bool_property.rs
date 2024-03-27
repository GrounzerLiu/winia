use crate::property::SharedProperty;

pub type BoolProperty = SharedProperty<bool>;


impl From<&BoolProperty> for BoolProperty{
    fn from(value: &BoolProperty) -> Self {
        value.clone()
    }
}