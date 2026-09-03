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
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

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
/// LeadingIconTab 图标与文本间距（TextDistanceFromLeadingIcon = 8dp）
pub const LEADING_ICON_TEXT_SPACING: f32 = 8.0;
/// 可滚动 TabRow 最小 tab 宽（ScrollableTabRowMinTabWidth = 90dp）
pub const SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH: f32 = 90.0;
/// 可滚动 TabRow 起始边缘 padding（ScrollableTabRowEdgeStartPadding = 52dp）
pub const SCROLLABLE_TAB_ROW_EDGE_START_PADDING: f32 = 52.0;

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
    /// ⚠ 当前 Tab::build 的选中色走 `self.selected_content_color.unwrap_or(content_color)`
    ///（TabRow 注入的 contentColor），不经由此函数——保留供未来 scrollable/自定义使用。
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

        // ⚠ 两段式依赖必须在 measure **最开头** get()（任何 measure_node 子调用
        // 之前）——此刻 ACTIVE_SLOT_KEY 仍是本节点（TabRow），依赖注册到 TabRow
        // 而非尾部的 divider/indicator 叶（对齐 navigation_bar.rs 注释：递归测量
        // 子节点后 key 会被改写）。动画帧 → TabRow 重测 → 指示条位置更新。
        self.offset_state.get();
        self.width_state.get();

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
            // 指示器居中于 tab slot（对齐 M3 规范——scrollable 版本也显式居中）
            (pos.left + (pos.width - w) / 2.0, w)
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

        // 读动画当前值用于 placement（peek 不注册依赖——依赖已在 measure 开头
        // 通过 get() 注册到 TabRow 节点；peek 只读当前值，零注册开销）
        let current_off = self.offset_state.peek();
        let current_w = self.width_state.peek();

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
    /// LeadingIconTab 模式：icon 左 + text 右（水平排列，SmallTabHeight）
    leading: bool,
    /// 外部交互源注入（缺省内部创建）
    interaction_source: Option<crate::ui::interaction::MutableInteractionSource>,
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
            leading: false,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    /// 设为 LeadingIconTab 模式（icon 左 + text 右，水平排列 SmallTabHeight）。
    pub fn leading_icon(mut self) -> Self {
        self.leading = true;
        self
    }

    /// 注入外部交互源（缺省内部创建），用于外部观察 pressed/hover/focus 状态。
    pub fn interaction_source(mut self, source: crate::ui::interaction::MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
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
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        let selected = self.selected;
        let enabled = self.enabled;

        // 颜色：默认取 contentColor（LocalContentColor.current）
        let content_color = WiniaTheme::content_color();
        let sel_color = self.selected_content_color.unwrap_or(content_color);
        let unsel_color = self.unselected_content_color.unwrap_or_else(|| {
            TabRowDefaults::unselected_content_color(&theme)
        });

        // 颜色过渡动画（对标 Compose TabTransition animateColor）：动画目标色
        // State<Color> + graphics_layer(color_filter: Tint) 渲染期染色——
        // 单层 text/icon 按插值色绘制，无需双文本交叉淡化。
        let color_anim = ctx.animate_color_as_state(
            if selected { sel_color } else { unsel_color },
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
                damping_ratio: 1.0,
                stiffness: 300.0,
                mass: 1.0,
                threshold: 0.01,
            }),
        );

        let has_text = self.text.is_some();
        let has_icon = self.icon.is_some();
        let has_content = self.content.is_some();
        let leading = self.leading;

        // 点击交互：优先用外部注入的交互源（须由调用方 remember 创建——
        // 与 Compose interactionSource 参数语义一致），缺省内部创建
        let interaction = match self.interaction_source.clone() {
            Some(src) => src,
            None => ctx.remember(|| crate::ui::interaction::MutableInteractionSource::new()).get(),
        };
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

        // 自定义 content 版：直接包装点击+ripple（颜色动画由用户 content 自理）
        if has_content {
            let content = self.content.unwrap();
            match ctx.start_restartable_group(key, item_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    content(ctx);
                    // ripple leaf（fill_max_size 确保全尺寸覆盖——BoxLayout
                    // Center 测子节点用 loosen 约束，无尺寸则塌缩 0×0）
                    let ripple_key = ctx.next_key();
                    ctx.start_leaf(ripple_key, ripple_modifier.fill_max_size());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
            return;
        }

        // 动态颜色 tint（渲染期 peek 零重组；layout_deps 由 TabLayoutPolicy.measure
        // 读 color_anim.get() 注册——动画帧重测 Tab 节点 → 重绘 → 新色生效）
        let tint_for_icon = color_anim.clone();
        let icon_tint_modifier = Modifier::new().graphics_layer(move || crate::modifier::GraphicsLayerParams {
            color_filter: Some(crate::modifier::ColorFilter::Tint {
                color: tint_for_icon.peek(),
                blend_mode: crate::modifier::BlendMode::SrcIn,
            }),
            ..crate::modifier::GraphicsLayerParams::default()
        });
        let tint_for_text = color_anim.clone();
        let text_tint_modifier = Modifier::new().graphics_layer(move || crate::modifier::GraphicsLayerParams {
            color_filter: Some(crate::modifier::ColorFilter::Tint {
                color: tint_for_text.peek(),
                blend_mode: crate::modifier::BlendMode::SrcIn,
            }),
            ..crate::modifier::GraphicsLayerParams::default()
        });

        // 默认 text/icon 版：TabLayoutPolicy
        let policy = TabLayoutPolicy { has_text, has_icon, leading, direction, color_anim: color_anim.clone() };

        match ctx.start_restartable_group(key, item_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 1) icon (if present)——Tint 染色后无需 with_content_color 再注入
                if let Some(icon) = self.icon {
                    let icon_key = ctx.next_key();
                    match ctx.start_restartable_group(icon_key, icon_tint_modifier, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            icon(ctx);
                        }
                    }
                    ctx.end_restartable_group();
                }
                // 2) text (if present) — 非 leading 水平填充 16dp；leading 模式
                // 不加（间距由 LEADING_ICON_TEXT_SPACING 精确控制——加了会把
                // icon-text 间隙撑成 8+16）。颜色由 tint 统一驱动
                if let Some(text) = self.text {
                    let text_key = ctx.next_key();
                    let style = TabRowDefaults::label_text_style();
                    let mut text_modifier = Modifier::new().then(text_tint_modifier);
                    if !leading {
                        text_modifier = text_modifier.padding_horizontal(HORIZONTAL_TEXT_PADDING);
                    }
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
    /// LeadingIconTab 模式：icon 左 + text 右
    leading: bool,
    /// 布局方向（RTL 时镜像 leading 排列）
    direction: LayoutDirection,
    /// 颜色动画 State——measure 开头 get() 注册 layout_dep，动画帧重测
    /// Tab 节点 → 触发重绘 → graphics_layer color_filter peek 新色
    color_anim: State<crate::modifier::Color>,
}

