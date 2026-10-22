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
const INDICATOR_H_PADDING: f32 =
    (NAVIGATION_RAIL_INDICATOR_WIDTH - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 有标签时指示器纵向内边距 = (ActiveIndicatorHeight - IconSize) / 2
const INDICATOR_V_PADDING_WITH_LABEL: f32 =
    (NAVIGATION_RAIL_INDICATOR_HEIGHT - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 无标签时指示器纵向内边距 = (ActiveIndicatorWidth - IconSize) / 2
/// → 无标签胶囊为 56×56 圆形（CornerFull）
const INDICATOR_V_PADDING_NO_LABEL: f32 =
    (NAVIGATION_RAIL_INDICATOR_WIDTH - NAVIGATION_RAIL_ICON_SIZE) / 2.0;
/// 图标-标签间距 = NavigationRailItemVerticalPadding
const ITEM_ICON_LABEL_GAP: f32 = 4.0;
/// rail 内容垂直 padding 与 item 间距 = NavigationRailVerticalPadding
const RAIL_VERTICAL_PADDING: f32 = 4.0;
/// header 之后的间距 = NavigationRailHeaderPadding
const HEADER_SPACER: f32 = 8.0;
/// Material DisabledAlpha
#[allow(dead_code)] // the tests in this file call it
const DISABLED_ALPHA: f32 = 0.38;
/// 宽轨 Start 态指示器横向内边距（leading/trailing 各 16，与导航栏水平 item 一致）
const WIDE_INDICATOR_H_PADDING: f32 = 16.0;
/// The gap after the wide rail's header = `NavigationRailBaselineItemTokens.HeaderSpaceMinimum`.
/// androidx computes `headerOffset = headerPlaceable.height + WNRHeaderPadding`
/// (`WideNavigationRail.kt:427`) with `WNRHeaderPadding` read from that token
/// (`WideNavigationRail.kt:1395-1396`), whose value is 40.
///
/// The plain rail uses a DIFFERENT token for the same slot — `NavigationRailHeaderPadding` = 8,
/// placed as a `Spacer` inside a `Column` whose `verticalArrangement` is
/// `spacedBy(NavigationRailVerticalPadding)` = 4 (`NavigationRail.kt:126-140`), so its gap is
/// 4 + 8 + 4 = 16. The two rails are deliberately not equal: 40 against 16.
const WIDE_RAIL_HEADER_PADDING: f32 = 40.0;

/// The wide rail item's own horizontal padding — `WNRItemHorizontalPadding`
/// (`WideNavigationRail.kt:1387`), 20. It is the margin the pill hangs off inside the item
/// (`placeAnimatedLabelAndIcon`: `rippleXTopIcon = itemHorizontalPadding`,
/// `NavigationItem.kt:1143`) and, applied on each side, what the rail measures an expanded item
/// against (`expandedItemMaxWidth = maxItemWidth + WNRItemHorizontalPadding`,
/// `WideNavigationRail.kt:378-381`). The width policy below adds it on both sides — see the
/// comment there.
const WIDE_RAIL_ITEM_H_PADDING: f32 = 20.0;

/// The wide rail item's collapsed minimum height — upstream's `TopIconItemMinHeight`
/// (`WideNavigationRail.kt:1403-1404`), which reads `NavigationRailBaselineItemTokens.ContainerHeight`
/// = 64. It is NOT the plain rail's `NavigationRailItemHeight` (56): the wide rail's top-icon item
/// is taller, and upstream's `item_topIconPosition_withLongLabel_automaticallyResizesHeight` uses
/// this very token as the "default height" a long label has to exceed.
const WIDE_RAIL_ITEM_MIN_HEIGHT: f32 = 64.0;

/// The wide rail item's EXPANDED minimum height — upstream animates `itemMinHeight` from
/// `TopIconItemMinHeight` down to `minimumA11ySize` as the rail expands
/// (`WideNavigationRail.kt:284-289`), and `minimumA11ySize` is
/// `LocalMinimumInteractiveComponentSize`, 48 by default.
const WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED: f32 = 48.0;

/// The vertical gap between wide rail items: upstream animates `itemVerticalSpacedBy` from
/// `NavigationRailCollapsedTokens.ItemVerticalSpace` (4) down to **0** when expanded
/// (`WideNavigationRail.kt:279-283`) — the expanded items carry their own spacing through the pill.
const WIDE_RAIL_ITEM_GAP_EXPANDED: f32 = 0.0;

/// The expanded (Start) item's icon-to-label gap = `NavigationRailHorizontalItemTokens.IconLabelSpace`
/// = 8 (`WideNavigationRail.kt:820`). The Top endpoint uses the VERTICAL item's `IconLabelSpace` = 4
/// instead (`:818`) — the two endpoints genuinely differ here.
const WIDE_START_ICON_LABEL_GAP: f32 = 8.0;

/// The expanded (Start) pill's vertical padding, one side = `ItemStartIconIndicatorVerticalPadding`
/// = (HorizontalItem.ActiveIndicatorHeight 56 − IconSize 24) / 2 = 16 (`WideNavigationRail.kt:1411-1413`,
/// read through `IndicatorExpandedPadding` at `:1172-1176`). The Top endpoint uses the vertical
/// item's 4 instead, so the pill grows 32 -> 56 tall across the morph.
///
/// (`:1406-1408` is the TOP item's twin, `(VerticalItem.ActiveIndicatorHeight 32 − 24) / 2 = 4` —
/// this comment cited it until a review checked the range.)
const WIDE_START_INDICATOR_V_PADDING: f32 = 16.0;

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

fn with_alpha_factor(color: Color, factor: f32) -> Color {
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
fn rail_alpha_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}

/// The indicator pill's size and shape morph: `NavigationRail.kt:242-246` animates
/// `sizeAnimationProgress` on `FastSpatial` (0.9 / 1400), whose damping is what makes the pill
/// settle with a slight overshoot. winia used damping 1.0 / stiffness 400.
///
/// Note this is the PLAIN rail's own token. The wide rail item morphs on `DefaultSpatial` instead —
/// see `wide_rail_item_morph_spec`.
fn rail_item_size_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

/// The container's own expand/collapse progress: `WideNavigationRail.kt:265-266` animates
/// `minWidth`/`widthFullRange` on `DefaultSpatial` (0.9 / 700). winia used damping 1.0 /
/// stiffness 400.
fn rail_container_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

/// The MODAL rail's width animation: upstream swaps in `FastSpatial` (0.9 / 1400) for the modal
/// form (`WideNavigationRail.kt:269`: `modalAnimationSpec = MotionSchemeKeyTokens.FastSpatial`),
/// which is what makes a modal rail open faster than an in-flow one.
fn modal_rail_container_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

/// The wide rail item's own morph (icon position and pill size): `NavigationItem.kt:486-491` and
/// `:1232-1237` both use `DefaultSpatial` (0.9 / 700), NOT the plain rail's `FastSpatial`. The wide
/// rail item is the one that travels between two full layouts, so it uses the slower spatial token.
fn wide_rail_item_morph_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

// ── NavigationRail 容器 ──

pub struct NavigationRail {
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    container_color: Option<Color>,
    window_insets: WindowInsets,
    modifier: Modifier,
}

impl NavigationRail {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: Box::new(content),
            header: None,
            container_color: None,
            window_insets: WindowInsets::default(),
            modifier: Modifier::new(),
        }
    }

    /// 容器底色（默认 theme.surface——CollapsedTokens.ContainerColor）
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// header 槽（通常放 FAB 或 logo）——之后自动插入 8dp Spacer
    pub fn header(mut self, h: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.header = Some(Box::new(h));
        self
    }

    /// 窗口避让（桌面默认零值——见 [WindowInsets] 文档）
    pub fn window_insets(mut self, insets: WindowInsets) -> Self {
        self.window_insets = insets;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.container_color);
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface);
        let insets = self.window_insets;
        let header = self.header;
        let content = self.content;
        // Surface(container) → Column(fillMaxHeight, widthIn(min 80),
        //   windowInsetsPadding, padding(vertical 4), spacedBy(4),
        //   CenterHorizontally)
        Column::new()
            .alignment(Alignment::Center)
            .spacing(RAIL_VERTICAL_PADDING)
            .modifier(
                Modifier::new()
                    .fill_max_height()
                    .min_width(NAVIGATION_RAIL_WIDTH)
                    .background(container, Shape::Rectangle)
                    .padding_top(insets.top)
                    .padding_bottom(insets.bottom)
                    .padding_start(insets.left)
                    .padding_end(insets.right)
                    .padding_vertical(RAIL_VERTICAL_PADDING)
                    .then(self.modifier),
            )
            .build(ctx, |ctx| {
                WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                    if let Some(header) = header {
                        header(ctx);
                        crate::layout::Spacer::vertical(HEADER_SPACER).build(ctx);
                    }
                    content(ctx);
                });
            });
    }
}

