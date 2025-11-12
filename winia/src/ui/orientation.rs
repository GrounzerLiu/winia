use std::ops::Not;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Horizontal)
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl Not for Orientation {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::Vertical => Self::Horizontal,
            Self::Horizontal => Self::Vertical,
        }
    }
}