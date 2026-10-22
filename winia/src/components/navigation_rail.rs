//! Material 3 NavigationRail / NavigationRailItem（侧边导航栏）。
//!
//! 对齐 androidx-main NavigationRail.kt（Collapsed 基线，785 行）：
//!
//! - rail 最小宽 80dp（NarrowContainerWidth）、撑满高度、surface 底色
//!   （CollapsedTokens.ContainerColor = Surface——注意与导航栏的
//!   surfaceContainer 不同）、垂直 padding 4dp、item 间 spacedBy(4dp)、
//!   header 之后 8dp Spacer
//! - item 高基准 56dp（= ActiveIndicatorWidth，非导航栏的 80）
//! - 指示器胶囊 56×32 由图标推导；纵向内边距按有无 label 分支：
//!   有标签 (32−24)/2 = 4、无标签 (56−24)/2 = 16 → 无标签为 56×56 圆形
//! - 颜色 token 与导航栏完全一致（OnSecondaryContainer/Secondary/
//!   SecondaryContainer/OnSurfaceVariant/@38% DisabledAlpha）
//! - 双进度动画（alphaProgress stiffness200 / sizeProgress stiffness400）
//!   与两段式布局依赖同导航栏
//! - ripple 独立恒定全尺寸节点（z 序最上层）——未选中悬浮也有完整胶囊
//!   状态层热区；选中态展开动画不受影响
//!
//! ## 与导航栏的差异速查
//!
//! | 项 | NavigationBar | NavigationRail |
//! |---|---|---|
//! | 方向 | 底部横条 | 侧边竖列 |
//! | item 高基准 | 80dp | 56dp |
//! | 无标签胶囊 | ——（始终有标签位） | 56×56 圆形 |
//! | 容器色 | surfaceContainer | surface |
//! | header 槽 | 无 | 有（FAB/logo） |
//!
//! ## 平台差异（有意为之）
//!
//! - windowInsets：桌面默认零值。API 已预留（[WindowInsets] +
//!   [NavigationRail::window_insets]），未来 Android 支持或自定义窗口装饰
//!   （标题栏模拟状态栏）时由平台层填充真实尺寸
//! - PredictiveBack 缩放效果：依赖 Android 返回手势进度输入，桌面无此源
//! - Expanded 宽轨已实现（见本文件后半部分）；组件文档：docs/navigation-rail.md

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};
use crate::layout::{Alignment, Arrangement, BoxLayout};
use crate::layout::components::Column;
use crate::modifier::{Modifier};
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::overlay::{next_overlay_id, OverlayAnimSpec, OverlayDesc, PopupPosition};
use crate::input::interaction::MutableInteractionSource;
use crate::theme::WiniaTheme;
use std::sync::Arc;


mod item;
mod modal;
mod rail;
mod tokens;
mod wide;

pub(crate) use item::*;
pub(crate) use tokens::*;
pub(crate) use wide::*;

pub use item::NavigationRailItem;
pub use modal::{modal_rail_expanded_shape, ModalWideNavigationRail};
pub use rail::NavigationRail;
pub use tokens::{
    rail_item_colors, NavigationRailItemColors, WindowInsets, NAVIGATION_RAIL_ICON_SIZE,
    NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_INDICATOR_WIDTH,
    NAVIGATION_RAIL_ITEM_HEIGHT, NAVIGATION_RAIL_WIDTH,
};
pub use wide::{
    WideNavigationRail, WideNavigationRailItem, WideNavigationRailState,
    WIDE_RAIL_COLLAPSED_WIDTH, WIDE_RAIL_EXPANDED_MAX_WIDTH, WIDE_RAIL_EXPANDED_MIN_WIDTH,
    WIDE_RAIL_TOP_PADDING,
};

#[cfg(test)]
mod tests;
