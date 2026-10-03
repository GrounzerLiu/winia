//! Selection state shared by the selection components — Compose's
//! `androidx.compose.foundation.selection`.
//!
//! `ToggleableState` is what a checkbox, a switch and a radio button are all made of, and the
//! accessibility layer reads it to publish an AT toggle state. It lived in `ui/checkbox.rs`, which
//! meant `semantics.rs` and `accessibility.rs` had to name a Material 3 checkbox to describe a state
//! that is older and more general than one. It belongs below the components.

/// 三态（对标 foundation `ToggleableState`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleableState {
    Off,
    On,
    Indeterminate,
}

impl ToggleableState {
    /// `ToggleableState(checked: Boolean)` 等价物
    pub fn from_bool(checked: bool) -> Self {
        if checked { Self::On } else { Self::Off }
    }

    /// 视觉“着色选中”态：On 与 Indeterminate 都算（颜色解析用）。
    /// 注意与 M3 `ToggleableState.isSelected`（仅 On）语义不同——外部如需
    /// “真选中”判断请用 `self == ToggleableState::On`。
    pub fn is_checked(self) -> bool {
        matches!(self, Self::On | Self::Indeterminate)
    }

    pub fn is_indeterminate(self) -> bool {
        self == Self::Indeterminate
    }
}
