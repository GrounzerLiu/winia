use crate::property::SharedProperty;
use crate::text::StyledText;

pub type TextProperty = SharedProperty<StyledText>;

impl TextProperty {
    pub fn from_str(text: &str) -> Self{
        let text = StyledText::from_str(text);
        Self::from_observable(text)
    }
}


impl From<&str> for TextProperty {
    fn from(text: &str) -> Self {
        Self::from_str(text)
    }
}

impl From<String> for TextProperty {
    fn from(text: String) -> Self {
        Self::from_str(&text)
    }
}