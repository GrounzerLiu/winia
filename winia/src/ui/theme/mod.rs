pub(crate) mod theme;
// pub use theme::*;
pub mod colors;
mod material_theme;
pub mod styles;
pub mod dimensions;

pub use material_theme::*;

use crate::ui::Theme;

pub trait Style {
    fn apply(&self, theme: &mut Theme, prefix: impl Into<String>);
}