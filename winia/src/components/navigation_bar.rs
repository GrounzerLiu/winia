//! Material 3 NavigationBar / NavigationBarItem.
//!
//! 对齐 androidx-main 源码（compose/material3/NavigationBar.kt + NavigationItem.kt，
//! NavigationBarTokens v0_11_0 / NavigationBarVerticalItemTokens）：
//!
//! - 容器高 80dp（TallContainerHeight）、surfaceContainer 底色、item 水平间距 8dp
//!   （NavigationBarItemHorizontalPadding）；item 等分宽度（RowScope.weight(1f) 语义）
//! - 选中指示器胶囊尺寸由图标推导：宽 = iconW + 2x16（IndicatorHorizontalPadding =
//!   (ActiveIndicatorWidth 56 - IconSize 24)/2），高 = iconH + 2x4（IndicatorVerticalPadding
//!   = (ActiveIndicatorHeight 32 - IconSize 24)/2）；CornerFull 形状
//! - 颜色 token：icon 选中 OnSecondaryContainer / 未选中 OnSurfaceVariant；
//!   label 选中 Secondary（v0_11_0 ItemActiveLabelTextColor——旧版规范为
//!   OnSurface，按最新源码对齐）/ 未选中 OnSurfaceVariant；指示器 SecondaryContainer；
//!   禁用 = OnSurfaceVariant @ 38%（DisabledAlpha）
//! - 双进度动画（对齐 alphaAnimationProgress/sizeAnimationProgress 分离）：
//!   sizeProgress（FastSpatial 近似——spring stiffness 400）驱动指示器宽度展开与
//!   图标/label 位置插值；alphaProgress（DefaultEffects 近似——stiffness 200）驱动
//!   指示器透明度与 label 淡入淡出
//! - alwaysShowLabel=false：未选中仅图标垂直居中；选中后图标上移至顶部位 +
//!   label 出现（位置按 sizeProgress 插值，精确复刻 placeLabelAndIcon 数学）
//! - ripple 只出现在指示器胶囊区域（bounded Pill），整个 item 可点击
//! - 布局期读进度走两段式依赖（measure 中 State::get 注册 layout_deps）——
//!   动画每帧只重测不重组；背景/透明度闭包绘制期 peek——零重组纯重绘
//!
//! # 水平 item（NavigationItemIconPosition::Start，中等窗口）
//!
//! 对齐 androidx-main ShortNavigationBar.kt + NavigationItem.kt StartIconMeasurePolicy /
//! placeLabelAndStartIcon（NavigationBarHorizontalItemTokens）：图标在左、label 在右，
//! 指示器胶囊横向包裹整组——宽 = iconW + gap(4) + labelW + leading+trailing(2x16)，
//! 高 = max(iconH, labelH) + 2x8 = ActiveIndicatorHeight 40；内容组整体水平居中、
//! 全部垂直居中；label 恒显示（alwaysShowLabel 淡出/位置插值仅垂直模式）；
//! 无 label 时退化为垂直模式的圆形指示器（TopIconOrIconOnlyMeasurePolicy）

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Modifier};
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::interaction::MutableInteractionSource;
use crate::theme::WiniaTheme;
use std::sync::Arc;

// ── Token 常量（androidx-main NavigationBarTokens / NavigationBarVerticalItemTokens /
//    NavigationBar.kt 私有常量）──

/// 容器高度 = NavigationBarTokens.TallContainerHeight
pub const NAVIGATION_BAR_HEIGHT: f32 = 80.0;
/// item 水平间距 = NavigationBarItemHorizontalPadding（Row spacedBy）
pub const NAVIGATION_BAR_ITEM_SPACING: f32 = 8.0;
/// 指示器基准宽 = NavigationBarVerticalItemTokens.ActiveIndicatorWidth
pub const NAVIGATION_BAR_INDICATOR_WIDTH: f32 = 56.0;
/// 指示器高 = NavigationBarVerticalItemTokens.ActiveIndicatorHeight
pub const NAVIGATION_BAR_INDICATOR_HEIGHT: f32 = 32.0;
/// 图标尺寸 = NavigationBarVerticalItemTokens.IconSize
pub const NAVIGATION_BAR_ICON_SIZE: f32 = 24.0;
/// (ActiveIndicatorWidth - IconSize) / 2 —— 指示器宽由图标推导的横向内边距
const INDICATOR_HORIZONTAL_PADDING: f32 =
    (NAVIGATION_BAR_INDICATOR_WIDTH - NAVIGATION_BAR_ICON_SIZE) / 2.0;