// ── NavigationRailItem ──

pub struct NavigationRailItem {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    label: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,

    enabled: bool,
    always_show_label: bool,
    colors: Option<NavigationRailItemColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl NavigationRailItem {
    pub fn new(selected: bool, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            selected,
            on_click: None,
            icon: Box::new(icon),
            label: None,
            enabled: true,
            always_show_label: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn on_click(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn label(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.label = Some(Box::new(content));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn always_show_label(mut self, always: bool) -> Self {
        self.always_show_label = always;
        self
    }

    pub fn colors(mut self, colors: NavigationRailItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.selected);
        ctx.changed(&self.enabled);
        ctx.changed(&self.always_show_label);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| rail_item_colors(&theme));
        let selected = self.selected;
        let enabled = self.enabled;
        let has_label = self.label.is_some();

        // 双进度动画（同导航栏）
        let alpha_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_alpha_spec());
        let size_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_item_size_spec());

        let icon_color = colors.icon_color(selected, enabled);
        let label_color = colors.text_color(selected, enabled);
        let ripple_color = icon_color;

        let interaction = self.interaction_source.clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());

        // 彩色胶囊背景：颜色 x alphaProgress——绘制期 peek 不注册依赖
        let indicator_color = colors.indicator;
        let indicator_alpha = alpha_progress.clone();
        let indicator_modifier = Modifier::new().background(
            move || with_alpha_factor(indicator_color, indicator_alpha.peek()),
            Shape::Pill,
        );
        // ripple 状态层载体——独立节点、恒定全尺寸（未选中不收拢）
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, ripple_color, true, Shape::Pill)
        } else {
            Modifier::new()
        };

        // label 包装层：alwaysShowLabel=false 时透明度跟 alphaProgress
        let label_alpha_state = alpha_progress.clone();
        let label_slot_modifier = if has_label && !self.always_show_label {
            Modifier::new().graphics_layer(move || GraphicsLayerParams {
                alpha: label_alpha_state.peek(),
                ..GraphicsLayerParams::default()
            })
        } else {
            Modifier::new()
        };

        let policy = NavigationRailItemLayoutPolicy {
            size_progress: size_progress.clone(),
            always_show_label: self.always_show_label,
            has_label,
        };

        // item：min 宽 80（NarrowContainerWidth）；androidx defaultMinSize(minHeight=ItemHeight)
        // 由 policy 内 min_h 表达（56 基准）
        let mut item_modifier = Modifier::new().min_width(NAVIGATION_RAIL_WIDTH);
        if enabled {
            if let Some(callback) = self.on_click.clone() {
                item_modifier = item_modifier.clickable_with_source(&interaction, move || callback());
            }
        }
        let item_modifier = item_modifier.then(self.modifier);

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 子节点顺序 = [indicator, icon, label?, ripple]——ripple 最后放置
                // （z 序最上层，对齐 androidx placeRelative 顺序）
                // 1) 彩色指示器胶囊（宽度随 sizeProgress 展开动画）
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, indicator_modifier);
                ctx.end_node();
                // 2) 图标（内容色注入）
                let icon = self.icon;
                wrap_slot(ctx, Modifier::new(), |ctx| {
                    WiniaTheme::with_content_color(icon_color, ctx, icon);
                });
                // 3) label（LabelMedium + 文字色）
                if let Some(label) = self.label {
                    wrap_slot(ctx, label_slot_modifier, |ctx| {
                        let mut style = crate::components::navigation_bar::NavigationBarDefaults::label_style();
                        style.color = Some(label_color);
                        crate::components::text::ProvideTextStyle(style, ctx, label);
                    });
                }
                // 4) 指示器 ripple（恒定全尺寸 leaf——悬浮/按压状态层载体）
                let ripple_key = ctx.next_key();
                ctx.start_leaf(ripple_key, ripple_modifier);
                ctx.end_node();
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

