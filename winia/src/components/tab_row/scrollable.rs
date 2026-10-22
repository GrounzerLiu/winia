use super::*;

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
    pub(crate) selected_tab_index: usize,
    pub(crate) content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) modifier: Modifier,
    pub(crate) scroll_state: Option<crate::modifier::ScrollState>,
    pub(crate) container_color: Option<Color>,
    pub(crate) content_color: Option<Color>,
    pub(crate) follow_content_size: bool,
    pub(crate) divider_color: Option<Color>,
    pub(crate) indicator_color: Option<Color>,
    pub(crate) indicator_shape: Option<Shape>,
    pub(crate) edge_padding: f32,
    pub(crate) min_tab_width: f32,
    /// A caller-supplied indicator (see [`ScrollableTabRow::indicator`]). Like the fixed variant's,
    /// it is composed DURING measurement, because the positions are measure-time data.
    pub(crate) indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
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
    pub(crate) selected_tab_index: usize,
    pub(crate) follow_content_size: bool,
    pub(crate) offset_state: State<f32>,
    pub(crate) width_state: State<f32>,
    pub(crate) initialized: std::sync::Arc<AtomicBool>,
    pub(crate) direction: LayoutDirection,
    pub(crate) edge_padding: f32,
    pub(crate) min_tab_width: f32,
    pub(crate) scroll_state: crate::modifier::ScrollState,
    /// 上次选中的 tab 索引（-1 = 首帧）——选中变化触发居中滚动
    pub(crate) last_selected: std::sync::Arc<AtomicI32>,
    /// 首帧标记：policy.measure 内 fling_limit 尚未回写（滚动容器在 measure 后
    /// 才写 fling_limit），首帧不应消费 last_selected。set(true) notify → 下帧
    /// 重测 → fling_limit 就绪 → 正确居中滚动。
    pub(crate) layout_seen: State<bool>,
    /// See [`ScrollableTabRow::indicator`]: composed at measure time with the positions.
    pub(crate) indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
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
    pub(crate) fn strip_intrinsic_width(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize]) -> f32 {
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
    pub(crate) fn scroll_selected_into_view(&self, positions: &[TabPosition], layout_width: f32, is_rtl: bool) {
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


