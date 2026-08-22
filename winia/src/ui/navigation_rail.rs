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
//! - ModalWideNavigationRail 与 Expanded 宽轨已实现（见本文件后半部分）；
//!   组件文档：docs/navigation-rail.md

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::layout::{Alignment, BoxLayout};
use crate::ui::icon_button::IconButton;
use crate::ui::icon::Icon;
use crate::ui::layout_components::{Column, Row};
use crate::modifier::{Color, GraphicsLayerParams, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::WiniaTheme;
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
const DISABLED_ALPHA: f32 = 0.38;
/// 宽轨 Start 态指示器横向内边距（leading/trailing 各 16，与导航栏水平 item 一致）
const WIDE_INDICATOR_H_PADDING: f32 = 16.0;
/// 模态面板圆角半径（androidx CornerLarge 简化为全角）
const WIDE_PANEL_CORNER_RADIUS: f32 = 16.0;

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
pub type NavigationRailItemColors = crate::ui::navigation_bar::NavigationBarItemColors;

/// NavigationRailItemDefaults.colors() 对齐：颜色 token 映射与导航栏完全一致
/// （ItemActiveIcon=OnSecondaryContainer / ItemActiveLabelText=Secondary /
/// ItemActiveIndicator=SecondaryContainer / Inactive=OnSurfaceVariant /
/// Disabled=@38%）。
pub fn rail_item_colors(theme: &crate::ui::theme::ThemeColors) -> NavigationRailItemColors {
    crate::ui::navigation_bar::NavigationBarItemColors::from_theme(theme)
}

fn with_alpha_factor(color: Color, factor: f32) -> Color {
    Color::from_argb(
        ((color.a as f32 * factor).round().min(255.0)) as u8,
        color.r,
        color.g,
        color.b,
    )
}

// ── 动画规格（同导航栏：DefaultEffects/FastSpatial 近似）──

const ALPHA_SPRING_STIFFNESS: f32 = 200.0;
const SIZE_SPRING_STIFFNESS: f32 = 400.0;

fn rail_spring(stiffness: f32) -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
        damping_ratio: 1.0,
        stiffness,
        mass: 1.0,
        threshold: 0.01,
    })
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
                        crate::ui::Spacer::vertical(HEADER_SPACER).build(ctx);
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
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_spring(ALPHA_SPRING_STIFFNESS));
        let size_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_spring(SIZE_SPRING_STIFFNESS));

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
                        let mut style = crate::ui::navigation_bar::NavigationBarDefaults::label_style();
                        style.color = Some(label_color);
                        crate::ui::text::ProvideTextStyle(style, ctx, label);
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
            position: Point::new((container_w - indicator_size.width) / 2.0, indicator_y),
        });
        placements.push(Placement {
            size: icon_size,
            position: Point::new((container_w - icon_size.width) / 2.0, selected_icon_y + offset),
        });
        if let Some(label) = label_size {
            placements.push(Placement {
                size: label,
                position: Point::new((container_w - label.width) / 2.0, label_y + offset),
            });
        }
        // ripple：恒定全尺寸、跟随指示器动画位置，z 序最上层
        placements.push(Placement {
            size: ripple_size,
            position: Point::new((container_w - ripple_size.width) / 2.0, indicator_y),
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
    use crate::core::composer::Composer;

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    fn make_item(selected: bool, always: bool) -> NavigationRailItem {
        NavigationRailItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE))
            .label(|ctx| crate::ui::Text::new("Home").build(ctx))
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
        let theme = crate::ui::theme::ThemeColors::default_light();
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
        let theme = crate::ui::theme::ThemeColors::default_light();
        let container = theme.surface;
        let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

        let source = MutableInteractionSource::new();
        let src_for_item = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationRail::new(move |ctx| {
                NavigationRailItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::ui::Text::new("B").build(ctx))
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
                    .label(|ctx| crate::ui::Text::new("A").build(ctx))
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

        // 展开：容器宽 220（ExpandedTokens.ContainerWidthMinimum）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state.expand();
            WideNavigationRail::new(state, |ctx| {
                NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::ui::Text::new("A").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
            })
            .header(|ctx| icon_leaf(ctx, 24.0))
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, WIDE_RAIL_EXPANDED_MIN_WIDTH);
    }

    #[test]
    fn wide_rail_item_morphs_between_top_and_start_layouts() {
        // p=0（收起）：Top 布局——胶囊 56x32 在图标后、图标标签垂直堆叠
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::ui::Text::new("Home").build(ctx),
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
                |ctx| crate::ui::Text::new("Home").build(ctx),
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


    #[test]
    fn modal_rail_state_open_close_toggles() {
        let mut composer = Composer::new();
        let mut state_ref = None;
        composer.compose(|ctx| {
            let state = ModalWideNavigationRailState::new(ctx);
            state_ref = Some(state);
        });
        let state = state_ref.unwrap();
        assert!(!state.is_open(), "默认关闭");
        state.open();
        assert!(state.is_open());
        state.close();
        assert!(!state.is_open());
        state.toggle();
        assert!(state.is_open());
    }

    #[test]
    fn modal_rail_build_smoke_both_visibility_states() {
        // 冒烟：visible 开/关两态组合均不 panic（overlay 注册走运行时管线，
        // 单元层只验证组合期行为）
        for open in [false, true] {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let state = ModalWideNavigationRailState::new(ctx);
                if open {
                    state.open();
                }
                ModalWideNavigationRail::new(state, |ctx| {
                    NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                        .label(|ctx| crate::ui::Text::new("A").build(ctx))
                        .on_click(|| {})
                        .build(ctx);
                })
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        }
    }
}