/// 单子节点包装（每个逻辑槽恰好贡献一个 arena 子节点——索引稳定）。
fn wrap_slot(ctx: &mut ComposeCtx, modifier: Modifier, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(Alignment::Center)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

/// item 布局：复刻 androidx NavigationRailItemLayout 的
/// placeLabelAndIcon / placeIcon 数学。
///
/// 与导航栏 policy 的差异：
/// - 高基准 56dp（非 80）
/// - 指示器纵向内边距按 has_label 分支（4 vs 16——无标签为圆形胶囊）
/// - 无标签走 placeIcon 全居中路径
///
/// 子节点索引约定：children[0] = 彩色指示器胶囊，
/// children[1] = icon，children[2] = label（仅有 label 时存在），
/// 最后一个子节点 = indicator ripple（恒定全尺寸，z 序最上层）。
#[derive(Debug)]
struct NavigationRailItemLayoutPolicy {
    size_progress: State<f32>,
    always_show_label: bool,
    has_label: bool,
}

impl MeasurePolicy for NavigationRailItemLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 进度最先读（注册 layout_deps 到本节点）
        let size_p = self.size_progress.get().max(0.0);

        let min_h = NAVIGATION_RAIL_ITEM_HEIGHT.max(constraints.min_height);
        let loose = constraints.loosen();

        // icon 先测（loose）
        let (icon_size, _) = measure_node(nodes, policies, children[1], loose);

        let total_indicator_w = icon_size.width + INDICATOR_H_PADDING * 2.0;
        let animated_indicator_w = total_indicator_w * size_p;
        // 纵向内边距按有无 label 分支（androidx IndicatorVerticalPaddingWithLabel/NoLabel）
        let indicator_v_padding = if self.has_label {
            INDICATOR_V_PADDING_WITH_LABEL
        } else {
            INDICATOR_V_PADDING_NO_LABEL
        };
        let indicator_h = icon_size.height + indicator_v_padding * 2.0;
        // ripple：恒定全尺寸（Constraints.fixed(totalW, h)）
        let ripple_idx = children.len() - 1;
        let (ripple_size, _) = measure_node(
            nodes,
            policies,
            children[ripple_idx],
            Constraints::new(total_indicator_w, total_indicator_w, indicator_h, indicator_h),
        );
        // 彩色指示器 tight（animatedW x indicatorH）
        let (indicator_size, _) = measure_node(
            nodes,
            policies,
            children[0],
            Constraints::new(animated_indicator_w, animated_indicator_w, indicator_h, indicator_h),
        );

        let label_size = if self.has_label {
            let (size, _) = measure_node(nodes, policies, children[2], loose);
            Some(size)
        } else {
            None
        };

        // 容器宽：拥抱内容、至少 rail 宽 80（androidx widthIn(min=80)，
        // item 不横向填满——M3 规范视觉胶囊居中于 rail）
        let container_w = if constraints.max_width < f32::MAX {
            let content_w = icon_size.width
                .max(total_indicator_w)
                .max(label_size.as_ref().map(|l| l.width).unwrap_or(0.0));
            content_w.max(NAVIGATION_RAIL_WIDTH).min(constraints.max_width)
        } else {
            icon_size.width.max(total_indicator_w)
        };
        let max_h = if constraints.max_height.is_finite() { constraints.max_height } else { f32::MAX };

        let (height, selected_icon_y, unselected_icon_y, label_y) = if let Some(label) = &label_size {
            // placeLabelAndIcon：
            // contentHeight = iconH + VPadWithLabel + ItemVerticalPadding + labelH
            let content_h = icon_size.height
                + INDICATOR_V_PADDING_WITH_LABEL
                + ITEM_ICON_LABEL_GAP
                + label.height;
            let v_pad = ((min_h - content_h) / 2.0).max(INDICATOR_V_PADDING_WITH_LABEL);
            let height = (content_h + v_pad * 2.0).min(max_h);
            let selected_icon_y = v_pad;
            let unselected_icon_y = if self.always_show_label {
                selected_icon_y
            } else {
                (height - icon_size.height) / 2.0
            };
            let label_y = selected_icon_y
                + icon_size.height
                + INDICATOR_V_PADDING_WITH_LABEL
                + ITEM_ICON_LABEL_GAP;
            (height, selected_icon_y, unselected_icon_y, label_y)
        } else {
            // placeIcon：全部垂直居中于 constrain(ItemHeight=56)
            let height = min_h.min(max_h);
            let center_y = (height - icon_size.height) / 2.0;
            (height, center_y, center_y, 0.0)
        };

        // 位置插值：offset = iconDistance x (1 - progress)
        let offset = (unselected_icon_y - selected_icon_y) * (1.0 - size_p);
        let indicator_y = selected_icon_y - INDICATOR_V_PADDING_WITH_LABEL + offset;

        let mut placements = Vec::with_capacity(children.len());
        placements.push(Placement {
            size: indicator_size,
            position: Offset::new((container_w - indicator_size.width) / 2.0, indicator_y),
        });
        placements.push(Placement {
            size: icon_size,
            position: Offset::new((container_w - icon_size.width) / 2.0, selected_icon_y + offset),
        });
        if let Some(label) = label_size {
            placements.push(Placement {
                size: label,
                position: Offset::new((container_w - label.width) / 2.0, label_y + offset),
            });
        }
        // ripple：恒定全尺寸、跟随指示器动画位置，z 序最上层
        placements.push(Placement {
            size: ripple_size,
            position: Offset::new((container_w - ripple_size.width) / 2.0, indicator_y),
        });

        (Size::new(container_w, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}


#[cfg(test)]
mod tests;

// ── ModalWideNavigationRail（M3 Expressive 模态宽轨）──
//
// 对齐 androidx-main `WideNavigationRail.kt` 的 `ModalWideNavigationRail`：
// - 折叠时是一个普通折叠宽轨，留在布局流内；展开时抬起为一层 modal。上游用
//   `ModalWideNavigationRailDialog` 包住展开态，winia 走自己的 `open_overlay(modal: true)`
// - 展开容器的形状 = ModalContainerShape（CornerLarge 16dp）、颜色 = ModalContainerColor
//   （SurfaceContainer）；折叠态沿用普通轨的形状与容器色
// - 遮罩不自己画：overlay 的 `modal: true` 已经渲染半透明 scrim（110/255，与
//   `ModalNavigationDrawer` 自绘的那层同一个 alpha），点击遮罩与 Escape 都走 overlay 的
//   dismiss 通道，落到 `on_dismiss` 上收起同一份 state
// - 内容因此是 `Fn` 而不是 `FnOnce`：overlay 每帧重新组合它的内容
//   （`alert_dialog` 的注释记着同一件事），折叠那条路径每帧也只调用一次

/// Modal 展开容器的形状 = NavigationRailExpandedTokens.ModalContainerShape（CornerLarge 16dp）。
/// 折叠态沿用普通宽轨的形状（ContainerShape = CornerNone，即 `Shape::Rectangle`）。
pub fn modal_rail_expanded_shape() -> Shape {
    Shape::rounded(16.0)
}

pub struct ModalWideNavigationRail {
    state: WideNavigationRailState,
    content: Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>,
    header: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    hide_on_collapse: bool,
    collapsed_shape: Shape,
    expanded_shape: Shape,
    container_color: Option<Color>,
    modal_container_color: Option<Color>,
    expanded_header_top_padding: f32,
    modifier: Modifier,
}

impl ModalWideNavigationRail {
    pub fn new(
        state: WideNavigationRailState,
        content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state,
            content: Arc::new(content),
            header: None,
            hide_on_collapse: false,
            collapsed_shape: Shape::Rectangle,
            expanded_shape: modal_rail_expanded_shape(),
            container_color: None,
            modal_container_color: None,
            expanded_header_top_padding: 0.0,
            modifier: Modifier::new(),
        }
    }

    /// Slide offscreen when collapsed instead of staying as a collapsed rail — upstream's dismissible
    /// variant (`hideOnCollapse`). The caller owns moving its own container away; this only decides
    /// whether the collapsed rail is drawn at all.
    pub fn hide_on_collapse(mut self, hide: bool) -> Self {
        self.hide_on_collapse = hide;
        self
    }

    /// Collapsed container shape (default `Shape::Rectangle` = `ContainerShape`/`CornerNone`).
    pub fn collapsed_shape(mut self, shape: Shape) -> Self {
        self.collapsed_shape = shape;
        self
    }

    /// Expanded (modal) container shape — defaults to `ModalContainerShape`.
    pub fn expanded_shape(mut self, shape: Shape) -> Self {
        self.expanded_shape = shape;
        self
    }

    /// Collapsed container colour (default `Surface`, as for the plain rail).
    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// Expanded container colour (defaults to `ModalContainerColor` = `surfaceContainer`).
    pub fn modal_container_color(mut self, c: Color) -> Self {
        self.modal_container_color = Some(c);
        self
    }

    /// Padding above the header in the EXPANDED panel — upstream's `expandedHeaderTopPadding`
    /// (`WideNavigationRail.kt:527`), applied as
    /// `Box(modifier = Modifier.padding(top = expandedHeaderTopPadding)) { header() }` at `:666`.
    ///
    /// Upstream's own documentation says what it is for: "It's usually needed in order to align the
    /// content of the rail between the collapsed and expanded animation" (`:508-510`). The two states
    /// do not share a vertical rhythm — a collapsed item is `TopIconItemMinHeight` 64 tall with a 4dp
    /// gap between items, an expanded one is 56 tall with no gap — so their items cannot line up one
    /// for one. What CAN line up is the first item, and this padding is how a caller moves the
    /// expanded panel's content to meet the collapsed rail's.
    ///
    /// Default 0.0, like upstream. Both of upstream's own usages pass 64dp (the height of the app bar
    /// above them): `NavigationRailSamples.kt:206` and `NavigationSuiteScaffoldDemo.kt:164`.
    pub fn expanded_header_top_padding(mut self, padding: f32) -> Self {
        self.expanded_header_top_padding = padding;
        self
    }

    pub fn header(mut self, h: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.header = Some(Arc::new(h));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.hide_on_collapse);
        ctx.changed(&self.collapsed_shape);
        ctx.changed(&self.expanded_shape);
        ctx.changed(&self.container_color);
        ctx.changed(&self.modal_container_color);
        ctx.changed(&self.expanded_header_top_padding);
        let expanded = self.state.is_expanded();
        ctx.changed(&expanded);
        let theme = WiniaTheme::colors();
        let header = self.header;
        let content = self.content;

        // The IN-FLOW rail. Upstream draws it whenever `hideOnCollapse` is false, expanded or not —
        // "Keep this rail even when expanded so that screen layout doesn't change"
        // (`WideNavigationRail.kt:570-591`) — and composes its content only while collapsed. winia
        // used to drop the whole node on expansion, which took a 96dp column out of the caller's
        // layout and pulled every column after it to the left.
        if !self.hide_on_collapse {
            let container = self.container_color.unwrap_or(theme.surface);
            let shape = self.collapsed_shape;
            let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
            let policy = WideNavigationRailLayoutPolicy {
                // Pinned at the collapsed end: this node is the collapsed rail in BOTH states, and
                // only the modal layer animates.
                progress: ctx.animate_float_as_state(0.0, rail_container_spec()),
                has_header: header.is_some(),
                rtl,
                // Pinned collapsed in the modal body; the arrangement is the caller's rail's.
                arrangement: Arrangement::Start,
            };
            let content = content.clone();
            let header = header.clone();
            let key = ctx.next_key();
            let root_modifier = self
                .modifier
                .clone()
                .fill_max_height()
                .background(container, shape);
            match ctx.start_restartable_group(key, root_modifier, policy) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // Content only while collapsed, so that it does not affect this collapsed rail
                    // or the screen layout.
                    if !expanded {
                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                            if let Some(header) = &header {
                                header(ctx);
                            }
                            content(ctx);
                        });
                    }
                }
            }
            ctx.end_restartable_group();
        }

        if !expanded {
            // Collapsed and nothing above the page. With `hide_on_collapse` there is not even the
            // rail above: the caller owns the offscreen position.
            return;
        }

        // Expanded: a layer above the page. `modal: true` is what draws the scrim, and a dismiss request
        // (scrim click or Escape) arrives as `on_dismiss`.
        let modal_color = self.modal_container_color.unwrap_or(theme.surface_container);
        let modal_shape = self.expanded_shape;
        let outer = self.modifier;
        let dismiss_state = self.state.clone();
        // The panel's own progress follows this same state (see the content closure): collapsing flips
        // `is_expanded()` while the overlay is still fading out, and that is what runs the width back down.
        let panel_state = self.state.clone();
        let id: State<u64> = ctx.remember(next_overlay_id);
        // Upstream applies the padding to the header INSIDE the expanded panel only
        // (`WideNavigationRail.kt:666`), never to the collapsed rail, so the collapsed branch above
        // keeps composing its header bare.
        let expanded_header_top_padding = self.expanded_header_top_padding;
        // The expanded panel's header slot exists whenever there is a header OR there is padding to
        // apply. Upstream's call site is `header = { Box(Modifier.padding(top = ...)) { header?.invoke() } }`
        // (`:665-669`) — a NON-null lambda even when the caller passed no header — so its layout always
        // measures a header placeable and `headerOffset = height + WNRHeaderPadding` always carries the
        // padding. Without that, a caller who only wants to shift the items would get nothing.
        let has_header = header.is_some() || expanded_header_top_padding > 0.0;
        // Captured out here for the same reason `direction` is passed to the overlay above: the content
        // closure below runs after composition, where `direction::current()` is no longer the truth.
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            // The rail knows its direction from COMPOSE time, where the composition local is still valid —
            // that is the only way `TopStart` can mirror under RTL: placement-time direction comes from the
            // anchor node, else the root node, and the root sits OUTSIDE the theme's direction provider
            // (settings::shell provides the direction for the window content, not for the window root).
            direction: Some(crate::layout::direction::current()),
            // `TopStart`, not `Center`: a modal rail hangs off the leading edge, and `Center` places the
            // panel by its OWN size in the middle of the window (`overlay.rs`: `Center => ((w - size.0) /
            // 2.0, (h - size.1) / 2.0)`). It is also what lets the container reach full height — the layout
            // policy takes `c.min_height` as its floor, and `fill_max_size` is what sets min = max; a
            // centred, loosely constrained panel never got that.
            position: PopupPosition::TopStart,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            // The modal rail owns the keyboard while it is up: Tab works inside the rail, and the page
            // behind the scrim cannot be reached.
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            on_dismiss: Some(Arc::new(move || dismiss_state.collapse())),
            fit_around_anchor: false,
            match_anchor_width: false,
            // Fade with the scrim. The panel ALSO animates its width and shape out of the collapsed rail
            // (see the content closure below): that is winia's stand-in for upstream's `positionProgress` +
            // `modalState` driving the container out of its collapsed position.
            enter_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            exit_anim: Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(200))),
            local_snapshot: Vec::new(),
            content: Box::new(move |ctx| {
                // The container hugs the leading edge and fills the height; its width is the layout
                // policy's (`min(available, 360)`, the modal cap), and the shape and colour are the modal
                // tokens rather than the plain rail's. Both the modifier and the policy are rebuilt here:
                // this closure is `Fn` (the overlay composes it every frame it is up), so nothing can be
                // moved out of it.
                // Grows out of the collapsed width on the rail's own spring, and shrinks back the same way:
                // the target FOLLOWS the state, so collapsing — which flips `is_expanded()` immediately,
                // while this overlay is still fading out — runs the width back down. NOT
                // `animate_float_as_state`: that seeds its `Animating` with the TARGET
                // (`composer.rs`: `Animating::new(target.clone())`), so its first frame would be at rest.
                let progress: State<f32> = ctx.remember(|| 0.0_f32);
                let target = if panel_state.is_expanded() { 1.0 } else { 0.0 };
                crate::animation::push_animatable(progress.clone(), target, modal_rail_container_spec());
                let panel = outer
                    .clone()
                    .fill_max_size()
                    .background(modal_color, modal_shape);
                let key = ctx.next_key();
                let policy = WideNavigationRailLayoutPolicy {
                    progress,
                    has_header,
                    rtl,
                    // The modal body's in-flow rail is always the collapsed one; the caller's
                    // arrangement belongs to the rail the panel was opened from.
                    arrangement: Arrangement::Start,
                };
                match ctx.start_restartable_group(key, panel, policy) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                            // The header slot carries the padding, so the rail's own
                            // `headerOffset = headerPlaceable.height + WNRHeaderPadding` moves the first
                            // item down by exactly this much — which is the whole point of the
                            // parameter. The slot is emitted even when the caller gave no header, as
                            // upstream's always-present wrapper Box is.
                            if expanded_header_top_padding > 0.0 {
                                wrap_slot(
                                    ctx,
                                    Modifier::new().padding_top(expanded_header_top_padding),
                                    |ctx| {
                                        if let Some(header) = &header {
                                            header(ctx);
                                        }
                                    },
                                );
                            } else if let Some(header) = &header {
                                header(ctx);
                            }
                            content(ctx);
                        });
                    }
                }
                ctx.end_restartable_group();
            }),
        });
    }
}

