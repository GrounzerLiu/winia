use super::super::*;

// ── Layout Modifier 方法 ──

impl Modifier {
    /// 设置固定宽高
    ///
    /// 对标 Compose `Modifier.size`：请求值会被**夹进** incoming 范围（`enforceIncoming = true`
    /// ——`SizeNode.measure` 对 `Constraints.fixed(w, h)` 做 `constraints.constrain`）。所以
    /// 请求超出父给的 max 时结果取 max，而不是溢出；想真的溢出用 [`Modifier::required_size`]。
    pub fn size(self, width: impl Into<SizeValue>, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Size {
            width: width.into(),
            height: height.into(),
        })
    }

    /// 仅设置宽度（支持动态：`State<f32>`/`DerivedValue`/闭包——measure 时读）
    pub fn width(self, w: impl Into<SizeValue>) -> Self {
        // 使用 Auto 占位高度，表示不约束
        self.push(ModifierElement::Size {
            width: w.into(),
            height: SizeValue::Auto,
        })
    }

    /// 仅设置高度（支持动态：`State<f32>`/`DerivedValue`/闭包——measure 时读）
    pub fn height(self, h: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::Size {
            width: SizeValue::Auto,
            height: h.into(),
        })
    }

    /// 四边等距 padding（支持动态：`State<f32>`/`DerivedValue`/闭包——
    /// 动画可作用于 padding，对标 Compose `animateDpAsState` + `padding`）
    pub fn padding(self, all: impl Into<Dimension>) -> Self {
        let v = all.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: v.clone(),
            end: v.clone(),
            bottom: v,
        })
    }

    /// 水平方向 padding（start + end）
    pub fn padding_horizontal(self, value: impl Into<Dimension>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: v.clone(),
            top: Dimension::Fixed(0.0),
            end: v,
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 垂直方向 padding（top + bottom）
    pub fn padding_vertical(self, value: impl Into<Dimension>) -> Self {
        let v = value.into();
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: v.clone(),
            end: Dimension::Fixed(0.0),
            bottom: v,
        })
    }

    /// 起始边（左）padding——单边，支持动态
    pub fn padding_start(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: value.into(),
            top: Dimension::Fixed(0.0),
            end: Dimension::Fixed(0.0),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 末尾边（右）padding——单边，支持动态
    pub fn padding_end(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: Dimension::Fixed(0.0),
            end: value.into(),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 顶部 padding——单边，支持动态
    pub fn padding_top(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: value.into(),
            end: Dimension::Fixed(0.0),
            bottom: Dimension::Fixed(0.0),
        })
    }

    /// 底部 padding——单边，支持动态
    pub fn padding_bottom(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: Dimension::Fixed(0.0),
            top: Dimension::Fixed(0.0),
            end: Dimension::Fixed(0.0),
            bottom: value.into(),
        })
    }

    /// 四边各自独立 padding（对标 Compose
    /// `Modifier.padding(start = .., top = .., end = .., bottom = ..)`）。
    /// 参数顺序：(start, top, end, bottom)——全部支持动态。
    /// 与其它 padding 方法可叠加（累积）。
    pub fn padding_sides(
        self,
        start: impl Into<Dimension>,
        top: impl Into<Dimension>,
        end: impl Into<Dimension>,
        bottom: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::PaddingSides {
            start: start.into(),
            top: top.into(),
            end: end.into(),
            bottom: bottom.into(),
        })
    }

    /// 宽度填满可用空间
    pub fn fill_max_width(self) -> Self {
        self.push(ModifierElement::FillMaxWidth { fraction: 1.0 })
    }

    /// Compose `fillMaxWidth(fraction)`: take that share of the incoming maximum. An unbounded
    /// maximum leaves the content untouched.
    pub fn fill_max_width_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxWidth { fraction })
    }

    /// A width RANGE — Compose's `Modifier.widthIn(min, max)`: the content is constrained into
    /// `[min, max]` as far as the incoming constraints allow (they win where they are tighter).
    /// One element, so its two bounds are applied together — unlike `min_width(..).max_width(..)`,
    /// which are two nodes and resolve by chain order.
    pub fn width_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min.into()),
            min_height: None,
            max_width: Some(max.into()),
            max_height: None,
            enforce_incoming: true,
        })
    }

    /// A height RANGE — Compose's `Modifier.heightIn(min, max)`; see [`width_in`](Self::width_in).
    pub fn height_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: None,
            min_height: Some(min.into()),
            max_width: None,
            max_height: Some(max.into()),
            enforce_incoming: true,
        })
    }

    /// Both axes in one node — Compose's `Modifier.sizeIn(minWidth, minHeight, maxWidth, maxHeight)`.
    pub fn size_in(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
        max_width: impl Into<Dimension>,
        max_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
            max_width: Some(max_width.into()),
            max_height: Some(max_height.into()),
            enforce_incoming: true,
        })
    }

    /// Width range that IGNORES incoming constraints — Compose's `requiredWidthIn(min, max)`.
    /// The incoming range still decides any bound this one leaves unspecified.
    pub fn required_width_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min.into()),
            min_height: None,
            max_width: Some(max.into()),
            max_height: None,
            enforce_incoming: false,
        })
    }

    /// Height range that ignores incoming constraints — Compose's `requiredHeightIn(min, max)`.
    pub fn required_height_in(self, min: impl Into<Dimension>, max: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: None,
            min_height: Some(min.into()),
            max_width: None,
            max_height: Some(max.into()),
            enforce_incoming: false,
        })
    }

    /// The four-bound escape hatch — Compose's `requiredSizeIn`.
    pub fn required_size_in(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
        max_width: impl Into<Dimension>,
        max_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::SizeIn {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
            max_width: Some(max_width.into()),
            max_height: Some(max_height.into()),
            enforce_incoming: false,
        })
    }

    /// A minimum that only applies where the incoming minimum is 0 — Compose's
    /// `defaultMinSize`: a component's own floor that a caller can lower from outside.
    pub fn default_min_size(
        self,
        min_width: impl Into<Dimension>,
        min_height: impl Into<Dimension>,
    ) -> Self {
        self.push(ModifierElement::DefaultMinSize {
            min_width: Some(min_width.into()),
            min_height: Some(min_height.into()),
        })
    }

    /// Measure without the incoming minimum width (and, with `unbounded`, without the incoming
    /// maximum), then align the content inside the resulting box — Compose's
    /// `Modifier.wrapContentWidth(align, unbounded)`.
    pub fn wrap_content_width(self, align: crate::layout::Alignment, unbounded: bool) -> Self {
        self.push(ModifierElement::WrapContent {
            width: true,
            height: false,
            align: crate::layout::ContentAlignment::new(align, crate::layout::Alignment::Center),
            unbounded,
        })
    }

    /// The height twin of [`wrap_content_width`](Self::wrap_content_width).
    pub fn wrap_content_height(self, align: crate::layout::Alignment, unbounded: bool) -> Self {
        self.push(ModifierElement::WrapContent {
            width: false,
            height: true,
            align: crate::layout::ContentAlignment::new(crate::layout::Alignment::Center, align),
            unbounded,
        })
    }

    /// Both axes — Compose's `Modifier.wrapContentSize(align, unbounded)`.
    pub fn wrap_content_size(
        self,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    ) -> Self {
        self.push(ModifierElement::WrapContent {
            width: true,
            height: true,
            align,
            unbounded,
        })
    }

    /// 最小宽度（对标 Compose `Modifier.widthIn(min = ...)`）——提升布局
    /// 最小约束，内容不足时撑到该宽度；受父 max 约束夹住。支持动态值。
    pub fn min_width(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MinWidth { value: value.into() })
    }

    /// 最小高度（对标 Compose `Modifier.heightIn(min = ...)`）
    pub fn min_height(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MinHeight { value: value.into() })
    }

    /// Maximum width (Compose `Modifier.widthIn(max = ...)`) — content that would be wider
    /// is constrained to it. A minimum above it wins, as in Compose's `sizeIn`. Supports a
    /// dynamic value, so an animation can drive the cap.
    pub fn max_width(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MaxWidth { value: value.into() })
    }

    /// Maximum height (Compose `Modifier.heightIn(max = ...)`; a conflicting min resolves by chain
    /// order, see [`Self::max_width`])
    pub fn max_height(self, value: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::MaxHeight { value: value.into() })
    }

    /// 高度填满可用空间
    pub fn fill_max_height(self) -> Self {
        self.push(ModifierElement::FillMaxHeight { fraction: 1.0 })
    }

    /// Compose `fillMaxHeight(fraction)`; see [`fill_max_width_fraction`](Self::fill_max_width_fraction).
    pub fn fill_max_height_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxHeight { fraction })
    }

    /// 宽高填满可用空间
    pub fn fill_max_size(self) -> Self {
        self.push(ModifierElement::FillMaxSize { fraction: 1.0 })
    }

    /// Compose `fillMaxSize(fraction)`.
    pub fn fill_max_size_fraction(self, fraction: f32) -> Self {
        self.push(ModifierElement::FillMaxSize { fraction })
    }

    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 合并式 offset：若链上已有 Offset 元素则更新其 x/y 分量，否则 push 新的。
    /// 这样 `.offset_x(a).offset_y(b)` 得到一个 `Offset{a,b}`（而非两个 Offset——
    /// get_offset 只取第一个，连用会静默丢分量）。对齐 `merge_graphics_layer` 模式。
    fn merge_offset(mut self, x: Option<Dimension>, y: Option<Dimension>) -> Self {
        let mut elements = std::mem::take(&mut self.elements);
        if let Some(el) = elements.iter_mut().find(|e| matches!(e, ModifierElement::Offset { .. })) {
            if let ModifierElement::Offset { x: ex, y: ey } = el {
                if let Some(nx) = x { *ex = nx; }
                if let Some(ny) = y { *ey = ny; }
            }
        } else {
            elements.push(ModifierElement::Offset {
                x: x.unwrap_or(Dimension::Fixed(0.0)),
                y: y.unwrap_or(Dimension::Fixed(0.0)),
            });
        }
        self.elements = elements;
        self
    }

    /// 对标 Compose `Modifier.offset`）。
    /// 值支持动态（`State`/`DerivedValue`/闭包——动画可作用于 offset）。
    pub fn offset(self, x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        // 已有 Offset（如前面 offset_x）时合并分量，而非再 push 一个
        self.merge_offset(Some(x.into()), Some(y.into()))
    }

    /// 仅 x 方向偏移（RTL 下镜像）。与 `offset_y` 连用合并为单个 Offset 元素。
    pub fn offset_x(self, x: impl Into<Dimension>) -> Self {
        self.merge_offset(Some(x.into()), None)
    }

    /// 仅 y 方向偏移（不受 RTL 影响——垂直方向）。与 `offset_x` 连用合并为单个 Offset 元素。
    pub fn offset_y(self, y: impl Into<Dimension>) -> Self {
        self.merge_offset(None, Some(y.into()))
    }

    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）。
    /// 值支持动态。
    pub fn absolute_offset(self, x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: y.into() })
    }

    /// 仅 x 方向绝对偏移（RTL 下不镜像）
    pub fn absolute_offset_x(self, x: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: x.into(), y: Dimension::Fixed(0.0) })
    }

    /// 仅 y 方向绝对偏移
    pub fn absolute_offset_y(self, y: impl Into<Dimension>) -> Self {
        self.push(ModifierElement::AbsoluteOffset { x: Dimension::Fixed(0.0), y: y.into() })
    }

    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    ///
    /// In a `Row`/`Column` this is the cross axis. A `Box` applies it to BOTH axes (the one-axis
    /// value is axis-symmetric: `Start` is top-start, `End` is bottom-end); use
    /// [`Self::content_align_self`] when the two axes need different values.
    pub fn align_self(self, alignment: crate::layout::Alignment) -> Self {
        self.push(ModifierElement::AlignSelf { alignment })
    }

    /// A child's own alignment inside a `Box`/`Stack`, on both axes — Compose's
    /// `BoxScope.Modifier.align(Alignment.TopEnd)`. Overrides `align_self` and the box's own
    /// `contentAlignment` for this child. Ignored by `Row`/`Column`, whose cross axis is one-axis.
    pub fn content_align_self(self, alignment: crate::layout::ContentAlignment) -> Self {
        self.push(ModifierElement::ContentAlignSelf { alignment })
    }

    /// Match the parent Box's size WITHOUT taking part in it — Compose's
    /// `BoxScope.Modifier.matchParentSize()`. The child is excluded from sizing the box and then
    /// measured at the box's size, which is what lets a scrim or a background cover the box without
    /// forcing the box to fill its own parent (a `fill_max_size` child does force that). Ignored by
    /// `Row`/`Column`/`Flow`.
    pub fn match_parent_size(self) -> Self {
        self.push(ModifierElement::MatchParentSize)
    }

    /// Declare this child's span inside a `Grid` — Compose's `GridScope.Modifier.gridItem`
    /// (`Grid.kt:158`). The alignment inside the cell defaults to Compose's `TopStart`; use
    /// [`grid_item_aligned`](Self::grid_item_aligned) to centre, stretch or bottom-align the child.
    pub fn grid_item(self, column_span: u32, row_span: u32) -> Self {
        self.push(ModifierElement::GridItem {
            column_span,
            row_span,
            alignment: None,
        })
    }

    /// Like [`grid_item`](Self::grid_item), with the child aligned inside its cell instead of
    /// stretched across it.
    pub fn grid_item_aligned(
        self,
        column_span: u32,
        row_span: u32,
        alignment: crate::layout::ContentAlignment,
    ) -> Self {
        self.push(ModifierElement::GridItem {
            column_span,
            row_span,
            alignment: Some(alignment),
        })
    }

    /// Hide this node without giving up its space — Compose's `Modifier.visible`
    /// (`Visible.kt:51`).
    ///
    /// While hidden the node is still measured and still takes part in its parent's layout
    /// (`AbsoluteSize`, like `View.INVISIBLE`), but it is not painted, not hit-tested, not
    /// focusable and absent from accessibility. That is different from `AnimatedVisibility`, which
    /// removes the node from layout, and from an alpha of 0, which keeps it interactive.
    pub fn visible(self, visible: bool) -> Self {
        self.push(ModifierElement::Visible { visible })
    }

    /// Align this child by one of its own alignment lines instead of by its edge — Compose's
    /// `RowScope.Modifier.alignBy` / `ColumnScope.Modifier.alignBy`.
    ///
    /// The child reports the line (a text leaf reports its first baseline) and the parent offsets it
    /// so that every such child's line lands on the same cross-axis position, growing the cross axis
    /// if the line plus what hangs below it needs more room. It takes precedence over
    /// [`Modifier::align_self`] and the parent's own alignment, as Compose's `getCrossAxisPosition`
    /// does (`RowColumnMeasurePolicy.kt:228-251`, `Row.kt:216-231`).
    pub fn align_by(self, line: crate::layout::AlignmentLine) -> Self {
        self.push(ModifierElement::AlignBy { line })
    }

    /// `Modifier.alignByBaseline()` — the common case: align this child's first text baseline with
    /// its siblings', which is how a label lines up with a taller icon or a larger font beside it.
    pub fn align_by_baseline(self) -> Self {
        self.align_by(crate::layout::AlignmentLine::FIRST_BASELINE)
    }

    /// Fill this child to the tallest item in its FlowRow (Compose's
    /// `FlowRowScope.fillMaxRowHeight`) — only the flow row reads this element.
    pub fn fill_max_row_height(self) -> Self {
        self.fill_max_row_height_fraction(1.0)
    }

    /// The fraction form of [`Modifier::fill_max_row_height`], between 0 and 1.
    pub fn fill_max_row_height_fraction(self, fraction: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&fraction),
            "fill_max_row_height fraction must be in 0..=1 (got {fraction})"
        );
        self.push(ModifierElement::FillMaxRowHeight { fraction })
    }

    /// Fill this child to the widest item in its FlowColumn (Compose's
    /// `FlowColumnScope.fillMaxColumnWidth`) — only the flow column reads this element.
    pub fn fill_max_column_width(self) -> Self {
        self.fill_max_column_width_fraction(1.0)
    }

    /// The fraction form of [`Modifier::fill_max_column_width`], between 0 and 1.
    pub fn fill_max_column_width_fraction(self, fraction: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&fraction),
            "fill_max_column_width fraction must be in 0..=1 (got {fraction})"
        );
        self.push(ModifierElement::FillMaxColumnWidth { fraction })
    }

    /// Report an alignment line for this node: the line sits `f(measured size)` from the node's top
    /// (a horizontal line) or start (a vertical one).
    ///
    /// This is where a custom line is PUBLISHED, and it is the stand-in for Compose declaring a line
    /// as the layout is produced — `Modifier.layout { measurable, constraints -> layout(w, h,
    /// alignmentLines = mapOf(line to offset)) { … } }`. winia cannot do it from a `MeasurePolicy`
    /// (a policy is never told its own node index), and a `Modifier` is the dataclass a component
    /// hands to its node, so a closure over the measured size belongs here.
    ///
    /// The line then behaves like any other: a container inherits it through the merger, a parent
    /// aligns a child by it with [`Modifier::align_by`], and [`Modifier::padding_from`] measures
    /// against it. Publish several by calling this several times.
    pub fn alignment_line(
        self,
        line: crate::layout::AlignmentLine,
        f: impl Fn(crate::unit::Size) -> f32 + Send + Sync + 'static,
    ) -> Self {
        self.push(ModifierElement::AlignmentLineValue { line, value: std::sync::Arc::new(f) })
    }

    /// Pad this node so the distance from its near edge to its content's alignment line is `before`
    /// and from the line to the far edge is `after` — Compose's `Modifier.paddingFrom`
    /// (`foundation/layout/AlignmentLine.kt:65`).
    ///
    /// Which edges those are follows the line's orientation: a horizontal line (the text baselines)
    /// means top and bottom. The other axis is untouched, and `None` is Compose's `Dp.Unspecified` —
    /// that side is unconstrained, and the content is then placed against the other one.
    ///
    /// The padding is capped by the incoming maximum on that axis, and `before` wins when both cannot
    /// fit (Compose's contract, `:44-55`).
    /// Compose's `Modifier.windowInsetsPadding(insets)`: pad by the window's current insets of this
    /// kind, resolved at measure time (an inset change re-measures only the nodes that asked), and
    /// consume them for the subtree.
    pub fn window_insets_padding(self, kind: crate::window::insets::InsetsKind) -> Self {
        self.push(ModifierElement::WindowInsetsPadding {
            kind,
            controller: crate::window::insets::current(),
        })
    }

    /// Compose's `Modifier.consumeWindowInsets(insets)`: mark the insets consumed for the subtree
    /// without padding by them.
    pub fn consume_window_insets(self, kind: crate::window::insets::InsetsKind) -> Self {
        self.push(ModifierElement::ConsumeWindowInsets {
            kind,
            controller: crate::window::insets::current(),
        })
    }

    /// Compose's `Modifier.statusBarsPadding()`.
    pub fn status_bars_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::StatusBars)
    }

    /// Compose's `Modifier.navigationBarsPadding()`.
    pub fn navigation_bars_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::NavigationBars)
    }

    /// Compose's `Modifier.systemBarsPadding()`.
    pub fn system_bars_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::SystemBars)
    }

    /// Compose's `Modifier.imePadding()`.
    pub fn ime_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::Ime)
    }

    /// Compose's `Modifier.displayCutoutPadding()`.
    pub fn display_cutout_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::DisplayCutout)
    }

    /// Compose's `Modifier.systemGesturesPadding()`.
    pub fn system_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::SystemGestures)
    }

    /// Compose's `Modifier.tappableElementPadding()`.
    pub fn tappable_element_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::TappableElements)
    }

    /// Compose's `Modifier.safeDrawingPadding()`.
    pub fn safe_drawing_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::SafeDrawing)
    }

    /// Compose's `Modifier.safeGesturesPadding()`.
    pub fn safe_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::SafeGestures)
    }

    /// Compose's `Modifier.safeContentPadding()`.
    pub fn safe_content_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::SafeContent)
    }

    /// Compose's `Modifier.mandatorySystemGesturesPadding()`.
    pub fn mandatory_system_gestures_padding(self) -> Self {
        self.window_insets_padding(crate::window::insets::InsetsKind::MandatorySystemGestures)
    }

    pub fn padding_from(
        self,
        line: crate::layout::AlignmentLine,
        before: Option<f32>,
        after: Option<f32>,
    ) -> Self {
        self.push(ModifierElement::PaddingFrom { line, before, after })
    }

    /// material3's default minimum touch target, `LocalMinimumInteractiveComponentSize`'s 48.dp
    /// (`material3/InteractiveComponentSize.kt:206-210`).
    pub const MINIMUM_INTERACTIVE_SIZE_DP: f32 = 48.0;

    /// Reserve at least a touch target's worth of space around a small component, centring it —
    /// Compose's `Modifier.minimumInteractiveComponentSize()` (`material3/InteractiveComponentSize.kt:69`),
    /// which material3's own Button, IconButton, Checkbox and friends apply for you.
    ///
    /// The box becomes `max(content, 48)` on both axes with the content centred inside it, and the two
    /// "where the visual content starts" lines are published
    /// ([`AlignmentLine::minimum_interactive_left`] / [`AlignmentLine::minimum_interactive_top`]) so a
    /// parent can align the small component by its real edge rather than by its invisible padding.
    ///
    /// Deviation, recorded: Compose reads the size from `LocalMinimumInteractiveComponentSize`, a
    /// CompositionLocal a screen can set to 0 to switch enforcement off for a whole subtree; winia takes
    /// the value from the modifier, with the 48 dp default in one place, and has no per-subtree switch.
    /// [`Modifier::minimum_interactive_size`] is there for a caller that wants another number.
    pub fn minimum_interactive_component_size(self) -> Self {
        self.minimum_interactive_size(Self::MINIMUM_INTERACTIVE_SIZE_DP)
    }

    /// [`Modifier::minimum_interactive_component_size`] with an explicit size. `size <= 0` disables it,
    /// which is Compose's "unspecified or 0.dp" case.
    pub fn minimum_interactive_size(self, size: f32) -> Self {
        self.push(ModifierElement::MinimumInteractiveSize { size })
    }

    /// Compose's `Modifier.paddingFromBaseline(top, bottom)`: `top` from the container's top edge to
    /// the FIRST text baseline, `bottom` from the LAST one to the container's bottom. Either side may
    /// be `None` (Compose's `Dp.Unspecified`), and Compose expresses it as one `paddingFrom` per
    /// specified side, which is what this does — the two compose, so both can be in force.
    pub fn padding_from_baseline(self, top: Option<f32>, bottom: Option<f32>) -> Self {
        let m = match top {
            Some(top) => self.padding_from(
                crate::layout::AlignmentLine::FIRST_BASELINE,
                Some(top),
                None,
            ),
            None => self,
        };
        match bottom {
            Some(bottom) => m.padding_from(
                crate::layout::AlignmentLine::LAST_BASELINE,
                None,
                Some(bottom),
            ),
            None => m,
        }
    }

    /// 布局权重（Row 中按比例分配宽度，Column 中按比例分配高度）
    ///
    /// The child occupies its whole share, which is Compose's `weight(weight, fill = true)`.
    /// A non-positive weight means "no weight": the child is measured like any other unweighted
    /// sibling. (Compose rejects `weight <= 0` at the modifier instead — `Row.kt:383-384`.)
    pub fn layout_weight(self, weight: f32) -> Self {
        self.push(ModifierElement::LayoutWeight { weight, fill: true })
    }

    /// 布局权重，是否填满自己的份额（对标 Compose `Modifier.weight(weight, fill)`）。
    ///
    /// With `fill = false` the share becomes the child's MAXIMUM main-axis size instead of an exact
    /// one, and the parent uses the size the child measured, so a child shorter than its share lets
    /// the container be shorter too. The arrangement is free to be a spreading one as well, because a
    /// content-sized container no longer grows to the maximum its parent offers.
    pub fn layout_weight_fill(self, weight: f32, fill: bool) -> Self {
        self.push(ModifierElement::LayoutWeight { weight, fill })
    }

    /// `aspect_ratio(ratio)`（对标 Compose `Modifier.aspectRatio`）——
    /// 约束本节点宽高比（ratio = 宽/高，必须 > 0）。
    ///
    /// `match_height_first = true` 时优先按高度约束推导宽度
    /// （对标 `matchHeightConstraintsFirst`）。
    pub fn aspect_ratio(self, ratio: f32, match_height_first: bool) -> Self {
        assert!(ratio > 0.0, "aspectRatio {ratio} must be > 0（Compose 前置校验）");
        self.push(ModifierElement::AspectRatio { ratio, match_height_first })
    }

    /// `required_size(w, h)`（对标 Compose `Modifier.requiredSize`）——
    /// 强制本节点为该尺寸，**忽略 incoming constraints 的收缩**（允许
    /// 溢出父约束——enforceIncoming=false 语义）。单轴用
    /// `required_width` / `required_height`。
    pub fn required_size(self, width: impl Into<SizeValue>, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width.into()), height: Some(height.into()) })
    }

    /// 仅强制宽度
    pub fn required_width(self, width: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: Some(width.into()), height: None })
    }

    /// 仅强制高度
    pub fn required_height(self, height: impl Into<SizeValue>) -> Self {
        self.push(ModifierElement::RequiredSize { width: None, height: Some(height.into()) })
    }

    /// `test_tag(tag)`（对标 Compose `Modifier.testTag`）——给节点打测试
    /// 标记，UI 测试/调试树用其定位节点（树 JSON 的 `tag` 字段）。
    pub fn test_tag(self, tag: impl Into<String>) -> Self {
        self.push(ModifierElement::TestTag { tag: tag.into() })
    }

    /// Declare what this node is, for a screen reader and for tests (Compose
    /// `Modifier.semantics { ... }`).
    ///
    /// A node that declares a role, a name or a state appears in the semantics tree
    /// (`crate::semantics::semantics_tree`); a click target claims its whole subtree and reports as
    /// ONE element named by the text it contains. Everything else stays transparent.
    ///
    /// Declarations along one chain MERGE by position: a field set later wins, and a field left
    /// unset keeps the earlier value. That is what lets a component state its role while a caller
    /// adds only a content description, and it is why components declare before they append the
    /// caller's modifier (`modifier.semantics(role).then(user_modifier)`) — the caller's declaration
    /// then wins where the two overlap.
    ///
    /// ```ignore
    /// Modifier::new()
    ///     .semantics(SemanticsConfig::new().role(SemanticsRole::Tab))
    ///     .then(Modifier::new().semantics(SemanticsConfig::new().content_description("Home")))
    /// // → role = Tab, description = Home
    /// ```
    pub fn semantics(self, config: crate::semantics::SemanticsConfig) -> Self {
        self.push(ModifierElement::Semantics(config))
    }

    /// Compose's `clearAndSetSemantics { … }`: replace this node's semantics with `config` and hide
    /// its entire subtree from the semantics tree. A config that declares nothing makes the node
    /// (and its subtree) invisible to accessibility, which is the Compose contract.
    pub fn clear_and_set_semantics(self, config: crate::semantics::SemanticsConfig) -> Self {
        self.push(ModifierElement::ClearAndSetSemantics(config))
    }

    /// Compose's `focusGroup()`: Tab treats this subtree as one group and remembers which child was
    /// focused, so re-entering the group restores that child.
    pub fn focus_group(self) -> Self {
        self.push(ModifierElement::FocusGroup)
    }

    /// Compose's `focusProperties { canFocus = … }`: `false` keeps the node out of focus search
    /// (Tab, arrow keys and programmatic focus requests) even when it carries `focusable`.
    pub fn focus_properties_can_focus(self, can_focus: bool) -> Self {
        self.push(ModifierElement::FocusProperties {
            can_focus: Some(can_focus),
        })
    }

    /// Compose's `onFocusChanged { … }`: `true` when this node gains focus, `false` when it loses it.
    pub fn on_focus_changed(self, callback: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::OnFocusChanged {
            callback: std::sync::Arc::new(callback),
        })
    }

    /// Compose's `Modifier.selectableGroup()`
    /// (`foundation/selection/SelectableGroup.kt:30`): one line, `semantics { selectableGroup() }`. Wrap a
    /// group of selectable children (radio buttons, tabs) so a reader announces them as a collection whose
    /// items are this node's children, instead of as unrelated controls.
    pub fn selectable_group(self) -> Self {
        self.semantics(crate::semantics::SemanticsConfig::new().selectable_group())
    }

    /// This chain's merged accessibility declaration: every `semantics` element folded in chain
    /// order, later fields winning over earlier ones.
    pub fn semantics_config(&self) -> crate::semantics::SemanticsConfig {
        self.elements
            .iter()
            .filter_map(|el| match el {
                ModifierElement::Semantics(config) => Some(config),
                _ => None,
            })
            .fold(crate::semantics::SemanticsConfig::default(), |merged, config| {
                config.clone().or(merged)
            })
    }

    /// Whether this chain carries `clearAndSetSemantics`; the measure/report side reads the config.
    pub fn clear_and_set_semantics_config(&self) -> Option<&crate::semantics::SemanticsConfig> {
        self.elements.iter().find_map(|el| match el {
            ModifierElement::ClearAndSetSemantics(config) => Some(config),
            _ => None,
        })
    }

    /// Whether this node is a focus traversal group.
    pub fn is_focus_group(&self) -> bool {
        self.elements.iter().any(|el| matches!(el, ModifierElement::FocusGroup))
    }

    /// `focusProperties { canFocus }` — `None` when the chain does not say.
    pub fn focus_can_focus(&self) -> Option<bool> {
        self.elements.iter().rev().find_map(|el| match el {
            ModifierElement::FocusProperties { can_focus } => *can_focus,
            _ => None,
        })
    }

    /// 布局方向——本节点 padding start/end 解析 + Row/Column 容器布局方向。
    /// 节点级便捷覆盖；子树级方向切换用
    /// `WiniaTheme::with_theme_and_direction`（CompositionLocal 作用域）。
    pub fn layout_direction(self, d: crate::layout::LayoutDirection) -> Self {
        self.push(ModifierElement::LayoutDirection(d))
    }

    /// Sibling paint order (Compose's `Modifier.zIndex`): a node with a higher `z` paints above its
    /// siblings, and a press that lands on several of them goes to the highest one. Read at paint and
    /// hit-test time, so it needs no layout pass of its own; the layout pass does record whether any
    /// child sets it (`LayoutNode::children_have_z`), which keeps the common case free.
    pub fn z_index(self, z: f32) -> Self {
        self.push(ModifierElement::ZIndex(z))
    }

    /// TextField 容器子节点角色标记（text-field-v2 容器化内部使用——
    /// TextFieldLayout policy 按角色布局）
    pub(crate) fn text_field_slot(self, role: crate::text::field::TextFieldSlotRole) -> Self {
        self.push(ModifierElement::TextFieldSlot { role })
    }

    /// `shadow(elevation, shape, clip, color)` — Compose's `Modifier.shadow`, which is a graphics
    /// layer: `graphicsLayer { shadowElevation = …; shape = …; clip = … }`. Skia's `SkShadowUtils`
    /// then derives the blur and the offset from the elevation as a physical Z height (see
    /// `render.rs::draw_elevation_shadow`) — the path FAB/Button/Card/Chip already take.
    ///
    /// This used to expand the call into winia's own analytic ambient+spot pair, whose alpha
    /// `render.rs` faded a second time; measured on a white surface that shadow was all but
    /// invisible beside the same colour on this path.
    ///
    /// `color` becomes both the ambient and the spot colour, as Compose's single
    /// `DefaultShadowColor` does. Returns `self` unchanged when there is nothing to draw
    /// (`elevation <= 0` and `clip == false`) — Compose's early return.
    pub fn shadow(
        self,
        elevation: f32,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        clip: bool,
        color: Color,
    ) -> Self {
        if elevation <= 0.0 && !clip {
            return self;
        }
        let shape = shape.into();
        let place = move |p: &mut GraphicsLayerParams, shape: Shape| {
            p.shadow_elevation = elevation;
            p.shadow_shape = Some(shape);
            p.clip = clip;
            p.ambient_shadow_color = color;
            p.spot_shadow_color = color;
        };
        // A dynamic shape has to be read at paint time; a static one is applied now, which keeps the
        // layer a plain value instead of a closure.
        if shape.is_dynamic() {
            self.merge_graphics_layer_per_paint(move |p| {
                let shape = shape.eval();
                place(p, shape);
            })
        } else {
            let value = shape.eval();
            self.merge_graphics_layer(move |p| place(p, value))
        }
    }

    /// `shadow(elevation)`——便捷版（默认形状/黑色/自动 clip）
    pub fn shadow_default(self, elevation: f32) -> Self {
        self.shadow(elevation, Shape::Rectangle, elevation > 0.0, Color::from_argb(255, 0, 0, 0))
    }

    /// `drop_shadow(shape, params)`（对标 Compose `Modifier.dropShadow`）——
    /// 完全自定义单层阴影（radius/spread/offset/color/alpha）。多个
    /// drop_shadow 可叠加（多阴影，Compose vararg 语义）。
    pub fn drop_shadow(
        self,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        params: impl Into<crate::graphics::Dyn<ShadowParams>>,
    ) -> Self {
        self.push(ModifierElement::Shadow {
            params: params.into(),
            shape: shape.into(),
            clip: false,
        })
    }
}

// ── Draw Modifier 方法 ──