impl MeasurePolicy for TabLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // ⚠ 必须最先读颜色动画 State——此刻 ACTIVE_SLOT_KEY 仍是本节点（Tab，
        // measure_node 进入时设置）；递归测量子节点后 key 会被改写。注册
        // layout_dep → 动画帧只重测本节点（不重组）→ 重绘 → tint peek 新色。
        self.color_anim.get();
        let n = children.len();
        // 识别内容子节点：无 ripple（最后一个除外）
        let content_children = &children[..n - 1];
        let has_icon = self.has_icon;
        let has_text = self.has_text;
        let leading = self.leading;

        // ⚠ 用 loose 约束测量内容：TabRow 传 tight（等分宽 × 行高），若直接
        // 用 tight 测 slot，icon/text slot 各自被撑到行高（如 72），content_height
        // 求和翻倍 → tab_height 溢出（72+72+20=164），ripple 覆盖多余区域且
        // icon/text 间距失真。loose 约束拿到内容自然尺寸，再钳制回 incoming。
        let loose = Constraints::new(0.0, constraints.max_width, 0.0, f32::MAX);
        let mut content_sizes = Vec::with_capacity(content_children.len());
        for &child in content_children {
            let (size, _) = measure_node(nodes, policies, child, loose);
            content_sizes.push(size);
        }

        // 自然尺寸
        let tab_width_natural = if leading && content_sizes.len() >= 2 {
            // LeadingIconTab：icon + 8dp + text 水平排列——自然宽 = 三者之和
            content_sizes.iter().map(|s| s.width).sum::<f32>() + LEADING_ICON_TEXT_SPACING
        } else {
            content_sizes.iter().map(|s| s.width).fold(0.0f32, f32::max)
        };
        let spec_height = if leading {
            SMALL_TAB_HEIGHT
        } else if has_icon && has_text { LARGE_TAB_HEIGHT } else { SMALL_TAB_HEIGHT };
        let content_height: f32 = if leading {
            // leading 垂直取 max（水平排列不叠加高度）
            content_sizes.iter().map(|s| s.height).fold(0.0f32, f32::max)
        } else {
            content_sizes.iter().map(|s| s.height).sum()
        };
        let tab_height_natural = if leading {
            spec_height.max(content_height)
        } else {
            spec_height.max(content_height + ICON_TEXT_SPACING)
        };

        // 钳制到 incoming 约束：TabRow 传 tight（tabWidth × rowHeight）——
        // tab 必须填满分配的 slot（ripple 覆盖整个 tab 区域，对齐 Compose
        // selectable + fillMaxWidth）；父约束松时保持自然尺寸。
        let tab_width = tab_width_natural.clamp(constraints.min_width, constraints.max_width);
        let tab_height = tab_height_natural.clamp(constraints.min_height, constraints.max_height);

        // 布局
        let mut placements = Vec::with_capacity(n);

        if leading && has_icon && has_text && content_sizes.len() >= 2 {
            // LeadingIconTab：icon 左 + 8dp + text 右，整组水平居中、垂直居中
            // ⚠ RTL 镜像：icon 移右侧、text 移左侧（Compose Row 在 RTL 下
            // 自动镜像子节点顺序——物理排列反转）
            let icon_size = content_sizes[0];
            let text_size = content_sizes[1];
            let total_w = icon_size.width + LEADING_ICON_TEXT_SPACING + text_size.width;
            let start_x = (tab_width - total_w) / 2.0;
            let icon_y = (tab_height - icon_size.height) / 2.0;
            let text_y = (tab_height - text_size.height) / 2.0;
            let is_rtl = self.direction == LayoutDirection::Rtl;
            let icon_x = if is_rtl {
                start_x + text_size.width + LEADING_ICON_TEXT_SPACING
            } else {
                start_x
            };
            let text_x = if is_rtl {
                start_x
            } else {
                start_x + icon_size.width + LEADING_ICON_TEXT_SPACING
            };
            placements.push(Placement {
                size: icon_size,
                position: Point::new(icon_x, icon_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Point::new(text_x, text_y),
            });
        } else if has_icon && has_text && content_sizes.len() >= 2 {
            // text+icon：icon 上、text 下，垂直居中排列，均水平居中
            let icon_size = content_sizes[0];
            let text_size = content_sizes[1];
            let total_h = icon_size.height + text_size.height;
            let start_y = (tab_height - total_h) / 2.0;
            placements.push(Placement {
                size: icon_size,
                position: Point::new((tab_width - icon_size.width) / 2.0, start_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Point::new((tab_width - text_size.width) / 2.0, start_y + icon_size.height),
            });
        } else if has_icon || has_text {
            // 单元素：水平 + 垂直居中
            for &size in content_sizes.iter() {
                let y = (tab_height - size.height) / 2.0;
                placements.push(Placement {
                    size,
                    position: Point::new((tab_width - size.width) / 2.0, y),
                });
            }
        } else {
            for &size in content_sizes.iter() {
                placements.push(Placement { size, position: Point::new(0.0, 0.0) });
            }
        }

        // ripple leaf：全尺寸覆盖（填满整个 tab slot）
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

// ── ScrollableTabRow ──

/// 可滚动 TabRow（对标 Compose PrimaryScrollableTabRow / SecondaryScrollableTabRow）。
///
/// 与固定 `TabRow` 不同：tabs 按内容自然宽（最小 `min_tab_width`）排列，
/// 超出视口可横向滚动；选中 tab 变化时自动滚动使其居中（对齐 Compose
/// `ScrollableTabData.calculateTabOffset`）。
///
/// 用法：
/// ```ignore
/// let state = ctx.remember(|| ScrollState::new()).get();
/// ScrollableTabRow::new(selected_index, |ctx| {
///     Tab::new(...).text(...).build(ctx);
/// }).scroll_state(state).build(ctx);
/// ```
pub struct ScrollableTabRow {
    selected_tab_index: usize,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    modifier: Modifier,
    scroll_state: Option<crate::modifier::ScrollState>,
    container_color: Option<Color>,
    content_color: Option<Color>,
    follow_content_size: bool,
    divider_color: Option<Color>,
    indicator_color: Option<Color>,
    indicator_shape: Option<Shape>,
    edge_padding: f32,
    min_tab_width: f32,
}

/// 可滚动 TabRow 默认值（对齐 Compose `TabRowDefaults`）。
pub struct ScrollableTabRowDefaults;

impl ScrollableTabRowDefaults {
    /// 最小 tab 宽（`ScrollableTabRowMinTabWidth = 90dp`）
    pub fn min_tab_width() -> f32 { SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH }
    /// 起始边缘 padding（`ScrollableTabRowEdgeStartPadding = 52dp`）
    pub fn edge_start_padding() -> f32 { SCROLLABLE_TAB_ROW_EDGE_START_PADDING }
}

impl ScrollableTabRow {
    pub fn new(
        selected_tab_index: usize,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            selected_tab_index,
            content: Box::new(content),
            modifier: Modifier::new(),
            scroll_state: None,
            container_color: None,
            content_color: None,
            follow_content_size: true, // 默认 Primary
            divider_color: None,
            indicator_color: None,
            indicator_shape: None,
            edge_padding: SCROLLABLE_TAB_ROW_EDGE_START_PADDING,
            min_tab_width: SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH,
        }
    }

    /// 设为 Secondary 风格（指示条全宽直角，内容色 OnSurface）。
    pub fn secondary(mut self) -> Self {
        self.follow_content_size = false;
        self.indicator_shape = Some(Shape::Rectangle);
        self
    }

    /// 滚动状态（缺省时内部 remember 创建）
    pub fn scroll_state(mut self, state: crate::modifier::ScrollState) -> Self {
        self.scroll_state = Some(state);
        self
    }

    /// 起始边缘 padding（默认 52dp）
    pub fn edge_padding(mut self, padding: f32) -> Self {
        self.edge_padding = padding;
        self
    }

    /// 最小 tab 宽（默认 90dp）
    pub fn min_tab_width(mut self, width: f32) -> Self {
        self.min_tab_width = width;
        self
    }

    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }
    pub fn divider_color(mut self, color: Color) -> Self {
        self.divider_color = Some(color);
        self
    }
    pub fn indicator_color(mut self, color: Color) -> Self {
        self.indicator_color = Some(color);
        self
    }
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
        ctx.changed(&self.edge_padding);
        ctx.changed(&self.min_tab_width);
        ctx.changed(&self.container_color);
        ctx.changed(&self.content_color);
        ctx.changed(&self.divider_color);
        ctx.changed(&self.indicator_color);
        ctx.changed(&self.indicator_shape);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        let is_rtl = direction == LayoutDirection::Rtl;

        let scroll_state = self.scroll_state.clone()
            .unwrap_or_else(|| ctx.remember(|| crate::modifier::ScrollState::new()).get());

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

        // 动画状态（同固定 TabRow）
        let offset_state = ctx.remember(|| 0.0f32);
        let width_state = ctx.remember(|| 0.0f32);
        let initialized = ctx.remember(|| std::sync::Arc::new(AtomicBool::new(false))).get();
        // ScrollableTabData：上次 selected（跨帧记住——动画触发依据）
        let last_selected = ctx.remember(|| std::sync::Arc::new(AtomicI32::new(-1))).get();
        // 上次 direction（0=LTR 1=RTL）——direction 变化时 offset 语义镜像翻转，
        // 必须重置 last_selected 强制下帧重新居中（用户实测：切方向后选中不居中）
        let last_dir = ctx.remember(|| std::sync::Arc::new(AtomicI32::new(-1))).get();
        let dir_code = if is_rtl { 1 } else { 0 };
        if last_dir.load(Ordering::Relaxed) != dir_code {
            last_dir.store(dir_code, Ordering::Relaxed);
            last_selected.store(-1, Ordering::Relaxed);
        }
        // 首帧标记：fling_limit 首帧未回写——延迟到第二帧再触发居中滚动
        let layout_seen = ctx.remember(|| false);

        let content = self.content;
        let policy = ScrollableTabRowLayoutPolicy {
            selected_tab_index: self.selected_tab_index,
            follow_content_size: self.follow_content_size,
            offset_state: offset_state.clone(),
            width_state: width_state.clone(),
            initialized: initialized.clone(),
            direction,
            edge_padding: self.edge_padding,
            min_tab_width: self.min_tab_width,
            scroll_state: scroll_state.clone(),
            last_selected,
            layout_seen: layout_seen.clone(),
            divider_color,
            indicator_color,
            indicator_shape,
        };

        // 根 modifier：横向滚动容器 + 背景色（background 覆盖视口——滚动内容
        // 在其上平移，对齐 Compose Surface 包 ScrollableTabRow 语义）。
        // ⚠ RTL：滚动容器标记 reverse（render 平移镜像——offset 0 显示内容
        // 末端），tab 位置在 policy 内镜像（最右为第一个 tab）。
        let root_modifier = Modifier::new()
            .fill_max_width()
            .horizontal_scroll(scroll_state)
            .horizontal_scroll_reverse(is_rtl)
            .background(container_color, Shape::Rectangle)
            .then(self.modifier);

        match ctx.start_restartable_group(key, root_modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    content(ctx);
                });
                // 分隔线 leaf（内容全宽——随滚动平移）
                let div_key = ctx.next_key();
                ctx.start_leaf(div_key, Modifier::new()
                    .fill_max_width()
                    .height(1.0)
                    .background(divider_color, Shape::Rectangle));
                ctx.end_node();
                // 指示条 leaf（内容坐标系——随滚动平移）
                let ind_key = ctx.next_key();
                ctx.start_leaf(ind_key, Modifier::new()
                    .background(indicator_color, indicator_shape));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }
}