// ── WideNavigationRail（M3 Expressive 宽轨）──
//
// 对齐 androidx-main WideNavigationRail.kt 的核心行为：
// - 容器宽 Collapsed 96dp ↔ Expanded min 220dp 动画过渡
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
/// 宽轨容器顶部 padding = NavigationRailCollapsedTokens.TopSpace
pub const WIDE_RAIL_TOP_PADDING: f32 = 44.0;
/// 宽轨 item 横向 padding
pub const WIDE_RAIL_ITEM_H_PADDING: f32 = 20.0;

/// 宽轨展开状态机（对标 WideNavigationRailState 的同步简化版）。
/// 持有展开目标 bool；动画由组件内部 animate_float_as_state 驱动。
#[derive(Clone)]
pub struct WideNavigationRailState {
    expanded: State<bool>,
}

impl WideNavigationRailState {
    pub fn new(ctx: &mut ComposeCtx) -> Self {
        Self { expanded: ctx.remember(|| false) }
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
        let width = (WIDE_RAIL_COLLAPSED_WIDTH
            + (WIDE_RAIL_EXPANDED_MIN_WIDTH - WIDE_RAIL_COLLAPSED_WIDTH) * p)
            .min(c.max_width);

        // 子项垂直堆叠：每项宽 = rail 宽，高松约束；水平居中由 item min 宽保证
        let mut y = WIDE_RAIL_TOP_PADDING;
        let mut placements = Vec::with_capacity(children.len());
        for &child in children {
            let (size, _) = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(0.0, width, 0.0, c.max_height),
            );
            placements.push(Placement {
                size,
                position: Point::new((width - size.width) / 2.0, y),
            });
            y += size.height + RAIL_VERTICAL_PADDING;
        }
        let height = (y + RAIL_VERTICAL_PADDING).min(c.max_height);
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
            modifier: Modifier::new(),
        }
    }

    pub fn container_color(mut self, c: Color) -> Self {
        self.container_color = Some(c);
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
        ctx.changed(&self.state.is_expanded());
        let theme = WiniaTheme::colors();
        let container = self.container_color.unwrap_or(theme.surface);
        let progress = ctx.animate_float_as_state(
            if self.state.is_expanded() { 1.0 } else { 0.0 },
            rail_spring(SIZE_SPRING_STIFFNESS),
        );
        let policy = WideNavigationRailLayoutPolicy { progress };
        let header = self.header;
        let content = self.content;
        let key = ctx.next_key();
        let root_modifier = Modifier::new()
            .fill_max_height()
            .background(container, Shape::Rectangle);
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
                rail_spring(SIZE_SPRING_STIFFNESS),
            ),
        };
        // 胶囊展开进度（选中态）——与变形进度相互独立
        let size_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, rail_spring(SIZE_SPRING_STIFFNESS));

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
                    let mut style = crate::ui::navigation_bar::NavigationBarDefaults::label_style();
                    style.color = Some(label_color);
                    crate::ui::text::ProvideTextStyle(style, ctx, label);
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
///   高 = max(iconH,labelH)+2×8 = 40；内容行整体居中
///
/// 子节点顺序 [indicator, icon, label, ripple]。
#[derive(Debug)]
struct WideNavigationRailItemLayoutPolicy {
    position_progress: State<f32>,
    size_progress: State<f32>,
}

