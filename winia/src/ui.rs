//! UI 组件 — composable 函数集合
//!
//! 每个组件都是一个 Builder 结构体 + `build(ctx)` 方法，
//! 注册到 Composer 的组合树中。

pub mod text;
pub mod button;
pub mod card;
pub mod layout_components;
pub mod window;
pub mod theme;
pub mod rich_text;
pub mod selection_container;
pub mod text_field;
pub mod text_transformation;
pub mod animated_visibility;
pub mod animated_size;
pub mod animated_content;
pub mod crossfade;
pub mod overlay;
pub mod interaction;
pub mod icon;
pub mod image;
pub mod icon_button;
pub mod icon_toggle_button;
pub mod checkbox;
pub mod switch;
pub mod chip;

pub use animated_visibility::{AnimatedVisibility, VisibilityTransition, SlideDirection};
pub use animated_size::AnimatedSize;
pub use animated_content::AnimatedContent;
pub use crossfade::Crossfade;
pub use overlay::{Popup, Dialog, DropdownMenu, DropdownMenuItem, PopupPosition};

pub use text::Text;
pub use text::TextAlign;
pub use text::TextOverflow;
pub use text::TextStyle;
pub use text::ProvideTextStyle;
pub use text::FontWeight;
pub use text::FontSlant;
pub use button::Button;
pub use button::ButtonBorder;
pub use button::ButtonColors;
pub use button::ButtonDefaults;
pub use button::ButtonElevation;
pub use button::ButtonSize;
pub use button::ButtonStyle;
pub use card::{Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle};
pub use layout_components::{Column, Row, Stack, Spacer};
pub use window::Window;
pub use theme::WiniaTheme;
pub use theme::ThemeColors;
pub use theme::is_system_dark_theme;
pub use rich_text::RichText;
pub use rich_text::RichTextScope;
pub use selection_container::SelectionContainer;
pub use selection_container::SelectionRegistrar;
pub use selection_container::LOCAL_SELECTION_REGISTRAR;
pub use text_field::{TextField, TextFieldValue, TextFieldVariant, TextFieldColors, TextChange};
pub use text_transformation::{VisualTransformation, OffsetMapping, TransformedText, IdentityTransformation, PasswordTransformation};
pub use interaction::{MutableInteractionSource, ComponentState};
pub use icon::{AxisValue, Icon, IconSource, PathFillType, Tint};
pub use image::{ContentScale, Image, ImageAlignment};
pub use icon_button::{IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize, IconButtonStyle};
pub use icon_toggle_button::{IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults};
pub use checkbox::{Checkbox, CheckboxColors, CheckboxDefaults, TriStateCheckbox, ToggleableState};
pub use switch::{Switch, SwitchColors, SwitchDefaults};
pub use chip::{Chip, ChipVariant, ChipColors, SelectableChipColors, ChipDefaults};