// ── WideNavigationRail（M3 Expressive 宽轨）──
//
// 对齐 androidx-main WideNavigationRail.kt 的核心行为：
// - Container width Collapsed 96dp ↔ expanded animates: the expanded width =
//   max(natural content width, 220).min(360).min(available width), i.e. **220 is the default and
//   360 is only the ceiling** — only genuinely wider content goes past 220 (androidx treats
//   ExpandedRailMinWidth as the default and ExpandedRailMaxWidth as the ceiling, then lets the
//   constraints settle it; this used to say min(available width, 360), now corrected to upstream)
// - item 图标位置随展开态 Top(收起)↔Start(展开) 变形
// - label 必选且恒显示；item 横向 padding 20dp、容器顶部 padding 44dp
//
// 简化（有意为之，文档注明）：指示器/内容几何在两端点间线性 lerp
// （androidx 用动态 PaddingValues + 分段 label 公式；端点视觉一致，
//  中间过渡为对角滑入而非淡出跳变）。

/// 宽轨收起宽度 = NavigationRailCollapsedTokens.ContainerWidth
pub const WIDE_RAIL_COLLAPSED_WIDTH: f32 = 96.0;
/// 宽轨展开最小宽 = NavigationRailExpandedTokens.ContainerWidthMinimum
pub const WIDE_RAIL_EXPANDED_MIN_WIDTH: f32 = 220.0;
/// 宽轨展开最大宽 = NavigationRailExpandedTokens.ContainerWidthMaximum. Upstream keeps it as
/// `private val ExpandedRailMaxWidth` and applies it as `.widthIn(max = ...)`, so the expanded rail is
/// `min(available width, this)` — not a fixed width. winia used to pin the expansion at the minimum.
pub const WIDE_RAIL_EXPANDED_MAX_WIDTH: f32 = 360.0;
/// 宽轨容器顶部 padding = NavigationRailCollapsedTokens.TopSpace
pub const WIDE_RAIL_TOP_PADDING: f32 = 44.0;

