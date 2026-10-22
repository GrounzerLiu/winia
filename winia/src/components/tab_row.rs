//! Material 3 TabRow / Tab 组件 — 对标 androidx-main TabRow.kt + Tab.kt
//!
//! 当前实现：Fixed TabRow（Primary/Secondary，等分）+ ScrollableTabRow（自然宽可滚动、
//! 选中居中）+ Tab 组件（text/icon 槽）。
//! 指示条动画：双 State<f32>（offset/width）在 build 期创建，TabRowLayoutPolicy.measure 期
//! 计算 target 后调 push_animatable，State::get() 注册 layout_deps（两段式依赖——动画帧只重测不重组）。
//! 首次布局免动画：initialized 标记（AtomicBool）跳过首次 push_animatable。
//!
//! ScrollableTabRow 的选中居中滚动依赖 fling_limit（= 内容宽 - 视口宽）——但该值由
//! measure_node 在 policy.measure **之后**才回写（node.rs:1535-1551），首帧读到恒 0 →
//! 目标恒 0 不滚动。故首帧用 layout_seen 标记（set(true) notify → 下帧重测）延迟消费
//! last_selected，第二帧 fling_limit 就绪后再触发居中滚动。
//!
//! 偏差记录（与 Compose 对照）：
//! - 无 TabBaselineLayout 基线精确数学（text+icon 竖排垂直居中，无 firstBaseline/lastBaseline 修正）
//! - LeadingIconTab 为 Tab builder 的 `.leading_icon()` 模式（icon 左 + 8dp + text 右，
//!   SmallTabHeight）——非独立组合函数；无 icon-only 独立 API
//! - Tab 颜色过渡用静态颜色（无 animateColor 插值；后续可加 graphics_layer 交叉淡化）
//! - RTL：Fixed TabRow 与 ScrollableTabRow 均支持 RTL——tab 布局镜像（物理 left 对齐），
//!   ScrollableTabRow 的滚动容器标记 scroll_reverse（render 平移镜像：offset 0 = 内容
//!   末端），居中滚动 target 绕 available 镜像（offset_rtl = available - offset_ltr）
//! - contentWidth = max(intrinsic tab width - 32dp, 24dp)（`maxIntrinsicWidth` 走框架的固有尺寸
//!   协议：固定版先按 slot 宽问 max intrinsic height、再按该高问 max intrinsic width，TabRow.kt:450-459；
//!   可滚动版按 `Constraints.Infinity` 问两者，TabRow.kt:582-603）
//! - 动画 spec = spring(damping_ratio=0.6, stiffness=700)（对齐 M3 Expressive DefaultSpatial）
//! - 指示条高固定 3dp（ActiveIndicatorHeight）
//! - 无 TabIndicatorScope 自定义指示器 API（当前内部固定）

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::unit::{Offset, Size};
use crate::layout::node::{
    intrinsic_size_of, measure_node, IntrinsicCtx, IntrinsicQuery, LayoutNode, MeasurePolicy,
    Placement,
};
use crate::layout::LayoutDirection;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::text::TextAlign;
use crate::theme::WiniaTheme;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};


mod defaults;
mod layout;
mod row;
mod scrollable;
mod shared;
mod tab;

pub(crate) use layout::*;
pub(crate) use row::*;
pub(crate) use shared::*;

pub use defaults::TabRowDefaults;
pub use row::{TabIndicatorScope, TabRow};
pub use scrollable::{ScrollableTabRow, ScrollableTabRowDefaults};
pub use shared::{
    TabPosition, ACTIVE_INDICATOR_HEIGHT, HORIZONTAL_TEXT_PADDING, ICON_TEXT_SPACING,
    LARGE_TAB_HEIGHT, LEADING_ICON_TEXT_SPACING, MIN_INDICATOR_WIDTH,
    SCROLLABLE_TAB_ROW_EDGE_START_PADDING, SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH, SMALL_TAB_HEIGHT,
    TAB_ROW_HEIGHT,
};
pub use tab::Tab;
