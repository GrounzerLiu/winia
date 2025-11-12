use std::ops::Add;
use std::str::FromStr;
use crate::shared::{SharedDerived, SharedSource};
use crate::text::StyledText;

pub type SharedText = SharedSource<StyledText>;
pub type SharedDerivedText = SharedDerived<StyledText>;

impl FromStr for SharedText {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SharedText::from(StyledText::from_str(s)?))
    }
}

impl FromStr for SharedDerivedText {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SharedDerivedText::from(StyledText::from_str(s)?))
    }
}

impl From<&str> for SharedText {
    fn from(text: &str) -> Self {
        Self::from_str(text).unwrap()
    }
}

impl From<&str> for SharedDerivedText {
    fn from(text: &str) -> Self {
        Self::from_str(text).unwrap()
    }
}

impl From<String> for SharedText {
    fn from(text: String) -> Self {
        Self::from_str(&text).unwrap()
    }
}

impl From<String> for SharedDerivedText {
    fn from(text: String) -> Self {
        Self::from_str(&text).unwrap()
    }
}

impl From<&String> for SharedText {
    fn from(text: &String) -> Self {
        Self::from_str(text).unwrap()
    }
}

impl From<&String> for SharedDerivedText {
    fn from(text: &String) -> Self {
        Self::from_str(text).unwrap()
    }
}

/*impl From<StyledText> for TextProperty {
    fn from(text: StyledText) -> Self {
        Self::from_observable(text)
    }
}*/

impl From<&StyledText> for SharedText {
    fn from(text: &StyledText) -> Self {
        Self::from(text.clone())
    }
}

impl From<&StyledText> for SharedDerivedText {
    fn from(text: &StyledText) -> Self {
        Self::new_derived(text.clone())
    }
}

impl From<SharedText> for StyledText {
    fn from(text: SharedText) -> Self {
        text.get()
    }
}

impl From<&SharedText> for StyledText {
    fn from(text: &SharedText) -> Self {
        text.get()
    }
}

impl From<SharedText> for String {
    fn from(text: SharedText) -> Self {
        text.get().to_string()
    }
}

impl From<&SharedText> for String {
    fn from(text: &SharedText) -> Self {
        text.get().to_string()
    }
}

impl<T: Into<SharedDerivedText>> Add<T> for SharedText {
    type Output = SharedDerivedText;

    fn add(self, rhs: T) -> Self::Output {
        (&self).add(rhs)
    }
}

impl<T: Into<SharedDerivedText>> Add<T> for &SharedText {
    type Output = SharedDerivedText;

    fn add(self, rhs: T) -> Self::Output {
        let lhs = self.clone();
        let rhs = rhs.into();
        SharedDerivedText::from_fn([lhs.as_ref().into(), rhs.as_ref().into()].into(), move || lhs.get() + rhs.get())
    }
}

impl<T: Into<SharedDerivedText>> Add<T> for SharedDerivedText {
    type Output = SharedDerivedText;

    fn add(self, rhs: T) -> Self::Output {
        let lhs = self.clone();
        let rhs = rhs.into();
        SharedDerivedText::from_fn([lhs.as_ref().into(), rhs.as_ref().into()].into(), move || lhs.get() + rhs.get())
    }
}

impl<T: Into<SharedDerivedText>> Add<T> for &SharedDerivedText {
    type Output = SharedDerivedText;

    fn add(self, rhs: T) -> Self::Output {
        (&self.clone()).add(rhs)
    }
}

// impl Add<&str> for SharedText {
//     type Output = SharedDerivedText;
// 
//     fn add(self, rhs: &str) -> Self::Output {
//         (&self).add(rhs)
//     }
// }

impl Add<SharedText> for &str {
    type Output = SharedDerivedText;

    fn add(self, rhs: SharedText) -> Self::Output {
        let lhs = StyledText::from_str(self).unwrap();
        let rhs = rhs.clone();
        SharedDerivedText::from_fn([rhs.as_ref().into()].into(), move || lhs.clone() + rhs.get())
    }
}

// impl Add<&str> for &SharedText {
//     type Output = SharedDerivedText;
// 
//     fn add(self, rhs: &str) -> Self::Output {
//         let lhs = self.clone();
//         let rhs = StyledText::from_str(rhs).unwrap();
//         SharedDerivedText::from_fn([lhs.as_ref().into()].into(), move || lhs.get() + rhs.clone())
//     }
// }

impl Add<&SharedText> for &str {
    type Output = SharedDerivedText;

    fn add(self, rhs: &SharedText) -> Self::Output {
        let lhs = StyledText::from_str(self).unwrap();
        let rhs = rhs.clone();
        SharedDerivedText::from_fn([rhs.as_ref().into()].into(), move || lhs.clone() + rhs.get())
    }
}

// impl Add<&str> for SharedDerivedText {
//     type Output = SharedDerivedText;
// 
//     fn add(self, rhs: &str) -> Self::Output {
//         let lhs = self.clone();
//         let rhs = StyledText::from_str(rhs).unwrap();
//         SharedDerivedText::from_fn([lhs.as_ref().into()].into(), move || lhs.get() + rhs.clone())
//     }
// }

impl Add<&SharedDerivedText> for &str {
    type Output = SharedDerivedText;

    fn add(self, rhs: &SharedDerivedText) -> Self::Output {
        let lhs = StyledText::from_str(self).unwrap();
        let rhs = rhs.clone();
        SharedDerivedText::from_fn([rhs.as_ref().into()].into(), move || lhs.clone() + rhs.get())
    }
}

// impl Add<&str> for &SharedDerivedText {
//     type Output = SharedDerivedText;
// 
//     fn add(self, rhs: &str) -> Self::Output {
//         (&self.clone()).add(rhs)
//     }
// }