/// 宽轨展开状态机（对标 WideNavigationRailState 的同步简化版）。
///
/// 三态语义（对齐 androidx currentValue/targetValue/isAnimating）：
/// - [`target`](Self::target)：目标状态——expand/collapse/toggle 立即翻转
/// - [`current`](Self::current)：当前视觉状态——变形动画收敛侧（组件挂载
///   进度后可用；读取注册依赖，动画完成帧驱动 UI 响应）
/// - [`is_animating`](Self::is_animating)：变形是否进行中（peek，不注册依赖）
#[derive(Clone)]
pub struct WideNavigationRailState {
    expanded: State<bool>,
    /// 组件 build 挂载的变形进度（0=收起 1=展开）——current/is_animating 的数据源
    progress: std::sync::Arc<parking_lot::Mutex<Option<State<f32>>>>,
}

impl WideNavigationRailState {
    pub fn new(ctx: &mut ComposeCtx) -> Self {
        Self {
            expanded: ctx.remember(|| false),
            progress: std::sync::Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// 目标状态（对齐 targetValue——toggle/expand/collapse 立即翻转）
    pub fn target(&self) -> bool {
        self.expanded.get()
    }

    /// 当前视觉状态（对齐 currentValue）：进度 >0.5 视为展开。
    /// 需组件已挂载进度（build 后可用）；未挂载时退化为 target。
    /// 读取注册依赖——动画完成帧驱动读取方重组。
    pub fn current(&self) -> bool {
        match &*self.progress.lock() {
            Some(p) => p.get() > 0.5,
            None => self.expanded.get(),
        }
    }

    /// 变形动画是否进行中（对齐 isAnimating；peek——不注册依赖）
    pub fn is_animating(&self) -> bool {
        match &*self.progress.lock() {
            Some(p) => {
                let v = p.peek();
                v > 0.01 && v < 0.99
            }
            None => false,
        }
    }

    /// 组件 build 时挂载变形进度（current/is_animating 的数据源）
    pub(crate) fn attach_progress(&self, p: State<f32>) {
        *self.progress.lock() = Some(p);
    }

    /// 内部展开状态（供 Extended FAB 等组件直接绑定）
    pub fn expanded_state(&self) -> State<bool> {
        self.expanded.clone()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn expand(&self) {
        self.expanded.set(true);
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
    }

    pub fn toggle(&self) {
        let e = self.expanded.get();
        self.expanded.set(!e);
    }
}

/// 宽轨容器布局：宽度 = lerp(96, 220, progress)，子项垂直堆叠居中。
/// measure 期读展开进度（注册 layout_deps——动画帧只重测不重组）。
#[derive(Debug)]
struct WideNavigationRailLayoutPolicy {
    progress: State<f32>,
    /// children[0] 是否为 header 槽（FAB/logo）——header 水平位置随展开态迁移
    has_header: bool,
    /// Whether the header's leading indent hangs off the RIGHT edge. Captured by `build` at compose
    /// time: placement runs after composition, where the direction local is gone. Only the header is
    /// mirrored — an item sits at `(width - w) / 2`, which is symmetric and so unchanged by mirroring.
    rtl: bool,
    /// Where the stack sits vertically (`Start` / `Center` / `End`).
    arrangement: Arrangement,
}

impl MeasurePolicy for WideNavigationRailLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        c: Constraints,
    ) -> (Size, Vec<Placement>) {
        let p = self.progress.get().max(0.0).min(1.0);

        // How wide the rail rests when expanded. `ContainerWidthMaximum` (360) is a CEILING, not the
        // target: upstream animates a min end and a max end and then lets the measurement settle it
        // (`WideNavigationRail.kt:270-305`):
        //
        //     var width = actualMinWidth                     // the 96 -> 220 animation
        //     if (expanded) {
        //         val widestElementWidth = maxOf(expandedItemMaxWidth, headerPlaceable?.width ?: 0)
        //         if (widestElementWidth > actualMinWidth && widestElementWidth > actualExpandedMinWidth)
        //             width = widthFullRange.coerceAtMost(widthConstrain)   // the 96 -> 360 animation
        //     }
        //
        // So an expanded rail is 220 unless the CONTENT is wider, and only then does it travel towards
        // 360 (`WideNavigationRail.kt:387-405`). Its own tests pin both ends: `rail_expanded_size`
        // (three items, no wide header) expects `ContainerWidthMinimum`, and `rail_expanded_maxSize`
        // (a 400dp header) expects `ContainerWidthMaximum`.
        //
        // Pinning it at `min(available, 360)` — which this policy used to do — makes the rail as wide
        // as the window on any wide screen, which is neither end of the upstream behaviour.
        //
        // The first pass measures every child with a LOOSE width so each reports what it wants instead
        // of filling the rail: the item policy returns its natural width when `max_width` is unbounded
        // (see `WideNavigationRailItemLayoutPolicy::measure`). The loop below then measures at the
        // settled width, which is the width the children are actually placed in.
        let mut natural_widest = 0.0f32;
        for (index, &child) in children.iter().enumerate() {
            let (size, _) = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(0.0, f32::MAX, 0.0, c.max_height),
            );
            // An ITEM contributes its own width plus `WNRItemHorizontalPadding` on EACH side. The
            // header contributes its measured width unchanged.
            //
            // The two margins are upstream's: the item's own is inside its width
            // (`widthStartIcon = indicatorRipplePlaceable.width + itemHorizontalPadding`,
            // `NavigationItem.kt:1128-1131`) and the rail adds a second around the widest element
            // (`expandedItemMaxWidth = maxItemWidth + WNRItemHorizontalPadding`,
            // `WideNavigationRail.kt:378-381`, used as `maxOf(expandedItemMaxWidth,
            // headerPlaceable?.width ?: 0)` at `:392`). winia's item reports the PILL width, which is
            // upstream's `indicatorRipplePlaceable.width` — the item's own 20dp is not in it — so
            // both margins are added here. A header is measured as it is by both.
            let wanted = if index == 0 && self.has_header {
                size.width
            } else {
                size.width + WIDE_RAIL_ITEM_H_PADDING * 2.0
            };
            natural_widest = natural_widest.max(wanted);
        }
        let collapsed = WIDE_RAIL_COLLAPSED_WIDTH.min(c.max_width);
        let expanded = natural_widest
            .max(WIDE_RAIL_EXPANDED_MIN_WIDTH)
            .min(WIDE_RAIL_EXPANDED_MAX_WIDTH)
            .min(c.max_width);
        let width = collapsed + (expanded - collapsed) * p;

        // 子项垂直堆叠。水平位置：
        // - item：x=0（收起 item 宽=rail 宽；展开 item 填满轨宽——居中恒等）
        // - header (androidx puts it at x=0 and leaves the alignment to the content; the M3
        //   visual is centred when collapsed ↔ leading-edge aligned when expanded):
        //   x = lerp(centred, WIDE_INDICATOR_H_PADDING, p), i.e. expanded the header's leading
        //   inset is 16, so the FAB icon lands at 16+16 = 32; the item's expanded pill, though,
        //   has a leading edge of 20 (`WNRItemHorizontalPadding`) and its icon sits at 36 —
        //   **a 4 dp difference**. This used to say "same x as the item icon", which was the
        //   pre-6d1b560 claim from when the item's leading edge was still 16; this only states
        //   the fact, no number moved.
        // Where the stack starts. `Arrangement::Start` is upstream's `Top`; `Center` and `End` need the
        // content's total height BEFORE the first child is placed, so this measures each child once more
        // — the same arithmetic the loop below uses (settled width, then the header's 40 or the item gap).
        // A rail that hugs its content has no slack, so the arrangement is then a no-op, as upstream's is.
        // The slack is what the ITEMS may spread into, but the header still OCCUPIES its own height plus
        // the header gap (`WNRHeaderPadding`) above them, so that has to come out of the available space
        // first — otherwise `End` pushes the items past the rail's bottom by exactly that much. Upstream
        // keeps the header at the top and arranges the items under it.
        //
        // The unbounded sentinel in this crate is `f32::MAX`, which IS finite (`layout/flex.rs`), and a
        // non-lazy scroll container rewrites a child's max to it (`layout/node.rs`). `is_finite()` would
        // let it through and every placement would land at ~3.4e38 — the same trap that guard was added
        // for. Compare against the sentinel itself.
        let mut header_h = 0.0f32;
        let mut items_h = 0.0f32;
        for (index, &child) in children.iter().enumerate() {
            let (size, _) = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(0.0, width, 0.0, c.max_height),
            );
            if index == 0 && self.has_header {
                header_h = size.height + WIDE_RAIL_HEADER_PADDING;
                continue;
            }
            items_h += size.height + RAIL_VERTICAL_PADDING
                + (WIDE_RAIL_ITEM_GAP_EXPANDED - RAIL_VERTICAL_PADDING) * p;
        }
        let bounded = c.max_height < f32::MAX;
        let slack = if bounded {
            (c.max_height
                - RAIL_VERTICAL_PADDING
                - WIDE_RAIL_TOP_PADDING
                - header_h
                - items_h)
                .max(0.0)
        } else {
            0.0
        };
        let arranged = match self.arrangement {
            Arrangement::Center => slack / 2.0,
            Arrangement::End => slack,
            _ => 0.0,
        };
        let mut y = WIDE_RAIL_TOP_PADDING;
        let first_item = usize::from(self.has_header);
        let mut placements = Vec::with_capacity(children.len());
        for (index, &child) in children.iter().enumerate() {
            // The header keeps the top; the arrangement moves the ITEMS under it.
            if index == first_item {
                y += arranged;
            }
            let (size, _) = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(0.0, width, 0.0, c.max_height),
            );
            let mut x = (width - size.width) / 2.0;
            if index == 0 && self.has_header {
                let leading = WIDE_INDICATOR_H_PADDING;
                x += (leading - x) * p;
                if self.rtl {
                    // `leading` is not "left": it resolves against the direction, so under RTL the
                    // header hangs off the RIGHT edge instead. The item path needs no equivalent —
                    // `(width - size.width) / 2.0` is symmetric, so mirroring leaves it where it was.
                    x = width - x - size.width;
                }
            }
            placements.push(Placement {
                size,
                position: Offset::new(x, y),
            });
            // After the HEADER the next child is offset by `WNRHeaderPadding`, not by the item gap:
            // `WideNavigationRail.kt` computes `headerOffset = headerPlaceable.height + WNRHeaderPadding`,
            // so the header-to-first-item space is 40 where every other gap is the item gap.
            //
            // That gap is animated too: upstream takes `itemVerticalSpacedBy` from
            // `NavigationRailCollapsedTokens.ItemVerticalSpace` (4) to **0** as the rail expands
            // (`WideNavigationRail.kt:279-283`) — an expanded item carries its own spacing in the
            // pill, so the collapsed 4dp between items disappears. winia used 4 at both ends.
            let advance = if index == 0 && self.has_header {
                WIDE_RAIL_HEADER_PADDING
            } else {
                RAIL_VERTICAL_PADDING + (WIDE_RAIL_ITEM_GAP_EXPANDED - RAIL_VERTICAL_PADDING) * p
            };
            y += size.height + advance;
        }
        // 高度：内容拥抱，但尊重约束下限（fill_max_height → min=max——
        // 套件 Row 中须撑满窗口高，否则轨底色只到内容底）
        let height = (y + RAIL_VERTICAL_PADDING)
            .max(c.min_height)
            .min(c.max_height);
        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

