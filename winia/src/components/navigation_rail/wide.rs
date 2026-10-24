use super::*;

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
    pub(crate) expanded: State<bool>,
    /// 组件 build 挂载的变形进度（0=收起 1=展开）——current/is_animating 的数据源
    pub(crate) progress: std::sync::Arc<parking_lot::Mutex<Option<State<f32>>>>,
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
pub(crate) struct WideNavigationRailLayoutPolicy {
    pub(crate) progress: State<f32>,
    /// children[0] 是否为 header 槽（FAB/logo）——header 水平位置随展开态迁移
    pub(crate) has_header: bool,
    /// Whether the header's leading indent hangs off the RIGHT edge. Captured by `build` at compose
    /// time: placement runs after composition, where the direction local is gone. Only the header is
    /// mirrored — an item sits at `(width - w) / 2`, which is symmetric and so unchanged by mirroring.
    pub(crate) rtl: bool,
    /// Where the stack sits vertically (`Start` / `Center` / `End`).
    pub(crate) arrangement: Arrangement,
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
    pub(crate) state: WideNavigationRailState,
    pub(crate) content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) header: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    pub(crate) container_color: Option<Color>,
    /// Container shape. `Shape::Rectangle` unless overridden — the modal form passes the modal container
    /// shape (`NavigationRailExpandedTokens.ModalContainerShape`, CornerLarge) through this.
    pub(crate) container_shape: Shape,
    /// `arrangement`: where the header + items sit vertically in the rail. Upstream's
    /// `Arrangement.Vertical` (`WideNavigationRail.kt`), which only offers Top / Center / Bottom —
    /// in winia's shared enum that is `Start` / `Center` / `End`, and the space-distributing variants
    /// behave like `Start` (the rail's items are a fixed stack, not a set to spread).
    pub(crate) arrangement: Arrangement,
    pub(crate) modifier: Modifier,
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
    pub(crate) selected: bool,
    pub(crate) on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) label: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) enabled: bool,
    pub(crate) colors: Option<NavigationRailItemColors>,
    pub(crate) interaction_source: Option<MutableInteractionSource>,
    /// rail 的展开进度（0=收起 Top 布局，1=展开 Start 布局）——由容器传入
    pub(crate) progress: Option<State<f32>>,
    /// 图标位置覆盖（对齐 androidx iconPosition——None = 跟随容器变形进度）
    pub(crate) icon_position: Option<crate::components::navigation_bar::NavigationItemIconPosition>,
    pub(crate) modifier: Modifier,
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
    pub(crate) position_progress: State<f32>,
    pub(crate) size_progress: State<f32>,
    /// 图标位置覆盖（Some = 钉死 Top(0.0)/Start(1.0)，不读进度——静态 item
    /// 不因容器动画帧重测；None = 跟随 position_progress）
    pub(crate) position_override: Option<f32>,
    /// RTL: the Start endpoint measures its indent off the LEADING edge (androidx names the token
    /// `FullWidthLeadingSpace`, and a leading value resolves against the direction — 16 is not "left",
    /// it is "leading"). The direction is captured by `build` at compose time because placement runs
    /// after composition, where `direction::current()` is no longer the placement-time truth.
    pub(crate) rtl: bool,
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
        let max_h = if constraints.max_height < f32::MAX { constraints.max_height } else { f32::MAX };

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
