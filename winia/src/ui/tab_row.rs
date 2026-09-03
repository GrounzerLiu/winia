//! Material 3 TabRow / Tab 组件 — 对标 androidx-main TabRow.kt + Tab.kt
//!
//! 当前实现：Fixed TabRow（Primary/Secondary，等分）+ Tab 组件（text/icon 槽）。
//! 指示条动画：双 State<f32>（offset/width）在 build 期创建，TabRowLayoutPolicy.measure 期
//! 计算 target 后调 push_animatable，State::get() 注册 layout_deps（两段式依赖——动画帧只重测不重组）。
//! 首次布局免动画：initialized 标记（AtomicBool）跳过首次 push_animatable。
//!
//! 偏差记录（与 Compose 对照）：
//! - 无 TabBaselineLayout 基线精确数学（text+icon 垂直居中，无 firstBaseline/lastBaseline 修正）
//! - Tab 颜色过渡用静态颜色（无 animateColor 插值；后续可加 graphics_layer 交叉淡化）
//! - TabRow 无 scrollable 变体（后续可加 PrimaryScrollableTabRow / SecondaryScrollableTabRow）
//! - RTL 镜像：tab 布局镜像，indicator 偏移直接对齐 tab 物理 left（非逻辑 start 偏移）
//! - contentWidth = max(tab 自然宽 - 32dp, 24dp)（无 maxIntrinsicWidth 调用，用 1st pass 测量近似）
//! - 动画 spec = spring(damping_ratio=0.6, stiffness=700)（对齐 M3 Expressive DefaultSpatial）
//! - 指示条高固定 3dp（ActiveIndicatorHeight）
//! - 无 TabIndicatorScope 自定义指示器 API（当前内部固定）

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::layout::LayoutDirection;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::text::TextAlign;
use crate::ui::theme::WiniaTheme;
use std::sync::atomic::{AtomicBool, Ordering};

// ── Token 常量 ──

/// 容器高度（PrimaryNavigationTabTokens.ContainerHeight = 48dp）
pub const TAB_ROW_HEIGHT: f32 = 48.0;
/// 指示条高度（PrimaryNavigationTabTokens.ActiveIndicatorHeight = 3dp）
pub const ACTIVE_INDICATOR_HEIGHT: f32 = 3.0;
/// 水平文本内边距（HorizontalTextPadding = 16dp）
pub const HORIZONTAL_TEXT_PADDING: f32 = 16.0;
/// 最小指示条宽度（24dp 触控目标）
pub const MIN_INDICATOR_WIDTH: f32 = 24.0;
/// 大号 Tab 高度（text+icon 时 = 72dp）
pub const LARGE_TAB_HEIGHT: f32 = 72.0;
/// 小号 Tab 高度（text-only 或 icon-only = 48dp）
pub const SMALL_TAB_HEIGHT: f32 = 48.0;
/// 图标与文本间距（IconDistanceFromBaseline 近似 = 20dp）
pub const ICON_TEXT_SPACING: f32 = 20.0;

// ── 动画规格 ──

/// 指示条动画 spring（对齐 M3 Expressive Motion DefaultSpatial：
/// spring(dampingRatio=0.6, stiffness=700)）
fn indicator_spring() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
        damping_ratio: 0.6,
        stiffness: 700.0,
        mass: 1.0,
        threshold: 0.01,
    })
}

// ── TabPosition ──

/// 单个 tab 的位置信息，供指示条计算 offset/width。
///
/// 对标 Compose `TabPosition` 的 left/width/contentWidth/right。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabPosition {
    /// 左边缘 x 坐标（绝对坐标，LTR 下从左算起，RTL 下从右算起但存物理 x）
    pub left: f32,
    /// 宽度（等分宽）
    pub width: f32,
    /// 内容宽度——用于 Primary 指示条（followContentSize=true）；min 24dp
    pub content_width: f32,
    /// 右边缘 x 坐标（left + width）
    pub right: f32,
}

impl TabPosition {
    pub fn new(left: f32, width: f32, content_width: f32) -> Self {
        Self { left, width, content_width, right: left + width }
    }
}

// ── TabRowDefaults ──

/// TabRow 默认值与颜色。
pub struct TabRowDefaults;