// ── WideNavigationRail 组件 ──

pub struct WideNavigationRail {
    state: WideNavigationRailState,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    container_color: Option<Color>,
    /// Container shape. `Shape::Rectangle` unless overridden — the modal form passes the modal container
    /// shape (`NavigationRailExpandedTokens.ModalContainerShape`, CornerLarge) through this.
    container_shape: Shape,
    /// `arrangement`: where the header + items sit vertically in the rail. Upstream's
    /// `Arrangement.Vertical` (`WideNavigationRail.kt`), which only offers Top / Center / Bottom —
    /// in winia's shared enum that is `Start` / `Center` / `End`, and the space-distributing variants
    /// behave like `Start` (the rail's items are a fixed stack, not a set to spread).
    arrangement: Arrangement,
    modifier: Modifier,
}

impl WideNavigationRail {
    pub fn new(
        state: WideNavigationRailState,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state,
            content: Box::new(content),
            header: None,
            container_color: None,
            container_shape: Shape::Rectangle,
            arrangement: Arrangement::Start,
            modifier: Modifier::new(),
        }
    }

    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
        self
    }

    /// Override the container shape (default `Shape::Rectangle` = `ContainerShape`/`CornerNone`).
    pub fn shape(mut self, shape: Shape) -> Self {
        self.container_shape = shape;
        self
    }

    /// `arrangement`: where the header and items sit vertically (default `Arrangement::Start` =
    /// upstream's `Arrangement.Top`).
    pub fn arrangement(mut self, arrangement: Arrangement) -> Self {
        self.arrangement = arrangement;
        self
    }

    pub fn header(mut self, h: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.header = Some(Box::new(h));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.container_color);
        ctx.changed(&self.container_shape);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.state.is_expanded());
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface);
        let shape = self.container_shape;
        let progress = ctx.animate_float_as_state(
            if self.state.is_expanded() { 1.0 } else { 0.0 },
            rail_container_spec(),
        );
        // 挂载进度到状态机——current()/is_animating() 的数据源
        self.state.attach_progress(progress.clone());
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        let policy = WideNavigationRailLayoutPolicy {
            progress,
            has_header: self.header.is_some(),
            rtl,
            arrangement: self.arrangement,
        };
        let header = self.header;
        let content = self.content;
        let key = ctx.next_key();
        let root_modifier = Modifier::new()
            .fill_max_height()
            .background(container, shape);
        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                    if let Some(header) = header {
                        header(ctx);
                    }
                    content(ctx);
                });
            }
        }
        ctx.end_restartable_group();
    }
}

