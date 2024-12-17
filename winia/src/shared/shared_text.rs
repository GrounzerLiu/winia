use std::ops::Add;
use std::str::FromStr;
use crate::core::RefClone;
use crate::shared::{Gettable, Shared};
use crate::text::StyledText;

pub type SharedText = Shared<StyledText>;

impl FromStr for SharedText {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SharedText::from(StyledText::from_str(s)?))
    }
}


impl From<&str> for SharedText {
    fn from(text: &str) -> Self {
        Self::from_str(text).unwrap()
    }
}

impl From<String> for SharedText {
    fn from(text: String) -> Self {
        Self::from_str(&text).unwrap()
    }
}

impl From<&String> for SharedText {
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

impl Add<SharedText> for SharedText {
    type Output = SharedText;

    fn add(self, rhs: SharedText) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = SharedText::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<&SharedText> for SharedText {
    type Output = SharedText;

    fn add(self, rhs: &SharedText) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = SharedText::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<SharedText> for &SharedText {
    type Output = SharedText;

    fn add(self, rhs: SharedText) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = SharedText::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}

impl Add<&SharedText> for &SharedText {
    type Output = SharedText;

    fn add(self, rhs: &SharedText) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        let mut output = SharedText::from_dynamic(Box::new(move || {
            lhs_clone.get() + rhs_clone.get()
        }));
        output.observe(self);
        output.observe(rhs);
        output
    }
}



// impl<T:ToString + 'static> Add<T> for SharedText {
//     type Output = SharedText;
//
//     fn add(self, rhs: T) -> Self::Output {
//         let lhs = self.ref_clone();
//         let mut output = SharedText::from_dynamic(Box::new(move || {
//             StyledText::from(lhs.get().to_string() + &rhs.to_string())
//         }));
//         output.observe(self);
//         output
//     }
// }