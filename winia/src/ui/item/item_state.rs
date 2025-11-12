#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ItemState {
    #[default]
    Enabled,
    Disabled,
    Focused,
    Hovered,
    Pressed,
}