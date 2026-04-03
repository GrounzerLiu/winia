use crate::event::MeasureMode;

#[derive(Debug, Default, Clone)]
pub enum Size {
    #[default]
    /// Compute the size by the item itself.
    Auto,
    /// Fill the available space in the parent item.
    Fill,
    /// Fixed size.
    Fixed(f32),
    /// Relative size. The value is a percentage of the parent item size.
    Relative(f32),
}

impl Size {
    pub fn create_measure_mode(&self, max_size: f32) -> MeasureMode {
        match self {
            Size::Auto => MeasureMode::Unspecified(max_size),
            Size::Fill => MeasureMode::Specified(max_size),
            Size::Fixed(f) => MeasureMode::Specified(*f),
            Size::Relative(f) => MeasureMode::Specified(max_size * *f),
        }
    }
}

impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Size::Auto => {
                matches!(other, Size::Auto)
            }
            Size::Fill => {
                matches!(other, Size::Fill)
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

impl From<f32> for Size {
    fn from(value: f32) -> Self {
        Size::Fixed(value)
    }
}

macro_rules! impl_from{
    ($($t:ty),*) => {
        $(
            impl From<$t> for Size {
                fn from(value: $t) -> Self {
                    Size::Fixed(value as f32)
                }
            }
        )*
    };
}
impl_from!(u8, u16, u32, u64, i8, i16, i32, i64, usize, isize, f64);

impl From<&str> for Size {
    fn from(value: &str) -> Self {
        if let Some(num_str) = value.strip_suffix('%') {
            if let Ok(num) = num_str.parse::<f32>() {
                return Size::Relative(num / 100.0);
            }
        } else if value.eq_ignore_ascii_case("auto") {
            return Size::Auto;
        } else if value.eq_ignore_ascii_case("fill") {
            return Size::Fill;
        } else if let Ok(num) = value.parse::<f32>() {
            return Size::Fixed(num);
        }
        Size::Auto
    }
}
