pub enum Dimension {
    Absolute(f32),
    Relative(f32),
}

impl From<f32> for Dimension {
    fn from(value: f32) -> Self {
        Dimension::Absolute(value)
    }
}

pub trait RelativeDimensionExt {
    fn r(value: f32) -> Dimension;
}

impl RelativeDimensionExt for f32 {
    fn r(value: f32) -> Dimension {
        Dimension::Relative(value)
    }
}