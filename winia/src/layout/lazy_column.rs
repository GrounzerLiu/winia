//! LazyColumn / LazyRow 组件 — 对标 Compose foundation lazy `LazyColumn` / `LazyRow`
//!
//! 核心机制（对齐 Compose lazy 架构）：
//! - **锚点滚动模型**：`LazyListState.first_visible_index + first_visible_offset`
//!   而非像素 offset——滚动位置由"第一个可见项的索引 + 其偏移"表达；
//! - **稳定 key**：`items()` 接受 `key` 工厂（数据 id 等稳定值）。插入/删除
//!   列表前部项后，按 key 找回原 firstVisibleItem 保持滚动位置（Compose
//!   `LazyListScrollPosition.updateScrollPositionIfTheFirstItemWasMoved` 语义）；
//! - **区间内容**：`item/items` 注册为 IntervalList（count + key 工厂 + content
//!   闭包），全局 index 经区间定位转局部 index（Compose `LazyListIntervalContent`）；
//! - **懒测量**：组合期按高度缓存预估可见范围 [start, end]，只注册/组合可见项；
//!   测量期真实排布并把高度写回缓存（未缓存项用默认高度预估，偏差触发补注册）。
//!
//! ⚠ 与 Compose 差异：winia 组合为命令式（build 直接注册节点），无 Compose 的
//! LazyLayout 测量期组合——这里用"组合期预估 + 测量期校正"两阶段模型。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::modifier::Modifier;
use std::marker::PhantomData;
use std::sync::Arc;

/// 未测量项的预估高度（首次进入视口时用，测量后写回缓存）
pub const LAZY_ITEM_ESTIMATED_HEIGHT: f32 = 48.0;
/// 超出视口后仍注册的额外项数（上下各预取，减少滚动时补注册抖动）
pub const LAZY_BEYOND_BOUNDS: usize = 4;

/// The lazy list's main axis. Sealed, like [`Axis`](super::axis::Axis): the two markers it carries are
/// the whole set.
///
/// The questions both container families ask — main and cross extents, `size`, `point` — are on
/// [`Axis`](super::axis::Axis); this adds what only a lazy list needs.
pub trait LazyAxis: super::axis::Axis {
    /// A child's constraints: the cross axis inherits the parent's max, the main axis is unbounded
    /// (wrap content).
    #[doc(hidden)]
    fn child_constraints(c: Constraints) -> Constraints;
    /// Replace the cross-axis max (`contentPadding` shrinks the item's usable width).
    #[doc(hidden)]
    fn with_cross_max(c: Constraints, v: f32) -> Constraints;
    /// Pin the main axis's min AND max to `v` — what `fillParentMaxWidth` / `fillParentMaxHeight` do.
    ///
    /// Off by default: the unbounded main axis is what lets an item wrap to its content (see
    /// [`Self::child_constraints`]). A PAGED list wants the opposite — each item exactly fills the
    /// viewport, which is what makes a snap position an item boundary.
    #[doc(hidden)]
    fn with_main_exact(c: Constraints, v: f32) -> Constraints;
    /// The scroll modifier, per axis (vertical / horizontal)
    #[doc(hidden)]
    fn scroll(state: crate::modifier::ScrollState) -> Modifier;
    /// Whether this axis scrolls vertically — the direction an item's placement animates in.
    #[doc(hidden)]
    const VERTICAL: bool;
}

impl LazyAxis for crate::layout::axis::VerticalAxis {
    const VERTICAL: bool = true;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: f32::MAX }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: v, min_height: 0.0, max_height: c.max_height }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: c.min_width, max_width: c.max_width, min_height: v, max_height: v }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().vertical_scroll(state)
    }
}

impl LazyAxis for crate::layout::axis::HorizontalAxis {
    const VERTICAL: bool = false;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: f32::MAX, min_height: 0.0, max_height: c.max_height }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: v }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: v, max_width: v, min_height: c.min_height, max_height: c.max_height }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().horizontal_scroll(state)
    }
}

/// Per-item placement animations, keyed by the item key. Shared by every layout of the list.
#[derive(Debug, Default)]
pub(crate) struct ItemAnimations {
    /// Incremented once per `place`; an item seen on the PREVIOUS frame animates its move, an item
    /// that was away resets instead of springing from a stale slot.
    pub frame: u64,
    pub items: std::collections::HashMap<u64, ItemAnimation>,
}

/// One animated item's current paint offset and the slot it occupied last frame.
#[derive(Debug)]
pub(crate) struct ItemAnimation {
    pub offset: crate::runtime::state::State<f32>,
    pub last_main: f32,
    pub last_frame: u64,
}

/// How many placement passes a key survives after it leaves the window before its animation state
/// is dropped. The map is keyed by item key, so a feed of ever-new keys would otherwise grow without
/// bound; an item that comes back after this window starts from a fresh slot anyway.
const ITEM_ANIMATION_RETENTION_FRAMES: u64 = 240;

fn retain_recent_items(anims: &mut ItemAnimations, frame: u64) {
    anims
        .items
        .retain(|_, item| frame.saturating_sub(item.last_frame) <= ITEM_ANIMATION_RETENTION_FRAMES);
}

mod interval;
mod list;
mod policy;
mod state;

pub(crate) use interval::*;
pub(crate) use list::*;
pub(crate) use policy::*;

pub use list::{ItemHeightCache, LazyColumn, LazyList, LazyRow};
pub use state::LazyListState;

#[cfg(test)]
mod tests;
