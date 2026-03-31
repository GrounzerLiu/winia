use crate::ui::item::ItemData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    Id(u32),
    Name(String),
}

impl Selector {
    pub fn is_match(&self, item_data: &ItemData) -> bool {
        match self {
            Selector::Id(id) => item_data.id() == *id,
            Selector::Name(name) => item_data.name.lock().as_str() == name.as_str(),
        }
    }
}

impl From<u32> for Selector {
    fn from(id: u32) -> Self {
        Selector::Id(id)
    }
}

impl From<String> for Selector {
    fn from(name: String) -> Self {
        Selector::Name(name)
    }
}

impl From<&str> for Selector {
    fn from(name: &str) -> Self {
        Selector::Name(name.to_string())
    }
}

impl From<&Selector> for Selector {
    fn from(selector: &Selector) -> Self {
        selector.clone()
    }
}