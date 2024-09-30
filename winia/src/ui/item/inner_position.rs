#[derive(Debug, Clone, Copy)]
pub enum InnerPosition {
    Start(f32),
    Middle(f32),
    End(f32),
    Relative(f32),
    Absolute(f32),
}
impl Default for InnerPosition {
    fn default() -> Self {
        Self::Middle(0.0)
    }
}