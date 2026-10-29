use super::*;

// ── ModifierElement ──

// ═══════════════════════════════════════════════════════════
// Modifier nodes: an experimental open extension point (branch exp/modifier-node).
// ───────────────────────────────────────────────────────────
// The goal is to turn `Modifier` from a closed enum into open nodes. Every new behaviour added to
// the enum has to touch four `match` sites (measure, render, hit testing, the app); with nodes a
// third party can add behaviour without editing the core.
//
// The design, two tracks, nothing broken:
// - `Modifier` gains `nodes: Vec<ModifierNode>` beside `elements`. The old builders
//   (background/clickable/...) keep pushing enum elements; extensions go through nodes.
// - Each behaviour is an `Arc<dyn XxxNode>`. Once the enum match is done, the core pipeline
//   (measure, render, input, debug) walks the node chain with the same semantics.
// - A node is `Debug + Send + Sync`, matching the existing element bounds. Click equality goes
//   through `node_key()` (TypeId plus an optional id) so `param_eq` can decide.
// - The pilot pair is BackgroundNode (draw) and ClickableNode (input); migrate one at a time once
//   "the enum untouched, the node chain working beside it" is proven.
//
// Stateful nodes: a node does not hold composition State itself, because `remember` has to be
// called inside a `#[composable] build` (the macro's statement-key context requires it). Instead:
// A component builds a State with `ctx.remember(...)`, clones it into the node (a State is an Arc
// wrapper, so the clone is cheap), reads it with `get()` at build time to register the
// compose/layout dependency, and evaluates it with `peek()` at paint time.
//
// A paint-time `get()` registers nothing (the dependency frame is gone by then: DEP_MODE is None -
// the same rule the draw-layer elements follow). Repaint comes from the build-time `get()` (a change
// recomposes -> new node -> repaint) or from an animation writing through `Animating` and requesting
// a redraw. The node has no private channel; it follows the elements (see
// `node_track_stateful_draw_follows_state`).
// e.g. `let src = ctx.remember(|| MutableInteractionSource::new()).get();`
//     `Modifier::new().click_node(MyClick { source: src.clone() })`。
// The state lives and dies with its composition slot (that is what `remember` means); a node is
// only behaviour plus a reference to that state. Removing the node lets the slot reclaim it, so
// there is no `onAttach`/`onDetach`.
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

