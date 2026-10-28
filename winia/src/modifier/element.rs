use super::*;

// ── ModifierElement ──

// ═══════════════════════════════════════════════════════════
// Modifier Node（实验性开放扩展点，分支 exp/modifier-node）
// ───────────────────────────────────────────────────────────
// 目标：把 Modifier 从"封闭枚举"变成"开放节点"。枚举每加一种行为
// 就要改 measure/render/hit/app 四处 match；node 化之后，第三方可以
// 不改核心实现自定义行为。
//
// 设计（双轨，零破坏）：
// - `Modifier` 内部新增 `nodes: Vec<ModifierNode>`，与 `elements` 并存。
//   旧 builder（background/clickable/…）照旧 push 枚举；新扩展走 node。
// - 每种行为是一个 `Arc<dyn XxxNode>`，核心管线（measure/render/input/
//   debug）在走完枚举 match 后，再走一遍 node 链（同语义合并）。
// - Node 要求 Debug + Send + Sync（与现有元素约束一致）；Click 参数相等
//   走 `node_key()`（默认按 TypeId + 可选 id，保证 param_eq 可判定）。
// - 试点：BackgroundNode（绘制）+ ClickableNode（输入）——验证"枚举
//   不动、node 链并行生效"后，再逐个迁移。
//
// 有状态节点（StatefulNode 约定）：node 本身不持有组合期 State（remember
// 必须在 `#[composable] build` 内调用——宏语句 key 上下文要求）。约定：
// A component builds a State with `ctx.remember(...)`, clones it into the node (a State is an Arc
// wrapper, so the clone is cheap), reads it with `get()` at build time to register the
// compose/layout dependency, and evaluates it with `peek()` at paint time.
//
// A paint-time `get()` registers nothing (the dependency frame is gone by then: DEP_MODE is None -
// the same rule the draw-layer elements follow). Repaint comes from the build-time `get()` (a change
// recomposes -> new node -> repaint) or from an animation writing through `Animating` and requesting
// a redraw. The node has no private channel; it follows the elements (see
// `node_track_stateful_draw_follows_state`).
// 例：`let src = ctx.remember(|| MutableInteractionSource::new()).get();`
//     `Modifier::new().click_node(MyClick { source: src.clone() })`。
// 状态生命周期跟组合槽走（remember 语义），node 只是"行为 + 状态引用"
// 的载体——节点移除时 State 随槽回收，无需 onAttach/onDetach。
// ═══════════════════════════════════════════════════════════

/// Draw node: invoked with the node rect at render time (cf. Compose DrawModifierNode).
/// The legacy `render_modifier_element` returning `Option<TextParams>` stays in core;
/// a node only paints (background/decoration) and never extracts text.
///
/// Ordering (measured): nodes paint uniformly at the background layer (after the enum
/// chain), with no chain-order interleaving. For above-content painting use
/// [`DrawWrapNode::draw_after`].
/// What a [`Modifier::draw_with_cache`] build closure sees: the inputs that invalidate its cache.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCacheScope {
    pub width: f32,
    pub height: f32,
    pub density: f32,
}

/// The node behind `draw_with_cache`: rebuilds its cache when the drawn size or density changes,
/// then hands the cached value to the draw closure on every paint — Compose's `drawWithCache`.
pub(super) struct DrawCacheNode<C> {
    pub(super) build: std::sync::Arc<dyn Fn(DrawCacheScope) -> C + Send + Sync>,
    pub(super) draw: std::sync::Arc<dyn Fn(&C, &skia_safe::Canvas, skia_safe::Rect) + Send + Sync>,
    pub(super) cache: std::sync::Mutex<Option<(DrawCacheScope, C)>>,
}

impl<C> std::fmt::Debug for DrawCacheNode<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DrawCacheNode")
    }
}

impl<C: Send + Sync + 'static> DrawNode for DrawCacheNode<C> {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        let scope = DrawCacheScope {
            width: rect.width(),
            height: rect.height(),
            density: crate::runtime::density::current_density().density,
        };
        let mut cache = self.cache.lock().expect("draw cache");
        let rebuild = cache.as_ref().map(|(cached, _)| *cached != scope).unwrap_or(true);
        if rebuild {
            *cache = Some((scope, (self.build)(scope)));
        }
        let (_, value) = cache.as_ref().expect("just built");
        (self.draw)(value, canvas, rect);
    }
}

