use crate::shared::{SharedDerived, SharedSource};
use crate::ui::item::Size;

pub type SharedSize = SharedSource<Size>;
pub type SharedDerivedSize = SharedDerived<Size>;

impl SharedSource<Size> {
    pub fn auto() -> Self {
        SharedSource::new(Size::Auto)
    }

    pub fn fixed(size: f32) -> Self {
        SharedSource::new(Size::Fixed(size))
    }

    pub fn relative(size: f32) -> Self {
        SharedSource::new(Size::Relative(size))
    }

    pub fn fill() -> Self {
        SharedSource::new(Size::Fill)
    }
}


macro_rules! impl_from {
    ($($ty:ty),*) => {
        $(
            impl From<$ty> for SharedSize {
                fn from(value: $ty) -> Self {
                    SharedSource::new(Size::from(value))
                }
            }

            impl From<$ty> for SharedDerivedSize {
                fn from(value: $ty) -> Self {
                    let shared = SharedSource::new(Size::from(value));
                    SharedDerived::from(shared)
                }
            }
        )*
    };
}
impl_from!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);