impl TabRowDefaults {
    /// 容器色（PrimaryNavigationTabTokens.ContainerColor = Surface）
    pub fn container_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.surface
    }

    /// Primary 内容色（PrimaryNavigationTabTokens.ActiveLabelTextColor = Primary）
    pub fn primary_content_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 内容色（SecondaryNavigationTabTokens.ActiveLabelTextColor = OnSurface）
    pub fn secondary_content_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.on_surface
    }

    /// 分隔线色（DividerTokens.Color = OutlineVariant）
    pub fn divider_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.outline_variant
    }

    /// Primary 指示条色（PrimaryNavigationTabTokens.ActiveIndicatorColor = Primary）
    pub fn primary_indicator_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 指示条色（Same as Primary 但无 shape）
    pub fn secondary_indicator_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Primary 指示条形状（ActiveIndicatorShape = RoundedCornerShape(3dp)）
    pub fn primary_indicator_shape() -> Shape {
        Shape::RoundedRect { corner_radius: 3.0 }
    }

    /// Secondary 指示条形状（直角全宽）
    pub fn secondary_indicator_shape() -> Shape {
        Shape::Rectangle
    }

    /// 选中 Tab 内容色（Primary 选中 = Primary，Secondary 选中 = OnSurface）
    pub fn selected_content_color(theme: &crate::ui::theme::ThemeColors, is_primary: bool) -> Color {
        if is_primary { theme.primary } else { theme.on_surface }
    }

    /// 未选中 Tab 内容色（InactiveLabelTextColor = OnSurfaceVariant）
    pub fn unselected_content_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.on_surface_variant
    }

    /// Tab 文本样式（LabelTextFont = TitleSmall = 14.0/20.0）
    pub fn label_text_style() -> crate::ui::text::TextStyle {
        let mut s = WiniaTheme::typography().title_small;
        s.text_align = Some(TextAlign::Center);
        s
    }
}

// ── TabRow ──

/// 固定（等分）TabRow，对标 Material 3 PrimaryTabRow / SecondaryTabRow。
///
/// 用法：
/// ```ignore
/// TabRow::new(selected_index, |ctx| {
///     Tab::new(true, || {}).text("Tab 1").build(ctx);
///     Tab::new(false, || {}).text("Tab 2").build(ctx);
/// }).build(ctx);
/// ```
///
/// `follow_content_size` = true 时指示条宽度跟随内容（Primary 风格），
/// false 时指示条宽度 = tab 全宽（Secondary 风格）。
pub struct TabRow {
    selected_tab_index: usize,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    modifier: Modifier,
    container_color: Option<Color>,
    content_color: Option<Color>,
    follow_content_size: bool,
    divider_color: Option<Color>,
    indicator_color: Option<Color>,
    indicator_shape: Option<Shape>,
}

