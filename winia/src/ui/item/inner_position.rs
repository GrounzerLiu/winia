use crate::ui::Unit;
use crate::ui::unit::Dp;

#[derive(Debug, Clone, Copy)]
pub enum InnerPosition {
    /// The f32 value the offset is relative to the start of the parent.
    Start(Unit),
    /// The f32 value the offset is relative to the middle of the parent.
    Middle(Unit),
    /// The f32 value the offset is relative to the end of the parent.
    End(Unit),
    /// 1.0 is the end of the parent, 0.0 is the start of the parent.
    Relative(f32),
    /// The f32 value the offset is relative to the start of the root [`Item`](crate::ui::Item).
    Absolute(Unit),
}
impl Default for InnerPosition {
    fn default() -> Self {
        Self::Middle(0.0.dp())
    }
}
