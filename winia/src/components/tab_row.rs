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
//! - contentWidth = max(intrinsic tab width - 32dp, 24dp)（`maxIntrinsicWidth` 走框架的固有尺寸
//!   协议：固定版先按 slot 宽问 max intrinsic height、再按该高问 max intrinsic width，TabRow.kt:450-459；
//!   可滚动版按 `Constraints.Infinity` 问两者，TabRow.kt:582-603）
//! - 动画 spec = spring(damping_ratio=0.6, stiffness=700)（对齐 M3 Expressive DefaultSpatial）
//! - 指示条高固定 3dp（ActiveIndicatorHeight）
//! - 无 TabIndicatorScope 自定义指示器 API（当前内部固定）

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::unit::{Offset, Size};
use crate::layout::node::{
    intrinsic_size_of, measure_node, IntrinsicCtx, IntrinsicQuery, LayoutNode, MeasurePolicy,
    Placement,
};
use crate::layout::LayoutDirection;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::text::TextAlign;
use crate::theme::WiniaTheme;
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

/// The indicator's slide, which M3 runs on the `DefaultSpatial` token
/// (`material3/TabRow.kt:406`: `MotionSchemeKeyTokens.DefaultSpatial.value<Dp>()`) — damping 0.9,
/// stiffness 700 (`tokens/StandardMotionTokens.kt:20-21`). winia used damping 0.6, which bounced more
/// than M3's motion does.
fn indicator_spring() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
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
    pub fn container_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.surface
    }

    /// Primary 内容色（PrimaryNavigationTabTokens.ActiveLabelTextColor = Primary）
    pub fn primary_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 内容色（SecondaryNavigationTabTokens.ActiveLabelTextColor = OnSurface）
    pub fn secondary_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface
    }

    /// 分隔线色（DividerTokens.Color = OutlineVariant）
    pub fn divider_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.outline_variant
    }

    /// Primary 指示条色（PrimaryNavigationTabTokens.ActiveIndicatorColor = Primary）
    pub fn primary_indicator_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 指示条色（Same as Primary 但无 shape）
    pub fn secondary_indicator_color(theme: &crate::theme::ThemeColors) -> Color {
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
    pub fn selected_content_color(theme: &crate::theme::ThemeColors, is_primary: bool) -> Color {
        if is_primary { theme.primary } else { theme.on_surface }
    }

    /// 未选中 Tab 内容色（InactiveLabelTextColor = OnSurfaceVariant）
    pub fn unselected_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface_variant
    }

    /// Tab 文本样式（LabelTextFont = TitleSmall = 14.0/20.0）
    pub fn label_text_style() -> crate::text::TextStyle {
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
    /// A caller-supplied indicator (Compose's `indicator` slot). Composed DURING measurement with the
    /// positions this row just computed, which is the only place they exist — see `TabIndicatorScope`.
    /// `Fn` behind an `Arc`, not `FnOnce`: every measurement runs it (a later frame measures again),
    /// the same shape `BoxWithConstraints`' content has.
    indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
}

/// What a custom indicator gets to know: where every tab ended up, and which one is selected.
///
/// Compose's `TabRow` hands its indicator a `TabIndicatorScope` with a `tabPositions` list, and those
/// positions only exist once the row has been measured — which is why the indicator is composed inside
/// measurement (`layout::subcompose`) instead of by the caller during composition.
#[derive(Debug, Clone)]
pub struct TabIndicatorScope {
    positions: Vec<TabPosition>,
    selected_index: usize,
}

impl TabIndicatorScope {
    /// Every tab's position, in tab order.
    pub fn tab_positions(&self) -> &[TabPosition] {
        &self.positions
    }

    /// The selected tab's index, clamped into range.
    pub fn selected_index(&self) -> usize {
        self.selected_index.min(self.positions.len().saturating_sub(1))
    }