/// ScrollableTabRow 布局：children = [tab0, ..., tabN-1, divider, indicator]。
///
/// tabs 按内容自然宽（≥ min_tab_width）从左排列；layoutWidth =
/// 2×edgePadding + ΣtabW；divider 全宽贴底；indicator 居中于选中 tab slot。
/// ScrollableTabData 等价逻辑：selected 变化 → 计算居中 offset → 动画滚动。
#[derive(Debug)]
struct ScrollableTabRowLayoutPolicy {
    selected_tab_index: usize,
    follow_content_size: bool,
    offset_state: State<f32>,
    width_state: State<f32>,
    initialized: std::sync::Arc<AtomicBool>,
    direction: LayoutDirection,
    edge_padding: f32,
    min_tab_width: f32,
    scroll_state: crate::modifier::ScrollState,
    /// 上次选中的 tab 索引（-1 = 首帧）——选中变化触发居中滚动
    last_selected: std::sync::Arc<AtomicI32>,
    /// 首帧标记：policy.measure 内 fling_limit 尚未回写（滚动容器在 measure 后
    /// 才写 fling_limit），首帧不应消费 last_selected。set(true) notify → 下帧
    /// 重测 → fling_limit 就绪 → 正确居中滚动。
    layout_seen: State<bool>,
    divider_color: Color,
    indicator_color: Color,
    indicator_shape: Shape,
}