/// (ActiveIndicatorHeight - IconSize) / 2 —— 纵向内边距
const INDICATOR_VERTICAL_PADDING: f32 =
    (NAVIGATION_BAR_INDICATOR_HEIGHT - NAVIGATION_BAR_ICON_SIZE) / 2.0;
/// NavigationBarIndicatorToLabelPadding——指示器底到 label 顶
const INDICATOR_TO_LABEL_PADDING: f32 = 4.0;
/// Material DisabledAlpha
const DISABLED_ALPHA: f32 = 0.38;
/// 无界约束下的最小 item 宽 = iconW + 2xNavigationBarItemToIconMinimumPadding
const ITEM_TO_ICON_MINIMUM_PADDING: f32 = 44.0;

// ── 水平 item token（androidx-main ShortNavigationBar.kt /
//    NavigationBarHorizontalItemTokens / StartIconMeasurePolicy）──

/// 水平 item 指示器高 = NavigationBarHorizontalItemTokens.ActiveIndicatorHeight
pub const NAVIGATION_BAR_H_INDICATOR_HEIGHT: f32 = 40.0;
/// (HorizontalActiveIndicatorHeight - IconSize) / 2 = 8 —— 水平指示器纵向内边距
const H_INDICATOR_VERTICAL_PADDING: f32 =
    (NAVIGATION_BAR_H_INDICATOR_HEIGHT - NAVIGATION_BAR_ICON_SIZE) / 2.0;
/// 水平指示器横向内边距 = ActiveIndicatorLeadingSpace（measure 中 x2 = leading+trailing）
const H_INDICATOR_HORIZONTAL_PADDING: f32 = 16.0;
/// NavigationBarTokens.ItemActiveIndicatorIconLabelSpace —— 水平 item 图标与 label 间距
const START_ICON_TO_LABEL_PADDING: f32 = 4.0;

/// 导航 item 图标位置（对齐 androidx NavigationItemIconPosition——
/// ShortNavigationBarItem / WideNavigationRailItem 共享的统一概念）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationItemIconPosition {
    /// 图标在上、label 在下（placeLabelAndIcon / placeIcon 数学；紧凑窗口）
    Top,
    /// 图标在左、label 在右（placeLabelAndStartIcon 数学；label 恒显示，
    /// alwaysShowLabel 的位置插值/淡出仅适用于 Top 模式；中等窗口）
    Start,
}

// ── 颜色 ──

/// 容器色（对标 androidx NavigationBarColors——仅 container token）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationBarColors {
    pub container: Color,
}

impl NavigationBarColors {
    pub fn new(container: Color) -> Self {
        Self { container }
    }

    /// ContainerColor = ColorSchemeKeyTokens.SurfaceContainer
    pub fn from_theme(theme: &crate::theme::ThemeColors) -> Self {
        Self { container: theme.surface_container }
    }
}

/// 对标 androidx NavigationBarItemColors（NavigationItemColors 子集）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationBarItemColors {
    pub selected_icon: Color,
    pub selected_label: Color,
    pub indicator: Color,
    pub unselected_icon: Color,
    pub unselected_label: Color,
    pub disabled_icon: Color,
    pub disabled_label: Color,
}

impl NavigationBarItemColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        selected_icon: Color,
        selected_label: Color,
        indicator: Color,
        unselected_icon: Color,
        unselected_label: Color,
        disabled_icon: Color,
        disabled_label: Color,
    ) -> Self {
        Self { selected_icon, selected_label, indicator, unselected_icon, unselected_label, disabled_icon, disabled_label }
    }

    /// defaultNavigationBarItemColors（NavigationBar.kt ColorScheme 扩展）：
    /// - selectedIcon = ItemActiveIconColor = OnSecondaryContainer
    /// - selectedLabel = ItemActiveLabelTextColor = Secondary（v0_11_0）
    /// - indicator = ItemActiveIndicatorColor = SecondaryContainer
    /// - unselectedIcon/Label = ItemInactive* = OnSurfaceVariant
    /// - disabled* = OnSurfaceVariant.copy(alpha = 0.38)
    pub fn from_theme(theme: &crate::theme::ThemeColors) -> Self {
        Self::new(
            theme.on_secondary_container,
            theme.secondary,
            theme.secondary_container,
            theme.on_surface_variant,
            theme.on_surface_variant,
            with_alpha_factor(theme.on_surface_variant, DISABLED_ALPHA),
            with_alpha_factor(theme.on_surface_variant, DISABLED_ALPHA),
        )
    }

    /// iconColor(selected, enabled)——对齐 NavigationBarItemColors::iconColor
    pub fn icon_color(&self, selected: bool, enabled: bool) -> Color {
        if !enabled {
            self.disabled_icon
        } else if selected {
            self.selected_icon
        } else {
            self.unselected_icon
        }
    }

    /// textColor(selected, enabled)——对齐 NavigationBarItemColors::textColor
    pub fn text_color(&self, selected: bool, enabled: bool) -> Color {
        if !enabled {
            self.disabled_label
        } else if selected {
            self.selected_label
        } else {
            self.unselected_label
        }
    }
}