    /// The selected tab's position, if there is a tab at all.
    pub fn selected_position(&self) -> Option<&TabPosition> {
        self.positions.get(self.selected_index())
    }
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
            indicator_slot: None,
        }
    }

    /// Supply the indicator yourself (Compose's `indicator` parameter).
    ///
    /// The closure runs DURING measurement, with the positions the row computed for this frame
    /// (`scope.tab_positions()`), so it can draw anything the positions allow — Compose's own
    /// `SecondaryIndicator`, a custom bar, a set of marks. It is composed into the row's layout, so
    /// whatever it emits is measured and painted with the row; returning nothing is allowed (an
    /// indicator that only draws, e.g. through `Modifier::draw`).
    ///
    /// Without this, the row's own indicator (a bar over the selected tab, animated) is what is drawn.
    pub fn indicator(
        mut self,
        indicator: impl Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync + 'static,
    ) -> Self {
        self.indicator_slot = Some(std::sync::Arc::new(indicator));
        self
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
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
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
            indicator_slot: self.indicator_slot,
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
struct TabRowLayoutPolicy {
    selected_tab_index: usize,
    follow_content_size: bool,
    offset_state: State<f32>,
    width_state: State<f32>,
    initialized: std::sync::Arc<AtomicBool>,
    direction: LayoutDirection,
    /// The caller's indicator, when there is one: composed at measure time with the positions
    /// (see `TabRow::indicator`).
    indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
}

/// Manual, not derived: the indicator slot is a closure, which has no `Debug`.
impl std::fmt::Debug for TabRowLayoutPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabRowLayoutPolicy")
            .field("selected_tab_index", &self.selected_tab_index)
            .field("follow_content_size", &self.follow_content_size)
            .field("has_indicator_slot", &self.indicator_slot.is_some())
            .finish()
    }
}

impl MeasurePolicy for TabRowLayoutPolicy {
    /// The row can compose a caller-supplied indicator during measurement, so its node has to be
    /// re-measured rather than folded (see the trait method's contract).
    fn subcomposes(&self) -> bool {
        self.indicator_slot.is_some()
    }

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

