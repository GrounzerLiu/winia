//! 主题系统 — Material Design 3 完整色彩方案 + dark-light 自动检测。
//!
//! ```ignore
//! WiniaTheme::auto(ctx, |ctx| {
//!     let c = WiniaTheme::colors();
//!     Text::new("Hello").color(c.primary).build(ctx);
//! });
//! ```

use crate::runtime::composition_local::CompositionLocal;
use crate::runtime::composer::ComposeCtx;
use crate::layout::LayoutDirection;
use crate::graphics::Color;
use crate::text::{FontWeight, TextStyle};
use crate::unit::{Sp, TextUnit};
use material_colors::color::Rgb;
use material_colors::theme::ThemeBuilder;
use std::sync::LazyLock;


/// outside it (the app's window-level test drives the same mode).
#[cfg(test)]
pub(crate) static THEME_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn theme_mode_test_lock() -> &'static std::sync::Mutex<()> {
    &THEME_TEST_SERIAL
}


mod colors;
mod motion;
mod provider;
mod typography;

pub(crate) use colors::*;
pub(crate) use motion::*;
pub(crate) use provider::*;

pub use colors::{is_system_dark_theme, set_system_dark_mode, ThemeColors};
pub use motion::MotionScheme;
pub use provider::{ThemeSpec, WindowTheme, WiniaTheme};
pub use typography::Typography;

#[cfg(test)]
mod tests;