/// A click node, queried along the hit path during input (Compose's click semantics).
///
/// Same precedence as the `Clickable` element: the core `on_click()` asks the enum first, then the
/// node chain.
///
/// First-only semantics: one Modifier activates only its first ClickNode, matching the enum's
/// `on_click()`, which takes the first Clickable. A second click node stays silent, so several
/// callbacks have to be merged into one node's `on_click`. `node_clicks()` exists for a future
/// multicast rule and is not wired up yet.
pub trait ClickNode: std::fmt::Debug + Send + Sync {
    fn on_click(&self);
    /// The bound interaction source, which is what drives the press ripple (`None` without one).
    fn interaction(&self) -> Option<crate::input::interaction::MutableInteractionSource> {
        None
    }
    /// The Skip-fingerprint rule, as for DrawNode: static parameters go into the key, transient or
    /// write-back animation values do not, and callbacks compare equal. The default is the TypeId
    /// name.
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// A pointer node: raw pointer events (Compose's `PointerInputModifierNode`).
///
/// Same semantics as the `PointerEvent` element: `pre` is the outside-in tunnel, `on` the
/// inside-out bubble, and returning true consumes the event. Custom gestures, hover handling and
/// drag detection hang here without touching the app.
///
/// Gesture routing stays separate, and that separation is load-bearing: a PointerNode does NOT
/// take part in `has_gesture()`'s target selection, because the gesture arena (tap and drag
/// recognition) answers only to the TapOn/DragOn elements. A pointer node sees the raw
/// down/move/up that `dispatch_ptr_event` delivers, which is what a hover log or a custom
/// coordinate transform needs.
///
/// A node that wants a gesture callback has to carry the matching element too (say `on_tap`), or
/// wait for the GestureNode the arena will bring. Counting a PointerNode as a gesture target would
/// silently steal the outer tap: the inner decorative node claims the target, `fire_gesture_action`
/// looks only at elements, and the outer TapOnTap never fires.
pub trait PointerNode: std::fmt::Debug + Send + Sync {
    /// The tunnel phase (outside in), `on_pre_ptr`.
    fn on_pre(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// The bubble phase (inside out), `on_ptr`.
    fn on_event(&self, _ev: &PointerEvent) -> bool {
        false
    }
    /// The Skip-fingerprint rule, as for DrawNode. The default is the TypeId name.
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// A keyboard node: key events (Compose's `KeyInputModifierNode`).
///
/// Same semantics as the `KbEvent` element: `pre` tunnels from root to focus, `on` bubbles from
/// focus to root, and returning true consumes the event. Custom shortcuts and input interception
/// hang here without touching the app.
pub trait KeyNode: std::fmt::Debug + Send + Sync {
    /// The tunnel phase (root to focus), `on_pre_key`.
    fn on_pre(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// The bubble phase (focus to root), `on_key`.
    fn on_event(&self, _ev: &KbEvent) -> bool {
        false
    }
    /// The Skip-fingerprint rule, as for DrawNode. The default is the TypeId name.
    fn node_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }
}

/// A layout node, type A: a constraint transform (Compose's `LayoutModifier`).
///
/// It takes the incoming constraints and hands the next link its own. Several type-A nodes run in
/// mounting order. The insertion point is after `resolved_size` and before padding, which is where
/// the existing chain puts it (see `measure_node`). Dynamic values are evaluated here, and reading
/// a State during measure registers the layout dependency, exactly as `Dimension::Dynamic` does.
///
/// Purity is required: `transform` must be a pure function of its key parameters and `State::get`.
/// Reading anything else mutable (an Atomic, the clock, a RefCell, a global) breaks the constant
/// folding, which caches per key and per incoming constraint and returns the old size on a match; a
/// non-pure read goes stale. The enum side has no such folding, so this is a node-only hazard.
pub trait LayoutModifierNode: std::fmt::Debug + Send + Sync {
    fn transform(&self, inner: crate::layout::Constraints) -> crate::layout::Constraints;
    /// The Skip-fingerprint rule, as for DrawNode. The default is the TypeId name.
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

/// One element in a `Modifier` chain.
///
/// Category: layout, draw, input, or content.
/// An enum rather than trait objects, so the layout system can classify an element up front.
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
    // ── Layout ──
    /// A size.
    /// A size: a static `Dimension`, or a `SizeValue` evaluated at layout time (a State or closure,
    /// which is how a layout property animates).
    Size { width: SizeValue, height: SizeValue },
    /// A minimum width (Compose's `Modifier.widthIn(min=...)`): it only raises an incoming min
    /// constraint, the max still clamps, and a dynamic value lets an animation drive the min.
    MinWidth { value: Dimension },
    /// A minimum height (Compose's `Modifier.heightIn(min=...)`).
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
    /// Padding on each of the four edges, each one a dynamic `SizeValue` so it can animate.
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
    /// A positional offset that does not change the layout size, only where the node paints; x
    /// mirrors under RTL. Compose's `Modifier.offset`, with a dynamic value so it can animate.
    Offset { x: Dimension, y: Dimension },
    /// An absolute offset, which does NOT mirror under RTL — Compose's `Modifier.absoluteOffset`.
    AbsoluteOffset { x: Dimension, y: Dimension },
    /// How the child aligns on the parent's cross axis, overriding the parent's default.
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
    /// Layout weight: width in a Row, height in a Column.
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
    /// An aspect ratio — Compose's `Modifier.aspectRatio`, where the ratio is width over height.
    AspectRatio { ratio: f32, match_height_first: bool },
    /// A required size (Compose's `Modifier.requiredSize`): the incoming constraints are ignored
    /// rather than shrunk to, so the node may overflow its parent.
    RequiredSize { width: Option<SizeValue>, height: Option<SizeValue> },
    /// A size report (Compose's `Modifier.onSizeChanged`): once the node is measured, the size in
    /// logical pixels. The element dedupes, so an unchanged size does not call back again.
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
    /// A test tag — Compose's `Modifier.testTag`, which UI tests target and the debug tree exposes
    /// in its JSON.
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
    /// The layout direction that resolves this node's start/end padding. A Row/Column reads this
    /// element on itself first, then the CompositionLocal. It does NOT inherit into a subtree: the
    /// subtree-level direction Compose offers is `WiniaTheme::with_theme_and_direction` (a
    /// CompositionLocal scope), and that is also how a whole demo flips. This element is the
    /// per-node override.
    LayoutDirection(crate::layout::LayoutDirection),
    /// Sibling paint order within this node (`Modifier::z_index`) — Compose's `Modifier.zIndex`.
    /// A higher value paints later, i.e. on top, and the hit test walks the same order in reverse so
    /// the topmost sibling receives a press first; equal values keep tree order.
    ZIndex(f32),
    /// A text-field child's role marker. The container's own measure policy places each role
    /// (leading, label, placeholder, prefix, input, suffix, trailing); it is a marker only and takes
    /// no part in measure or paint.
    TextFieldSlot { role: crate::text::field::TextFieldSlotRole },
    /// A shadow — Compose's `Modifier.shadow`: a blurred elevation plus clipping the content.
    /// A shadow with single-layer parameters. The elevation shortcut expands into two elements,
    /// ambient and spot.
    Shadow {
        params: crate::graphics::Dyn<ShadowParams>,
        shape: crate::graphics::Dyn<Shape>,
        clip: bool,
    },

    // ── Draw ──
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
    /// A clip.
    Clip { shape: crate::graphics::Dyn<Shape> },
    /// A content blur (native GPU, nothing read back). The radius is a carrier: a state or closure
    /// is read per paint.
    Blur { radius: crate::graphics::Dyn<f32> },
    /// A background blur (frosted glass, one snapshot shared across nodes). Same carrier radius as
    /// `Blur`.
    BackdropBlur { radius: crate::graphics::Dyn<f32> },
    /// The text-field container's visuals (internal to `TextField`): the M3 Filled/Outlined
    /// background, indicator, outline, label and supporting text. Every visual state (enabled,
    /// focused, in-error, label floated) is resolved at compose time and painted statically at
    /// render time; the transition animations are a later step.
    TextFieldVisual {
        variant: crate::text::field::TextFieldVariant,
        shape: crate::graphics::Dyn<Shape>,
        colors: crate::text::field::TextFieldColors,
        enabled: bool,
        /// material3's `readOnly`, carried here because the RENDER needs it: material3 draws its caret as
        /// `showCursor = enabled && !readOnly && …` (`foundation/text/CoreTextField.kt`), so a read-only
        /// field shows no caret. winia drew one whenever the field had focus.
        read_only: bool,
        /// The caret colour, resolved at compose time: error_cursor in the error state, primary
        /// otherwise.
        cursor_color: Color,
        /// The indicator/border colour: an animation `State` driven by `animate_color_as_state`,
        /// interpolated in Oklab, peeked during rendering.
        indicator_color: crate::runtime::state::State<crate::graphics::Color>,
        /// The focus transition, 0 unfocused to 1 focused, which animates the 1 to 2 px width.
        focus_progress: crate::runtime::state::State<f32>,
        /// The visual transformation's offset mapping (password masking, formatted input), which
        /// every render and hit-test crossing needs. `None` means identity.
        offset_mapping: Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>>,
        /// Supporting text, drawn 4 dp below the container.
        supporting: Option<SupportingVisual>,
    },
    /// A text field's offset mapping, and the mount point for a bare `TextField` with NO M3
    /// container visuals. `TextFieldVisual` exists only when a variant (Filled/Outlined) is set, so
    /// without this element a bare field with a visual transformation would have no reachable
    /// mapping: click targeting and the caret lookup would see `None` and write a display offset
    /// into the selection, out of range.
    ///
    /// It stores the mapping only and has no render effect; every render match arm falls through.
    TextFieldOffsetMapping {
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
    },

    // ── Content ──
    /// Text content, set by the `Text` component and consumed at render time.
    TextContent { content: String, font_size: f32, color: Color, font_weight: crate::text::FontWeight, font_style: crate::text::FontSlant, max_lines: usize, align: crate::text::TextAlign, overflow: crate::text::TextOverflow, soft_wrap: bool, letter_spacing: f32, line_height: Option<f32>, min_lines: usize, underline: bool, line_through: bool, font_family: Option<String>, on_text_layout: Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult) + Send + Sync>>, auto_size: Option<(f32, f32)> },
    /// Compose's richer `onTextLayout` sink: the snapshot plus the paragraph, so the per-line queries
    /// (`getLineForOffset`, `getBoundingBox`, …) can answer. Its own element, because the callback
    /// type would otherwise grow every hand-written `TextContent { .. }` literal in the suite.
    TextLayoutCallback {
        callback: std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync>,
    },
    /// How the glyphs are PAINTED, kept beside `TextContent` instead of inside it: a shadow and an
    /// OpenType feature string change the shaped paragraph without changing any of the measure-time
    /// parameters, and giving them their own element keeps every existing `TextContent { .. }`
    /// literal (the whole test suite builds them by hand) unchanged.
    TextPaint {
        shadow: Option<crate::text::TextShadow>,
        font_feature_settings: Option<String>,
        draw_style: Option<crate::text::TextDrawStyle>,
        brush: Option<crate::text::TextBrush>,
        transform: Option<crate::text::TextGeometricTransform>,
        line_height_style: Option<crate::text::LineHeightStyle>,
    },
    /// Rich-text content, inline drawables included, set by `RichText`.
    RichTextContent {
        content: String,
        drawables: Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>,
        /// An inline element's CHARACTER range in the content; a length above 1 is a multi-character
        /// placeholder.
        drawable_ranges: Vec<Range<usize>>,
        /// The resolved style runs.
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
        /// Compose's `lineHeightStyle`: where the run sits in a taller line, and what the ends trim.
        line_height_style: Option<crate::text::LineHeightStyle>,
    },

    // ── Input ──
    /// Clickable. With a `Some(interaction)` it emits press/focus/hover itself, like Compose's
    /// `clickable(interactionSource)`.
    Clickable { on_click: Arc<dyn Fn() + Send + Sync>, interaction: Option<MutableInteractionSource> },
    /// The tap callback (Compose's `onTap` in `detectTapGestures`); the position is in the
    /// component's local coordinates.
    TapOnTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// The double-tap callback.
    TapOnDoubleTap { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// The long-press callback. It fires on pointer up here, where Compose fires immediately — a
    /// known difference.
    TapOnLongPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// The press callback, fired on down.
    TapOnPress { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// Drag start, once the touch slop is first exceeded.
    DragOnStart { cb: Arc<dyn Fn((f32, f32)) + Send + Sync> },
    /// Drag move: the current position and the delta.
    DragOnMove { cb: Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync> },
    /// Drag end.
    DragOnEnd { cb: Arc<dyn Fn() + Send + Sync> },
    /// Drag cancel (the gesture was interrupted).
    DragOnCancel { cb: Arc<dyn Fn() + Send + Sync> },
    /// Focusable. With a `Some(interaction)` it emits Focus/Unfocus as focus changes.
    Focusable { interaction: Option<MutableInteractionSource> },
    /// Hover: pointer enter and exit emit Hover Enter/Exit (Compose's `hoverable`).
    Hoverable { interaction: MutableInteractionSource },
    /// The cursor this node asks for while hovered — Compose Desktop's `pointerHoverIcon`.
    /// `override_descendants` makes this node's icon win over any icon on a descendant.
    PointerHoverIcon {
        icon: crate::input::PointerIcon,
        override_descendants: bool,
    },
    /// A ripple (Compose's indication): a radial gradient that spreads from the press point and fades
    /// after release, computed from elapsed time at render time so it needs no animation state.
    ///
    /// A `Some(shape)` clips the ripple to it — a Button passes its container shape, so an Outlined
    /// or Text button without a background element still clips correctly. `None` infers the shape
    /// from a Background/Border element.
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
    /// Icon drawing (internal to `Icon`): source, tint, auto-mirror and the variable axes.
    CustomDraw { f: Arc<dyn Fn(&skia_safe::Canvas, skia_safe::Rect) + Send + Sync> },
    /// Suppress the framework's focus ring, for a component that draws its own (a Slider rings the
    /// thumb rather than the whole component).
    NoFocusRing,
    DrawIcon { spec: crate::graphics::IconSpec },
    /// Picture content (`Image`): a bitmap or SVG with a `ContentScale`, an alignment and an alpha.
    /// Unlike DrawIcon it does not tint, it scales by `ContentScale`, and the alignment is the
    /// caller's.
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
    /// The focus requester's id, tying the node to a `FocusRequester`.
    FocusRequesterId { id: u64 },
    /// Key events.
    KbEvent {
        on_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
        on_pre_key: Option<Arc<dyn Fn(&KbEvent) -> bool + Send + Sync>>,
    },
    /// Pointer events.
    PointerEvent {
        on_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
        on_pre_ptr: Option<Arc<dyn Fn(&PointerEvent) -> bool + Send + Sync>>,
    },
    /// Vertical scrolling, bound to an offset State.
    /// The whole `ScrollState` (offset, is_scroll_in_progress, fling_limit). The input path (drag
    /// and wheel in the app) reads it through the node to run a fling and to clamp at the limit.
    VerticalScroll { state: ScrollState },
    /// The scroll container at this node keeps its clip and its viewport, but the GESTURE paths skip
    /// it: what a lazy list with `user_scroll(false)` needs. Dropping the container instead (the old
    /// behaviour) also dropped the clip, so the list painted its items outside its own box.
    ScrollInputDisabled,
    /// The lazy list's content-height marker (used by LazyColumn to compute `max_offset` in
    /// `apply_scroll_delta`). The node's own height is the viewport; the content's total height is
    /// written back into this State by the measure pass.
    LazyScroll { content_height: crate::runtime::state::Backchannel<f32>, reverse: bool },
    /// The item's placement/fade contract. Lazy lists and flow layouts read it; the renderer applies
    /// the host's paint channels. Inert outside an item-aware layout.
    AnimateItem(crate::animation::item::ItemAnimationSpec),
    /// The container's item-animation host. The renderer draws its exit pictures inside this
    /// container's scroll clip/translate, so a ghost cannot land outside the viewport.
    ItemAnimationHost(crate::layout::item_animation::ItemAnimationHost),
    /// Horizontal scrolling.
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
    /// A nested-scroll connection: an ancestor may consume part of the delta/velocity before or
    /// after the child does.
    NestedScroll { connection: Arc<dyn crate::nested_scroll::NestedScrollConnection> },
    /// A graphic-layer transform (scale, alpha, rotation, translation): it repaints without
    /// relayouting.
    GraphicsLayer { params: crate::graphics::Dyn<GraphicsLayerParams> },
    /// A shared-element transition marker (pure data: the pairing identity plus the morph spec). The
    /// endpoint registers at compose time; render ignores it. Bounds never enter the modifier — the
    /// flight is a render-phase behaviour, so a bounds change never forces a re-entry.
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