        // 1st pass: Compose's intrinsic fold (TabRow.kt:450-459). The row is as tall as the tallest
        // tab's MAX intrinsic height AT the width every tab gets, and a tab's content width is its
        // MAX intrinsic width AT that height — then clamped back into the slot. No tab is measured
        // here: `intrinsic_size_of` inverts each tab's own modifier chain, so a tab whose content
        // prices itself (a weighted child, a fixed-size slot) answers without a measurement pass.
        let mut tab_row_height = 0.0f32;
        let mut natural_widths = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let h = intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxHeight, tab_width,
            );
            if h > tab_row_height { tab_row_height = h; }
        }
        for i in 0..tab_count {
            natural_widths.push(intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxWidth, tab_row_height,
            ));
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
            let content_width = (natural_widths[i].min(tab_width) - HORIZONTAL_TEXT_PADDING * 2.0)
                .max(MIN_INDICATOR_WIDTH);
            positions.push(TabPosition::new(x, tab_width, content_width));
            placements.push(Placement { size, position: Offset::new(x, 0.0) });
        }

        // 分隔线
        let (div_size, _) = measure_node(
            nodes, policies, children[n - 2],
            Constraints::new(0.0, row_width, 0.0, f32::MAX),
        );
        placements.push(Placement {
            size: Size::new(row_width, div_size.height),
            position: Offset::new(0.0, tab_row_height - div_size.height),
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

        if let Some(slot) = &self.indicator_slot {
            // The caller's indicator, composed HERE because this is the only place the positions exist.
            // It replaces the row's own bar (Compose's `indicator` parameter does the same), and it is
            // laid out in the row's own coordinate space, so `scope.selected_position()` is a position
            // in the row rather than an offset for the row's bar.
            //
            // BEFORE the row's own indicator bookkeeping on purpose: those two states drive a spring, and
            // skipping their `set` while still pushing the animation every frame leaves that spring
            // chasing a target it never receives — measured as one value animation alive for 312 of 356
            // loop iterations (`[anim] animating=true values=1`, id `StateId(15)`), which kept the window
            // repainting at 60 fps with nothing changing.
            let scope = TabIndicatorScope {
                positions: positions.clone(),
                selected_index: self.selected_tab_index,
            };
            // `subcompose_overlay`, not `subcompose`: an indicator is drawn ON the row, it does not
            // define the row's size (the tabs do). With the size-defining variant the row's own node
            // became the indicator's box and the whole row stopped painting.
            crate::layout::subcompose::subcompose_overlay(
                Constraints::new(0.0, row_width, 0.0, tab_row_height),
                // The scope is CLONED per call: `subcompose` runs its content as `Fn`, and each call
                // needs its own copy (a Vec of a few floats per measurement).
                |ctx| slot(ctx, scope.clone()),
            );
            // The default bar's node still exists (the children list is [tabs.., divider, indicator]),
            // so give it a ZERO area: without a placement it would keep the previous frame's geometry
            // and paint on top of the caller's indicator.
            placements.push(Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(0.0, tab_row_height),
            });
            return (Size::new(row_width, tab_row_height), placements);
        }

        // The row's OWN indicator: initialise on the first measurement, animate afterwards.
        if !self.initialized.load(Ordering::Relaxed) {
            self.offset_state.set(target_offset);
            self.width_state.set(target_width);
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
            position: Offset::new(current_off, tab_row_height - indicator_h),
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

    // ── Intrinsic measurement: folded from the tabs, never by running `measure` ──
    //
    // `measure` derives the indicator's target from `constraints.max_width`, starts a spring toward
    // it, and consumes the row's first-measurement flag. The trait's defaults answer an intrinsic by
    // running exactly that measure with the queried axis UNBOUNDED (`node.rs:1003-1051`; Compose's own
    // default, which is why Compose's animating nodes extend
    // `LayoutModifierNodeWithPassThroughIntrinsics`, `AnimationModifier.kt:259-280`). Measured before
    // these overrides: an ancestor asking for width intrinsics handed the row `max_width = f32::MAX`,
    // the indicator's target became `Infinity`, and the bar was placed at `NaN`.
    //
    // Both folds repeat the first pass of `measure` (`TabRow.kt:450-459`) with the indicator
    // bookkeeping left out, so the numbers are the same ones the row itself uses.

    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        tabs_intrinsic_width(ctx, children, IntrinsicQuery::MinWidth, height)
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        tabs_intrinsic_width(ctx, children, IntrinsicQuery::MaxWidth, height)
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        tabs_intrinsic_height(ctx, children, width)
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        tabs_intrinsic_height(ctx, children, width)
    }
}

/// The spec a `Tab` animates its content colour with, following Compose's `TabTransition`
/// (`material3/Tab.kt:270-291`): a `updateTransition(selected)` whose `animateColor` picks
/// `DefaultEffects` when `false isTransitioningTo true` — i.e. while the tab is ARRIVING at the
/// selected colour — and `FastEffects` otherwise, so leaving the selected state is the quicker move.
/// (`Tab.kt` has no `animateColorAsState`; it is this transition that carries the direction.)
///
/// Kept as a function so both directions are assertable: a change back to one spring for both, or to a
/// tween, fails a test rather than only changing how a tab feels.
fn tab_colour_spec(arriving: bool) -> crate::animation::SpringSpec {
    if arriving {
        crate::animation::SpringSpec::effects_default()
    } else {
        crate::animation::SpringSpec::effects_fast()
    }
}

/// TabRow children are `[tabs.., divider, indicator]`; the divider and the indicator are placed
/// inside the row's height, so neither prices it.
fn tab_count_of(children: &[usize]) -> usize {
    children.len().saturating_sub(2)
}

/// A fixed `TabRow` gives every tab an equal slot (`tab_width = row_width / tab_count`), so the row's
/// natural width is `tab_count` slots as wide as the widest tab's content wants to be.
fn tabs_intrinsic_width(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    query: IntrinsicQuery,
    height: f32,
) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    let widest = (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], query, height))
        .fold(0.0, f32::max);
    widest * tab_count as f32
}

