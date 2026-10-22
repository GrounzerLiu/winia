use super::*;

// ── Token 常量（androidx-main tokens/NavigationRail*.kt）──

/// rail 最小宽 = NavigationRailCollapsedTokens.NarrowContainerWidth，
/// 同时是 item 的最小宽（NavigationRailItemWidth）
pub const NAVIGATION_RAIL_WIDTH: f32 = 80.0;
/// item 高基准 = NavigationRailItemHeight（= VerticalItemTokens.ActiveIndicatorWidth）
pub const NAVIGATION_RAIL_ITEM_HEIGHT: f32 = 56.0;
/// 指示器胶囊宽 = NavigationRailVerticalItemTokens.ActiveIndicatorWidth
pub const NAVIGATION_RAIL_INDICATOR_WIDTH: f32 = 56.0;
/// 指示器胶囊高（有标签）= NavigationRailVerticalItemTokens.ActiveIndicatorHeight
pub const NAVIGATION_RAIL_INDICATOR_HEIGHT: f32 = 32.0;
/// 图标尺寸 = NavigationRailBaselineItemTokens.IconSize
pub const NAVIGATION_RAIL_ICON_SIZE: f32 = 24.0;
/// 指示器横向内边距 = (ActiveIndicatorWidth - IconSize) / 2
pub(crate) const INDICATOR_H_PADDING: f32 =
    (NAVIGATION_RAIL_INDICATOR_WIDTH - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 有标签时指示器纵向内边距 = (ActiveIndicatorHeight - IconSize) / 2
pub(crate) const INDICATOR_V_PADDING_WITH_LABEL: f32 =
    (NAVIGATION_RAIL_INDICATOR_HEIGHT - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 无标签时指示器纵向内边距 = (ActiveIndicatorWidth - IconSize) / 2
/// → 无标签胶囊为 56×56 圆形（CornerFull）
pub(crate) const INDICATOR_V_PADDING_NO_LABEL: f32 =
    (NAVIGATION_RAIL_INDICATOR_WIDTH - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 图标-标签间距 = NavigationRailItemVerticalPadding
pub(crate) const ITEM_ICON_LABEL_GAP: f32 = 4.0;
/// rail 内容垂直 padding 与 item 间距 = NavigationRailVerticalPadding
pub(crate) const RAIL_VERTICAL_PADDING: f32 = 4.0;
/// header 之后的间距 = NavigationRailHeaderPadding
pub(crate) const HEADER_SPACER: f32 = 8.0;
/// Material DisabledAlpha
#[allow(dead_code)] // the tests in this file call it
pub(crate) const DISABLED_ALPHA: f32 = 0.38;
/// 宽轨 Start 态指示器横向内边距（leading/trailing 各 16，与导航栏水平 item 一致）
pub(crate) const WIDE_INDICATOR_H_PADDING: f32 = 16.0;
/// The gap after the wide rail's header = `NavigationRailBaselineItemTokens.HeaderSpaceMinimum`.
/// androidx computes `headerOffset = headerPlaceable.height + WNRHeaderPadding`
/// (`WideNavigationRail.kt:427`) with `WNRHeaderPadding` read from that token
/// (`WideNavigationRail.kt:1395-1396`), whose value is 40.
///
/// The plain rail uses a DIFFERENT token for the same slot — `NavigationRailHeaderPadding` = 8,
/// placed as a `Spacer` inside a `Column` whose `verticalArrangement` is
/// `spacedBy(NavigationRailVerticalPadding)` = 4 (`NavigationRail.kt:126-140`), so its gap is
/// 4 + 8 + 4 = 16. The two rails are deliberately not equal: 40 against 16.
pub(crate) const WIDE_RAIL_HEADER_PADDING: f32 = 40.0;

/// The wide rail item's own horizontal padding — `WNRItemHorizontalPadding`
/// (`WideNavigationRail.kt:1387`), 20. It is the margin the pill hangs off inside the item
/// (`placeAnimatedLabelAndIcon`: `rippleXTopIcon = itemHorizontalPadding`,
/// `NavigationItem.kt:1143`) and, applied on each side, what the rail measures an expanded item
/// against (`expandedItemMaxWidth = maxItemWidth + WNRItemHorizontalPadding`,
/// `WideNavigationRail.kt:378-381`). The width policy below adds it on both sides — see the
/// comment there.
pub(crate) const WIDE_RAIL_ITEM_H_PADDING: f32 = 20.0;

/// The wide rail item's collapsed minimum height — upstream's `TopIconItemMinHeight`
/// (`WideNavigationRail.kt:1403-1404`), which reads `NavigationRailBaselineItemTokens.ContainerHeight`
/// = 64. It is NOT the plain rail's `NavigationRailItemHeight` (56): the wide rail's top-icon item
/// is taller, and upstream's `item_topIconPosition_withLongLabel_automaticallyResizesHeight` uses
/// this very token as the "default height" a long label has to exceed.
pub(crate) const WIDE_RAIL_ITEM_MIN_HEIGHT: f32 = 64.0;

/// The wide rail item's EXPANDED minimum height — upstream animates `itemMinHeight` from
/// `TopIconItemMinHeight` down to `minimumA11ySize` as the rail expands
/// (`WideNavigationRail.kt:284-289`), and `minimumA11ySize` is
/// `LocalMinimumInteractiveComponentSize`, 48 by default.
pub(crate) const WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED: f32 = 48.0;

/// The vertical gap between wide rail items: upstream animates `itemVerticalSpacedBy` from
/// `NavigationRailCollapsedTokens.ItemVerticalSpace` (4) down to **0** when expanded
/// (`WideNavigationRail.kt:279-283`) — the expanded items carry their own spacing through the pill.
pub(crate) const WIDE_RAIL_ITEM_GAP_EXPANDED: f32 = 0.0;

/// The expanded (Start) item's icon-to-label gap = `NavigationRailHorizontalItemTokens.IconLabelSpace`
/// = 8 (`WideNavigationRail.kt:820`). The Top endpoint uses the VERTICAL item's `IconLabelSpace` = 4
/// instead (`:818`) — the two endpoints genuinely differ here.
pub(crate) const WIDE_START_ICON_LABEL_GAP: f32 = 8.0;

/// The expanded (Start) pill's vertical padding, one side = `ItemStartIconIndicatorVerticalPadding`
/// = (HorizontalItem.ActiveIndicatorHeight 56 − IconSize 24) / 2 = 16 (`WideNavigationRail.kt:1411-1413`,
/// read through `IndicatorExpandedPadding` at `:1172-1176`). The Top endpoint uses the vertical
/// item's 4 instead, so the pill grows 32 -> 56 tall across the morph.
///
/// (`:1406-1408` is the TOP item's twin, `(VerticalItem.ActiveIndicatorHeight 32 − 24) / 2 = 4` —
/// this comment cited it until a review checked the range.)
pub(crate) const WIDE_START_INDICATOR_V_PADDING: f32 = 16.0;

/// 窗口避让尺寸（对标 androidx WindowInsets 的桌面简化版）。
///
/// 桌面平台默认全零（无系统栏叠加）。自定义窗口装饰（自绘标题栏模拟状态栏）或平台后端填充
/// 真实系统栏尺寸后传入 [NavigationRail::window_insets]；insets padding 在最外层，rail 自身
/// 垂直 padding 在其内。The foundation type is [`crate::window::insets::Insets`].
pub use crate::window::insets::Insets as WindowInsets;

// ── 颜色 ──

/// NavigationRailItemColors：androidx 中与 NavigationBarItemColors 同构
/// （7 个色槽 + 相同 token 映射）——直接复用导航栏类型，零重复。
pub type NavigationRailItemColors = crate::components::navigation_bar::NavigationBarItemColors;

/// NavigationRailItemDefaults.colors() 对齐：颜色 token 映射与导航栏完全一致
/// （ItemActiveIcon=OnSecondaryContainer / ItemActiveLabelText=Secondary /
/// ItemActiveIndicator=SecondaryContainer / Inactive=OnSurfaceVariant /
/// Disabled=@38%）。
pub fn rail_item_colors(theme: &crate::theme::ThemeColors) -> NavigationRailItemColors {
    crate::components::navigation_bar::NavigationBarItemColors::from_theme(theme)
}

pub(crate) fn with_alpha_factor(color: Color, factor: f32) -> Color {
    Color::from_argb(
        ((color.a as f32 * factor).round().min(255.0)) as u8,
        color.r,
        color.g,
        color.b,
    )
}

// ── The motion-scheme tokens this component names (all verified against androidx) ──

/// An item's icon/label fade: `NavigationRail.kt:236-240` animates `alphaAnimationProgress` on
/// `DefaultEffects` (damping 1.0 / stiffness 1600). winia used damping 1.0 / stiffness 200.
pub(crate) fn rail_alpha_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}

/// The indicator pill's size and shape morph: `NavigationRail.kt:242-246` animates
/// `sizeAnimationProgress` on `FastSpatial` (0.9 / 1400), whose damping is what makes the pill
/// settle with a slight overshoot. winia used damping 1.0 / stiffness 400.
///
/// Note this is the PLAIN rail's own token. The wide rail item morphs on `DefaultSpatial` instead —
/// see `wide_rail_item_morph_spec`.
pub(crate) fn rail_item_size_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

/// The container's own expand/collapse progress: `WideNavigationRail.kt:265-266` animates
/// `minWidth`/`widthFullRange` on `DefaultSpatial` (0.9 / 700). winia used damping 1.0 /
/// stiffness 400.
pub(crate) fn rail_container_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

/// The MODAL rail's width animation: upstream swaps in `FastSpatial` (0.9 / 1400) for the modal
/// form (`WideNavigationRail.kt:269`: `modalAnimationSpec = MotionSchemeKeyTokens.FastSpatial`),
/// which is what makes a modal rail open faster than an in-flow one.
pub(crate) fn modal_rail_container_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

/// The wide rail item's own morph (icon position and pill size): `NavigationItem.kt:486-491` and
/// `:1232-1237` both use `DefaultSpatial` (0.9 / 700), NOT the plain rail's `FastSpatial`. The wide
/// rail item is the one that travels between two full layouts, so it uses the slower spatial token.
pub(crate) fn wide_rail_item_morph_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