impl TabRow {
    /// 创建固定 TabRow。
    ///
    /// `content` 闭包内应逐个调用 `Tab::build`（每个 Tab 产生一个子节点）。
    /// TabRow 内部会追加 divider 与 indicator 两个子节点。
    pub fn new(
        selected_tab_index: usize,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            selected_tab_index,
            content: Box::new(content),
            modifier: Modifier::new(),
            container_color: None,
            content_color: None,
            follow_content_size: true, // 默认 Primary
            divider_color: None,
            indicator_color: None,
            indicator_shape: None,
        }
    }

    /// 设为 Secondary 风格（指示条全宽直角，内容色 OnSurface）。
    pub fn secondary(mut self) -> Self {
        self.follow_content_size = false;
        self.indicator_shape = Some(Shape::Rectangle);
        self
    }

    /// 容器背景色（默认 Surface）
    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }
    /// 内容色（默认 Primary 或 OnSurface）
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }
    /// 分隔线色（默认 OutlineVariant）
    pub fn divider_color(mut self, color: Color) -> Self {
        self.divider_color = Some(color);
        self
    }
    /// 指示条色（默认 Primary）
    pub fn indicator_color(mut self, color: Color) -> Self {
        self.indicator_color = Some(color);
        self
    }
    /// 指示条形状（默认 RoundedRect(3) 或 Rectangle）
    pub fn indicator_shape(mut self, shape: Shape) -> Self {
        self.indicator_shape = Some(shape);
        self
    }
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.selected_tab_index);
        ctx.changed(&self.follow_content_size);
        ctx.changed(&self.container_color);
        ctx.changed(&self.content_color);
        ctx.changed(&self.divider_color);
        ctx.changed(&self.indicator_color);
        ctx.changed(&self.indicator_shape);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);

        let container_color = self.container_color.unwrap_or_else(|| TabRowDefaults::container_color(&theme));
        let content_color = self.content_color.unwrap_or_else(|| {
            if self.follow_content_size { TabRowDefaults::primary_content_color(&theme) }
            else { TabRowDefaults::secondary_content_color(&theme) }
        });
        let divider_color = self.divider_color.unwrap_or_else(|| TabRowDefaults::divider_color(&theme));
        let indicator_color = self.indicator_color.unwrap_or_else(|| {
            if self.follow_content_size { TabRowDefaults::primary_indicator_color(&theme) }
            else { TabRowDefaults::secondary_indicator_color(&theme) }
        });
        let indicator_shape = self.indicator_shape.unwrap_or_else(|| {
            if self.follow_content_size { TabRowDefaults::primary_indicator_shape() }
            else { TabRowDefaults::secondary_indicator_shape() }
        });

        // 动画状态
        let offset_state = ctx.remember(|| 0.0f32);
        let width_state = ctx.remember(|| 0.0f32);
        // AtomicBool 非 Clone——用 Arc 包装在 State 中共享
        let initialized = ctx.remember(|| std::sync::Arc::new(AtomicBool::new(false))).get();
        // Arc<AtomicBool> 可 Clone，共享于 policy 与 build 之间

        let content = self.content;
        let policy = TabRowLayoutPolicy {
            selected_tab_index: self.selected_tab_index,
            follow_content_size: self.follow_content_size,
            offset_state: offset_state.clone(),
            width_state: width_state.clone(),
            initialized: initialized.clone(),
            direction,
        };

        // 根 modifier：背景色 + 用户 modifier
        let root_modifier = Modifier::new()
            .fill_max_width()
            .background(container_color, Shape::Rectangle)
            .then(self.modifier);

        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 内容色注入
                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    content(ctx);
                });
                // 分隔线 leaf
                let div_key = ctx.next_key();
                ctx.start_leaf(div_key, Modifier::new()
                    .fill_max_width()
                    .height(1.0)
                    .background(divider_color, Shape::Rectangle));
                ctx.end_node();
                // 指示条 leaf
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, Modifier::new()
                    .background(indicator_color, indicator_shape));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }
}

// ── TabRowLayoutPolicy ──

/// TabRow 布局：children = [tab0, tab1, ..., tabN-1, divider, indicator]。
/// 固定最后两个子节点为 divider 和 indicator。
#[derive(Debug)]
struct TabRowLayoutPolicy {
    selected_tab_index: usize,
    follow_content_size: bool,
    offset_state: State<f32>,
    width_state: State<f32>,
    initialized: std::sync::Arc<AtomicBool>,
    direction: LayoutDirection,
}

