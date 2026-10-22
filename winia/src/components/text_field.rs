//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：旧版 winia v1 的 text_field 实现 + Compose Foundation 1.11.4 源码
//! （KeyCommand/KeyMapping/UndoManager 语义）

use crate::text::field::{
    TextFieldColors, TextFieldSlotRole, TextFieldVariant,
};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::modifier::Modifier;
use crate::composable;
use crate::components::text::{ProvideTextStyle};
use crate::text::TextStyle;
use crate::theme::WiniaTheme;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;


mod colors;
mod edit;
mod field;
mod layout;
mod value;

pub(crate) use edit::*;
#[cfg(test)]
pub(crate) use field::*;
pub(crate) use layout::*;
pub(crate) use value::*;

pub use field::TextField;
pub use value::{TextChange, TextFieldValue};

#[cfg(test)]
mod tests;
