use std::ops::Add;
use std::str::FromStr;
use crate::core::RefClone;
use crate::property::{Gettable, Property};
use crate::text::StyledText;

pub type TextProperty = Property<StyledText>;

impl FromStr for TextProperty {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TextProperty::from_observable(StyledText::from_str(s).unwrap()))
    }
}


impl From<&str> for TextProperty {
    fn from(text: &str) -> Self {
        Self::from_str(text).unwrap()
    }
}

impl From<String> for TextProperty {
    fn from(text: String) -> Self {
        Self::from_str(&text).unwrap()
    }
}

impl From<&String> for TextProperty {
    fn from(text: &String) -> Self {
        Self::from_str(text).unwrap()
    }
}

/*impl From<StyledText> for TextProperty {
    fn from(text: StyledText) -> Self {
        Self::from_observable(text)
    }
}*/

impl From<&StyledText> for TextProperty {
    fn from(text: &StyledText) -> Self {
        Self::from_observable(text.clone())
    }
}

impl From<TextProperty> for StyledText {
    fn from(text: TextProperty) -> Self {
        text.get()
    }
}

impl From<&TextProperty> for StyledText {
    fn from(text: &TextProperty) -> Self {
        text.get()
    }
}

impl From<TextProperty> for String {
    fn from(text: TextProperty) -> Self {
        text.get().to_string()
    }
}

impl From<&TextProperty> for String {
    fn from(text: &TextProperty) -> Self {
        text.get().to_string()
    }
}

impl Add<TextProperty> for TextProperty {
    type Output = TextProperty;

    fn add(self, rhs: TextProperty) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = TextProperty::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<&TextProperty> for TextProperty {
    type Output = TextProperty;

    fn add(self, rhs: &TextProperty) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = TextProperty::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<TextProperty> for &TextProperty {
    type Output = TextProperty;

    fn add(self, rhs: TextProperty) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = TextProperty::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<&TextProperty> for &TextProperty {
    type Output = TextProperty;

    fn add(self, rhs: &TextProperty) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = TextProperty::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}



impl<T:ToString + 'static> Add<T> for TextProperty {
    type Output = TextProperty;

    fn add(self, rhs: T) -> Self::Output {
        let lhs = self.ref_clone();
        let mut output = TextProperty::from_dynamic(Box::new(move || {
            StyledText::from(lhs.get().to_string() + &rhs.to_string())
        }));
        output.observe(self);
        output
    }
}