pub struct NavigationBarDefaults;

impl NavigationBarDefaults {
    pub fn colors(theme: &crate::theme::ThemeColors) -> NavigationBarColors {
        NavigationBarColors::from_theme(theme)
    }

    pub fn item_colors(theme: &crate::theme::ThemeColors) -> NavigationBarItemColors {
        NavigationBarItemColors::from_theme(theme)
    }

    pub fn height() -> f32 {
        NAVIGATION_BAR_HEIGHT
    }

    /// LabelTextStyle = TypographyKeyTokens.LabelMedium
    pub fn label_style() -> crate::text::TextStyle {
        WiniaTheme::typography().label_medium
    }
}

fn with_alpha_factor(color: Color, factor: f32) -> Color {
    Color::from_argb(
        ((color.a as f32 * factor).round().min(255.0)) as u8,
        color.r,
        color.g,
        color.b,
    )
}

// ── 动画规格 ──

/// An item's icon/label fade: M3 runs it on the `DefaultEffects` token
/// (`material3/NavigationBar.kt:264`: `animationSpec = MotionSchemeKeyTokens.DefaultEffects.value()`),
/// i.e. damping 1.0 / stiffness 1600 (`tokens/StandardMotionTokens.kt`:
/// `SpringDefaultEffectsDamping/Stiffness = 1.0f / 1600.0f`). winia used damping 1.0 / stiffness 200,
/// five times softer.
fn nav_alpha_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}

/// The indicator pill's size: M3 runs it on the `FastSpatial` token
/// (`material3/NavigationBar.kt:270`), damping 0.9 / stiffness 1400
/// (`tokens/StandardMotionTokens.kt`: `SpringFastSpatialDamping/Stiffness`) — the 0.9 is why the pill
/// settles with a slight overshoot where winia's no-bouncy 400 settled flat.
fn nav_size_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

// ── NavigationBar 容器 ──

pub struct NavigationBar {
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    colors: Option<NavigationBarColors>,
    modifier: Modifier,
}

impl NavigationBar {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self { content: Box::new(content), colors: None, modifier: Modifier::new() }
    }

    pub fn colors(mut self, colors: NavigationBarColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| NavigationBarDefaults::colors(&theme));
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let content = self.content;
        let policy = NavigationBarLayoutPolicy { direction };
        // Surface(surfaceContainer) + Row(fillMaxWidth, minHeight 80, spacedBy 8, CenterVertically)
        let root_modifier = Modifier::new()
            .fill_max_width()
            .height(NAVIGATION_BAR_HEIGHT)
            .background(colors.container, Shape::Rectangle)
            .then(self.modifier);
        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Surface contentColor = contentColorFor(surfaceContainer) = onSurface
                WiniaTheme::with_content_color(theme.on_surface, ctx, content);
            }
        }
        ctx.end_restartable_group();
    }
}

/// 容器布局：item 等分宽度（weight(1f) 语义——tight 主轴约束）+ 8dp 间距 + RTL 镜像。
#[derive(Debug)]
struct NavigationBarLayoutPolicy {
    direction: LayoutDirection,
}