/// The row is as tall as the TALLEST tab's max intrinsic height AT the width every tab gets — the
/// measure's own first pass.
fn tabs_intrinsic_height(ctx: &mut IntrinsicCtx<'_>, children: &[usize], width: f32) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    let tab_width = if width >= f32::MAX {
        f32::MAX
    } else {
        width / tab_count as f32
    };
    (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], IntrinsicQuery::MaxHeight, tab_width))
        .fold(0.0, f32::max)
}

/// A scrollable strip asks every tab at `Constraints.Infinity` — width and height — so its height is
/// the tallest tab's max intrinsic height there.
fn strip_intrinsic_height(ctx: &mut IntrinsicCtx<'_>, children: &[usize]) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], IntrinsicQuery::MaxHeight, f32::MAX))
        .fold(0.0, f32::max)
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
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
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
    pub fn interaction_source(mut self, source: crate::interaction::MutableInteractionSource) -> Self {
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
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let selected = self.selected;
        let enabled = self.enabled;

        // 颜色：默认取 contentColor（LocalContentColor.current）
        let content_color = WiniaTheme::content_color();
        let sel_color = self.selected_content_color.unwrap_or(content_color);
        let unsel_color = self.unselected_content_color.unwrap_or_else(|| {
            TabRowDefaults::unselected_content_color(&theme)
        });

        // The content colour, animated per Compose's `TabTransition` — see `tab_colour_spec` for the
        // reference and why the direction decides the spec. The remembered `was_selected` is what says
        // which side of that transition this frame is on.
        let was_selected = ctx.remember(|| selected);
        let arriving = selected && !was_selected.peek();
        was_selected.set(selected);
        let color_spec = crate::animation::AnimationSpec::Spring(tab_colour_spec(arriving));
        let color_anim = ctx.animate_color_as_state(if selected { sel_color } else { unsel_color }, color_spec);

        let has_text = self.text.is_some();
        let has_icon = self.icon.is_some();
        let has_content = self.content.is_some();
        let leading = self.leading;

        // 点击交互：优先用外部注入的交互源（须由调用方 remember 创建——
        // 与 Compose interactionSource 参数语义一致），缺省内部创建
        let interaction = match self.interaction_source.clone() {
            Some(src) => src,
            None => ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get(),
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
        item_modifier = item_modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Tab)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new().selected(self.selected).enabled(enabled)),
        );
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
        let icon_tint_modifier = Modifier::new().graphics_layer(move || crate::graphics::GraphicsLayerParams {
            color_filter: Some(crate::graphics::ColorFilter::Tint {
                color: tint_for_icon.peek(),
                blend_mode: crate::graphics::BlendMode::SrcIn,
            }),
            ..crate::graphics::GraphicsLayerParams::default()
        });
        let tint_for_text = color_anim.clone();
        let text_tint_modifier = Modifier::new().graphics_layer(move || crate::graphics::GraphicsLayerParams {
            color_filter: Some(crate::graphics::ColorFilter::Tint {
                color: tint_for_text.peek(),
                blend_mode: crate::graphics::BlendMode::SrcIn,
            }),
            ..crate::graphics::GraphicsLayerParams::default()
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
                            crate::components::text::ProvideTextStyle(style, ctx, text);
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
    color_anim: State<crate::graphics::Color>,
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
                position: Offset::new(icon_x, icon_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Offset::new(text_x, text_y),
            });
        } else if has_icon && has_text && content_sizes.len() >= 2 {
            // text+icon：icon 上、text 下，垂直居中排列，均水平居中
            let icon_size = content_sizes[0];
            let text_size = content_sizes[1];
            let total_h = icon_size.height + text_size.height;
            let start_y = (tab_height - total_h) / 2.0;
            placements.push(Placement {
                size: icon_size,
                position: Offset::new((tab_width - icon_size.width) / 2.0, start_y),
            });
            placements.push(Placement {
                size: text_size,
                position: Offset::new((tab_width - text_size.width) / 2.0, start_y + icon_size.height),
            });
        } else if has_icon || has_text {
            // 单元素：水平 + 垂直居中
            for &size in content_sizes.iter() {
                let y = (tab_height - size.height) / 2.0;
                placements.push(Placement {
                    size,
                    position: Offset::new((tab_width - size.width) / 2.0, y),
                });
            }
        } else {
            for &size in content_sizes.iter() {
                placements.push(Placement { size, position: Offset::new(0.0, 0.0) });
            }
        }

        // ripple leaf：全尺寸覆盖（填满整个 tab slot）
        placements.push(Placement {
            size: Size::new(tab_width, tab_height),
            position: Offset::new(0.0, 0.0),
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
    /// A caller-supplied indicator (see [`ScrollableTabRow::indicator`]). Like the fixed variant's,
    /// it is composed DURING measurement, because the positions are measure-time data.
    indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
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
            indicator_slot: None,
        }
    }

    /// Supply the indicator yourself — the same contract as [`TabRow::indicator`] (Compose's
    /// `indicator` parameter), with one difference the scrollable variant owns: the positions are in
    /// the SCROLLING CONTENT's coordinate space, so an indicator built from them scrolls with the tabs
    /// (which is what Compose's scrollable indicator does).
    pub fn indicator(
        mut self,
        indicator: impl Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync + 'static,
    ) -> Self {
        self.indicator_slot = Some(std::sync::Arc::new(indicator));
        self
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
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
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
            indicator_slot: self.indicator_slot,
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
    /// See [`ScrollableTabRow::indicator`]: composed at measure time with the positions.
    indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
}

