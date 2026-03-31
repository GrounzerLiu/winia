use letclone::clone;
use crate::depend;
use crate::shared::SharedDerived;
use crate::ui::Radius;

pub mod corner {
    pub static NONE: &str = "shape_corner_none";
    pub static EXTRA_SMALL: &str = "shape_corner_extra_small";
    pub mod extra_small {

        pub static TOP: &str = "shape_corner_extra_small_top";
    }
    pub static SMALL: &str = "shape_corner_small";
    pub static MEDIUM: &str = "shape_corner_medium";
    pub static LARGE: &str = "shape_corner_large";
    pub mod large {
        pub static TOP: &str = "shape_corner_large_top";
        pub static START: &str = "shape_corner_large_start";
        pub static END: &str = "shape_corner_large_end";
    }
    pub static EXTRA_LARGE: &str = "shape_corner_extra_large";
    pub mod extra_large {
        pub static TOP: &str = "shape_corner_extra_large_top";
    }
    pub static FULL: &str = "shape_corner_full";
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub top_start: f32,
    pub top_end: f32,
    pub bottom_start: f32,
    pub bottom_end: f32,
}

impl From<SharedDerived<Corner>> for Radius {
    fn from(value: SharedDerived<Corner>) -> Self {
        let top_start = SharedDerived::from_fn(
            depend!(value),
            {
                clone!(value);
                move || value.get().top_start
            },
        );
        let top_end = SharedDerived::from_fn(
            depend!(value),
            {
                clone!(value);
                move || value.get().top_end
            }
        );
        let bottom_start = SharedDerived::from_fn(
            depend!(value),
            {
                clone!(value);
                move || value.get().bottom_start
            }
        );
        let bottom_end = SharedDerived::from_fn(
            depend!(value),
            {
                clone!(value);
                move || value.get().bottom_end
            }
        );
        Radius {
            top_start,
            top_end,
            bottom_start,
            bottom_end,
        }
    }
}