impl MeasurePolicy for TabRowLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let n = children.len();
        let tab_count = if n >= 2 { n - 2 } else { 0 };
        let row_width = constraints.max_width;
        let is_rtl = self.direction == LayoutDirection::Rtl;

        // 指示条高
        let indicator_h = ACTIVE_INDICATOR_HEIGHT;

        if tab_count == 0 {
            return (Size::new(row_width, 0.0), Vec::new());
        }

        let tab_width = row_width / tab_count as f32;

        // 1st pass: 自然高（loose 约束）
        let mut tab_row_height = 0.0f32;
        let mut natural_widths = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let (size, _) = measure_node(
                nodes, policies, children[i],
                Constraints::new(0.0, tab_width, 0.0, f32::MAX),
            );
            if size.height > tab_row_height { tab_row_height = size.height; }
            natural_widths.push(size.width);
        }

        // 2nd pass: tight 约束
        let mut placements = Vec::with_capacity(n);
        let mut positions = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let tight = Constraints::new(tab_width, tab_width, tab_row_height, tab_row_height);
            let (size, _) = measure_node(nodes, policies, children[i], tight);
            let x = if is_rtl {
                row_width - (i as f32 + 1.0) * tab_width
            } else {
                i as f32 * tab_width
            };
            let content_width = (natural_widths[i] - HORIZONTAL_TEXT_PADDING * 2.0)
                .max(MIN_INDICATOR_WIDTH);
            positions.push(TabPosition::new(x, tab_width, content_width));
            placements.push(Placement { size, position: Point::new(x, 0.0) });
        }

        // 分隔线
        let (div_size, _) = measure_node(
            nodes, policies, children[n - 2],
            Constraints::new(0.0, row_width, 0.0, f32::MAX),
        );
        placements.push(Placement {
            size: Size::new(row_width, div_size.height),
            position: Point::new(0.0, tab_row_height - div_size.height),
        });

        // 指示条
        let (target_offset, target_width) = if self.selected_tab_index < tab_count {
            let pos = &positions[self.selected_tab_index];
            let w = if self.follow_content_size { pos.content_width } else { pos.width };
            (pos.left, w)
        } else {
            (0.0, 0.0)
        };

        if !self.initialized.load(Ordering::Relaxed) {
            self.offset_state.set_silent(target_offset);
            self.width_state.set_silent(target_width);
            self.initialized.store(true, Ordering::Relaxed);
        } else {
            let spec = indicator_spring();
            crate::animation::push_animatable(self.offset_state.clone(), target_offset, spec.clone());
            crate::animation::push_animatable(self.width_state.clone(), target_width, spec);
        }

        // 读取动画值（注册 layout_deps——两段式依赖）
        let current_off = self.offset_state.get();
        let current_w = self.width_state.get();

        placements.push(Placement {
            size: Size::new(current_w, indicator_h),
            position: Point::new(current_off, tab_row_height - indicator_h),
        });

        (Size::new(row_width, tab_row_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }
}

// ── Tab 组件 ──

/// Material 3 Tab（对齐 Compose Tab.kt 通用版本）。
///
/// 用法：
/// ```ignore
/// Tab::new(selected, || { on_click() })
///     .text("Label")
///     .icon(|| Icon::new(icon_source).size(24.0).build(ctx))
///     .build(ctx);
/// ```
///
/// 或自定义 content：
/// ```ignore
/// Tab::new(selected, || { on_click() })
///     .content(|ctx| { Text::new("Tab").build(ctx); })
///     .build(ctx);
/// ```
pub struct Tab {
    selected: bool,
    on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    text: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    enabled: bool,
    selected_content_color: Option<Color>,
    unselected_content_color: Option<Color>,
    modifier: Modifier,
}

impl Tab {
    pub fn new(selected: bool, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            selected,
            on_click: Some(std::sync::Arc::new(on_click)),
            text: None,
            icon: None,
            content: None,
            enabled: true,
            selected_content_color: None,
            unselected_content_color: None,
            modifier: Modifier::new(),
        }
    }

    pub fn text(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.text = Some(Box::new(content));
        self
    }
    pub fn icon(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.icon = Some(Box::new(content));
        self
    }
    /// 自定义 content（替代 text/icon 槽）
    pub fn content(mut self, content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn selected_content_color(mut self, color: Color) -> Self {
        self.selected_content_color = Some(color);
        self
    }
    pub fn unselected_content_color(mut self, color: Color) -> Self {
        self.unselected_content_color = Some(color);
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
        ctx.changed(&self.selected_content_color);
        ctx.changed(&self.unselected_content_color);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let selected = self.selected;
        let enabled = self.enabled;

        // 颜色：默认取 contentColor（LocalContentColor.current）
        let content_color = WiniaTheme::content_color();
        let sel_color = self.selected_content_color.unwrap_or(content_color);
        let unsel_color = self.unselected_content_color.unwrap_or_else(|| {
            TabRowDefaults::unselected_content_color(&theme)
        });

        // 使用静态颜色（无动画——后续可加 graphics_layer 交叉淡化）
        let text_color = if selected { sel_color } else { unsel_color };

        let has_text = self.text.is_some();
        let has_icon = self.icon.is_some();
        let has_content = self.content.is_some();

        // 点击交互
        let interaction = ctx.remember(|| crate::ui::interaction::MutableInteractionSource::new()).get();
        let callback = self.on_click.clone();

        let mut item_modifier = Modifier::new();
        if enabled {
            if let Some(cb) = callback {
                item_modifier = item_modifier.clickable_with_source(&interaction, move || cb());
            }
        }
        // 全节点 ripple（bounded，使用选中色）
        let ripple_modifier = if enabled {
            Modifier::new().ripple_with_shape(&interaction, sel_color, true, Shape::Rectangle)
        } else {
            Modifier::new()
        };
        let item_modifier = item_modifier.then(self.modifier);

        // 自定义 content 版：直接包装点击+ripple
        if has_content {
            let content = self.content.unwrap();
            match ctx.start_restartable_group(key, item_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    content(ctx);
                    // ripple leaf
                    let ripple_key = ctx.next_key();
                    ctx.start_leaf(ripple_key, ripple_modifier);
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
            return;
        }

        // 默认 text/icon 版：TabLayoutPolicy
        let policy = TabLayoutPolicy { has_text, has_icon };

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 1) icon (if present)
                if let Some(icon) = self.icon {
                    let icon_key = ctx.next_key();
                    match ctx.start_restartable_group(icon_key, Modifier::new(), crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            WiniaTheme::with_content_color(text_color, ctx, icon);
                        }
                    }
                    ctx.end_restartable_group();
                }
                // 2) text (if present) — 水平填充 16dp
                if let Some(text) = self.text {
                    let text_key = ctx.next_key();
                    let mut style = TabRowDefaults::label_text_style();
                    style.color = Some(text_color);
                    let text_modifier = Modifier::new().padding_horizontal(HORIZONTAL_TEXT_PADDING);
                    match ctx.start_restartable_group(text_key, text_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            crate::ui::text::ProvideTextStyle(style, ctx, text);
                        }
                    }
                    ctx.end_restartable_group();
                }
                // 3) ripple leaf (full size, bounded)
                let ripple_key = ctx.next_key();
                ctx.start_leaf(ripple_key, ripple_modifier);
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }
}

