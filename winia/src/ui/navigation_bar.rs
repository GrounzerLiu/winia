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
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Color, GraphicsLayerParams, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::WiniaTheme;
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
    pub fn from_theme(theme: &crate::ui::theme::ThemeColors) -> Self {
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
    pub fn from_theme(theme: &crate::ui::theme::ThemeColors) -> Self {
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
    pub fn colors(theme: &crate::ui::theme::ThemeColors) -> NavigationBarColors {
        NavigationBarColors::from_theme(theme)
    }

    pub fn item_colors(theme: &crate::ui::theme::ThemeColors) -> NavigationBarItemColors {
        NavigationBarItemColors::from_theme(theme)
    }

    pub fn height() -> f32 {
        NAVIGATION_BAR_HEIGHT
    }

    /// LabelTextStyle = TypographyKeyTokens.LabelMedium
    pub fn label_style() -> crate::ui::text::TextStyle {
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

/// 临界阻尼 spring（对齐 MotionScheme DefaultEffects/FastSpatial 的
/// no-bouncy 特征；stiffness 分别近似 mediumLow/medium）。
fn nav_spring(stiffness: f32) -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
        damping_ratio: 1.0,
        stiffness,
        mass: 1.0,
        threshold: 0.01,
    })
}

/// alphaProgress 规格（DefaultEffects 近似）
const ALPHA_SPRING_STIFFNESS: f32 = 200.0;
/// sizeProgress 规格（FastSpatial 近似）
const SIZE_SPRING_STIFFNESS: f32 = 400.0;

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
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
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
            placements.push(Placement { size, position: Point::new(px, 0.0) });
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
        let alpha_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, nav_spring(ALPHA_SPRING_STIFFNESS));
        let size_progress =
            ctx.animate_float_as_state(if selected { 1.0 } else { 0.0 }, nav_spring(SIZE_SPRING_STIFFNESS));

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
                position: Point::new(
                    (container_w - indicator_size.width) / 2.0,
                    (height - indicator_h) / 2.0,
                ),
            });
            placements.push(Placement {
                size: icon_size,
                position: Point::new(icon_x, (height - icon_size.height) / 2.0),
            });
            placements.push(Placement {
                size: label_size,
                position: Point::new(
                    icon_x + icon_size.width + START_ICON_TO_LABEL_PADDING,
                    (height - label_size.height) / 2.0,
                ),
            });
            // ripple：居中、恒定全尺寸——最后放置（z 序最上层，状态层覆盖胶囊）
            placements.push(Placement {
                size: ripple_size,
                position: Point::new(
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
            position: Point::new((container_w - indicator_size.width) / 2.0, indicator_y),
        });
        placements.push(Placement {
            size: icon_size,
            position: Point::new((container_w - icon_size.width) / 2.0, selected_icon_y + offset),
        });
        // label：alwaysShowLabel=false 且 progress=0 时 alpha 已为 0——照常放置即可
        if let Some(label) = label_size {
            placements.push(Placement {
                size: label,
                position: Point::new((container_w - label.width) / 2.0, label_y + offset),
            });
        }
        // ripple：恒定全尺寸、跟随指示器的动画位置（始终是"胶囊当前所在
        // 完整矩形"——未选中悬浮时状态层与可见图标对齐），z 序最上层
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

    fn make_item(selected: bool, always: bool) -> NavigationBarItem {
        NavigationBarItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
            .label(|ctx| crate::ui::Text::new("Home").build(ctx))
            .always_show_label(always)
            .on_click(|| {})
    }

    fn compose_layout(item: NavigationBarItem) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| item.build(ctx));
        composer.layout(Constraints::new(0.0, f32::MAX, 0.0, f32::MAX));
        composer
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn navigation_bar_tokens_match_androidx_main() {
        assert_eq!(NAVIGATION_BAR_HEIGHT, 80.0, "TallContainerHeight");
        assert_eq!(NAVIGATION_BAR_ITEM_SPACING, 8.0, "NavigationBarItemHorizontalPadding");
        assert_eq!(NAVIGATION_BAR_INDICATOR_WIDTH, 56.0, "ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_BAR_INDICATOR_HEIGHT, 32.0, "ActiveIndicatorHeight");
        assert_eq!(NAVIGATION_BAR_ICON_SIZE, 24.0, "IconSize");
        assert_eq!(INDICATOR_HORIZONTAL_PADDING, 16.0);
        assert_eq!(INDICATOR_VERTICAL_PADDING, 4.0);
        assert_eq!(INDICATOR_TO_LABEL_PADDING, 4.0, "NavigationBarIndicatorToLabelPadding");
        assert_eq!(DISABLED_ALPHA, 0.38);
        // 水平 item（NavigationBarHorizontalItemTokens / ShortNavigationBar.kt）
        assert_eq!(NAVIGATION_BAR_H_INDICATOR_HEIGHT, 40.0, "Horizontal ActiveIndicatorHeight");
        assert_eq!(H_INDICATOR_VERTICAL_PADDING, 8.0);
        assert_eq!(H_INDICATOR_HORIZONTAL_PADDING, 16.0, "ActiveIndicatorLeadingSpace");
        assert_eq!(START_ICON_TO_LABEL_PADDING, 4.0, "ItemActiveIndicatorIconLabelSpace");
    }

    #[test]
    fn item_colors_follow_v0_11_0_tokens() {
        let theme = crate::ui::theme::ThemeColors::default_light();
        let c = NavigationBarItemColors::from_theme(&theme);
        assert_eq!(c.selected_icon, theme.on_secondary_container, "ItemActiveIconColor");
        assert_eq!(c.selected_label, theme.secondary, "ItemActiveLabelTextColor(v0_11_0)");
        assert_eq!(c.indicator, theme.secondary_container, "ItemActiveIndicatorColor");
        assert_eq!(c.unselected_icon, theme.on_surface_variant, "ItemInactiveIconColor");
        assert_eq!(c.unselected_label, theme.on_surface_variant, "ItemInactiveLabelTextColor");
        assert_eq!(c.disabled_icon, with_alpha_factor(theme.on_surface_variant, 0.38));
        assert_eq!(c.disabled_label, with_alpha_factor(theme.on_surface_variant, 0.38));
        // 颜色解析分支对齐 iconColor/textColor
        assert_eq!(c.icon_color(true, true), c.selected_icon);
        assert_eq!(c.icon_color(false, true), c.unselected_icon);
        assert_eq!(c.icon_color(true, false), c.disabled_icon);
        assert_eq!(c.text_color(false, false), c.disabled_label);
    }

    #[test]
    fn bar_splits_width_equally_with_spacing_and_rtl_mirrors() {
        let build_items = |ctx: &mut ComposeCtx| {
            for i in 0..3 {
                NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                    .on_click(|| {})
                    .build(ctx);
            }
        };
        let mut ltr = Composer::new();
        ltr.compose(|ctx| {
            NavigationBar::new(build_items)
                .modifier(Modifier::new().test_tag("bar"))
                .build(ctx)
        });
        ltr.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        let root = ltr.layout_root_idx().unwrap();
        let nodes = ltr.arena_nodes();
        let item_w = (360.0 - 2.0 * NAVIGATION_BAR_ITEM_SPACING) / 3.0;
        for i in 0..3usize {
            let item = child(nodes, root, i);
            assert_eq!(item.measured_size.width, item_w, "item 等分宽度");
            assert_eq!(
                item.position.x,
                i as f32 * (item_w + NAVIGATION_BAR_ITEM_SPACING),
                "LTR 第 i 个 item 的 x"
            );
            assert_eq!(item.measured_size.height, NAVIGATION_BAR_HEIGHT);
        }

        let build_items_rtl = |ctx: &mut ComposeCtx| {
            for i in 0..3 {
                NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                    .on_click(|| {})
                    .build(ctx);
            }
        };
        let mut rtl = Composer::new();
        rtl.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                crate::ui::theme::ThemeColors::default_light(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| NavigationBar::new(build_items_rtl).build(ctx),
            );
        });
        rtl.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        let rroot = rtl.layout_root_idx().unwrap();
        let rnodes = rtl.arena_nodes();
        let first = child(rnodes, rroot, 0);
        assert_eq!(first.position.x, 360.0 - item_w, "RTL 下第一个 item 镜像到最右");
    }

    #[test]
    fn selected_item_geometry_matches_androidx_place_label_and_icon() {
        // 无界宽度单测：containerW = iconW + 2x44 = 112
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple_node = child(nodes, root, 3);

        // contentH = 24 + 4 + 4 + 16(labelMedium 行高) = 48；vPad = (80-48)/2 = 16
        let v_pad = (NAVIGATION_BAR_HEIGHT - 48.0) / 2.0;
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_BAR_HEIGHT);
        // 指示器：宽 = 24+32 = 56（progress=1 全宽），y = iconY - 4，水平居中
        assert_eq!(indicator.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
        assert_eq!(indicator.position.y, v_pad - INDICATOR_VERTICAL_PADDING);
        assert_eq!(
            (nodes[root].measured_size.width - indicator.measured_size.width) / 2.0,
            indicator.position.x,
            "指示器水平居中"
        );
        // ripple：恒定全尺寸——选中态与胶囊重合
        assert_eq!(ripple_node.measured_size, indicator.measured_size);
        assert_eq!(ripple_node.position, indicator.position);
        // 图标：y = vPad（选中顶部位）
        assert_eq!(icon.position.y, v_pad);
        assert_eq!(icon.measured_size, Size::new(24.0, 24.0));
        // label：y = iconY + iconH + 4 + 4
        assert_eq!(
            label.position.y,
            v_pad + NAVIGATION_BAR_ICON_SIZE + INDICATOR_VERTICAL_PADDING + INDICATOR_TO_LABEL_PADDING
        );
    }

    #[test]
    fn unselected_item_without_always_label_centers_icon() {
        let composer = compose_layout(make_item(false, false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let ripple = child(nodes, root, 3);

        // progress=0：offset = (28-16)x1 = 12 → 图标垂直居中 (80-24)/2 = 28
        assert_eq!(icon.position.y, (NAVIGATION_BAR_HEIGHT - NAVIGATION_BAR_ICON_SIZE) / 2.0);
        assert_eq!(indicator.position.y, icon.position.y - INDICATOR_VERTICAL_PADDING);
        // 彩色指示器宽度收拢为 0（progress=0）
        assert_eq!(indicator.measured_size.width, 0.0);
        // 状态层载体（ripple 节点）恒定全尺寸——未选中悬浮仍有完整胶囊热区，
        // 且跟随指示器的动画位置（与可见图标对齐）
        assert_eq!(ripple.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(ripple.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
        assert_eq!(ripple.position.x, indicator.position.x, "ripple 与胶囊同 x");
        assert_eq!(ripple.position.y, indicator.position.y, "ripple 跟随胶囊动画位置");
    }

    fn make_horizontal_item(selected: bool) -> NavigationBarItem {
        NavigationBarItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
            .label(|ctx| crate::ui::Text::new("Home").build(ctx))
            .icon_position(NavigationItemIconPosition::Start)
            .on_click(|| {})
    }

    #[test]
    fn horizontal_item_geometry_matches_androidx_place_label_and_start_icon() {
        // 有界宽 120：验证内容组居中 + 指示器包裹整组（StartIconMeasurePolicy 数学）
        let mut composer = Composer::new();
        composer.compose(|ctx| make_horizontal_item(true).build(ctx));
        composer.layout(Constraints::new(0.0, 120.0, 0.0, NAVIGATION_BAR_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple_node = child(nodes, root, 3);

        assert_eq!(nodes[root].measured_size, Size::new(120.0, NAVIGATION_BAR_HEIGHT));
        let label_w = label.measured_size.width;
        // 内容组宽 = iconW + gap(4) + labelW；iconX = (containerW - contentW)/2
        let content_w = NAVIGATION_BAR_ICON_SIZE + START_ICON_TO_LABEL_PADDING + label_w;
        let icon_x = (120.0 - content_w) / 2.0;
        // 全部垂直居中
        assert_eq!(icon.position, Point::new(icon_x, (NAVIGATION_BAR_HEIGHT - 24.0) / 2.0));
        assert_eq!(
            label.position,
            Point::new(icon_x + NAVIGATION_BAR_ICON_SIZE + START_ICON_TO_LABEL_PADDING, (NAVIGATION_BAR_HEIGHT - label.measured_size.height) / 2.0)
        );
        // 指示器：宽 = contentW + leading+trailing(2x16)，高 = max(24,labelH)+2x8 = 40，居中
        assert_eq!(indicator.measured_size.width, content_w + H_INDICATOR_HORIZONTAL_PADDING * 2.0);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_H_INDICATOR_HEIGHT);
        assert_eq!(
            indicator.position,
            Point::new((120.0 - indicator.measured_size.width) / 2.0, (NAVIGATION_BAR_HEIGHT - NAVIGATION_BAR_H_INDICATOR_HEIGHT) / 2.0)
        );
        // ripple：恒定全尺寸、与选中态胶囊重合（Constraints.fixed(totalW, h)）
        assert_eq!(ripple_node.measured_size, indicator.measured_size);
        assert_eq!(ripple_node.position, indicator.position);
    }

    #[test]
    fn horizontal_item_without_label_falls_back_to_circular_indicator() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationBarItem::new(true, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
                .icon_position(NavigationItemIconPosition::Start)
                .on_click(|| {})
                .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, NAVIGATION_BAR_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        // 无 label → TopIconOrIconOnlyMeasurePolicy：56x32 圆形胶囊（同垂直）
        assert_eq!(indicator.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
    }

    #[test]
    fn always_show_label_keeps_positions_stable_when_unselected() {
        let selected = compose_layout(make_item(true, true));
        let unselected = compose_layout(make_item(false, true));
        let sn = selected.arena_nodes();
        let un = unselected.arena_nodes();
        let s_icon = child(sn, selected.layout_root_idx().unwrap(), 1);
        let u_icon = child(un, unselected.layout_root_idx().unwrap(), 1);
        assert_eq!(s_icon.position.y, u_icon.position.y, "alwaysShowLabel 时图标位置不随选中变化");
    }

    #[test]
    fn enabled_item_indicator_carries_pill_ripple_and_dynamic_background() {
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 节点拆分：children[0] = 彩色胶囊、最后一个子节点 = ripple（状态层载体）
        let indicator = child(nodes, root, 0);
        let ripple_node = child(nodes, root, 3);
        // ripple 挂在独立节点上且为 Pill 形状
        let ripple = ripple_node
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Ripple { shape, .. } => Some(shape),
                _ => None,
            })
            .expect("ripple 节点应有 ripple");
        assert_eq!(*ripple, Some(Shape::Pill));
        // 背景为动态闭包：progress=1 时输出全 alpha 指示器色
        let theme = crate::ui::theme::ThemeColors::default_light();
        let color_fn = indicator
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn),
                _ => None,
            })
            .expect("彩色指示器应有动态背景");
        assert_eq!(color_fn(), theme.secondary_container);
        // 职责分离：ripple 节点无背景、胶囊节点无 ripple
        assert!(ripple_node.modifier.elements().iter().all(|el| !matches!(
            el,
            crate::modifier::ModifierElement::Background { .. }
        )));
        assert!(indicator.modifier.elements().iter().all(|el| !matches!(
            el,
            crate::modifier::ModifierElement::Ripple { .. }
        )));
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

    #[test]
    fn unselected_item_hover_shows_state_layer_on_full_pill_rect() {
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = crate::ui::theme::ThemeColors::default_light();
        let container = theme.surface_container;
        let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

        // item1（未选中）用自备交互源——测试手动发射 Hover Enter
        let source = MutableInteractionSource::new();
        let src_for_item = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationBar::new(move |ctx| {
                NavigationBarItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::ui::Text::new("A").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
                NavigationBarItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::ui::Text::new("B").build(ctx))
                    .interaction_source(src_for_item.clone())
                    .on_click(|| {})
                    .build(ctx);
                NavigationBarItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::ui::Text::new("C").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
            })
            .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));

        // 悬浮未选中项 → 状态层透明度动画（500ms tween，真实时钟）——
        // 轮询推进至收敛（STATE_LAYER_HOVER 0.08）
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
        let mut surface = skia_safe::surfaces::raster_n32_premul((360, 80)).expect("navbar surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // 未选中项胶囊矩形内（避开图标 168..192）取色：item1 x 起点 = 114.67+8，
        // ripple_x = (item_w - 56)/2 → 胶囊左缘全局 ≈152；取 (158, 28)
        // 像素 = 容器色上叠加 onSurfaceVariant @8% —— 必然偏离纯容器色
        let mut s = surface;
        let px = pixel_at(&mut s, 158, 28);
        assert_ne!(px, to_rgba(container), "悬浮未选中项应显示状态层（完整胶囊热区）");
        // 无悬浮的 item2 同位置保持纯容器色（对照）
        assert_eq!(pixel_at(&mut s, 320, 28), to_rgba(container), "无悬浮项不应有状态层");
    }

    #[test]
    fn disabled_item_has_no_interaction_elements() {
        let composer = compose_layout(make_item(true, true).enabled(false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert!(nodes[root].modifier.clickable_interaction().is_none());
        // 禁用项：ripple 节点 Modifier 为空（无状态层载体）、彩色胶囊无 ripple
        let ripple_node = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        assert!(ripple_node.modifier.ripple_interaction().is_none(), "禁用项无状态层载体");
        assert!(indicator.modifier.ripple_interaction().is_none(), "禁用项指示器无 ripple");
    }

    #[test]
    fn scaffold_bottom_bar_integrates_navigation_bar() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            crate::ui::Scaffold::new(|ctx, _| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_size().test_tag("content"));
                ctx.end_node();
            })
            .top_bar(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_width().height(64.0));
                ctx.end_node();
            })
            .bottom_bar(|ctx| {
                NavigationBar::new(|ctx| {
                    for i in 0..3 {
                        NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                            .label(move |ctx| {
                                crate::ui::Text::new(format!("Item{i}")).build(ctx);
                            })
                            .on_click(|| {})
                            .build(ctx);
                    }
                })
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // Scaffold 子节点顺序：[top, bottom, content, fab]
        let bottom_slot = child(nodes, root, 1);
        let content = child(nodes, root, 2);
        assert_eq!(bottom_slot.measured_size.height, NAVIGATION_BAR_HEIGHT, "bottom bar 高度 80dp");
        assert_eq!(content.position.y, 64.0);
        assert_eq!(content.measured_size.height, 640.0 - 64.0 - NAVIGATION_BAR_HEIGHT);
        // NavigationBar 内 item 等分（bottom slot -> bar -> items）
        let bar_idx = nodes[bottom_slot_idx(nodes, root)].children[0];
        let item_w = (360.0 - 2.0 * NAVIGATION_BAR_ITEM_SPACING) / 3.0;
        assert_eq!(child(nodes, bar_idx, 0).measured_size.width, item_w);
    }

    fn bottom_slot_idx(nodes: &[LayoutNode], root: usize) -> usize {
        nodes[root].children[1]
    }
}