impl MeasurePolicy for NavigationBarLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        c: Constraints,
    ) -> (Size, Vec<Placement>) {
        let width = c.max_width;
        let height = c.max_height.min(NAVIGATION_BAR_HEIGHT);
        let n = children.len();
        if n == 0 {
            return (Size::new(width, height), Vec::new());
        }
        let spacing_total = NAVIGATION_BAR_ITEM_SPACING * (n as f32 - 1.0);
        let item_w = ((width - spacing_total) / n as f32).max(0.0);
        let mut placements = Vec::with_capacity(n);
        let mut x = 0.0f32;
        for &child in children {
            // weight(1f)：主轴 tight；交叉轴松（item 自身 minHeight 80 决定高度）
            let (size, _) = measure_node(nodes, policies, child, Constraints::new(item_w, item_w, 0.0, c.max_height));
            let px = if self.direction == LayoutDirection::Ltr {
                x
            } else {
                width - x - item_w
            };
            placements.push(Placement { size, position: Offset::new(px, 0.0) });
            x += item_w + NAVIGATION_BAR_ITEM_SPACING;
        }
        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

// ── NavigationBarItem ──

pub struct NavigationBarItem {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    label: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    enabled: bool,
    always_show_label: bool,
    icon_position: NavigationItemIconPosition,
    colors: Option<NavigationBarItemColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl NavigationBarItem {
    pub fn new(selected: bool, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            selected,
            on_click: None,
            icon: Box::new(icon),
            label: None,
            enabled: true,
            always_show_label: true,
            icon_position: NavigationItemIconPosition::Top,
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

    /// 图标位置：Top（默认，紧凑窗口）或 Start（中等窗口，图标在左）。
    pub fn icon_position(mut self, position: NavigationItemIconPosition) -> Self {
        self.icon_position = position;
        self
    }

    pub fn colors(mut self, colors: NavigationBarItemColors) -> Self {
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
        ctx.changed(&self.icon_position);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| NavigationBarDefaults::item_colors(&theme));
        let selected = self.selected;
        let enabled = self.enabled;
        let has_label = self.label.is_some();
        // 水平模式 label 恒显示（M3 Expressive 规范）——alwaysShowLabel 淡出仅垂直模式
        let horizontal = self.icon_position == NavigationItemIconPosition::Start;

        // 双进度动画（alphaProgress / sizeProgress 分离——见模块文档）
        let alpha_progress = ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, nav_alpha_spec());
        let size_progress = ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, nav_size_spec());

        let icon_color = colors.icon_color(selected, enabled);
        let label_color = colors.text_color(selected, enabled);
        // ripple 色跟随图标色（未选中 onSurfaceVariant / 选中 onSecondaryContainer）
        let ripple_color = icon_color;

        let interaction = self.interaction_source.clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());

        // 指示器背景：颜色 x alphaProgress——绘制期每帧求值（peek 不注册依赖）
        let indicator_color = colors.indicator;
        let indicator_alpha = alpha_progress.clone();
        let indicator_modifier = Modifier::new().background(
            move || with_alpha_factor(indicator_color, indicator_alpha.peek()),
            Shape::Pill,
        );
        // 指示器 ripple（状态层载体）——**独立节点、恒定全尺寸**：未选中时彩色
        // 胶囊收拢为 0 宽，但悬浮/按压的状态层仍需以完整胶囊矩形呈现
        // （对齐 androidx IndicatorRipple 与 Indicator 分离的设计）
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, ripple_color, true, Shape::Pill)
        } else {
            Modifier::new()
        };

        // label 包装层：垂直 alwaysShowLabel=false 时透明度跟 alphaProgress
        let label_alpha_state = alpha_progress.clone();
        let label_alpha_always = self.always_show_label || horizontal;
        let label_slot_modifier = if has_label && !label_alpha_always {
            Modifier::new().graphics_layer(move || GraphicsLayerParams {
                alpha: label_alpha_state.peek(),
                ..GraphicsLayerParams::default()
            })
        } else {
            Modifier::new()
        };

        let policy = NavigationBarItemLayoutPolicy {
            size_progress: size_progress.clone(),
            always_show_label: self.always_show_label,
            has_label,
            horizontal,
        };

        let mut item_modifier = Modifier::new();
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
                // （z 序最上层，对齐 androidx placeRelative 顺序：状态层覆盖在
                // 彩色胶囊与内容之上，选中/未选中悬浮均有状态层）
                // 1) 彩色指示器胶囊（宽度随 sizeProgress 展开动画）
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, indicator_modifier);
                ctx.end_node();
                // 2) 图标（内容色注入——Icon Tint::Auto 取 content color）
                let icon = self.icon;
                wrap_slot(ctx, Modifier::new(), |ctx| {
                    WiniaTheme::with_content_color(icon_color, ctx, icon);
                });
                // 3) label（LabelMedium + 文字色）
                if let Some(label) = self.label {
                    wrap_slot(ctx, label_slot_modifier, |ctx| {
                        let mut style = NavigationBarDefaults::label_style();
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

/// 单子节点包装（保证每个逻辑槽恰好贡献一个 arena 子节点——索引稳定）。
fn wrap_slot(ctx: &mut ComposeCtx, modifier: Modifier, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(Alignment::Center)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

/// item 布局：精确复刻 androidx NavigationItem（Top/Start icon position）的
/// placeLabelAndIcon / placeIcon 数学（见模块文档）。
///
/// 子节点索引约定：children[0] = 彩色指示器胶囊（宽度随 sizeProgress
/// 收拢/展开），children[1] = icon，children[2] = label（仅有 label 时存在），
/// 最后一个子节点 = indicator ripple（恒定全尺寸状态层载体，z 序最上层）。
#[derive(Debug)]
struct NavigationBarItemLayoutPolicy {
    /// sizeProgress——measure 入口 get() 注册**布局依赖**（两段式：
    /// 动画帧只重测本节点，不重组）
    size_progress: State<f32>,
    always_show_label: bool,
    has_label: bool,
    /// 水平 item（StartIconMeasurePolicy 路径）
    horizontal: bool,
}

impl MeasurePolicy for NavigationBarItemLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 1) 必须最先读进度——此刻 ACTIVE_SLOT_KEY 仍是本节点（measure_node
        //    进入时设置）；递归测量子节点后 key 会被改写，届时读取将注册到子节点
        let size_p = self.size_progress.get().max(0.0);

        let min_h = NAVIGATION_BAR_HEIGHT.max(constraints.min_height);
        let loose = constraints.loosen();

        // ripple 是最后一个子节点（z 序最上层）
        let ripple_idx = children.len() - 1;
        // 2) 图标先测（loose）——指示器尺寸由它推导
        let (icon_size, _) = measure_node(nodes, policies, children[1], loose);

        // ── 水平 item（StartIconMeasurePolicy + placeLabelAndStartIcon）──
        // 指示器胶囊横向包裹 [icon + gap + label] 整组，全部垂直居中
        if self.horizontal && self.has_label {
            // label 测量：宽度预算 = 可用宽 - iconW - gap
            // （androidx: looseConstraints.offset(horizontal = -(iconW + pad))）
            let label_max_w = if loose.max_width.is_finite() {
                (loose.max_width - icon_size.width - START_ICON_TO_LABEL_PADDING).max(0.0)
            } else {
                f32::INFINITY
            };
            let (label_size, _) = measure_node(
                nodes,
                policies,
                children[2],
                Constraints::new(0.0, label_max_w, 0.0, loose.max_height),
            );
            // totalIndicatorWidth = iconW + labelW + gap + (leading+trailing)
            let total_indicator_w = icon_size.width
                + label_size.width
                + START_ICON_TO_LABEL_PADDING
                + H_INDICATOR_HORIZONTAL_PADDING * 2.0;
            // indicatorHeight = max(iconH, labelH) + 2x8 (= ActiveIndicatorHeight 40)
            let indicator_h =
                icon_size.height.max(label_size.height) + H_INDICATOR_VERTICAL_PADDING * 2.0;
            // ripple：恒定全尺寸（Constraints.fixed(totalW, h)）——未选中不收拢，
            // 悬浮/按压状态层始终以完整胶囊矩形呈现（androidx IndicatorRipple）
            let (ripple_size, _) = measure_node(
                nodes,
                policies,
                children[ripple_idx],
                Constraints::new(total_indicator_w, total_indicator_w, indicator_h, indicator_h),
            );
            let animated_indicator_w = total_indicator_w * size_p;
            let (indicator_size, _) = measure_node(
                nodes,
                policies,
                children[0],
                Constraints::new(
                    animated_indicator_w,
                    animated_indicator_w,
                    indicator_h,
                    indicator_h,
                ),
            );

            let container_w = if constraints.max_width.is_finite() {
                constraints.max_width
            } else {
                icon_size.width + ITEM_TO_ICON_MINIMUM_PADDING * 2.0
            };
            let max_h = if constraints.max_height.is_finite() {
                constraints.max_height
            } else {
                f32::MAX
            };
            let height = min_h.min(max_h);

            // placeLabelAndStartIcon：内容组(icon+gap+label)整体水平居中、
            // 全部垂直居中；指示器居中（宽含 leading/trailing 后恰好包住内容组）
            let content_w =
                icon_size.width + START_ICON_TO_LABEL_PADDING + label_size.width;
            let icon_x = (container_w - content_w) / 2.0;

            let mut placements = Vec::with_capacity(children.len());
            // 彩色胶囊：居中（宽随 progress 收拢/展开）
            placements.push(Placement {
                size: indicator_size,
                position: Offset::new(
                    (container_w - indicator_size.width) / 2.0,
                    (height - indicator_h) / 2.0,
                ),
            });
            placements.push(Placement {
                size: icon_size,
                position: Offset::new(icon_x, (height - icon_size.height) / 2.0),
            });
            placements.push(Placement {
                size: label_size,
                position: Offset::new(
                    icon_x + icon_size.width + START_ICON_TO_LABEL_PADDING,
                    (height - label_size.height) / 2.0,
                ),
            });
            // ripple：居中、恒定全尺寸——最后放置（z 序最上层，状态层覆盖胶囊）
            placements.push(Placement {
                size: ripple_size,
                position: Offset::new(
                    (container_w - ripple_size.width) / 2.0,
                    (height - indicator_h) / 2.0,
                ),
            });
            return (Size::new(container_w, height), placements);
        }

        let total_indicator_w = icon_size.width + INDICATOR_HORIZONTAL_PADDING * 2.0;
        let animated_indicator_w = total_indicator_w * size_p;
        let indicator_h = icon_size.height + INDICATOR_VERTICAL_PADDING * 2.0;
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
        // ripple：恒定全尺寸（56x32 基准胶囊）——未选中不收拢，悬浮/按压
        // 状态层始终以完整胶囊矩形呈现（androidx IndicatorRipple 分离设计）
        let (ripple_size, _) = measure_node(
            nodes,
            policies,
            children[ripple_idx],
            Constraints::new(total_indicator_w, total_indicator_w, indicator_h, indicator_h),
        );

        // 容器宽：有界取约束宽；无界回退 iconW + 2x44（NavigationBarItemToIconMinimumPadding）
        let container_w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            icon_size.width + ITEM_TO_ICON_MINIMUM_PADDING * 2.0
        };
        let max_h = if constraints.max_height.is_finite() { constraints.max_height } else { f32::MAX };

        let (height, selected_icon_y, unselected_icon_y, label_y) = if let Some(label) = &label_size {
            // placeLabelAndIcon：
            // contentHeight = iconH + IndicatorVerticalPadding + IndicatorToLabelPadding + labelH
            let content_h = icon_size.height + INDICATOR_VERTICAL_PADDING + INDICATOR_TO_LABEL_PADDING + label.height;
            let v_pad = ((min_h - content_h) / 2.0).max(INDICATOR_VERTICAL_PADDING);
            let height = (content_h + v_pad * 2.0).min(max_h);
            let selected_icon_y = v_pad;
            let unselected_icon_y = if self.always_show_label {
                selected_icon_y
            } else {
                (height - icon_size.height) / 2.0
            };
            let label_y = selected_icon_y + icon_size.height + INDICATOR_VERTICAL_PADDING + INDICATOR_TO_LABEL_PADDING;
            (height, selected_icon_y, unselected_icon_y, label_y)
        } else {
            // placeIcon：全部垂直居中
            let height = min_h.min(max_h);
            let center_y = (height - icon_size.height) / 2.0;
            (height, center_y, center_y, 0.0)
        };

        // 位置插值：offset = iconDistance x (1 - progress)
        let offset = (unselected_icon_y - selected_icon_y) * (1.0 - size_p);
        let indicator_y = selected_icon_y - INDICATOR_VERTICAL_PADDING + offset;

        let mut placements = Vec::with_capacity(children.len());
        placements.push(Placement {
            size: indicator_size,
            position: Offset::new((container_w - indicator_size.width) / 2.0, indicator_y),
        });
        placements.push(Placement {
            size: icon_size,
            position: Offset::new((container_w - icon_size.width) / 2.0, selected_icon_y + offset),
        });
        // label：alwaysShowLabel=false 且 progress=0 时 alpha 已为 0——照常放置即可
        if let Some(label) = label_size {
            placements.push(Placement {
                size: label,
                position: Offset::new((container_w - label.width) / 2.0, label_y + offset),
            });
        }
        // ripple：恒定全尺寸、跟随指示器的动画位置（始终是"胶囊当前所在
        // 完整矩形"——未选中悬浮时状态层与可见图标对齐），z 序最上层
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