// ── TabLayoutPolicy ──

/// Tab 内部布局：对标 Compose TabBaselineLayout。
/// children = [icon?, text?, ripple]。
///
/// 简化基线：
/// - text-only / icon-only ⇒ 垂直居中
/// - text + icon ⇒ 垂直居中排列（icon 上、text 下，居中）
///   （偏差：Compose 用 FirstBaseline/LastBaseline 精确偏移）
#[derive(Debug)]
struct TabLayoutPolicy {
    has_text: bool,
    has_icon: bool,
}

impl MeasurePolicy for TabLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let n = children.len();
        // 识别内容子节点：无 ripple（最后一个除外）
        let content_children = &children[..n - 1];
        let has_icon = self.has_icon;
        let has_text = self.has_text;

        // 测量内容节点（icon 和 text slots）
        let mut content_sizes = Vec::with_capacity(content_children.len());
        for &child in content_children {
            let (size, _) = measure_node(nodes, policies, child, constraints);
            content_sizes.push(size);
        }

        // tabWidth = max(content 宽度)
        let tab_width = content_sizes.iter().map(|s| s.width).fold(0.0f32, f32::max);
        let spec_height = if has_icon && has_text { LARGE_TAB_HEIGHT } else { SMALL_TAB_HEIGHT };
        let content_height: f32 = content_sizes.iter().map(|s| s.height).sum();
        let tab_height = spec_height.max(content_height + ICON_TEXT_SPACING);

        // 布局
        let mut placements = Vec::with_capacity(n);

        if has_icon && has_text && content_sizes.len() >= 2 {
            // text+icon：垂直居中，text 靠下
            let icon_h = content_sizes[0].height;
            let text_h = content_sizes[1].height;
            let total_h = icon_h + text_h;
            let start_y = (tab_height - total_h) / 2.0;
            placements.push(Placement {
                size: content_sizes[0],
                position: Point::new(0.0, start_y),
            });
            placements.push(Placement {
                size: content_sizes[1],
                position: Point::new(0.0, start_y + icon_h),
            });
        } else if has_icon || has_text {
            // 单元素垂直居中
            for &size in content_sizes.iter() {
                let y = (tab_height - size.height) / 2.0;
                placements.push(Placement {
                    size,
                    position: Point::new(0.0, y),
                });
            }
        } else {
            for &size in content_sizes.iter() {
                placements.push(Placement { size, position: Point::new(0.0, 0.0) });
            }
        }

        // ripple leaf：全尺寸覆盖
        placements.push(Placement {
            size: Size::new(tab_width, tab_height),
            position: Point::new(0.0, 0.0),
        });

        (Size::new(tab_width, tab_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::constraints::Constraints;
    use crate::ui::text::Text;

    fn tab_row_layout(
        selected: usize,
        follow_content: bool,
        count: usize,
        direction: LayoutDirection,
    ) -> Composer {
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, direction, ctx, |ctx| {
                let mut row = TabRow::new(selected, move |ctx| {
                    for i in 0..count {
                        let label = format!("Tab {}", i);
                        Tab::new(i == selected, || {})
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                });
                if !follow_content {
                    row = row.secondary();
                }
                row.build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        c
    }

    #[test]
    fn tab_row_creates_dividers_and_indicator() {
        let c = tab_row_layout(0, true, 3, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        assert_eq!(children.len(), 5, "should have 3 tabs + divider + indicator");
        // divider is at index 3 (1dp height)
        let div = &nodes[children[3]];
        assert_eq!(div.measured_size.height, 1.0);
        // indicator is at index 4
        let ind = &nodes[children[4]];
        assert_eq!(ind.measured_size.height, ACTIVE_INDICATOR_HEIGHT);
    }

    #[test]
    fn tab_row_tabs_equal_width() {
        let c = tab_row_layout(1, true, 4, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // tab0 = children[0], tab1 = children[1], tab2 = children[2], tab3 = children[3]
        for i in 0..4 {
            assert_eq!(nodes[children[i]].measured_size.width, 90.0, "tab {} width", i);
        }
    }

    #[test]
    fn tab_row_primary_indicator_position() {
        let c = tab_row_layout(1, true, 3, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        // selected=1, tabWidth=120.0, target_offset=120.0
        // target_width = contentWidth = max(natural_width - 32, 24)
        // natural_width of "Tab 1" text ≈ 65, so contentWidth ≈ 33
        assert_eq!(ind.position.x, 120.0, "indicator offset for tab 1");
        assert!(ind.measured_size.width >= 24.0, "indicator content width min 24");
        assert!(ind.measured_size.width < 120.0, "indicator content width < full tab");
    }

    #[test]
    fn tab_row_secondary_indicator_width() {
        let c = tab_row_layout(2, false, 4, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]];
        // selected=2, tabWidth=90.0, target_width=tabWidth (full)
        assert_eq!(ind.position.x, 180.0, "indicator offset for tab 2");
        assert_eq!(ind.measured_size.width, 90.0, "secondary indicator full width");
    }

    #[test]
    fn tab_row_rtl_mirror() {
        let ltr = tab_row_layout(0, true, 2, LayoutDirection::Ltr);
        let rtl = tab_row_layout(0, true, 2, LayoutDirection::Rtl);
        let ltr_root = ltr.layout_root_idx().unwrap();
        let rtl_root = rtl.layout_root_idx().unwrap();
        let ltr_nodes = ltr.arena_nodes();
        let rtl_nodes = rtl.arena_nodes();
        let ltr_children = &ltr_nodes[ltr_root].children;
        let rtl_children = &rtl_nodes[rtl_root].children;

        // LTR: tab0 at x=0, tab1 at x=180
        assert_eq!(ltr_nodes[ltr_children[0]].position.x, 0.0);
        assert_eq!(ltr_nodes[ltr_children[1]].position.x, 180.0);
        // RTL: tab0 at x=180, tab1 at x=0
        assert_eq!(rtl_nodes[rtl_children[0]].position.x, 180.0);
        assert_eq!(rtl_nodes[rtl_children[1]].position.x, 0.0);
    }

    #[test]
    fn tab_row_0_tabs_does_not_panic() {
        let mut c = Composer::new();
        c.compose(|ctx| {
            let colors = crate::ui::theme::ThemeColors::default_light();
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |_| {}).build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        // should not panic
    }

    #[test]
    fn tab_creates_content() {
        let mut c = Composer::new();
        c.compose(|ctx| {
            Tab::new(true, || {})
                .text(|ctx| Text::new("Tab").build(ctx))
                .build(ctx);
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        // Tab should have at least text + ripple
        assert!(nodes[root].children.len() >= 2, "Tab should have text + ripple children");
    }
}