// ── WideNavigationRailItem ──

pub struct WideNavigationRailItem {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    label: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    enabled: bool,
    colors: Option<NavigationRailItemColors>,
    interaction_source: Option<MutableInteractionSource>,
    /// rail 的展开进度（0=收起 Top 布局，1=展开 Start 布局）——由容器传入
    progress: Option<State<f32>>,
    /// 图标位置覆盖（对齐 androidx iconPosition——None = 跟随容器变形进度）
    icon_position: Option<crate::components::navigation_bar::NavigationItemIconPosition>,
    modifier: Modifier,
}

impl WideNavigationRailItem {
    pub fn new(
        selected: bool,
        icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            selected,
            on_click: None,
            icon: Box::new(icon),
            label: Box::new(label),
            enabled: true,
            colors: None,
            interaction_source: None,
            progress: None,
            icon_position: None,
            modifier: Modifier::new(),
        }
    }

    pub fn on_click(mut self, callback: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(callback));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: NavigationRailItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 绑定容器的展开进度（WideNavigationRail 自动传入）
    pub fn progress(mut self, progress: State<f32>) -> Self {
        self.progress = Some(progress);
        self
    }

    /// 图标位置覆盖（对齐 androidx `iconPosition`）：Top 钉死竖排位形、
    /// Start 钉死横排位形——不随容器变形进度插值（默认 None = 跟随进度）。
    pub fn icon_position(
        mut self,
        position: crate::components::navigation_bar::NavigationItemIconPosition,
    ) -> Self {
        self.icon_position = Some(position);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.selected);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| rail_item_colors(&theme));
        let selected = self.selected;
        let enabled = self.enabled;
        // 变形进度：优先绑定容器进度；未绑定时按 selected 自身动画（独立使用）
        let position_progress = match &self.progress {
            Some(p) => p.clone(),
            None => ctx.animate_float_as_state(
                if selected { 1.0 } else { 0.0 },
                wide_rail_item_morph_spec(),
            ),
        };
        // 胶囊展开进度（选中态）——与变形进度相互独立
        let size_progress = ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, wide_rail_item_morph_spec());

        let icon_color = colors.icon_color(selected, enabled);
        let label_color = colors.text_color(selected, enabled);
        let ripple_color = icon_color;

        let interaction = self.interaction_source.clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());

        let indicator_color = colors.indicator;
        let indicator_alpha = size_progress.clone();
        let indicator_modifier = Modifier::new().background(
            move || with_alpha_factor(indicator_color, indicator_alpha.peek()),
            Shape::Pill,
        );
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, ripple_color, true, Shape::Pill)
        } else {
            Modifier::new()
        };

        // Captured at COMPOSE time: placement runs after composition, where the direction local is gone.
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        let policy = WideNavigationRailItemLayoutPolicy {
            position_progress: position_progress.clone(),
            size_progress: size_progress.clone(),
            position_override: self.icon_position.map(|pos| match pos {
                crate::components::navigation_bar::NavigationItemIconPosition::Top => 0.0,
                crate::components::navigation_bar::NavigationItemIconPosition::Start => 1.0,
            }),
            rtl,
        };

        let mut item_modifier = Modifier::new().min_width(NAVIGATION_RAIL_WIDTH);
        if enabled {
            if let Some(callback) = self.on_click.clone() {
                item_modifier = item_modifier.clickable_with_source(&interaction, move || callback());
            }
        }
        let item_modifier = item_modifier.then(self.modifier);

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 子节点顺序 = [indicator, icon, label, ripple]
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, indicator_modifier);
                ctx.end_node();
                let icon = self.icon;
                wrap_slot(ctx, Modifier::new(), |ctx| {
                    WiniaTheme::with_content_color(icon_color, ctx, icon);
                });
                // 宽轨 label 必选且恒显示
                let label = self.label;
                wrap_slot(ctx, Modifier::new(), |ctx| {
                    let mut style = crate::components::navigation_bar::NavigationBarDefaults::label_style();
                    style.color = Some(label_color);
                    crate::components::text::ProvideTextStyle(style, ctx, label);
                });
                let ripple_key = ctx.next_key();
                ctx.start_leaf(ripple_key, ripple_modifier);
                ctx.end_node();
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

/// 宽轨 item 布局：Top（收起）↔ Start（展开）两套几何按进度 lerp。
///
/// 端点数学：
/// - Top(p=0)：同基础 rail item 有标签路径——胶囊 56×32 在图标后方，
///   图标/标签垂直堆叠居中
/// - Start(p=1): the horizontal item — the pill wraps the whole [icon + gap(8) + label] group,
///   height = max(iconH,labelH)+2×16 = 56 (`WIDE_START_INDICATOR_V_PADDING`), pill x = 20
///   (`WNRItemHorizontalPadding`), icon x = 20+16 = 36, label x = 36+24+8 = 68.
///   (this used to say gap 4 / height 40 / inset 16, which are the pre-6d1b560 numbers.)
///
/// 子节点顺序 [indicator, icon, label, ripple]。
#[derive(Debug)]
struct WideNavigationRailItemLayoutPolicy {
    position_progress: State<f32>,
    size_progress: State<f32>,
    /// 图标位置覆盖（Some = 钉死 Top(0.0)/Start(1.0)，不读进度——静态 item
    /// 不因容器动画帧重测；None = 跟随 position_progress）
    position_override: Option<f32>,
    /// RTL: the Start endpoint measures its indent off the LEADING edge (androidx names the token
    /// `FullWidthLeadingSpace`, and a leading value resolves against the direction — 16 is not "left",
    /// it is "leading"). The direction is captured by `build` at compose time because placement runs
    /// after composition, where `direction::current()` is no longer the placement-time truth.
    rtl: bool,
}

