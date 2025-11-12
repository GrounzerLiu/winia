use std::ops::Mul;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    horizontal: HorizontalAlignment,
    vertical: VerticalAlignment,
}
impl Alignment {
    pub fn top_start() -> Self {
        Self {
            horizontal: HorizontalAlignment::Start,
            vertical: VerticalAlignment::Top,
        }
    }

    pub fn top_center() -> Self {
        Self {
            horizontal: HorizontalAlignment::Center,
            vertical: VerticalAlignment::Top,
        }
    }

    pub fn top_end() -> Self {
        Self {
            horizontal: HorizontalAlignment::End,
            vertical: VerticalAlignment::Top,
        }
    }

    pub fn center_start() -> Self {
        Self {
            horizontal: HorizontalAlignment::Start,
            vertical: VerticalAlignment::Center,
        }
    }

    pub fn center() -> Self {
        Self {
            horizontal: HorizontalAlignment::Center,
            vertical: VerticalAlignment::Center,
        }
    }

    pub fn center_end() -> Self {
        Self {
            horizontal: HorizontalAlignment::End,
            vertical: VerticalAlignment::Center,
        }
    }

    pub fn bottom_start() -> Self {
        Self {
            horizontal: HorizontalAlignment::Start,
            vertical: VerticalAlignment::Bottom,
        }
    }

    pub fn bottom_center() -> Self {
        Self {
            horizontal: HorizontalAlignment::Center,
            vertical: VerticalAlignment::Bottom,
        }
    }

    pub fn bottom_end() -> Self {
        Self {
            horizontal: HorizontalAlignment::End,
            vertical: VerticalAlignment::Bottom,
        }
    }

    pub fn horizontal(&self) -> HorizontalAlignment {
        self.horizontal
    }

    pub fn vertical(&self) -> VerticalAlignment {
        self.vertical
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

impl Mul<HorizontalAlignment> for VerticalAlignment {
    type Output = Alignment;

    fn mul(self, rhs: HorizontalAlignment) -> Self::Output {
        Alignment {
            horizontal: rhs,
            vertical: self,
        }
    }
}

impl Mul<VerticalAlignment> for HorizontalAlignment {
    type Output = Alignment;

    fn mul(self, rhs: VerticalAlignment) -> Self::Output {
        Alignment {
            horizontal: self,
            vertical: rhs,
        }
    }
}