use crate::ui::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasureMode {
    /// Indicates that the parent has determined an exact size for the child.
    Specified(f32),
    /// Indicates that the child can determine its own size. The value of this enum is the maximum size the child can use.
    Unspecified(f32),
}

impl MeasureMode {
    pub fn from_size(size: Size, max: f32) -> Self {
        match size {
            Size::Auto => MeasureMode::Unspecified(max),
            Size::Fill => MeasureMode::Specified(max),
            Size::Fixed(size) => MeasureMode::Specified(size),
            Size::Relative(ratio) => MeasureMode::Specified(max * ratio.clamp(0.0, f32::MAX)),
        }
    }
    pub fn value(self) -> f32 {
        match self {
            MeasureMode::Specified(value) => value,
            MeasureMode::Unspecified(value) => value,
        }
    }
}

impl Into<f32> for MeasureMode {
    fn into(self) -> f32 {
        self.value()
    }
}
