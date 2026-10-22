use super::*;

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
    pub(crate) selected_tab_index: usize,
    pub(crate) content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    pub(crate) modifier: Modifier,
    pub(crate) container_color: Option<Color>,
    pub(crate) content_color: Option<Color>,
    pub(crate) follow_content_size: bool,
    pub(crate) divider_color: Option<Color>,
    pub(crate) indicator_color: Option<Color>,
    pub(crate) indicator_shape: Option<Shape>,
    /// A caller-supplied indicator (Compose's `indicator` slot). Composed DURING measurement with the
    /// positions this row just computed, which is the only place they exist — see `TabIndicatorScope`.
    /// `Fn` behind an `Arc`, not `FnOnce`: every measurement runs it (a later frame measures again),
    /// the same shape `BoxWithConstraints`' content has.
    pub(crate) indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
}

/// What a custom indicator gets to know: where every tab ended up, and which one is selected.
///
/// Compose's `TabRow` hands its indicator a `TabIndicatorScope` with a `tabPositions` list, and those
/// positions only exist once the row has been measured — which is why the indicator is composed inside
/// measurement (`layout::subcompose`) instead of by the caller during composition.
#[derive(Debug, Clone)]
pub struct TabIndicatorScope {
    pub(crate) positions: Vec<TabPosition>,
    pub(crate) selected_index: usize,
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
pub(crate) struct TabRowLayoutPolicy {
    pub(crate) selected_tab_index: usize,
    pub(crate) follow_content_size: bool,
    pub(crate) offset_state: State<f32>,
    pub(crate) width_state: State<f32>,
    pub(crate) initialized: std::sync::Arc<AtomicBool>,
    pub(crate) direction: LayoutDirection,
    /// The caller's indicator, when there is one: composed at measure time with the positions
    /// (see `TabRow::indicator`).
    pub(crate) indicator_slot: Option<std::sync::Arc<dyn Fn(&mut ComposeCtx, TabIndicatorScope) + Send + Sync>>,
}