/// Manual, not derived: the indicator slot is a closure, which has no `Debug`.
impl std::fmt::Debug for ScrollableTabRowLayoutPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollableTabRowLayoutPolicy")
            .field("selected_tab_index", &self.selected_tab_index)
            .field("has_indicator_slot", &self.indicator_slot.is_some())
            .finish()
    }
}

impl MeasurePolicy for ScrollableTabRowLayoutPolicy {
    /// The row can compose a caller-supplied indicator during measurement (see the trait method's
    /// contract and `ScrollableTabRow::indicator`).
    fn subcomposes(&self) -> bool {
        self.indicator_slot.is_some()
    }

    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        _constraints: Constraints,
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

        // 1st pass: Compose's intrinsic fold for the scrollable variant (TabRow.kt:582-600). The
        // content sits in a horizontal scroll out there, so both axes are unbounded at the query:
        // `maxIntrinsicHeight(Constraints.Infinity)` for the height and `maxIntrinsicWidth(
        // Constraints.Infinity)` for a tab's natural width — the latter is also what the indicator's
        // contentWidth is measured from, "based on incoming content size, not on forced minimum
        // width" (TabRow.kt:596-600). No tab is measured in this pass.
        let mut layout_height = 0.0f32;
        let mut natural_widths = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let h = intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxHeight, f32::MAX,
            );
            if h > layout_height { layout_height = h; }
        }
        for i in 0..tab_count {
            natural_widths.push(intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxWidth, f32::MAX,
            ));
        }

        // 2nd pass：tight 高度（minHeight=maxHeight=layoutHeight）、自然宽 ≥ minTabWidth
        // 先测量所有 tab 收集宽度（RTL 需要总宽才能镜像放置）
        let mut tab_measurements: Vec<(f32, Size, f32)> = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let c = Constraints::new(self.min_tab_width, f32::MAX, layout_height, layout_height);
            let (size, _) = measure_node(nodes, policies, children[i], c);
            let width = size.width.max(self.min_tab_width);
            // Compose subtracts the horizontal padding from the RAW intrinsic width and floors it at
            // the 24dp touch target (TabRow.kt:601-603) — the minimum width a tab is forced to does
            // NOT shrink the indicator's content box, so this is the one place that does not clamp to
            // the placed width.
            let content_width = (natural_widths[i] - HORIZONTAL_TEXT_PADDING * 2.0)
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
            placements.push(Placement { size, position: Offset::new(left, 0.0) });
        }

        // 分隔线（内容全宽）
        let (div_size, _) = measure_node(
            nodes, policies, children[n - 2],
            Constraints::new(0.0, layout_width, 0.0, f32::MAX),
        );
        placements.push(Placement {
            size: Size::new(layout_width, div_size.height),
            position: Offset::new(0.0, layout_height - div_size.height),
        });

        // 指示条（与固定版同：居中于 slot，双 State 动画）
        let (target_offset, target_width) = if self.selected_tab_index < tab_count {
            let pos = &positions[self.selected_tab_index];
            let w = if self.follow_content_size { pos.content_width } else { pos.width };
            (pos.left + (pos.width - w) / 2.0, w)
        } else {
            (0.0, 0.0)
        };

        if let Some(slot) = &self.indicator_slot {
            // The caller's indicator, composed with the positions this measurement produced. Those
            // positions are in the SCROLLING CONTENT's space, so what the caller builds scrolls with
            // the tabs — Compose's scrollable indicator behaves the same way.
            let scope = TabIndicatorScope {
                positions: positions.clone(),
                selected_index: self.selected_tab_index,
            };
            // Overlay-shaped here too: the tabs define the row's size, not the indicator.
            crate::layout::subcompose::subcompose_overlay(
                Constraints::new(0.0, layout_width, 0.0, layout_height),
                |ctx| slot(ctx, scope.clone()),
            );
            // The default bar's node still exists (the children list is [tabs.., divider, indicator]),
            // so give it a ZERO area: without a placement it would keep the previous frame's geometry
            // and paint on top of the caller's indicator.
            placements.push(Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(0.0, layout_height),
            });
            // Scroll-into-view still runs: the selected tab is centred whether or not the caller draws
            // the bar (it is the one that needs the offsets recorded by the measure).
            self.scroll_selected_into_view(&positions, layout_width, is_rtl);
            return (Size::new(layout_width, layout_height), placements);
        }

        if !self.initialized.load(Ordering::Relaxed) {
            self.offset_state.set(target_offset);
            self.width_state.set(target_width);
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
            position: Offset::new(current_off, layout_height - indicator_h),
        });

        self.scroll_selected_into_view(&positions, layout_width, is_rtl);

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

    // ── Intrinsic measurement: the strip's own arithmetic, never `measure` ──
    //
    // Same reason as the fixed row above: `measure` starts the indicator spring from
    // `constraints`-derived geometry and consumes the first-measurement flag, so the trait's default
    // (which runs `measure` with the queried axis unbounded) turns an intrinsic query into state.
    // The strip is a horizontal scroll of natural-width tabs, so both folds repeat the measure's own
    // first pass (`TabRow.kt:582-603`): every tab asked at `Constraints.Infinity`, heights folded by
    // max, widths summed with the minimum touch-target width this row forces (`min_tab_width`).

    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        _height: f32,
    ) -> f32 {
        self.strip_intrinsic_width(ctx, children)
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        _height: f32,
    ) -> f32 {
        self.strip_intrinsic_width(ctx, children)
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        _width: f32,
    ) -> f32 {
        strip_intrinsic_height(ctx, children)
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        _width: f32,
    ) -> f32 {
        strip_intrinsic_height(ctx, children)
    }
}

impl ScrollableTabRowLayoutPolicy {
    /// The strip's natural width: the tabs side by side at their natural widths, each forced up to
    /// the row's minimum touch-target width, plus the row's edge padding — the `layout_width` its
    /// measure computes.
    fn strip_intrinsic_width(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize]) -> f32 {
        let tab_count = tab_count_of(children);
        if tab_count == 0 {
            return 0.0;
        }
        let tabs: f32 = (0..tab_count)
            .map(|i| {
                ctx.child_intrinsic(children[i], IntrinsicQuery::MaxWidth, f32::MAX)
                    .max(self.min_tab_width)
            })
            .sum();
        2.0 * self.edge_padding + tabs
    }

    /// ScrollableTabData：选中变化 → 居中滚动（对齐 Compose calculateTabOffset）。
    ///
    /// Extracted from `measure` because a caller-supplied indicator takes the same early-return path
    /// and must still get this: the selected tab is centred whether or not the caller draws the bar.
    fn scroll_selected_into_view(&self, positions: &[TabPosition], layout_width: f32, is_rtl: bool) {
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
    }
}

// ── 测试 ──


#[cfg(test)]
mod tests;
