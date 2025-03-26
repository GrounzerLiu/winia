use crate::ui::Unit;
use crate::ui::unit::Dp;

#[derive(Debug, Default, Clone)]
pub enum Size {
    #[default]
    Compact,
    Expanded,
    Fixed(Unit),
    Relative(f32),
}

impl From<Unit> for Size {
    fn from(unit: Unit) -> Self {
        Size::Fixed(unit)
    }
}

impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Size::Compact => {
                matches!(other, Size::Compact)
            }
            Size::Expanded => {
                matches!(other, Size::Expanded)
            }
            Size::Fixed(f) => match other {
                Size::Fixed(f2) => f == f2,
                _ => false,
            },
            Size::Relative(f) => match other {
                Size::Relative(f2) => f == f2,
                _ => false,
            },
        }
    }
}

impl Into<Size> for &str {
    fn into(self) -> Size {
        if self.ends_with("%") {
            let size = self[..self.len() - 1].parse::<f32>().unwrap() / 100.0;
            if size < 0.0 || size > 1.0 {
                panic!("Relative size must be between 0 and 100");
            }
            Size::Relative(size)
        } else {
            let size = self.parse::<f32>().unwrap();
            if size < 0.0 {
                panic!("Fixed size must be greater than 0");
            }
            Size::Fixed(size.dp())
        }
    }
}

impl Into<Size> for &Size {
    fn into(self) -> Size {
        self.clone()
    }
}
