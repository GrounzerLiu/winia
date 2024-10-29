#[derive(Debug, Default, Clone)]
pub enum Size {
    #[default]
    Compact,
    Expanded,
    Fixed(f32),
    Relative(f32),
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
            Size::Fixed(f) => {
                match other {
                    Size::Fixed(f2) => f == f2,
                    _ => false,
                }
            }
            Size::Relative(f) => {
                match other {
                    Size::Relative(f2) => f == f2,
                    _ => false,
                }
            }
        }
    }
}

impl Into<Size> for u32 {
    fn into(self) -> Size {
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for f32 {
    fn into(self) -> Size {
        if self < 0.0 {
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self)
    }
}

impl Into<Size> for f64 {
    fn into(self) -> Size {
        if self < 0.0 {
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for i32 {
    fn into(self) -> Size {
        if self < 0 {
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for &str {
    fn into(self) -> Size {
        if self.ends_with("%") {
            let size = self[..self.len() - 1].parse::<f32>().unwrap() / 100.0;
            Size::Relative(size)
        } else {
            let size = self.parse::<f32>().unwrap();
            Size::Fixed(size)
        }
    }
}

impl Into<Size> for &Size {
    fn into(self) -> Size {
        self.clone()
    }
}