impl MeasurePolicy for ScrollableTabRowLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 两段式依赖：动画值 get() 注册到本节点（滚动容器）——动画帧重测本节点
        self.offset_state.get();
        self.width_state.get();

        let n = children.len();
        let tab_count = if n >= 2 { n - 2 } else { 0 };
        let indicator_h = ACTIVE_INDICATOR_HEIGHT;

        if tab_count == 0 {
            return (Size::new(0.0, 0.0), Vec::new());
        }

        // 1st pass：自然高（loose 约束——scroll 容器子内容无限宽）
        let mut layout_height = 0.0f32;
        let mut natural_widths = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let (size, _) = measure_node(
                nodes, policies, children[i],
                Constraints::new(0.0, f32::MAX, 0.0, f32::MAX),
            );
            if size.height > layout_height { layout_height = size.height; }
            // 自然宽（不 clamp——contentWidth 用 min(intrinsic, placedWidth)）
            natural_widths.push(size.width);
        }

        // 2nd pass：tight 高度（minHeight=maxHeight=layoutHeight）、自然宽 ≥ minTabWidth
        // 先测量所有 tab 收集宽度（RTL 需要总宽才能镜像放置）
        let mut tab_measurements: Vec<(f32, Size, f32)> = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let c = Constraints::new(self.min_tab_width, f32::MAX, layout_height, layout_height);
            let (size, _) = measure_node(nodes, policies, children[i], c);
            let width = size.width.max(self.min_tab_width);
            let content_width = (natural_widths[i].min(width) - HORIZONTAL_TEXT_PADDING * 2.0)
                .max(MIN_INDICATOR_WIDTH);
            tab_measurements.push((width, size, content_width));
        }
        let layout_width = 2.0 * self.edge_padding
            + tab_measurements.iter().map(|(w, _, _)| w).sum::<f32>();

        // 放置 tab：LTR 从左往右（edge_padding 起），RTL 从右往左（镜像）
        let is_rtl = self.direction == LayoutDirection::Rtl;
        let mut placements = Vec::with_capacity(n);
        let mut positions = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let (width, size, content_width) = tab_measurements[i];
            let left = if is_rtl {
                layout_width - self.edge_padding
                    - tab_measurements[..=i].iter().map(|(w, _, _)| w).sum::<f32>()
            } else {
                self.edge_padding
                    + tab_measurements[..i].iter().map(|(w, _, _)| w).sum::<f32>()
            };
            positions.push(TabPosition::new(left, width, content_width));
            placements.push(Placement { size, position: Point::new(left, 0.0) });
        }

        // 分隔线（内容全宽）
        let (div_size, _) = measure_node(
            nodes, policies, children[n - 2],
            Constraints::new(0.0, layout_width, 0.0, f32::MAX),
        );
        placements.push(Placement {
            size: Size::new(layout_width, div_size.height),
            position: Point::new(0.0, layout_height - div_size.height),
        });

        // 指示条（与固定版同：居中于 slot，双 State 动画）
        let (target_offset, target_width) = if self.selected_tab_index < tab_count {
            let pos = &positions[self.selected_tab_index];
            let w = if self.follow_content_size { pos.content_width } else { pos.width };
            (pos.left + (pos.width - w) / 2.0, w)
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

        let current_off = self.offset_state.peek();
        let current_w = self.width_state.peek();

        placements.push(Placement {
            size: Size::new(current_w, indicator_h),
            position: Point::new(current_off, layout_height - indicator_h),
        });

        // ── ScrollableTabData：选中变化 → 居中滚动（对齐 Compose calculateTabOffset）──
        let sel = self.selected_tab_index as i32;
        // ⚠ 首帧延迟：policy.measure 执行时滚动容器的 fling_limit 尚未回写
        //（measure_node 在 policy.measure **之后** 才写 fling_limit，node.rs:1535-1551），
        // 首帧 max_value=0 → available=0 → target 恒 0 → 不会居中。因此首帧只
        // 标记 layout_seen（notify → 下帧重测）而不消费 last_selected，第二帧
        // fling_limit 就绪后再触发居中滚动。
        if !self.layout_seen.get() {
            self.layout_seen.set(true);
        } else if self.last_selected.load(Ordering::Relaxed) != sel {
            self.last_selected.store(sel, Ordering::Relaxed);
            if let Some(pos) = positions.get(self.selected_tab_index) {
                // 可见宽 = 内容总宽 - 最大滚动量（fling_limit 由滚动容器布局回写）
                let total_w = layout_width;
                let max_value = self.scroll_state.fling_limit.get().max(0.0);
                let visible = (total_w - max_value).max(0.0);
                // 居中：把 tab 中心对齐视口中心，clamp 到 [0, availableSpace]
                let scroller_center = visible / 2.0;
                let centered = pos.left - (scroller_center - pos.width / 2.0);
                let available = (total_w - visible).max(0.0);
                // ⚠ RTL：容器标记 scroll_reverse——render 平移镜像（offset 0 = 内容末端），
                // target 需绕 available 镜像：offset_rtl = available - offset_ltr
                let target = if is_rtl {
                    (available - centered).clamp(0.0, available)
                } else {
                    centered.clamp(0.0, available)
                };
                let spec = indicator_spring();
                self.scroll_state.animate_scroll_to(target, max_value, spec);
            }
        }

        (Size::new(layout_width, layout_height), placements)
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
        // selected=1, tabWidth=120.0, contentWidth≈33 (natural≈65-32)
        // 居中后: offset = 120 + (120-33)/2 = 163.5
        let tab_width = 120.0;
        let content_width = 33.0; // natural width - 32, min 24
        let expected_x = tab_width + (tab_width - content_width) / 2.0;
        assert!((ind.position.x - expected_x).abs() < 1.0, "indicator x={} expected≈{}", ind.position.x, expected_x);
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

    #[test]
    fn tab_leading_icon_lays_out_horizontally() {
        // LeadingIconTab：icon 左 + 8dp + text 右（水平排列），整体水平居中
        use crate::ui::icon::{Icon, IconSource};
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                Tab::new(true, || {})
                    .leading_icon()
                    .modifier(Modifier::new().size(180.0, 48.0))
                    .icon(|ctx| {
                        Icon::new(IconSource::svg(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143ZM233-120l65-281L80-590l288-25 112-265 112 265 288 25-218 189 65 281-247-149-247 149Zm247-350Z"/></svg>"#,
                        ))
                        .size(24.0)
                        .build(ctx);
                    })
                    .text(|ctx| Text::new("Favorites").build(ctx))
                    .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [icon, text, ripple]
        assert_eq!(children.len(), 3, "leading tab = icon + text + ripple");
        let icon = &nodes[children[0]];
        let text = &nodes[children[1]];
        let ripple = &nodes[children[2]];
        // icon 在 text 左侧（非 leading 是 icon 上 text 下）
        assert!(icon.position.x < text.position.x, "icon.x={} 应 < text.x={}", icon.position.x, text.position.x);
        // 垂直方向对齐：各自在 slot 内垂直居中（icon 24px → y=12，text 20px →
        // y=14——差异来自尺寸不同）。关键判据：text 顶不落于 icon 底之下
        // （非 leading 竖排时 text.y 远大于 icon.y + icon.h）
        assert!(text.position.y < icon.position.y + icon.measured_size.height + 2.0,
            "leading icon/text 应同一水平带：icon.y={} h={} text.y={}",
            icon.position.y, icon.measured_size.height, text.position.y);
        // 间距 = LEADING_ICON_TEXT_SPACING（icon 右缘到 text 左缘）
        let gap = text.position.x - (icon.position.x + icon.measured_size.width);
        assert!((gap - LEADING_ICON_TEXT_SPACING).abs() < 1.0, "icon-text 间距应=8，实际 {gap}");
        // 整组水平居中 + ripple 覆盖全 tab
        assert!(icon.position.x > 0.0, "leading 内容应居中（icon.x={}）", icon.position.x);
        assert_eq!(ripple.measured_size.width, nodes[root].measured_size.width);
    }

    #[test]
    fn tab_leading_icon_rtl_mirrors_icon_to_right() {
        // RTL 回归：LeadingIconTab 在 RTL 下 icon 应移右侧、text 移左侧
        //（用户实测：Tab Three 切 RTL 后不镜像——Tab 内部布局此前不感知方向）
        use crate::ui::icon::{Icon, IconSource};
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Rtl, ctx, |ctx| {
                Tab::new(true, || {})
                    .leading_icon()
                    .modifier(Modifier::new().size(180.0, 48.0))
                    .icon(|ctx| {
                        Icon::new(IconSource::svg(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143Z"/></svg>"#,
                        ))
                        .size(24.0)
                        .build(ctx);
                    })
                    .text(|ctx| Text::new("Favorites").build(ctx))
                    .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let icon = &nodes[children[0]];
        let text = &nodes[children[1]];
        // RTL：icon 在 text 右侧（LTR 是 icon 在左）
        assert!(icon.position.x > text.position.x,
            "RTL leading icon.x={} 应 > text.x={}（icon 移右）",
            icon.position.x, text.position.x);
        // 间距保持 8dp（text 右缘到 icon 左缘）
        let gap = icon.position.x - (text.position.x + text.measured_size.width);
        assert!((gap - LEADING_ICON_TEXT_SPACING).abs() < 1.0, "RTL icon-text 间距应=8，实际 {gap}");
        // 同一水平带
        assert!(text.position.y < icon.position.y + icon.measured_size.height + 2.0);
    }

    #[test]
    fn tab_accepts_injected_interaction_source() {
        // interaction_source 注入：外部 remember 创建的源应被使用
        //（不 panic，渲染结构同内部创建路径）
        use crate::ui::interaction::MutableInteractionSource;
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        let src: MutableInteractionSource = {
            let mut out = None;
            c.compose(|ctx| {
                let s = ctx.remember(|| MutableInteractionSource::new()).get();
                out = Some(s);
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let s = out.clone().unwrap();
                    Tab::new(true, || {})
                        .interaction_source(s)
                        .text(|ctx| Text::new("Tab").build(ctx))
                        .build(ctx);
                });
            });
            out.unwrap()
        };
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        assert!(nodes[root].children.len() >= 2, "injected-source tab should build");
        let _ = src;
    }

    #[test]
    fn tab_content_version_ripple_covers_full_tab() {
        // P0-1 回归：自定义 content 版 ripple 必须全尺寸覆盖 tab slot
        // （BoxLayout(Center) 测子节点用 loosen——无尺寸则塌缩 0×0）
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |ctx| {
                    Tab::new(true, || {})
                        .content(|ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(40.0, 20.0));
                            ctx.end_node();
                        })
                        .build(ctx);
                })
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, divider, indicator]
        let tab = &nodes[children[0]];
        let tab_children = &tab.children;
        // tab children = [content, ripple]
        assert_eq!(tab_children.len(), 2, "content 版应有 content + ripple");
        let ripple = &nodes[tab_children[1]];
        // content 版无 spec 高——tab 高度 = 内容高（20），宽度 = 整行（1 tab → 360）
        assert_eq!(ripple.measured_size.width, 360.0, "ripple 应全宽覆盖 tab slot");
        assert_eq!(ripple.measured_size.height, 20.0, "ripple 应全高覆盖 tab slot");
    }

    #[test]
    fn tab_row_selected_out_of_range_falls_back_to_origin() {
        // selected >= tab_count 时指示条归位 (0,0)，不 panic
        let c = tab_row_layout(99, true, 2, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[3]]; // [tab0, tab1, divider, indicator]
        assert_eq!(ind.position.x, 0.0, "越界 selected 指示条 x=0");
        assert_eq!(ind.measured_size.width, 0.0, "越界 selected 指示条宽 0");
    }

    #[test]
    fn tab_row_indicator_animates_on_selected_change() {
        // 动画推进：selected 0→1 → 步进 update_animations → offset 单调变化并收敛到新 target
        use crate::core::state::State;
        use std::time::{Duration, Instant};
        // 动画注册表全局共享——持串行锁防并行 clear/竞态（与 lazy_column/animated_size 同）
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            colors: crate::ui::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let sel = selected.clone();
                    TabRow::new(sel.get(), move |ctx| {
                        for i in 0..3 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            Tab::new(sel2.get() == i, move || {})
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let colors = crate::ui::theme::ThemeColors::default_light();

        let mut c = Composer::new();
        c.compose(make_scene(selected.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        // 首帧：initialized=false → set_silent 直接到位（tab0，居中 ≈ 43.5）
        {
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            let ind = &nodes[children[4]]; // [tab0..2, divider, indicator]
            let x0 = ind.position.x;
            assert!(x0 > 0.0 && x0 < 120.0, "首帧指示条应在 tab0 内，x={x0}");
        }

        // 切到 tab1：recompose + layout → push_animatable 分支启动动画
        selected.set(1);
        assert!(c.recompose(make_scene(selected.clone(), colors)), "状态变化应触发重组");
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let tab_width = 120.0;
        let content_width = 33.0; // natural≈65-32
        let target = tab_width + (tab_width - content_width) / 2.0; // ≈163.5

        // 首帧动画刚启动——位置应开始离开 tab0 方向（不要求立即到位）
        let deadline = Instant::now() + Duration::from_millis(2000);
        let mut last_x = {
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            nodes[children[4]].position.x
        };
        let mut converged = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            let x = nodes[children[4]].position.x;
            // 单调逼近 + 收敛到 target ±1
            if (x - last_x).abs() < 0.5 && (x - target).abs() < 1.0 {
                converged = true;
                break;
            }
            last_x = x;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(converged, "指示条应动画收敛到 {target}，实际 {last_x}");
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        assert!(
            (ind.position.x - target).abs() < 1.0,
            "最终指示条 x={} 应≈{}",
            ind.position.x,
            target
        );
    }

    #[test]
    fn tab_row_rtl_indicator_mirrors() {
        // RTL：selected=0 指示条应镜像到最右侧 tab（x=180，2 tabs）
        let c = tab_row_layout(0, true, 2, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[3]]; // [tab0, tab1, divider, indicator]
        // tab0 在 RTL 下 x=180；指示条居中于该 slot
        let tab_width = 180.0;
        // contentWidth ≈ 35（natural≈67-32），居中偏移 = 180 + (180-35)/2 = 252.5
        assert!(
            (ind.position.x - 252.5).abs() < 1.0,
            "RTL 指示条 x={} 应≈252.5",
            ind.position.x
        );
    }

    // ── ScrollableTabRow ──

    fn scrollable_tab_row_layout(
        selected: usize,
        count: usize,
    ) -> (Composer, crate::modifier::ScrollState) {
        scrollable_tab_row_layout_dir(selected, count, LayoutDirection::Ltr)
    }

    fn scrollable_tab_row_layout_dir(
        selected: usize,
        count: usize,
        direction: LayoutDirection,
    ) -> (Composer, crate::modifier::ScrollState) {
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        let state = crate::modifier::ScrollState::new();
        let st = state.clone();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, direction, ctx, |ctx| {
                let state_outer = st.clone();
                let state_inner = st.clone();
                ScrollableTabRow::new(selected, move |ctx| {
                    let state3 = state_inner.clone();
                    for i in 0..count {
                        let label = format!("Tab {}", i);
                        let sel = i == selected;
                        let st4 = state3.clone();
                        Tab::new(sel, move || { let _ = st4; })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                })
                .scroll_state(state_outer)
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        (c, state)
    }

    #[test]
    fn scrollable_tab_row_lays_out_natural_widths_with_padding() {
        let (c, _) = scrollable_tab_row_layout(0, 3);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        assert_eq!(children.len(), 5);
        // 每个 tab ≥ minTabWidth(90)，从 edgePadding(52) 起排
        let tab0 = &nodes[children[0]];
        let tab1 = &nodes[children[1]];
        assert!(tab0.position.x >= SCROLLABLE_TAB_ROW_EDGE_START_PADDING - 0.01, "tab0 x={}", tab0.position.x);
        assert!(tab0.measured_size.width >= 90.0 - 0.01, "tab0 宽={}", tab0.measured_size.width);
        assert!(
            (tab1.position.x - (tab0.position.x + tab0.measured_size.width)).abs() < 0.01,
            "tab1 紧接 tab0：tab0.x={} tab0.w={} tab1.x={}",
            tab0.position.x, tab0.measured_size.width, tab1.position.x
        );
    }

    #[test]
    fn scrollable_tab_row_indicator_centered_on_selected() {
        let (c, _) = scrollable_tab_row_layout(1, 4);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]]; // [tab0..3, divider, indicator]
        // tab1 起点 = 52 + w0；指示条居中于该 slot
        let tab1 = &nodes[children[1]];
        let expected = tab1.position.x + tab1.measured_size.width / 2.0;
        // 指示条中心 = x + width/2；Primary followContent → width < tab width
        let ind_center = ind.position.x + ind.measured_size.width / 2.0;
        assert!(
            (ind_center - expected).abs() < 1.0,
            "指示条应居中于 tab1（中心 {} vs tab1 中心 {}）",
            ind_center,
            expected
        );
    }

    #[test]
    fn scrollable_tab_row_scrolls_selected_into_view() {
        // 选中变化 → ScrollableTabData 触发居中滚动（offset 动画推进）
        use crate::core::state::State;
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            scroll_state: crate::modifier::ScrollState,
            colors: crate::ui::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let sel = selected.clone();
                    let st_outer = scroll_state.clone();
                    let st_inner = scroll_state.clone();
                    ScrollableTabRow::new(sel.get(), move |ctx| {
                        let st3 = st_inner.clone();
                        for i in 0..10 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            let st4 = st3.clone();
                            Tab::new(sel2.get() == i, move || { let _ = st4; })
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .scroll_state(st_outer)
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        let scroll_state = crate::modifier::ScrollState::new();

        c.compose(make_scene(selected.clone(), scroll_state.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let off0 = scroll_state.offset.get();
        assert_eq!(off0, 0.0, "首帧 offset 0");

        // 切到最后一个 tab → 居中滚动（动画推进）
        selected.set(9);
        assert!(c.recompose(make_scene(selected.clone(), scroll_state.clone(), colors)));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "选中变化应触发滚动动画（offset 单调增加）");
        // 内容总宽 = 2*52 + ΣtabW（10 个 ≥90）→ 远超视口 360，应滚到末尾附近
        assert!(last > 200.0, "应滚到末尾附近（offset={last}）");
    }

    #[test]
    fn scrollable_tab_row_centers_initial_selection() {
        // 回归：一开始 selected=9（10 tabs）——首帧 fling_limit 未回写，
        // 居中滚动应延迟到第二帧执行（此前首帧 target 恒 0 永不居中）
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let (mut c, scroll_state) = scrollable_tab_row_layout(9, 10);
        // 首帧布局：layout_seen=false → 不滚动（此时 fling_limit 尚为 0）
        assert_eq!(scroll_state.offset.get(), 0.0, "首帧不应滚动");
        // 第二帧：fling_limit 已回写 → 触发居中滚动
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "初始选中 9 应触发居中滚动（第二帧起）");
        // 选中末尾 tab：内容总宽 > 视口 → offset 应滚到末尾附近（> 200）
        assert!(last > 200.0, "应滚到末尾附近（offset={last}）");
    }

    #[test]
    fn scrollable_tab_row_rtl_mirrors_tab_positions() {
        // RTL：tabs 从右往左排（tab0 在最右），滚动容器标记 scroll_reverse
        let (c, _) = scrollable_tab_row_layout_dir(0, 3, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        // scroll_reverse 已由 node.rs 从 modifier 提取（render 平移镜像）
        assert!(nodes[root].scroll_reverse, "RTL 滚动容器应标记 scroll_reverse");
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        let tab0 = &nodes[children[0]];
        let tab1 = &nodes[children[1]];
        let tab2 = &nodes[children[2]];
        // 内容总宽 = 2*52 + Σw；tab0 在最右（x 最大）
        assert!(tab0.position.x > tab1.position.x, "tab0.x={} 应 > tab1.x={}（RTL 镜像）",
            tab0.position.x, tab1.position.x);
        assert!(tab1.position.x > tab2.position.x, "tab1.x={} 应 > tab2.x={}",
            tab1.position.x, tab2.position.x);
        // 右边缘 = 总宽 - edge_padding（tab0 右端对齐内容末端）
        let total = tab0.position.x + tab0.measured_size.width;
        assert!((total + SCROLLABLE_TAB_ROW_EDGE_START_PADDING - (c.arena_nodes()[children[3]].position.x + c.arena_nodes()[children[3]].measured_size.width)).abs() < 0.01,
            "tab0 右端应贴内容末端（总宽={total}）");
        // 左边缘 = edge_padding（tab2 左端对齐内容起点）
        assert!((tab2.position.x - SCROLLABLE_TAB_ROW_EDGE_START_PADDING).abs() < 0.01,
            "tab2.x={} 应 = edge_padding", tab2.position.x);
    }

    #[test]
    fn scrollable_tab_row_rtl_indicator_centered_on_selected() {
        // RTL：selected=1（中间 tab）→ 指示条居中于镜像后的 tab1 slot
        let (c, _) = scrollable_tab_row_layout_dir(1, 4, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]]; // [tab0..3, divider, indicator]
        let tab1 = &nodes[children[1]];
        let expected = tab1.position.x + tab1.measured_size.width / 2.0;
        let ind_center = ind.position.x + ind.measured_size.width / 2.0;
        assert!(
            (ind_center - expected).abs() < 1.0,
            "RTL 指示条应居中于 tab1（中心 {} vs tab1 中心 {}）",
            ind_center,
            expected
        );
    }

    #[test]
    fn scrollable_tab_row_rtl_scrolls_selected_into_view() {
        // RTL：选中变化 → 居中滚动（target 绕 available 镜像：
        // offset_rtl = available - offset_ltr——首帧不滚、第二帧起动画推进）
        use crate::core::state::State;
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            scroll_state: crate::modifier::ScrollState,
            colors: crate::ui::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Rtl, ctx, |ctx| {
                    let sel = selected.clone();
                    let st_outer = scroll_state.clone();
                    let st_inner = scroll_state.clone();
                    ScrollableTabRow::new(sel.get(), move |ctx| {
                        let st3 = st_inner.clone();
                        for i in 0..10 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            let st4 = st3.clone();
                            Tab::new(sel2.get() == i, move || { let _ = st4; })
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .scroll_state(st_outer)
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let mut c = Composer::new();
        let colors = crate::ui::theme::ThemeColors::default_light();
        let scroll_state = crate::modifier::ScrollState::new();

        c.compose(make_scene(selected.clone(), scroll_state.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let off0 = scroll_state.offset.get();
        assert_eq!(off0, 0.0, "首帧 offset 0");

        // 切到最后一个 tab → RTL 镜像居中滚动（动画推进）
        selected.set(9);
        assert!(c.recompose(make_scene(selected.clone(), scroll_state.clone(), colors)));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "RTL 选中变化应触发镜像居中滚动");
        // 内容总宽 = 2*52 + ΣtabW（10 个 ≥90）→ 远超视口 360
        // tab9 物理最左（x≈52）→ LTR centered = c - vw/2 < 0 → clamp 0；
        // RTL target = available - centered ≈ available（滚到末尾附近）
        assert!(last > 200.0, "RTL 应滚到末尾附近（offset={last}）");
    }
}