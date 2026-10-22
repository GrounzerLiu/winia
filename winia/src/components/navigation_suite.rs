//! NavigationSuiteScaffold——自适应导航套件（对齐 androidx
//! material3-adaptive-navigation-suite 的 NavigationSuiteScaffold /
//! NavigationSuiteType）。
//!
//! 一套 items 定义，按窗口尺寸类自动切换导航形态：
//!
//! - `ShortNavigationBarCompact`：窄窗（宽 Compact）——底栏 + Top 图标位 +
//!   EqualWeight 均布
//! - `ShortNavigationBarMedium`：矮窗（高 Compact）——底栏 + Start 图标位 +
//!   Centered 分组
//! - `WideNavigationRailCollapsed`：中宽（宽 Medium）——收起宽轨
//! - `WideNavigationRailExpanded`：宽窗（宽 Expanded）——展开宽轨
//! - `None`：不渲染导航
//!
//! 默认推算（[`navigation_suite_type`]）：窄窗→Compact 底栏；否则矮窗→Medium
//! 底栏；否则→**收起**宽轨（上游从不默认展开——展开是 rail 自己的状态，或显式
//! `.layout_type(WideNavigationRailExpanded)`）。窗口 resize 由 app 层触发重组，形态实时切换。
//!
//! `primaryActionContent` (the FAB slot) is implemented: `.primary_action(..)` floats it 16dp above
//! the bar, 16dp in from the trailing edge by default, and `.primary_action_alignment(..)` moves it.
//! Upstream only PLACES it for a bar shape (its rail branch measures the slot without placing it).
//! winia's scaffold does the same, and for a vertical shape it passes the action in as the RAIL'S
//! HEADER (`NavigationSuiteScaffold.kt:183-187`), so a FAB still shows on a rail layout.
//! [`NavigationSuiteScaffoldLayout`], the standalone placement component, parks that slot off-view
//! instead of dropping it — a different component, and the one place the parking applies.
//!
//! Deliberate deviation, `NavigationSuiteScaffoldLayout`'s content constraint: upstream measures the
//! content with a TIGHT `min = max` (`NavigationSuiteScaffold.kt:428-437`), so it fills whatever the
//! navigation leaves. winia uses a max-only constraint, so a content that hugs its own size keeps that
//! size instead of being stretched to the freed space.
//!
//! This is NOT a workaround for the show/hide animation, though an earlier revision of this note said
//! it was. `NavigationSuiteScaffold` does not use `NavigationSuiteScaffoldLayout` at all — it
//! hand-builds its `Row`/`Column`/`Stack` and gives the content `layout_weight(1.0)` +
//! `fill_max_size`, so its content already takes `height - navigation * progress` every frame and does
//! so green. Tightening the standalone layout's constraint is therefore untested rather than known to
//! be harmful; whoever wants to match upstream here should try it and run
//! `the_scaffold_layout_places_the_three_slots` plus a new hugging-content case, not cite the
//! visibility test.
//!
//! Each positional item's icon and label use separate movable handles. Their remembered values and
//! node identities survive bar/rail switches, and their own State reads still invalidate the current
//! host. The latest callback payloads are staged through a Backchannel; a scaffold build re-enters the
//! shape hosts so changed captures reach the retained content rather than remaining behind Skip.
//!
//! Verified by production-key tests and a deterministic UI round trip: all twelve tagged payload
//! nodes retain their identities, independent counters keep updating before/after moves, and labels
//! display new caller-captured values. Initializer counts or requested-shape text alone are not evidence.
//!
//! Deliberate differences: handles are positional (there is no keyed-destination API), the optional
//! collapse/swap/expand morph is winia-specific, and only icon/label payload state moves, not the
//! shape-specific item wrapper. Compose uses one retained content lambda and a plain `when`
//! (`NavigationSuiteScaffold.kt:577-578`). A fully absent placement is disposed, not cached.

mod layout;
mod scaffold;
mod types;

pub use layout::{NavigationSuite, NavigationSuiteScaffoldLayout};
pub use scaffold::NavigationSuiteScaffold;
pub use types::{
    navigation_suite_type, remember_navigation_suite_scaffold_state, NavigationSuiteColors,
    NavigationSuiteDefaults, NavigationSuiteItem, NavigationSuiteItemColors, NavigationSuiteItems,
    NavigationSuiteScaffoldState, NavigationSuiteScaffoldValue, NavigationSuiteType,
};

#[cfg(test)]
mod tests;