pub trait DrawNode: std::fmt::Debug + Send + Sync {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect);
    /// Skip fingerprint MUST rule (P1-3): nodes with params MUST override, covering
    /// **all output-affecting static params** (color/shape/value/flags); write-back
    /// channels (State write-back) and transient animation values (per-frame
    /// progress/alpha) MUST NOT enter the key (they travel the dependency/repaint
    /// channel, otherwise every animation frame Enters); callback fields count as
    /// equal (rebuild does not Enter). Default = TypeId name (fine for paramless
    /// nodes; forgetting the override on a param node silently mis-hits Skip and
    /// leaves stale output).
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// Wrapping draw node (cf. Compose `DrawModifierNode` `drawWithContent`: paint before
/// and after content; Compose `drawBehind` = before only, above-content decor = after only).
///
/// Position (`render_pass1`):
/// - `draw_before`: same layer as [`DrawNode`] (after the enum chain, before text/children).
/// - `draw_after`: after children and after `draw_ripple`, before scroll/clip restore —
///   above content (covers text and children), inside the scroll translate (same stack
///   as ripple).
///
/// `modifier` param: for reading chain enums when needed (e.g. inferring the shape from
/// the nearest Background/Border — same logic as inside `draw_ripple`). The render side
/// passes the owning node's `&Modifier`. Ignore it when unneeded.
///
/// `node_key` MUST rule is the same as [`DrawNode`] (all static visual params in;
/// write-back/transient values out).
pub trait DrawWrapNode: std::fmt::Debug + Send + Sync {
    fn draw_before(
        &self,
        _canvas: &skia_safe::Canvas,
        _rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
    }
    fn draw_after(
        &self,
        _canvas: &skia_safe::Canvas,
        _rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
    }
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 点击节点：输入期沿命中路径查询（对标 Compose 点击语义）。
/// 与 `Clickable` 枚举同优先级——核心 `on_click()` 先查枚举、再查 node。
///
/// 单首语义（P1-2 结论）：每 Modifier 仅首个 ClickNode 生效（与枚举 `on_click()`
/// 取首个 Clickable 一致）。多 push 两个 click_node 时第二个静默不触发——
/// 如需多回调请合并进一个 node 的 `on_click`。`node_clicks()` 迭代器保留供
/// 未来多播语义（暂未启用）。
pub trait ClickNode: std::fmt::Debug + Send + Sync {
    fn on_click(&self);
    /// 绑定的交互源（press 波纹用；无则 None）。
    fn interaction(&self) -> Option<crate::input::interaction::MutableInteractionSource> {
        None
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）：静态参数进 key；回写/瞬态动画值
    /// 不进；回调视为相同。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 指针节点：原始指针事件（对标 Compose PointerInputModifierNode）。
/// 与 `PointerEvent` 枚举同语义（pre = 外→内隧道，on = 内→外冒泡），返回 true
/// 即消费（停止冒泡）。第三方自定义手势/悬停/拖拽识别挂这里，无需改 app.rs。
///
/// ⚠ 手势路由隔离（P0-1 教训）：PointerNode **不参与** `has_gesture()` 手势目标
/// 选择——手势竞技场（tap/drag 识别）只认 TapOn/DragOn 枚举。PointerNode 只收
/// `dispatch_ptr_event` 的原始 down/move/up（hover 日志、自定义坐标换算等装饰性
/// 用途）。需要手势回调的 node 应同时挂对应枚举（如 on_tap），或等手势竞技场
/// 落地后的 GestureNode。把 PointerNode 纳入 has_gesture 会静默偷走外层 tap
/// （内层装饰性 node 锁定 gesture 目标，fire_gesture_action 只查枚举 → 外层
/// TapOnTap 永不触发）。
pub trait PointerNode: std::fmt::Debug + Send + Sync {
    /// 隧道阶段（外→内）：对应 `on_pre_ptr`。
    fn on_pre(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// 冒泡阶段（内→外）：对应 `on_ptr`。
    fn on_event(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 键盘节点：按键事件（对标 Compose KeyInputModifierNode）。
/// 与 `KbEvent` 枚举同语义（pre = root→focused 隧道，on = focused→root 冒泡），
/// 返回 true 即消费。第三方自定义快捷键/输入拦截挂这里，无需改 app.rs。
pub trait KeyNode: std::fmt::Debug + Send + Sync {
    /// 隧道阶段（root→focused）：对应 `on_pre_key`。
    fn on_pre(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// 冒泡阶段（focused→root）：对应 `on_key`。
    fn on_event(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// 布局节点 A 型：约束变换（对标 Compose LayoutModifier）。
/// 输入 incoming 约束，输出给下一环节的约束。多个 A 型 node 按挂载序串行。
/// 插入点 = resolved_size 之后、padding 之前（与现有链序同，见 measure_node）。
/// 动态值在此求值（measure 期读 State 注册布局依赖——与 Dimension::Dynamic 同）。
///
/// 纯度 MUST（P1-6）：`transform` MUST 为“key 参数 + State::get”的纯函数——
/// 禁止读外部可变（Atomic/时钟/RefCell/全局）。常量折叠按 incoming 缓存，
/// 同 key 同约束直接返回旧尺寸；非纯读取即 stale（枚举侧无此口子，node 独有）。
pub trait LayoutModifierNode: std::fmt::Debug + Send + Sync {
    fn transform(&self, inner: crate::layout::Constraints) -> crate::layout::Constraints;
    /// Skip 指纹 MUST 规范（同 DrawNode）。默认 = TypeId 名。
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// Open node container (second track alongside `ModifierElement`).
#[derive(Debug, Clone)]
pub enum ModifierNode {
    Draw(std::sync::Arc<dyn DrawNode>),
    /// Wrapping draw node (before + after content). Renders at the same slots
    /// as Draw for before, and after children/ripple for after.
    DrawWrap(std::sync::Arc<dyn DrawWrapNode>),
    Click(std::sync::Arc<dyn ClickNode>),
    Pointer(std::sync::Arc<dyn PointerNode>),
    Key(std::sync::Arc<dyn KeyNode>),
    Layout(std::sync::Arc<dyn LayoutModifierNode>),
}

/// Modifier 链中的单个元素。
///
/// 按类别分为 Layout / Draw / Input 三类。
/// 用 enum 而非 trait object，便于后续 Layout 系统分类提取。
/// Which side of the window an inset-sized node takes its measurement from — Compose's
/// `WindowInsetsWidthCalculator` / `WindowInsetsHeightCalculator` pairs (`WindowInsetsSize.kt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsetSizeSide {
    /// The inline start: the left inset in LTR, the right one in RTL.
    Start,
    /// The inline end: the right inset in LTR, the left one in RTL.
    End,
    Top,
    Bottom,
}

/// One resolved size-shaping step, in chain order — see [`Modifier::size_steps`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum SizeStep {
    /// A `Size` element: resolved fixed axes. `None` is Auto/Intrinsic — those say nothing here
    /// (an intrinsic axis is resolved after the walk, by `intrinsic_*_request`).
    Size { width: Option<f32>, height: Option<f32> },
    /// A `RequiredSize` element: resolved numeric axes; the intrinsic overloads are handled later.
    RequiredSize { width: Option<f32>, height: Option<f32> },
    /// A `fillMax*` element, applied where it stands: the share of the CURRENT maximum.
    Fill { width: Option<f32>, height: Option<f32> },
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    /// A resolved `SizeIn`; `None` is Compose's `Dp.Unspecified`.
    SizeIn {
        min_width: Option<f32>,
        min_height: Option<f32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        enforce_incoming: bool,
    },
    /// `defaultMinSize` — the minimum applies only where the incoming one is 0.
    DefaultMin {
        min_width: Option<f32>,
        min_height: Option<f32>,
    },
    /// A `width/height(IntrinsicSize)` request, kept in chain order so the measure pipeline can
    /// snapshot the constraints the request saw. A `fillMax` outside the request tightens those
    /// constraints (and the intrinsic answer honours them); a `fillMax` inside it does not.
    Intrinsic { width: bool, height: bool },
    /// `aspectRatio` — Compose's `AspectRatioNode`: derive a size from the constraints at THIS
    /// position and fix what follows to it (`Constraints.fixed`). Emitted in chain order so the
    /// derivation sees exactly the constraints the aspect modifier would.
    AspectRatio { ratio: f32, match_height_first: bool },
    /// A `padding` element, resolved. Compose's padding node deflates the constraints at ITS position,
    /// so this is a step too: `.padding(8).size(300)` must fix 300 inside a range already 16 smaller,
    /// and `.size(300).padding(8)` must deflate afterwards. Emitted in chain order; the node's
    /// OUTER size still adds the total padding at the end.
    Padding { start: f32, top: f32, end: f32, bottom: f32 },
    /// A `windowInsetsPadding` element: the inset is resolved from the window state DURING the walk,
    /// at this element's chain position (it is consumed for the subtree there). The payload is an
    /// INDEX into `Modifier::elements` (the controller is not `Copy`), and `size_steps` emits exactly
    /// one step per element, so the index is stable for the chain it was built from.
    InsetPadding(usize),
    /// A `windowInsets*Width` / `windowInsets*Height` element at its chain position: it does not pad,
    /// it SETS one axis to the (unconsumed) inset on that side and measures the child inside it.
    InsetSize(usize),
    /// A consume-only `consumeWindowInsets` element, at its chain position: it pads by nothing and
    /// only marks the inset as consumed for what follows.
    ConsumeInsets(usize),
    /// `wrapContent*` — relax the incoming minimum (and max when unbounded) for what follows.
    Wrap {
        width: bool,
        height: bool,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    },
}

#[derive(Clone)]
pub(crate) enum ModifierElement {
    // ── Layout 类 ──
    /// 固定尺寸
    /// 尺寸（静态 Dimension 或动态求值 SizeValue——布局属性动画用 State/闭包）
    Size { width: SizeValue, height: SizeValue },
    /// 最小宽度（对标 Compose `Modifier.widthIn(min=...)`——仅提升 incoming
    /// min 约束；宽度超限时被 max 夹住；支持动态值——动画可作用于 min）
    MinWidth { value: Dimension },
    /// 最小高度（对标 Compose `Modifier.heightIn(min=...)`）
    MinHeight { value: Dimension },
    /// Maximum width (Compose `Modifier.widthIn(max = ...)`) — lowers the incoming max
    /// constraint, so content that would be wider is constrained to it, and is then held at or
    /// above the min. Supports a dynamic value.
    ///
    /// A min and a max on the same axis may conflict; which one wins is Compose's chain-order rule,
    /// replayed by [`Modifier::size_steps`] — the max written first yields the max, the min
    /// written first yields the min.
    MaxWidth { value: Dimension },
    /// Maximum height (Compose `Modifier.heightIn(max = ...)`)
    MaxHeight { value: Dimension },
    /// 四边 padding（每边独立，支持动态 SizeValue——动画可作用于 padding）
    PaddingSides {
        start: Dimension,
        top: Dimension,
        end: Dimension,
        bottom: Dimension,
    },
    /// Padding equal to a part of the window's insets (Compose's windowInsetsPadding). The applied
    /// insets are consumed for the subtree, so a nested call pads by nothing.
    WindowInsetsPadding {
        kind: crate::window::insets::InsetsKind,
        controller: crate::window::insets::WindowInsetsController,
    },
    /// Consume a part of the window's insets without padding (Compose's consumeWindowInsets).
    ConsumeWindowInsets {
        kind: crate::window::insets::InsetsKind,
        controller: crate::window::insets::WindowInsetsController,
    },
    /// Set ONE axis to the unconsumed inset on a side — Compose's `windowInsetsStartWidth` and
    /// friends (`WindowInsetsSize.kt`). The child is measured inside that size and the node reports
    /// (inset, child cross). It does not consume the inset.
    WindowInsetsSize {
        kind: crate::window::insets::InsetsKind,
        controller: crate::window::insets::WindowInsetsController,
        side: InsetSizeSide,
    },
    /// Fill the incoming maximum width — Compose `fillMaxWidth`. `fraction < 1` takes that share
    /// of the maximum; an unbounded maximum is a no-op (Compose's `FillNode`).
    FillMaxWidth { fraction: f32 },
    /// Fill the incoming maximum height (see `FillMaxWidth` for the fraction rules).
    FillMaxHeight { fraction: f32 },
    /// Fill the incoming maximum on both axes.
    FillMaxSize { fraction: f32 },
    /// A size RANGE on one or both axes — Compose's `widthIn`/`heightIn`/`sizeIn`
    /// (`enforce_incoming = true`) and `requiredWidthIn`/`requiredSizeIn` (`false`). Bounds are
    /// resolved during measure, so a `Dimension::Dynamic` animates through layout.
    SizeIn {
        min_width: Option<Dimension>,
        min_height: Option<Dimension>,
        max_width: Option<Dimension>,
        max_height: Option<Dimension>,
        enforce_incoming: bool,
    },
    /// Compose's `defaultMinSize`: a minimum that applies only where the incoming minimum is 0.
    DefaultMinSize {
        min_width: Option<Dimension>,
        min_height: Option<Dimension>,
    },
    /// Compose's `wrapContentWidth`/`wrapContentHeight`/`wrapContentSize`: measure without the
    /// incoming minimum (and maximum, when `unbounded`) and align the content inside the resulting box.
    WrapContent {
        width: bool,
        height: bool,
        align: crate::layout::ContentAlignment,
        unbounded: bool,
    },
    /// 位置偏移（不影响布局尺寸，仅移动绘制位置；RTL 下 x 镜像——
    /// 对标 Compose `Modifier.offset`；值支持动态——动画可作用于 offset）
    Offset { x: Dimension, y: Dimension },
    /// 绝对偏移（RTL 下**不**镜像——对标 Compose `Modifier.absoluteOffset`）
    AbsoluteOffset { x: Dimension, y: Dimension },
    /// 子节点在父容器中的交叉轴对齐（覆盖父容器的默认对齐）
    AlignSelf { alignment: crate::layout::Alignment },
    /// A child's own alignment inside a `Box`/`Stack`, on BOTH axes — Compose's
    /// `BoxScope.Modifier.align(Alignment.TopEnd)`, which a one-axis `Alignment` cannot express.
    ContentAlignSelf { alignment: crate::layout::ContentAlignment },
    /// Compose's `BoxScope.matchParentSize`: the child does NOT take part in sizing its box and is
    /// then measured at the box's size. Only a `Box`/`Stack` reads it.
    MatchParentSize,
    /// A grid item placed into a NAMED area — Compose's `GridScope.Modifier.gridItem(areaId)`
    /// (`Grid.kt:226`). Only `Grid` reads it; an unknown id falls back to auto-placement.
    GridItemArea { area_id: u64 },
    /// A grid item's span and cell alignment — Compose's `GridScope.Modifier.gridItem`
    /// (`Grid.kt:158-230`). Only `Grid` reads it.
    /// A flex item's share of its FlexBox — Compose's Modifier.flex(FlexConfig)
    /// (FlexBox.kt:2215-2253). Only FlexBox reads it.
    FlexItem(crate::layout::flexbox::FlexItemConfig),
    GridItem {
        column_span: u32,
        row_span: u32,
        alignment: Option<crate::layout::ContentAlignment>,
    },
    /// Compose's `Modifier.visible` (`Visible.kt:51`): while `false` the node is measured and keeps
    /// its space, but it is not painted, not hit-tested, not focusable and invisible to
    /// accessibility — `View.INVISIBLE`, not `View.GONE`.
    Visible { visible: bool },
    /// Align this child by one of its own alignment lines instead of by its edge (Compose's
    /// `RowScope`/`ColumnScope` `Modifier.alignBy`). See [`Modifier::align_by`].
    AlignBy { line: crate::layout::AlignmentLine },
    /// FlowRowScope's `fillMaxRowHeight`: fill this child to a fraction of the tallest item in its
    /// flow row. Only the flow row reads it.
    FillMaxRowHeight { fraction: f32 },
    /// FlowColumnScope's `fillMaxColumnWidth`: fill this child to a fraction of the widest item in
    /// its flow column. Only the flow column reads it.
    FillMaxColumnWidth { fraction: f32 },
    /// A line a node reports, computed from its measured size — see
    /// [`Modifier::alignment_line`]. Compose's `layout(alignmentLines = …)` in modifier form.
    AlignmentLineValue {
        line: crate::layout::AlignmentLine,
        value: std::sync::Arc<dyn Fn(crate::unit::Size) -> f32 + Send + Sync>,
    },
    /// Reserve a touch target of at least this many logical pixels on BOTH axes, centring the content
    /// inside it — Compose's `Modifier.minimumInteractiveComponentSize`. `size <= 0` is off.
    MinimumInteractiveSize { size: f32 },
    /// Pad this node so its content's alignment line sits `before` from the near edge and `after`
    /// from the far one (Compose's `Modifier.paddingFrom`). `None` is Compose's
    /// `Dp.Unspecified` — that side is not constrained.
    ///
    /// The axis follows the line's orientation, and the padding is worked out AFTER the content is
    /// measured, because it is the content's line that says how much padding is needed.
    PaddingFrom {
        line: crate::layout::AlignmentLine,
        before: Option<f32>,
        after: Option<f32>,
    },
    /// 布局权重（Row 中分配宽度，Column 中分配高度）
    ///
    /// `fill` mirrors Compose's `weight(weight, fill)`: with `fill = true` the child is measured
    /// tight to its share and the parent keeps that share (`ColumnScope.weight`'s documented
    /// behaviour). With `fill = false` the child is measured with its share as the MAXIMUM and the
    /// parent keeps the size the child actually asked for. That is the half material3's date picker
    /// dialog depends on to collapse in input mode — `DatePickerDialog.android.kt:95` wraps its
    /// content in `Box(Modifier.weight(1f, fill = false))`, commented "Fill is false to support
    /// collapsing the dialog's height when switching to input mode". The arrangement does not matter:
    /// since the `Arrangement` variants were aligned with Compose (`layout/flex.rs`'s
    /// `measured_main`), a spreading arrangement no longer grows a content-sized container to the
    /// maximum its parent offers, so the collapse survives `SpaceBetween` as well — which is what
    /// material3 itself relies on.
    ///
    /// The share really is a MAXIMUM for a `fill = false` child: a child asking for more is coerced
    /// into it, because a `Size` is clamped into the incoming range (`layout/node.rs`'s measure, the
    /// same rule `Modifier::size` documents) before anything flex computes. `Modifier::required_size`
    /// is the escape hatch for a child that must leave the share on purpose.
    LayoutWeight { weight: f32, fill: bool },
    /// 宽高比约束（对标 Compose `Modifier.aspectRatio`——ratio = 宽/高）
    AspectRatio { ratio: f32, match_height_first: bool },
    /// 强制尺寸（对标 Compose `Modifier.requiredSize`——忽略 incoming
    /// constraints 的收缩，允许溢出父约束）
    RequiredSize { width: Option<SizeValue>, height: Option<SizeValue> },
    /// 尺寸上报（对标 Compose `Modifier.onSizeChanged`）——节点测量完成后
    /// 以逻辑像素回调 (width, height)；元素内部去重，尺寸未变化不重复回调
    OnSizeChanged { callback: std::sync::Arc<dyn Fn(f32, f32) + Send + Sync> },
    /// Fires with the placed box, in window coordinates, when it changes — Compose's
    /// `onGloballyPositioned`. Delivered after `OnPlaced` for the same pass.
    OnGloballyPositioned { callback: std::sync::Arc<dyn Fn(crate::layout::GlobalPosition) + Send + Sync> },
    /// Compose's `onPlaced`: the same box, delivered before the `OnGloballyPositioned` callbacks.
    OnPlaced { callback: std::sync::Arc<dyn Fn(crate::layout::GlobalPosition) + Send + Sync> },
    /// Compose's `layoutId`: a tag for this layout node, visible in the debug tree.
    LayoutId { id: std::sync::Arc<str> },
    /// Compose's `bringIntoViewRequester`: this node's requests are served after the layout pass.
    BringIntoViewRequester { requester: crate::layout::BringIntoViewRequester },
    /// 测试标记（对标 Compose `Modifier.testTag`——UI 测试定位；
    /// 调试树 JSON 暴露 tag 字段）
    TestTag { tag: String },
    /// Accessibility declaration (role / name / state) — resolved into the semantics tree by
    /// `crate::semantics`. A node with this element still does layout as usual; the element exists
    /// only to be read (like `TestTag`), so it is a no-op for layout, drawing and input.
    Semantics(crate::semantics::SemanticsConfig),
    /// Compose's `clearAndSetSemantics`: REPLACES this node's declaration and hides its whole
    /// subtree from the semantics tree.
    ClearAndSetSemantics(crate::semantics::SemanticsConfig),
    /// Compose's `focusGroup()`: the node is a traversal group that remembers which child had focus.
    FocusGroup,
    /// Compose's `focusProperties { canFocus = … }` — the part that gates focus eligibility.
    FocusProperties { can_focus: Option<bool> },
    /// Compose's `onFocusChanged`: the callback receives `true` when the node gains focus, `false`
    /// when it loses it. Dispatched when the flag changes, including a loss caused by removal.
    OnFocusChanged { callback: std::sync::Arc<dyn Fn(bool) + Send + Sync> },
    /// 布局方向——本节点 padding start/end 的解析方向；Row/Column 组件
    /// 优先读自身此元素（其次 CompositionLocal 全局方向）。
    /// ⚠ 当前**不**向子树继承（对标 Compose 的子树级方向用
    /// `WiniaTheme::with_theme_and_direction`——CompositionLocal 作用域；
    /// 全 demo 切换也走它）。此元素是节点级便捷覆盖。
    LayoutDirection(crate::layout::LayoutDirection),
    /// Sibling paint order within this node (`Modifier::z_index`) — Compose's `Modifier.zIndex`.
    /// A higher value paints later, i.e. on top, and the hit test walks the same order in reverse so
    /// the topmost sibling receives a press first; equal values keep tree order.
    ZIndex(f32),
    /// TextField 容器子节点角色标记（text-field-v2 容器化——自定义
    /// MeasurePolicy 按角色布局：leading/label/placeholder/prefix/
    /// input/suffix/trailing；仅标记，不参与测量/绘制）
    TextFieldSlot { role: crate::text::field::TextFieldSlotRole },
    /// 阴影（对标 Compose `Modifier.shadow`——elevation 模糊 + 内容裁剪）
    /// 阴影（对标 Compose `Modifier.shadow`——单层参数；elevation 便捷版
    /// 展开为 ambient+spot 两层元素）
    Shadow {
        params: crate::graphics::Dyn<ShadowParams>,
        shape: crate::graphics::Dyn<Shape>,
        clip: bool,
    },

    // ── Draw 类 ──
    /// Background colour + shape. The colour is a [`Dyn`](crate::graphics::Dyn): a literal is stored
    /// inline, a closure is evaluated per paint.
    Background { color: crate::graphics::Dyn<Color>, shape: crate::graphics::Dyn<Shape> },
    /// A gradient or brush fill. Separate from `Background` because a brush is not a color: it needs a
    /// shader, and its geometry is resolved against the node's bounds at paint time.
    BackgroundBrush {
        brush: crate::graphics::Dyn<crate::graphics::Brush>,
        shape: crate::graphics::Dyn<Shape>,
    },
    /// A border stroke. The colour and the shape are carriers: a literal is inline, a state or a
    /// closure is read at paint time. One element covers both - there is no `BorderDynamic` anymore.
    Border {
        width: Dimension,
        color: crate::graphics::Dyn<Color>,
        shape: crate::graphics::Dyn<Shape>,
    },
    /// 裁剪
    Clip { shape: crate::graphics::Dyn<Shape> },
    /// 内容模糊（GPU 原生，零回读）。The radius is a carrier: a state or closure is read per paint.
    Blur { radius: crate::graphics::Dyn<f32> },
    /// 背景模糊（毛玻璃，单 snapshot 多节点共享）。Carrier radius, same as `Blur`.
    BackdropBlur { radius: crate::graphics::Dyn<f32> },
    /// 文本输入框容器视觉（TextField 组件——M3 Filled/Outlined 容器：    /// 背景/指示线/边框/label/支持文本）。组合期解析全部视觉状态
    /// （enabled/focused/is_error/label 悬浮），渲染期静态绘制——
    /// 状态过渡动画由后续迭代接入。
    TextFieldVisual {
        variant: crate::text::field::TextFieldVariant,
        shape: crate::graphics::Dyn<Shape>,
        colors: crate::text::field::TextFieldColors,
        enabled: bool,
        /// material3's `readOnly`, carried here because the RENDER needs it: material3 draws its caret as
        /// `showCursor = enabled && !readOnly && …` (`foundation/text/CoreTextField.kt`), so a read-only
        /// field shows no caret. winia drew one whenever the field had focus.
        read_only: bool,
        /// 光标色（组合期解析：error → error_cursor，否则 primary）
        cursor_color: Color,
        /// The indicator/border colour: an animation `State` driven by `animate_color_as_state`,
        /// interpolated in Oklab, peeked during rendering.
        indicator_color: crate::runtime::state::State<crate::graphics::Color>,
        /// 焦点过渡进度（0 = unfocused，1 = focused——宽度 1↔2px 动画）
        focus_progress: crate::runtime::state::State<f32>,
        /// 视觉变换偏移映射（密码掩码/格式化——渲染/定位跨界转换；
        /// None = 恒等）
        offset_mapping: Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>>,
        /// 支持文本（画在容器底部外侧 4dp）
        supporting: Option<SupportingVisual>,
    },
    /// 文本输入框偏移映射（TextField 组件内部使用——**无 M3 容器视觉**的
    /// 裸 TextField 挂载点）。TextFieldVisual 只在有 variant（Filled/Outlined）
    /// 时挂载；裸 TextField + visual_transformation 时 offset_mapping 无处
    /// 可达（点击定位/渲染光标查找 None → 显示偏移直写 selection 越界）。
    /// 仅存映射、无渲染/绘制副作用（render 各 match 走 `_ =>` 兜底）。
    TextFieldOffsetMapping {
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
    },

    // ── Content 类 ──
    /// 文本内容（由 Text 组件设置，渲染阶段消费）
    TextContent { content: String, font_size: f32, color: Color, font_weight: crate::text::FontWeight, font_style: crate::text::FontSlant, max_lines: usize, align: crate::text::TextAlign, overflow: crate::text::TextOverflow, soft_wrap: bool, letter_spacing: f32, line_height: Option<f32>, min_lines: usize, underline: bool, line_through: bool, font_family: Option<String>, on_text_layout: Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult) + Send + Sync>>, auto_size: Option<(f32, f32)> },
    /// 富文本内容（含内联 drawable，由 RichText 组件设置）
    RichTextContent {
        content: String,
        drawables: Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>,
        /// 内联元素在 content 中的字符范围（长度 > 1 表示多字符占位符）
        drawable_ranges: Vec<Range<usize>>,
        /// 已解析的样式范围列表
        spans: Vec<RichSpanStyle>,
        /// Paragraph parameters. `RichText` used to build a bare `ParagraphStyle`, so a rich
        /// block could not cap its lines, ellipsize, align or space its lines at all — it just
        /// wrapped. Compose hands the same set to `BasicText` whether the text is annotated or
        /// not, and these are that set.
        align: crate::text::TextAlign,
        max_lines: usize,
        overflow: crate::text::TextOverflow,
        soft_wrap: bool,
        /// Fixed line height in logical pixels (`None` = the font's own).
        line_height: Option<f32>,
        min_lines: usize,
    },

    // ── Input 类 ──
    /// 可点击
    /// 可点击——`interaction` 为 Some 时自动发射 press/focus/hover 交互
    /// （对标 Compose clickable(interactionSource)）
    Clickable { on_click: Arc<dyn Fn() + Send + Sync>, interaction: Option<MutableInteractionSource> },
    /// 点按手势回调（对标 Compose `detectTapGestures` 的 onTap）——
    /// 位置参数为组件本地坐标
    TapOnTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 双击回调
    TapOnDoubleTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 长按回调（当前在 up 时判定——与 Compose 即时触发有差异）
    TapOnLongPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 按下回调（down 立即触发）
    TapOnPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 拖拽开始（首次超过 touch slop）
    DragOnStart { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// 拖拽移动（当前位置, 增量）
    DragOnMove { cb: Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync> },
    /// 拖拽结束
    DragOnEnd { cb: Arc<dyn Fn() + Send + Sync> },
    /// 拖拽取消（系统打断）
    DragOnCancel { cb: Arc<dyn Fn() + Send + Sync> },
    /// 可获得焦点——`interaction` 为 Some 时焦点变化自动发射 Focus/Unfocus
    Focusable { interaction: Option<MutableInteractionSource> },
    /// 悬停——指针进入/离开自动发射 Hover Enter/Exit（对标 Compose hoverable）
    Hoverable { interaction: MutableInteractionSource },
    /// The cursor this node asks for while hovered — Compose Desktop's `pointerHoverIcon`.
    /// `override_descendants` makes this node's icon win over any icon on a descendant.
    PointerHoverIcon {
        icon: crate::input::PointerIcon,
        override_descendants: bool,
    },
    /// 水波纹指示（对标 Compose indication/ripple）——按下时从按压点扩散的
    /// 径向渐变圆，释放后淡出；渲染期按时间计算，无额外动画状态。
    /// `shape = Some` 时波纹裁剪到该形状（Button 传入容器 shape——
    /// Outlined/Text 无背景元素时也能正确裁剪）；None 则从 Background/Border 推断。
    Ripple {
        source: MutableInteractionSource,
        /// The ripple colour — a literal, a state or a closure read per paint.
        color: crate::graphics::Dyn<Color>,
        bounded: bool,
        shape: Option<crate::graphics::Dyn<Shape>>,
        /// The BACKGROUND state layer's diameter when it is fixed (Compose's `ripple(bounded, radius)` names a
        /// radius; material3's checkbox and radio pass `StateLayerSize`, 40 dp). `None` = derive it from the
        /// node's diagonal, which is this framework's original behaviour.
        state_layer_diameter: Option<crate::graphics::Dyn<f32>>,
    },
    /// 图标绘制（Icon 组件内部使用）——source/tint/autoMirror/可变轴
    CustomDraw { f: Arc<dyn Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync> },
    /// 禁用框架焦点环（组件自绘焦点环时用——如 Slider 焦点环包围 thumb 而非整组件）
    NoFocusRing,
    DrawIcon { spec: crate::graphics::IconSpec },
    /// 图片内容（Image 组件——位图/SVG，ContentScale + 对齐 + alpha；
    /// 与 DrawIcon 的区别：不染色、按 ContentScale 缩放、对齐可控）
    ImageContent {
        source: crate::graphics::IconSource,
        content_scale: crate::graphics::ContentScale,
        alignment: crate::graphics::ImageAlignment,
        /// Image opacity — a literal, a state or a closure read per paint (a fade-in image).
        alpha: crate::graphics::Dyn<f32>,
        /// Optional colour filter — a literal or a paint-time carrier.
        color_filter: Option<crate::graphics::Dyn<ColorFilter>>,
        filter_quality: FilterQuality,
    },
    /// 焦点请求器 ID（与 FocusRequester 关联）
    FocusRequesterId { id: u64 },
    /// 键盘事件
    KbEvent {
        on_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
        on_pre_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
    },
    /// 指针事件
    PointerEvent {
        on_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
        on_pre_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
    },
    /// 垂直滚动（绑定偏移 State）
    /// 完整 ScrollState（offset + is_scroll_in_progress + fling_limit）——
    /// 输入路径（app.rs 拖拽/wheel）经节点取整状态做 fling/极限
    VerticalScroll { state: ScrollState },
    /// The scroll container at this node keeps its clip and its viewport, but the GESTURE paths skip
    /// it: what a lazy list with `user_scroll(false)` needs. Dropping the container instead (the old
    /// behaviour) also dropped the clip, so the list painted its items outside its own box.
    ScrollInputDisabled,
    /// Lazy 列表内容高度标记（LazyColumn 用——apply_scroll_delta 计算 max_offset；
    /// 节点自身高度是视口，内容总高由测量回写到此 State）
    LazyScroll { content_height: crate::runtime::state::Backchannel<f32>, reverse: bool },
    /// The item's placement/fade contract. Lazy lists and flow layouts read it; the renderer applies
    /// the host's paint channels. Inert outside an item-aware layout.
    AnimateItem(crate::animation::item::ItemAnimationSpec),
    /// The container's item-animation host. The renderer draws its exit pictures inside this
    /// container's scroll clip/translate, so a ghost cannot land outside the viewport.
    ItemAnimationHost(crate::layout::item_animation::ItemAnimationHost),
    /// 水平滚动
    HorizontalScroll { state: ScrollState, reverse: bool },
    /// Compose's inner shadow: a rim drawn inside the shape, above the content.
    InnerShadow {
        shape: crate::graphics::Dyn<Shape>,
        color: Color,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
    },
    /// Compose's `clipScrollableContainer(orientation)`: the main axis is clipped to the container,
    /// the cross axis keeps 30 dp of slack so a shadow there is not cut off.
    ClipScrollableContainer { vertical: bool },
    /// Compose's `overscroll(effect)`: the container absorbs the delta it cannot scroll into the
    /// effect instead of letting it fall through.
    Overscroll { effect: crate::overscroll::OverscrollEffect },
    /// 嵌套滚动连接（祖先可在 child 前后部分消费 delta/velocity）。
    NestedScroll { connection: Arc<dyn crate::nested_scroll::NestedScrollConnection> },
    /// 图形层变换（scale/alpha/rotation/translation——只触发重绘，不触发布局）
    GraphicsLayer { params: crate::graphics::Dyn<GraphicsLayerParams> },
    /// 共享元素转场标记（components::shared_transition——纯数据标记：配对身份 + 变形规格；
    /// 组合期注册端点（Phase 2），渲染期忽略（`_ =>` 兜底）。bounds 不进
    /// modifier——飞行是 render-phase 行为，bounds 变化永不强制 Enter）
    SharedTransition {
        scope_id: u64,
        key: String,
        kind: crate::transition::SharedKind,
        transform: crate::transition::BoundsTransform,
        path: crate::transition::PathMotion,
        /// Overlay z-order for the flying pair (Compose `zIndexInOverlay`,
        /// default 0). Orders retained ghosts back-to-front; in-tree targets
        /// keep tree order (documented Tier 0 limitation).
        z_index: f32,
        /// sharedBounds enter/exit (Compose `enter`/`exit` — target plays
        /// enter, source plays exit; `None` on sharedElement markers, which
        /// have no such parameters and always crossfade).
        enter: Option<crate::animation::visibility::VisibilityTransition>,
        exit: Option<crate::animation::visibility::VisibilityTransition>,
        /// Render this endpoint in the transition layer during the flight
        /// (Compose `renderInOverlayDuringTransition`, default `true`): the
        /// flying element escapes ancestor clips and ancestor layer
        /// transforms and paints above non-shared content. `false` keeps the
        /// pre-Phase-6 in-tree painting (ancestors keep clipping it). Frozen
        /// at flight resolve — flipping it mid-flight must not move the
        /// element between passes.
        render_in_overlay: bool,
        /// Which SCENE this end belongs to, when it is composed inside a scene host that publishes
        /// one (winia's nav: each transition layer provides its scene id, see
        /// `components::shared_transition::provide_nav_scene`). `None` outside such a host. The flight
        /// system uses it to pair ends that are BOTH alive — during a nav transition the outgoing
        /// and incoming scenes both carry the same shared key, and without a scene the winner is
        /// whichever the tree walk happened to visit last, which reads as a new switch every frame.
        scene: Option<u64>,
    },
    /// Elevate a **non-shared** subtree into the layer for the duration of a
    /// transition (Compose `Modifier.renderInSharedTransitionScopeOverlay`):
    /// pinned app bars / FABs keep their spatial relationship while shared
    /// elements fly over them, and return to ordinary tree order when the
    /// transition ends. `z_index` is Compose's `zIndexInOverlay` — shared
    /// endpoints default to 0, so a bar that must stay on top passes more.
    SharedScopeOverlay {
        scope_id: u64,
        z_index: f32,
    },
    /// Marks the subtree a scene host composed for ONE scene (winia's nav layers): `id` is that
    /// scene's stable id. Scene membership is read from ANCESTRY when it is needed instead of being
    /// stored on each shared marker, because a marker's modifier element is built once and then reused
    /// across frames, so a scene id captured there freezes its first value (measured: one end reported
    /// `scene=None` while the other reported the LEAVING scene, so a nav flight paired the leaving hero
    /// with itself and never grew geometrically).
    SceneTag {
        id: u64,
    },
}