impl MeasurePolicy for WideNavigationRailItemLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 进度最先读（layout_deps）。位置覆盖时跳过——钉死位形不因动画帧重测
        let p = self.position_override.unwrap_or_else(|| self.position_progress.get().max(0.0).min(1.0));
        let size_p = self.size_progress.get().max(0.0);

        let loose = constraints.loosen();
        let (icon_size, _) = measure_node(nodes, policies, children[1], loose);
        let (label_size, _) = measure_node(nodes, policies, children[2], loose);

        // ── 端点指标（androidx WideNavigationRail.kt 的 `IndicatorCollapsedPadding` /
        // `IndicatorExpandedPadding`，:1163-1176）──
        // Top（收起）：胶囊 56×32 在图标后——纵向内边距 = VerticalItem.ActiveIndicatorHeight 32
        // 减 IconSize 24 的一半 = 4；图标-标签间距 = VerticalItem.IconLabelSpace = 4。
        let top_total_w = icon_size.width + INDICATOR_H_PADDING * 2.0;
        let top_ind_h = icon_size.height + INDICATOR_V_PADDING_WITH_LABEL * 2.0;
        // Start（展开）：胶囊包裹 [icon + HorizontalItem.IconLabelSpace(8) + label]，
        // 纵向内边距 = HorizontalItem.ActiveIndicatorHeight 56 减 24 的一半 = 16（故胶囊高 56）。
        let start_content_w = icon_size.width + WIDE_START_ICON_LABEL_GAP + label_size.width;
        let start_total_w = start_content_w + WIDE_INDICATOR_H_PADDING * 2.0;
        let start_ind_h = icon_size.height.max(label_size.height) + WIDE_START_INDICATOR_V_PADDING * 2.0;

        let full_w = top_total_w + (start_total_w - top_total_w) * p;
        let full_h = top_ind_h + (start_ind_h - top_ind_h) * p;
        let animated_w = full_w * size_p;

        let ripple_idx = children.len() - 1;
        let (ripple_size, _) = measure_node(
            nodes,
            policies,
            children[ripple_idx],
            Constraints::new(full_w, full_w, full_h, full_h),
        );
        let (indicator_size, _) = measure_node(
            nodes,
            policies,
            children[0],
            Constraints::new(animated_w, animated_w, full_h, full_h),
        );

        // Upstream animates `itemMinHeight` from `TopIconItemMinHeight` (64) down to
        // `minimumA11ySize` (48) as the rail expands (`WideNavigationRail.kt:284-289`), so the
        // two endpoints have DIFFERENT floors. winia used the plain rail's 56 for both.
        let endpoint_min_h =
            WIDE_RAIL_ITEM_MIN_HEIGHT + (WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED - WIDE_RAIL_ITEM_MIN_HEIGHT) * p;
        let min_h = endpoint_min_h.max(constraints.min_height);
        // 收起：拥抱内容、至少 96（CollapsedTokens.ContainerWidth）；
        // 展开：item 填满轨宽（androidx item 为全宽命中目标——若取"轨宽-内边距"
        // 再被容器居中，item x 会随展开漂移，图标绝对坐标跟着动）。
        // 视觉边距由指示器前导缩进承担（见 Start 端点放置——FullWidthLeadingSpace）
        let container_w = if constraints.max_width >= f32::MAX {
            icon_size.width.max(full_w)
        } else {
            let content_w = icon_size.width.max(full_w).max(label_size.width);
            if p > 0.0 {
                constraints.max_width.max(content_w)
            } else {
                content_w.max(WIDE_RAIL_COLLAPSED_WIDTH).min(constraints.max_width)
            }
        };
        let max_h = if constraints.max_height.is_finite() { constraints.max_height } else { f32::MAX };

        // ── Top 端点放置 ──
        let top_content_h = icon_size.height
            + INDICATOR_V_PADDING_WITH_LABEL
            + ITEM_ICON_LABEL_GAP
            + label_size.height;
        let top_v_pad = ((min_h - top_content_h) / 2.0).max(INDICATOR_V_PADDING_WITH_LABEL);
        let top_height = top_content_h + top_v_pad * 2.0;
        let cx = container_w / 2.0;
        let top_icon = Offset::new(cx - icon_size.width / 2.0, top_v_pad);
        let top_label = Offset::new(
            cx - label_size.width / 2.0,
            top_v_pad + icon_size.height + INDICATOR_V_PADDING_WITH_LABEL + ITEM_ICON_LABEL_GAP,
        );
        let top_pill = Offset::new(cx - top_total_w / 2.0, top_v_pad - INDICATOR_V_PADDING_WITH_LABEL);

        // ── Start 端点放置 ──
        // androidx 展开态内容起始对齐（IndicatorExpandedPadding.horizontal =
        // FullWidthLeadingSpace）：胶囊锚定前导缩进、icon/label 固定 x——
        // 不可随容器居中：组含 label，居中会把图标左缘右推（container 越宽
        // 偏移越大 → 展开动画中图标向右漂移，androidx 行为是图标横向不动）
        // The leading indent: 16 to the pill, 32 to the icon inside it. Under RTL the leading edge is the
        // RIGHT one, so all three x are mirrored within the container.
        // The Start endpoint anchors at `WNRItemHorizontalPadding` (20), not at the pill inset:
        // upstream places the ripple and the indicator at `itemHorizontalPadding`
        // (`placeAnimatedLabelAndIcon`, `NavigationItem.kt:1143-1152`) and puts the icon one
        // `FullWidthLeadingSpace` (16) further in, so the icon sits 36 from the leading edge, not 32.
        // winia anchored the pill at 16 and the icon at 32 — both 4 short.
        let start_pill_x = WIDE_RAIL_ITEM_H_PADDING;
        let start_icon_x = if self.rtl {
            container_w - WIDE_RAIL_ITEM_H_PADDING - WIDE_INDICATOR_H_PADDING - icon_size.width
        } else {
            WIDE_RAIL_ITEM_H_PADDING + WIDE_INDICATOR_H_PADDING
        };
        let start_height = min_h.max(start_ind_h);
        let start_icon = Offset::new(start_icon_x, (start_height - icon_size.height) / 2.0);
        let start_label = Offset::new(
            if self.rtl {
                start_icon_x - WIDE_START_ICON_LABEL_GAP - label_size.width
            } else {
                start_icon_x + icon_size.width + WIDE_START_ICON_LABEL_GAP
            },
            (start_height - label_size.height) / 2.0,
        );
        let start_pill = Offset::new(
            if self.rtl {
                container_w - start_pill_x - full_w
            } else {
                start_pill_x
            },
            (start_height - start_ind_h) / 2.0,
        );

        // ── lerp 合成 ──
        let height = top_height + (start_height - top_height) * p;
        let icon_pos = Offset::new(
            top_icon.x + (start_icon.x - top_icon.x) * p,
            top_icon.y + (start_icon.y - top_icon.y) * p,
        );
        let label_pos = Offset::new(
            top_label.x + (start_label.x - top_label.x) * p,
            top_label.y + (start_label.y - top_label.y) * p,
        );
        let pill_pos = Offset::new(
            top_pill.x + (start_pill.x - top_pill.x) * p,
            top_pill.y + (start_pill.y - top_pill.y) * p,
        );

        let mut placements = Vec::with_capacity(children.len());
        placements.push(Placement {
            size: indicator_size,
            position: pill_pos,
        });
        placements.push(Placement {
            size: icon_size,
            position: icon_pos,
        });
        placements.push(Placement {
            size: label_size,
            position: label_pos,
        });
        placements.push(Placement {
            size: ripple_size,
            position: pill_pos,
        });

        (Size::new(container_w, height.min(max_h)), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }

}