impl MeasurePolicy for WideNavigationRailItemLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 进度最先读（layout_deps）
        let p = self.position_progress.get().max(0.0).min(1.0);
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
        // 拥抱内容、至少收起轨宽 96（CollapsedTokens.ContainerWidth）
        let container_w = if constraints.max_width < f32::MAX {
            let content_w = icon_size.width
                .max(full_w)
                .max(label_size.width);
            content_w.max(WIDE_RAIL_COLLAPSED_WIDTH).min(constraints.max_width)
        } else {
            icon_size.width.max(full_w)
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
        let top_icon = Point::new(cx - icon_size.width / 2.0, top_v_pad);
        let top_label = Point::new(
            cx - label_size.width / 2.0,
            top_v_pad + icon_size.height + INDICATOR_V_PADDING_WITH_LABEL + ITEM_ICON_LABEL_GAP,
        );
        let top_pill = Point::new(cx - top_total_w / 2.0, top_v_pad - INDICATOR_V_PADDING_WITH_LABEL);

        // ── Start 端点放置 ──
        let start_height = min_h.max(start_ind_h);
        let row_x = (container_w - start_content_w) / 2.0;
        let start_icon = Point::new(row_x, (start_height - icon_size.height) / 2.0);
        let start_label = Point::new(
            row_x + icon_size.width + ITEM_ICON_LABEL_GAP,
            (start_height - label_size.height) / 2.0,
        );
        let start_pill = Point::new(
            (container_w - start_total_w) / 2.0,
            (start_height - start_ind_h) / 2.0,
        );

        // ── lerp 合成 ──
        let height = top_height + (start_height - top_height) * p;
        let icon_pos = Point::new(
            top_icon.x + (start_icon.x - top_icon.x) * p,
            top_icon.y + (start_icon.y - top_icon.y) * p,
        );
        let label_pos = Point::new(
            top_label.x + (start_label.x - top_label.x) * p,
            top_label.y + (start_label.y - top_label.y) * p,
        );
        let pill_pos = Point::new(
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
// ── ModalWideNavigationRail（模态宽轨）──
//
// 基于 ui::Dialog 的模态呈现：scrim 遮罩点击关闭、面板 SurfaceContainer 底色。
// 简化注明：容器形状全角圆角（androidx 为 end-side CornerLarge）。

/// 模态宽轨开合状态机
#[derive(Clone)]
pub struct ModalWideNavigationRailState {
    open: State<bool>,
}

impl ModalWideNavigationRailState {
    pub fn new(ctx: &mut ComposeCtx) -> Self {
        Self { open: ctx.remember(|| false) }
    }

    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    pub fn open(&self) {
        self.open.set(true);
    }

    pub fn close(&self) {
        self.open.set(false);
    }

    pub fn toggle(&self) {
        let o = self.open.get();
        self.open.set(!o);
    }
}

pub struct ModalWideNavigationRail {
    state: ModalWideNavigationRailState,
    content: Box<dyn Fn(&mut ComposeCtx) + Send + Sync>,
    modifier: Modifier,
}

impl ModalWideNavigationRail {
    pub fn new(
        state: ModalWideNavigationRailState,
        content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self { state, content: Box::new(content), modifier: Modifier::new() }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.state.is_open());
        let state = self.state;
        let content = self.content;
        let theme = WiniaTheme::colors();
        // scrim 色 = 黑 @32%（M3 Scrim 规范值）；modal 面板底色 = SurfaceContainer
        let scrim = Color::from_argb(82, 0, 0, 0);
        let panel_container = theme.surface_container;
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let d_state = state.clone();
                crate::ui::Dialog::new(state.is_open())
                    .on_dismiss_request(move || d_state.close())
                    .dismiss_on_outside(false)
                    .build(ctx, move |ctx| {
                        Row::new()
                            .modifier(Modifier::new().fill_max_size())
                            .build(ctx, |ctx| {
                                // scrim：占满剩余空间，点击关闭
                                let s_close = state.clone();
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_height()
                                            .background(scrim, Shape::Rectangle)
                                            .clickable(move || s_close.close()),
                                    )
                                    .build(ctx, |_| {});
                                // 面板：左缘全高 SurfaceContainer
                                let p_close = state.clone();
                                Column::new()
                                    .alignment(Alignment::Center)
                                    .spacing(RAIL_VERTICAL_PADDING)
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_height()
                                            .width(WIDE_RAIL_EXPANDED_MIN_WIDTH)
                                            .background(panel_container, Shape::rounded(WIDE_PANEL_CORNER_RADIUS))
                                            .padding_top(WIDE_RAIL_TOP_PADDING)
                                            .padding_vertical(RAIL_VERTICAL_PADDING),
                                    )
                                    .build(ctx, |ctx| {
                                        WiniaTheme::with_content_color(theme.on_surface, ctx, |ctx| {
                                            let c_close = state.clone();
                                            // 面板顶部关闭按钮位（菜单语义——点击收起）
                                            IconButton::new()
                                                .on_click(move || c_close.close())
                                                .build(ctx, |ctx| {
                                                    Icon::svg_path("M3 18h18v-2H3v2zm0-5h18v-2H3v2zm0-7v2h18V6H3z").size(24.0).build(ctx);
                                                });
                                            content(ctx);
                                        });
                                    });
                            });
                    });
            }
        }
        ctx.end_restartable_group();
    }
}
