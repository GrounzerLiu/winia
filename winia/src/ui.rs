//! UI 组件 — composable 函数集合
//!
//! 每个组件都是一个 Builder 结构体 + `build(ctx)` 方法，
//! 注册到 Composer 的组合树中。

pub mod text;
pub mod button;
pub mod layout_components;
pub mod window;
pub mod theme;
pub mod rich_text;
pub mod selection_container;

pub use text::Text;
pub use text::TextAlign;
pub use text::TextOverflow;
pub use text::TextStyle;
pub use text::ProvideTextStyle;
pub use text::FontWeight;
pub use text::FontSlant;
pub use button::Button;
pub use button::ButtonStyle;
pub use layout_components::{Column, Row, Stack};
pub use window::Window;
pub use theme::WiniaTheme;
pub use theme::ThemeColors;
pub use theme::is_system_dark_theme;
pub use rich_text::RichText;
pub use rich_text::RichTextScope;
pub use selection_container::SelectionContainer;
pub use selection_container::SelectionRegistrar;
pub use selection_container::Rect;
pub use selection_container::LOCAL_SELECTION_REGISTRAR;