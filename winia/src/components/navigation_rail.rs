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
use crate::layout::{Alignment, BoxLayout};
use crate::layout::components::Column;
use crate::modifier::{Modifier};
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::overlay::{next_overlay_id, OverlayDesc, PopupPosition};
use crate::interaction::MutableInteractionSource;
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

/// 窗口避让尺寸（对标 androidx WindowInsets 的桌面简化版）。
///
/// 桌面平台默认全零（无系统栏叠加）。未来 Android 支持或自定义窗口装饰
/// （自绘标题栏模拟状态栏）时，由平台层填充真实系统栏尺寸后传入
/// [NavigationRail::window_insets]。应用顺序对齐 androidx：
/// insets padding 在最外层，rail 自身垂直 padding 在其内。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl WindowInsets {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
}

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

/// An item's icon/label fade: `NavigationRail.kt:265` uses `DefaultEffects` (damping 1.0 /
/// stiffness 1600). winia used damping 1.0 / stiffness 200.
fn rail_alpha_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}

/// The indicator pill's size and shape morph: `NavigationRail.kt:271` uses `FastSpatial`
/// (0.9 / 1400), whose damping is what makes the pill settle with a slight overshoot. winia used
/// damping 1.0 / stiffness 400.
fn rail_item_size_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

/// The container's own expand/collapse progress: `WideNavigationRail.kt:230` uses `DefaultSpatial`
/// (0.9 / 700) — the rail is not a modal drawer, so the `FastSpatial` it reserves for the modal form
/// does not apply. winia used damping 1.0 / stiffness 400.
fn rail_container_spec() -> crate::animation::AnimationSpec {
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
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;

    /// The three springs this component names, pinned to their androidx tokens: item alpha
    /// `DefaultEffects` (1.0/1600), item size `FastSpatial` (0.9/1400), container expand
    /// `DefaultSpatial` (0.9/700). winia used one no-bouncy spring at 200/400 for all three.
    #[test]
    fn the_rail_springs_are_the_tokens_its_sources_name() {
        match rail_alpha_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (1.0, 1600.0));
            }
            other => panic!("alpha must be a spring, got {other:?}"),
        }
        match rail_item_size_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("item size must be a spring, got {other:?}"),
        }
        match rail_container_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 700.0));
            }
            other => panic!("the container must be a spring, got {other:?}"),
        }
    }

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    fn make_item(selected: bool, always: bool) -> NavigationRailItem {
        NavigationRailItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE))
            .label(|ctx| crate::components::Text::new("Home").build(ctx))
            .always_show_label(always)
            .on_click(|| {})
    }

    fn compose_layout(item: NavigationRailItem) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| item.build(ctx));
        composer.layout(Constraints::new(0.0, f32::MAX, 0.0, f32::MAX));
        composer
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn navigation_rail_tokens_match_androidx_main() {
        assert_eq!(NAVIGATION_RAIL_WIDTH, 80.0, "NarrowContainerWidth");
        assert_eq!(NAVIGATION_RAIL_ITEM_HEIGHT, 56.0, "= ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_RAIL_INDICATOR_WIDTH, 56.0, "ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_RAIL_INDICATOR_HEIGHT, 32.0, "ActiveIndicatorHeight");
        assert_eq!(NAVIGATION_RAIL_ICON_SIZE, 24.0, "BaselineItemTokens.IconSize");
        assert_eq!(INDICATOR_H_PADDING, 16.0);
        assert_eq!(INDICATOR_V_PADDING_WITH_LABEL, 4.0);
        assert_eq!(INDICATOR_V_PADDING_NO_LABEL, 16.0, "无标签→56x56 圆形胶囊");
        assert_eq!(ITEM_ICON_LABEL_GAP, 4.0, "NavigationRailItemVerticalPadding");
        assert_eq!(RAIL_VERTICAL_PADDING, 4.0, "NavigationRailVerticalPadding");
        assert_eq!(HEADER_SPACER, 8.0, "NavigationRailHeaderPadding");
    }

    #[test]
    fn rail_item_colors_follow_v0_11_0_tokens() {
        let theme = crate::theme::ThemeColors::default_light();
        let c = rail_item_colors(&theme);
        assert_eq!(c.selected_icon, theme.on_secondary_container, "ItemActiveIcon");
        assert_eq!(c.selected_label, theme.secondary, "ItemActiveLabelText");
        assert_eq!(c.indicator, theme.secondary_container, "ItemActiveIndicator");
        assert_eq!(c.unselected_icon, theme.on_surface_variant, "ItemInactiveIcon");
        assert_eq!(c.disabled_icon, with_alpha_factor(theme.on_surface_variant, DISABLED_ALPHA));
    }

    #[test]
    fn selected_item_geometry_matches_androidx_place_label_and_icon() {
        // 无界宽度单测：containerW = max(iconW, 胶囊全宽 56) = 56
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple = child(nodes, root, 3);

        // contentH = 24 + 4 + 4 + 16 = 48；vPad = ((56-48)/2).max(4) = 4
        let v_pad = ((NAVIGATION_RAIL_ITEM_HEIGHT - 48.0) / 2.0).max(INDICATOR_V_PADDING_WITH_LABEL);
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
        // 指示器：56x32（progress=1 全宽），y = iconY - 4 = 0
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        assert_eq!(indicator.position.y, v_pad - INDICATOR_V_PADDING_WITH_LABEL);
        // 图标：y = vPad
        assert_eq!(icon.position.y, v_pad);
        assert_eq!(icon.measured_size, Size::new(24.0, 24.0));
        // label：y = iconY + iconH + 4 + 4
        assert_eq!(
            label.position.y,
            v_pad + NAVIGATION_RAIL_ICON_SIZE + INDICATOR_V_PADDING_WITH_LABEL + ITEM_ICON_LABEL_GAP
        );
        // ripple：恒定全尺寸、选中态与胶囊重合、z 最后
        assert_eq!(ripple.measured_size, indicator.measured_size);
        assert_eq!(ripple.position, indicator.position);
    }

    #[test]
    fn unselected_item_without_always_label_centers_icon() {
        let composer = compose_layout(make_item(false, false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let ripple = child(nodes, root, 3);

        // progress=0：offset = ((56-24)/2 - 4)x1 = 12 → 图标居中 (56-24)/2 = 16
        assert_eq!(icon.position.y, (NAVIGATION_RAIL_ITEM_HEIGHT - NAVIGATION_RAIL_ICON_SIZE) / 2.0);
        assert_eq!(indicator.position.y, icon.position.y - INDICATOR_V_PADDING_WITH_LABEL);
        // 彩色胶囊收拢
        assert_eq!(indicator.measured_size.width, 0.0);
        // ripple 恒定全尺寸且跟随动画位置
        assert_eq!(ripple.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(ripple.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        assert_eq!(
            ripple.position.x + ripple.measured_size.width / 2.0,
            indicator.position.x + indicator.measured_size.width / 2.0,
            "ripple 与胶囊中心对齐");
        assert_eq!(ripple.position.y, indicator.position.y);
    }

    #[test]
    fn no_label_item_gets_circular_full_size_pill() {
        // 无标签：vPadNoLabel=16 → 指示器 56x56（CornerFull 即圆形）
        let composer = compose_layout(
            NavigationRailItem::new(true, |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE))
                .on_click(|| {}),
        );
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        // placeIcon：全部居中；胶囊 56x56
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
    }

    #[test]
    fn bounded_item_hugs_content_instead_of_filling_width() {
        // androidx：Box(widthIn(min=80)) 内容居中——item 不横向填满约束
        let mut composer = Composer::new();
        composer.compose(|ctx| make_item(true, true).build(ctx));
        composer.layout(Constraints::new(0.0, 220.0, 0.0, NAVIGATION_RAIL_ITEM_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, NAVIGATION_RAIL_WIDTH);
    }

    #[test]
    fn disabled_item_has_no_interaction_elements() {
        let composer = compose_layout(make_item(true, true).enabled(false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert!(nodes[root].modifier.clickable_interaction().is_none());
        let ripple_node = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        assert!(ripple_node.modifier.ripple_interaction().is_none(), "禁用项无状态层载体");
        assert!(indicator.modifier.ripple_interaction().is_none(), "禁用项指示器无 ripple");
    }

    #[test]
    fn unselected_item_hover_shows_state_layer_on_full_pill_rect() {
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = crate::theme::ThemeColors::default_light();
        let container = theme.surface;
        let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

        let source = MutableInteractionSource::new();
        let src_for_item = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationRail::new(move |ctx| {
                NavigationRailItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("B").build(ctx))
                    .interaction_source(src_for_item.clone())
                    .on_click(|| {})
                    .build(ctx);
            })
            .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 400.0));

        source.emit_hover_enter();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
        while std::time::Instant::now() < deadline {
            crate::animation::update_animations();
            if source.hover_opacity_value() >= 0.079 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        let root = composer.layout_root_idx().unwrap();
        let mut surface = skia_safe::surfaces::raster_n32_premul((120, 400)).expect("rail surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // 布局：rail 宽 120、item 宽 80 居中（起点 x=20）、顶部 padding 4。
        // 胶囊全局矩形 = x 32..88（item_x 20 + ripple_x 12），y 4..36；
        // 取 (40, 12)（胶囊内、避开图标 48..72）
        let px = pixel_at(&mut surface, 40, 12);
        assert_ne!(px, to_rgba(container), "悬浮未选中项应显示状态层");
    }

    fn pixel_at(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8, u8) {
        let mut pixels = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut pixels, 4, (x, y));
        (pixels[0], pixels[1], pixels[2], pixels[3])
    }
    // ── WideNavigationRail ──

    #[test]
    fn wide_rail_width_endpoints_match_tokens() {
        // 收起：容器宽 96（CollapsedTokens.ContainerWidth）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            assert!(!state.is_expanded(), "默认收起");
            WideNavigationRail::new(state, |ctx| {
                NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("A").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
            })
            .header(|ctx| icon_leaf(ctx, 24.0))
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, WIDE_RAIL_COLLAPSED_WIDTH);

        // Expanded: the width is `min(available, ContainerWidthMaximum)`, never below the minimum —
        // upstream animates both the min and the max end and lets the constraints settle it. So 400dp of
        // room gives the MAX, and a 200dp window gives the 200dp it has (the outer `.min(c.max_width)`).
        let build_expanded = |max_w: f32| {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let state = WideNavigationRailState::new(ctx);
                state.expand();
                WideNavigationRail::new(state, |ctx| {
                    NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                        .label(|ctx| crate::components::Text::new("A").build(ctx))
                        .on_click(|| {})
                        .build(ctx);
                })
                .header(|ctx| icon_leaf(ctx, 24.0))
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, max_w, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.width
        };
        assert_eq!(build_expanded(400.0), WIDE_RAIL_EXPANDED_MAX_WIDTH);
        assert_eq!(build_expanded(200.0), 200.0);
    }

    #[test]
    fn wide_rail_item_morphs_between_top_and_start_layouts() {
        // p=0（收起）：Top 布局——胶囊 56x32 在图标后、图标标签垂直堆叠
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(0.0))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        // 图标在标签上方（垂直堆叠）
        assert!(icon.position.y < label.position.y);
        // 中心点对齐（居中排列——宽度不同 x 不同）
        assert_eq!(icon.position.x + icon.measured_size.width / 2.0, label.position.x + label.measured_size.width / 2.0);

        // p=1（展开）：Start 布局——胶囊包裹 [icon+gap+label]，高 40，水平排列
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(1.0))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let ripple = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        // 水平排列：label 在 icon 右侧、同垂直中心
        assert!(icon.position.x < label.position.x, "Start 布局图标在左");
        // 展开态起始对齐（androidx FullWidthLeadingSpace）：icon/label/胶囊 x
        // 为固定值，不随容器加宽漂移（回归：居中实现会使图标 x 随展开 +20）
        assert_eq!(indicator.position.x, WIDE_INDICATOR_H_PADDING);
        assert_eq!(icon.position.x, WIDE_INDICATOR_H_PADDING * 2.0);
        assert_eq!(label.position.x, WIDE_INDICATOR_H_PADDING * 2.0 + NAVIGATION_RAIL_ICON_SIZE + ITEM_ICON_LABEL_GAP);
        assert_eq!(
            icon.position.y + icon.measured_size.height / 2.0,
            label.position.y + label.measured_size.height / 2.0,
        );
        // 胶囊高 40（max(24,labelH)+16）、宽包住整组
        assert_eq!(ripple.measured_size.height, 40.0);
        let content_w = 24.0 + ITEM_ICON_LABEL_GAP + label.measured_size.width;
        assert_eq!(
            ripple.measured_size.width,
            content_w + WIDE_INDICATOR_H_PADDING * 2.0,
        );
        // 选中态彩色胶囊与 ripple 重合
        assert_eq!(indicator.position, ripple.position);
    }

    #[test]
    fn wide_rail_item_target_width_expands_when_expanded() {
        // 收起：拥抱内容 min 96；展开：填满轨宽（androidx item 为全宽命中目标）
        let cases: [(bool, f32); 2] = [(false, WIDE_RAIL_COLLAPSED_WIDTH), (true, 220.0)];
        for (expanded, expected_w) in cases {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let progress = ctx.animate_float_as_state(
                    if expanded { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
                        damping_ratio: 1.0, stiffness: 400.0, mass: 1.0, threshold: 0.01,
                    }),
                );
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("Home").build(ctx),
                )
                .progress(progress)
                .on_click(|| {})
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            assert_eq!(
                nodes[root].measured_size.width,
                expected_w,
                "expanded={}",
                expanded
            );
        }
    }

    /// icon_position 覆盖：progress=0（收起）但钉死 Start——布局应为横排位形
    /// （图标左 label 右、胶囊高 40），不随容器变形进度插值
    #[test]
    fn wide_rail_item_icon_position_override_pins_start_layout() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(0.0))
            .icon_position(crate::components::navigation_bar::NavigationItemIconPosition::Start)
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple = child(nodes, root, 3);
        assert!(icon.position.x < label.position.x, "钉死 Start：图标在 label 左");
        assert_eq!(icon.position.y + 12.0, label.position.y + label.measured_size.height / 2.0,
            "同垂直中心");
        assert_eq!(ripple.measured_size.height, 40.0, "Start 指示器高 40");
    }

    /// 三态语义：target 立即翻转 / current 跟随进度收敛侧 / is_animating 中段为真
    #[test]
    fn wide_rail_state_target_current_is_animating() {
        let mut composer = Composer::new();
        let mut state_ref = None;
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state_ref = Some(state);
        });
        let state = state_ref.unwrap();
        assert!(!state.target() && !state.current() && !state.is_animating(), "初始全收起");

        state.expand();
        assert!(state.target(), "target 立即翻转");
        assert!(state.current(), "进度未挂载时 current 退化为 target");
        assert!(!state.is_animating());

        // 模拟组件挂载进度 + 动画中间帧
        let progress = State::new(0.5);
        state.attach_progress(progress.clone());
        assert!(state.is_animating(), "进度 0.5 → 动画中");
        assert!(!state.current(), "进度 0.5 → 视觉仍收起");

        progress.set(1.0);
        assert!(state.current(), "进度 1.0 → 视觉展开");
        assert!(!state.is_animating(), "进度收敛 → 动画结束");

        state.collapse();
        assert!(!state.target() && state.current(), "target 翻转、current 尚未跟随");
    }

    #[test]
    fn wide_rail_state_toggles_expansion() {
        let mut composer = Composer::new();
        let mut state_ref = None;
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state.toggle();
            state_ref = Some(state);
        });
        let state = state_ref.unwrap();
        assert!(state.is_expanded(), "toggle 后应展开");
        state.collapse();
        assert!(!state.is_expanded());
        state.expand();
        assert!(state.is_expanded());
    }

    /// The modal form: collapsed it is an ordinary rail and registers no overlay; expanded it raises a
    /// modal overlay, and that `modal` flag is what draws the scrim (upstream opens a
    /// `ModalWideNavigationRailDialog` for the same reason).
    #[test]
    fn modal_rail_is_a_plain_rail_until_it_expands() {
        // Collapsed: no overlay, and the rail is on screen at the collapsed width.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert!(
            composer.take_overlays().is_empty(),
            "a collapsed modal rail opens no overlay"
        );
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(
            composer.arena_nodes()[root].measured_size.width,
            WIDE_RAIL_COLLAPSED_WIDTH,
            "and what is on screen is the collapsed rail"
        );

        // Expanded: exactly one overlay, and it is modal — that flag is the scrim.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state.expand();
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "an expanded modal rail opens one overlay");
        assert!(
            overlays[0].modal,
            "and it is modal: that flag is what draws the scrim"
        );
    }

    /// `hide_on_collapse` (upstream's dismissible variant): collapsed, the rail is not drawn at all — the
    /// caller owns its offscreen position — but expanding still raises the modal layer.
    #[test]
    fn hide_on_collapse_draws_nothing_until_expanded() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .hide_on_collapse(true)
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let widths: Vec<f32> = composer
            .arena_nodes()
            .iter()
            .map(|n| n.measured_size.width)
            .collect();
        assert!(
            !widths.contains(&WIDE_RAIL_COLLAPSED_WIDTH),
            "collapsed with hide_on_collapse, no rail is drawn: {widths:?}"
        );
        assert!(composer.take_overlays().is_empty(), "and no overlay either");
    }
}

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
        let expanded = self.state.is_expanded();
        ctx.changed(&expanded);
        let theme = WiniaTheme::colors();
        let header = self.header;
        let content = self.content;

        // Collapsed: the plain rail, unexpanded. Upstream draws this outside the dialog, next to a
        // same-sized spacer that keeps the page from shifting when the modal opens; winia's rail already
        // sits in the layout, so there is nothing to compensate for.
        if !expanded {
            if self.hide_on_collapse {
                // Nothing on screen — the caller owns the offscreen position.
                return;
            }
            let container = self.container_color.unwrap_or(theme.surface);
            let shape = self.collapsed_shape;
            let policy = WideNavigationRailLayoutPolicy {
                progress: ctx.animate_float_as_state(0.0, rail_container_spec()),
                has_header: header.is_some(),
            };
            let content = content.clone();
            let header = header.clone();
            let key = ctx.next_key();
            let root_modifier = self
                .modifier
                .fill_max_height()
                .background(container, shape);
            match ctx.start_restartable_group(key, root_modifier, policy) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                        if let Some(header) = &header {
                            header(ctx);
                        }
                        content(ctx);
                    });
                }
            }
            ctx.end_restartable_group();
            return;
        }

        // Expanded: a layer above the page. `modal: true` is what draws the scrim, and a dismiss request
        // (scrim click or Escape) arrives as `on_dismiss`.
        let modal_color = self.modal_container_color.unwrap_or(theme.surface_container);
        let modal_shape = self.expanded_shape;
        let outer = self.modifier;
        let dismiss_state = self.state.clone();
        let id: State<u64> = ctx.remember(next_overlay_id);
        // Captured by the `Fn` content closure below, which may run every frame: both are rebuilt inside it
        // rather than moved, because an `Fn` closure cannot give them up.
        let panel_progress: State<f32> = ctx.remember(|| 1.0_f32);
        let has_header = header.is_some();
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            position: PopupPosition::Center,
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
            enter_anim: None,
            exit_anim: None,
            local_snapshot: Vec::new(),
            content: Box::new(move |ctx| {
                // The container hugs the leading edge and fills the height; its width is the layout
                // policy's (`min(available, 360)`, the modal cap), and the shape and colour are the modal
                // tokens rather than the plain rail's. Both the modifier and the policy are rebuilt here:
                // this closure is `Fn` (the overlay composes it every frame it is up), so nothing can be
                // moved out of it.
                let panel = outer
                    .clone()
                    .fill_max_size()
                    .background(modal_color, modal_shape);
                let key = ctx.next_key();
                let policy = WideNavigationRailLayoutPolicy {
                    progress: panel_progress.clone(),
                    has_header,
                };
                match ctx.start_restartable_group(key, panel, policy) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                            if let Some(header) = &header {
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
// - 容器宽 Collapsed 96dp ↔ Expanded 220..360dp 动画过渡：展开后的宽度取
//   min(可用宽, 360) 且不低于 220（androidx 把 ExpandedRailMinWidth 与
//   ExpandedRailMaxWidth 作为两个动画值，再由约束落定）
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
        // Upstream animates BOTH ends (Collapsed -> ExpandedRailMinWidth and Collapsed ->
        // ExpandedRailMaxWidth) and then lets the constraints settle the width, so an expanded rail is
        // `min(available, max)` and never below the minimum. Pinning it at the minimum left the rail
        // narrower than M3 on any window wider than 220dp.
        let expanded = c
            .max_width
            .min(WIDE_RAIL_EXPANDED_MAX_WIDTH)
            .max(WIDE_RAIL_EXPANDED_MIN_WIDTH);
        let width = (WIDE_RAIL_COLLAPSED_WIDTH + (expanded - WIDE_RAIL_COLLAPSED_WIDTH) * p)
            .min(c.max_width);

        // 子项垂直堆叠。水平位置：
        // - item：x=0（收起 item 宽=rail 宽；展开 item 填满轨宽——居中恒等）
        // - header（androidx 放 x=0、对齐由内容自理；M3 视觉为收起居中 ↔
        //   展开与 item 胶囊前导对齐）：x = lerp(居中, 16, p)——FAB 图标展开后
        //   与 item 图标（前导缩进 16 + 内边距 16 = 32）同 x，不随变宽漂移
        let mut y = WIDE_RAIL_TOP_PADDING;
        let mut placements = Vec::with_capacity(children.len());
        for (index, &child) in children.iter().enumerate() {
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
            }
            placements.push(Placement {
                size,
                position: Offset::new(x, y),
            });
            y += size.height + RAIL_VERTICAL_PADDING;
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
        let policy = WideNavigationRailLayoutPolicy { progress, has_header: self.header.is_some() };
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
                rail_item_size_spec(),
            ),
        };
        // 胶囊展开进度（选中态）——与变形进度相互独立
        let size_progress = ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_item_size_spec());

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

        let policy = WideNavigationRailItemLayoutPolicy {
            position_progress: position_progress.clone(),
            size_progress: size_progress.clone(),
            position_override: self.icon_position.map(|pos| match pos {
                crate::components::navigation_bar::NavigationItemIconPosition::Top => 0.0,
                crate::components::navigation_bar::NavigationItemIconPosition::Start => 1.0,
            }),
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
/// - Start(p=1)：横向 item——胶囊包裹 [icon + gap(4) + label] 整组，
///   高 = max(iconH,labelH)+2×8 = 40；内容行起始对齐（前导缩进 16）
///
/// 子节点顺序 [indicator, icon, label, ripple]。
#[derive(Debug)]
struct WideNavigationRailItemLayoutPolicy {
    position_progress: State<f32>,
    size_progress: State<f32>,
    /// 图标位置覆盖（Some = 钉死 Top(0.0)/Start(1.0)，不读进度——静态 item
    /// 不因容器动画帧重测；None = 跟随 position_progress）
    position_override: Option<f32>,
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

        // ── 端点指标 ──
        // Top：胶囊 56×32 在图标后
        let top_total_w = icon_size.width + INDICATOR_H_PADDING * 2.0;
        let top_ind_h = icon_size.height + INDICATOR_V_PADDING_WITH_LABEL * 2.0;
        // Start：胶囊包裹 [icon+gap+label]，高 40
        let start_content_w = icon_size.width + ITEM_ICON_LABEL_GAP + label_size.width;
        let start_total_w = start_content_w + WIDE_INDICATOR_H_PADDING * 2.0;
        let start_ind_h = icon_size.height.max(label_size.height) + 8.0 * 2.0;

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

        let min_h = NAVIGATION_RAIL_ITEM_HEIGHT.max(constraints.min_height);
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
        let start_icon_x = WIDE_INDICATOR_H_PADDING * 2.0; // 胶囊(16) + 胶囊内边距(16)
        let start_height = min_h.max(start_ind_h);
        let start_icon = Offset::new(start_icon_x, (start_height - icon_size.height) / 2.0);
        let start_label = Offset::new(
            start_icon_x + icon_size.width + ITEM_ICON_LABEL_GAP,
            (start_height - label_size.height) / 2.0,
        );
        let start_pill = Offset::new(WIDE_INDICATOR_H_PADDING, (start_height - start_ind_h) / 2.0);

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
