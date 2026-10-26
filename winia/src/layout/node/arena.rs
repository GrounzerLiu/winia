use super::*;

// ── LayoutNode ──

/// Where a node's pixels come from while a shared-element transition is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PaintDisposition {
    /// Ordinary node: the tree walk paints it where it sits.
    #[default]
    InTree,
    /// Painted by the transition layer instead of the tree walk, because the tree walk cannot escape an
    /// ancestor clip: this covers both a flight end lifted above the tree (Compose's shared-element
    /// overlay) and chrome that opted in (Compose `renderInSharedTransitionScopeOverlay`).
    InLayer,
    /// A marked node that a flight owns but is NOT an end of: the flight paints the animated copy, so
    /// this one is a placeholder and must not paint (Compose renders only the becoming-visible copy).
    /// Without it, a scene that re-composes a copy of a flying key painted a second, static, fully
    /// opaque element — measured on the demo: three nodes for `hero` in one frame, the extra one at its
    /// own 96x96 rect with `role: null`, `alpha: 1.00`.
    Placeholder,
}

/// How a container combines the values its children report for one line — Compose's
/// `AlignmentLine(merger)` (`ui/layout/AlignmentLine.kt:60-66`).
///
/// Compose holds a closure; winia holds the two mergers Compose actually ships, so a line stays a
/// `Copy` constant that can be compared and used in a `const`. A caller-defined merger is the part
/// that stays unaligned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineMerger {
    /// The smallest child value — the FIRST of several lines, as `FirstBaseline = (::min)` is.
    Min,
    /// The largest — the LAST, as `LastBaseline = (::max)` is.
    Max,
}

impl LineMerger {
    pub(crate) fn merge(self, a: f32, b: f32) -> f32 {
        match self {
            LineMerger::Min => a.min(b),
            LineMerger::Max => a.max(b),
        }
    }
}

/// A line a node reports and that a parent can align children by — Compose's `AlignmentLine`.
///
/// A container INHERITS its children's lines and publishes the merged value, shifted into its own
/// coordinates, which is what lets an outer Row align by a baseline that lives several levels down
/// (`measure_node_inner` does the merging, right after the policy has placed the children).
///
/// `horizontal` says which axis the line runs across, Compose's `HorizontalAlignmentLine` /
/// `VerticalAlignmentLine` split: a horizontal line is a distance from the node's TOP and a Row reads
/// it down its cross axis, while a vertical line is a distance from the node's START and a Column
/// aligns children by it. The two built-ins are the text baselines; anything else comes from
/// [`AlignmentLine::new`] and is published with [`Modifier::alignment_line`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AlignmentLine {
    pub horizontal: bool,
    pub(crate) id: u64,
    /// How a container merges several children's values for this line.
    pub(crate) merger: LineMerger,
}

/// Ids for caller-defined lines, so two lines built the same way are different lines.
///
/// The type compares by value, so a constant id would make every `AlignmentLine::vertical(Min)`
/// collide with every other one — the two built-ins have fixed ids below the counter's start.
pub(crate) static NEXT_ALIGNMENT_LINE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(2);

impl AlignmentLine {
    /// The distance from a node's top to the baseline of its first line of text
    /// (`AlignmentLine.FirstBaseline`, whose merger is `::min`). Published by text leaves, which is
    /// what makes `Modifier::align_by_baseline` useful.
    pub const FIRST_BASELINE: AlignmentLine =
        AlignmentLine { horizontal: true, id: 0, merger: LineMerger::Min };

    /// The distance to the baseline of the LAST line of text (`AlignmentLine.LastBaseline`,
    /// `::max`) — the line to align by when the bottom of the text block is what should line up.
    pub const LAST_BASELINE: AlignmentLine =
        AlignmentLine { horizontal: true, id: 1, merger: LineMerger::Max };

    /// A caller-defined line: Compose's `HorizontalAlignmentLine(merger)` /
    /// `VerticalAlignmentLine(merger)` in one constructor, chosen by `horizontal`.
    ///
    /// Each call is a NEW line — they never compare equal — so a component can hold one in a
    /// `static`/`OnceLock` and hand it out, the way material3 holds
    /// `MinimumInteractiveLeftAlignmentLine` and `MinimumInteractiveTopAlignmentLine`
    /// (`material3/InteractiveComponentSize.kt:166-167`).
    pub fn new(horizontal: bool, merger: LineMerger) -> Self {
        Self {
            horizontal,
            id: NEXT_ALIGNMENT_LINE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            merger,
        }
    }

    /// Compose's `HorizontalAlignmentLine(merger)`: a distance from the node's top.
    pub fn horizontal(merger: LineMerger) -> Self {
        Self::new(true, merger)
    }

    /// Compose's `VerticalAlignmentLine(merger)`: a distance from the node's start.
    pub fn vertical(merger: LineMerger) -> Self {
        Self::new(false, merger)
    }

    /// Apply this line's merger to two values, for a container combining its children's lines.
    pub fn merge(&self, a: f32, b: f32) -> f32 {
        self.merger.merge(a, b)
    }

    /// material3's `MinimumInteractiveLeftAlignmentLine`: how far the VISUAL content starts from the
    /// left of a box that `Modifier::minimum_interactive_size` enlarged to the touch-target minimum.
    ///
    /// A parent aligns a small component by this when its icon or label has to line up with its
    /// neighbours, rather than with the invisible padding the larger touch target adds
    /// (`material3/InteractiveComponentSize.kt:167`). Built once: a line is identified by value, so
    /// every call to [`AlignmentLine::vertical`] would otherwise make a different one.
    pub fn minimum_interactive_left() -> AlignmentLine {
        static LINE: std::sync::OnceLock<AlignmentLine> = std::sync::OnceLock::new();
        *LINE.get_or_init(|| AlignmentLine::vertical(LineMerger::Min))
    }

    /// material3's `MinimumInteractiveTopAlignmentLine` — the vertical twin of
    /// [`AlignmentLine::minimum_interactive_left`] (`material3/InteractiveComponentSize.kt:166`).
    pub fn minimum_interactive_top() -> AlignmentLine {
        static LINE: std::sync::OnceLock<AlignmentLine> = std::sync::OnceLock::new();
        *LINE.get_or_init(|| AlignmentLine::horizontal(LineMerger::Min))
    }
}

/// One node of the layout tree.
///
/// Each LayoutNode is one measurable/layoutable cell of the UI tree and carries the modifier chain
/// and its children.
pub struct LayoutNode {
    /// The unique identifier (an exact lookup handle for the render pass).
    pub id: u64,
    pub modifier: Modifier,
    pub measured_size: Size,
    pub position: Offset,
    /// The alignment lines this node reports, as offsets from its own top edge — Compose's
    /// `Measured[alignmentLine]`.
    ///
    /// Written by whoever measures the node (a text leaf publishes its baseline — see
    /// `measure_and_cache_text`), read by a Row/Column that a parent aligns children by
    /// (`Modifier::align_by`). Cleared at the start of every measure, so a line never outlives the
    /// measurement that produced it; an unchanged node is folded by `measure_node` and keeps its
    /// lines. Usually empty, and an empty `Vec` does not allocate.
    pub alignment_lines: Vec<(AlignmentLine, f32)>,
    /// The size of the CONTENT inside a node whose own `Modifier::minimum_interactive_size` grew the
    /// box, as `(width, height)`. Kept as a field rather than read back from the
    /// `MinimumInteractive*AlignmentLine`s because a container MERGES its children's alignment lines
    /// (`node.rs:3346-3367`), so every ancestor of a Button inherits that offset — and reading the line
    /// while painting an ancestor would inset the ancestor's own background. Only the node the modifier
    /// belongs to writes this, and only while painting itself does it read it.
    ///
    /// The CONTENT size, not the inset: the renderer paints the component's box where the modifier
    /// actually put it (centred in the node box), and a parent may force the node narrower than the
    /// touch target — a ButtonGroup squeezing an item does. An inset computed from the touch target
    /// then took twice the difference off the painted box, so a squeezed item drifted away from its
    /// leading edge and shrank past what the layout had asked for.
    pub minimum_interactive_content: Option<(f32, f32)>,
    /// The box to PAINT instead of deriving one from the content, as `(left, top, w, h)` inside the
    /// node. A parent that forces the node's width sets this — a ButtonGroup does, so a squeezed
    /// item paints narrower and a pressed one wider. It is kept apart from
    /// `minimum_interactive_content` on purpose: that one is the size the component DECLARED, and
    /// overwriting it with the painted width made each frame derive the inset from the last frame's
    /// paint (the two fed back into each other).
    pub minimum_interactive_painted: Option<(f32, f32, f32, f32)>,
    /// How far this node's children have ALREADY been shifted by the row that squeezed it. The
    /// shift has to be idempotent: `place` runs every frame, but a node whose constraints did not
    /// change is served from the fold cache and its children are NOT re-placed — so a plain
    /// `position -= shift` applies again and again and walks the content off the box.
    pub content_shift: Option<f32>,
    /// Where a LEAF's own content sits inside its box when a modifier moved it — the measured
    /// `(x, y, content width, content height)`, or `None` for the ordinary case.
    ///
    /// It has to be carried rather than recomputed: `Modifier::padding_from` places the content
    /// against its measured alignment line and `Modifier::minimum_interactive_size` centres it inside
    /// a larger box, and in both cases the renderer draws the content itself (a text draws at its
    /// node's origin, inset by the padding it can query). A container does not use this — it moves its
    /// children.
    pub content_box_override: Option<(f32, f32, f32, f32)>,
    /// The padding this node RESOLVED during its last measurement — element padding plus window
    /// insets, in `(start, top, end, bottom)` order, with the insets already consumed for the subtree.
    ///
    /// The engine needs it after the walk (outer size, placement shift) and the subcomposition
    /// adoption path needs it too: it used to recompute `get_padding_sides()` from the modifier, which
    /// drops the insets and re-resolves a dynamic dimension. This field is the single source.
    pub resolved_padding: (f32, f32, f32, f32),
    /// The children's indices (an arena tree: the nodes live in `NodeArena.nodes` and survive
    /// recomposition).
    pub children: Vec<usize>,
    /// The index of the measure policy in `NodeArena.policies` (a pool apart from the nodes, so the
    /// two borrows cannot clash).
    pub measure_policy: Option<usize>,
    /// Whether the leaf carries a TextContent.
    pub(crate) has_text_content: bool,
    /// Whether the leaf carries a RichTextContent.
    pub(crate) has_richtext_content: bool,
    /// Whether the leaf carries an ImageContent (the Image component).
    pub(crate) has_image_content: bool,
    /// Whether the node has focus.
    pub focused: bool,
    /// Called when the node leaves the layout tree (the Window lifecycle hooks in here).
    pub(crate) on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// Whether the node has to measure again (false when a clean slot was reused).
    pub(crate) dirty: bool,
    /// A layout-level invalidation (the two-stage dependency: a layout animation value re-measures
    /// without recomposing; cleared once measured).
    pub(crate) layout_dirty: bool,
    /// The constraints of the last measure (lets a constant layout skip re-measuring).
    pub(crate) cached_constraints: Option<Constraints>,
    /// The layout direction, snapshotted during composition (an explicit modifier beats the
    /// CompositionLocal default): measure and render use it to mirror padding start/end and to align
    /// text.
    pub(crate) layout_direction: LayoutDirection,
    /// The slot key of the composable call that produced the node (used to find children on replay).
    pub(crate) slot_key: u64,
    /// Whether any of this node's children carries `Modifier::z_index`, recorded in the layout pass
    /// that already walks them. `false` — every node that does not use z-index — lets the renderer and
    /// the hit test skip the ordering work entirely.
    pub(crate) children_have_z: bool,
    /// The Paragraph cached by the measure phase, so the render pass does not rebuild it.
    pub(crate) cached_paragraph: std::cell::RefCell<Option<crate::text::Paragraph>>,
    /// A scroll container's viewport height (written by `measure_node` and read by
    /// `apply_scroll_delta`).
    pub(crate) scroll_viewport_height: f32,
    /// A scroll container's viewport width (horizontal scrolling; the twin of
    /// `scroll_viewport_height`).
    pub(crate) scroll_viewport_width: f32,
    /// A scroll container's total content height (the lazy lists compute `max_offset` from it;
    /// 0 means unset).
    pub(crate) scroll_content_height: f32,
    /// A lazy list's reverse layout (the render pass mirrors the scroll translate to
    /// content - vh - offset).
    pub(crate) scroll_reverse: bool,
    /// A scroll container's total content width (horizontal scrolling; the twin of
    /// `scroll_content_height`).
    pub(crate) scroll_content_width: f32,
    /// The parent's id, set by `add_child` (keyboard events bubble with it).
    pub(crate) parent_id: Option<u64>,
    /// The last `(x, y, width, height)` reported to this node's position callbacks, in window
    /// coordinates. `None` means "never reported", so the first placement always fires.
    pub(crate) last_global_position: Option<(f32, f32, f32, f32)>,
    /// The focus flag last delivered to this node's `on_focus_changed` callbacks. Starts `false`, so
    /// gaining focus fires `true` and only a real change fires `false`.
    pub(crate) focus_reported: bool,
    /// For a focus-group node: the descendant that had focus when the group was last left.
    pub(crate) focus_group_memory: Option<u64>,
    /// Per-item paint channels from a layout's item animation host: placement offset and opacity.
    pub(crate) item_paint: Option<crate::layout::item_animation::ItemPaint>,
    /// Bumped whenever this node is measured, so the item exit-picture cache can tell that content
    /// under the same box may have changed.
    pub(crate) content_epoch: u64,
    /// The SelectionRegistrar of the CompositionLocal scope (Text nodes keep a reference to it).
    pub(crate) registrar: std::cell::RefCell<Option<crate::text::selection::SelectionRegistrar>>,
    /// The text cursor's x offset (TextField; the render pass draws the bar when focused).
    pub(crate) cursor_x: std::cell::Cell<f32>,
    pub(crate) cursor_height: std::cell::Cell<f32>,
    /// The cursor's character index (set by TextField; the render pass locates it through TextLayout).
    pub(crate) cursor_index: std::cell::Cell<usize>,
    /// Whether the cursor is visible (the blink toggles it; TextField sets it).
    pub(crate) cursor_visible: std::cell::Cell<bool>,
    /// A display-focus flag (the text-field-v2 container move left focus on the CONTAINER, so an
    /// input child renders its cursor and selection from this; written during composition, falling
    /// back to `node.focused`).
    pub(crate) display_focused: std::cell::Cell<bool>,
    /// The cursor-position callback (TextField updates its selection from it after a click).
    pub(crate) cursor_callback: std::cell::RefCell<Option<Box<dyn Fn(usize) + Send>>>,
    /// The IME preedit callback (TextField handles Preedit with it).
    pub(crate) ime_callback: std::cell::RefCell<Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>>,
    /// The IME composing range (the render pass underlines it).
    pub(crate) composing_range: std::cell::RefCell<Option<std::ops::Range<usize>>>,
    /// The focus ring's colour, captured from the theme during composition (the CompositionLocal is
    /// gone by render time, so the theme cannot be read there); unset falls back to the default blue.
    pub(crate) focus_color: std::cell::Cell<crate::graphics::Color>,
    /// The IME composing underline's colour, captured from the theme during composition (the
    /// CompositionLocal is unreadable at render time); unset falls back to the default colour.
    pub(crate) composing_color: std::cell::Cell<crate::graphics::Color>,
    /// The shared-element flight (phase 2): while `Some`, the render pass morphs the node from its
    /// start rectangle to its end one (offset, scale, fade, corner radii) and the hit test skips it.
    /// The coordinator rewrites it every frame and clears it to `None` when the flight ends. It is
    /// deliberately left out of the node cache — a flight is transient, and a reused node has to be
    /// rebuilt from a clean state (the coordinator refills it by slot).
    pub(crate) transition: Option<crate::transition::TransitionVisual>,
    /// Where this node's pixels come from while a shared-element transition runs. One enum instead of
    /// several booleans: the three dispositions are mutually exclusive, the render walk becomes a
    /// single comparison, and an illegal combination (a node that is both elevated chrome and a
    /// placeholder) cannot be expressed at all. Recomputed every frame — transient, never cached.
    pub(crate) paint: PaintDisposition,
    /// Flight layout override (Compose `ResizeMode` / `PlaceHolderSize`),
    /// written per frame by the coordinator while a flight owns this node.
    /// `None` = ordinary layout — the default path never touches it.
    pub(crate) flight_measure: Option<FlightMeasure>,
    /// The box the node's own layout/paint occupies while a flight reports a
    /// placeholder size: render, clip and hit testing must use THIS, because
    /// `measured_size` then carries the size the parent was told (Compose
    /// `PlaceHolderSize`), and parents re-write it from their placements.
    /// `None` = `measured_size` is the content box, as always.
    pub(crate) flight_content_size: Option<Size>,
    /// The text this node was last MATERIALIZED with (see [`text_snapshot`]).
    ///
    /// It answers the one question the old per-frame cache map held it for: a slot can be Clean while
    /// the text its node carries changed (the read lives in an ancestor container, so nothing marked
    /// the leaf dirty), and a node that folds its measurement would then paint `cached_paragraph`'s old
    /// content. Storing it on the node means the comparison needs no map lookup — and the map itself,
    /// which cost a `slot_key → CachedNode` insert per node per frame, is gone.
    ///
    /// Written only when it DIFFERS (see `text_content_matches`): a node whose text does not change
    /// clones nothing, so a settled frame does no work here at all.
    pub(crate) last_text: Option<TextSnapshot>,
    /// Whether this node's OWN measure policy subcomposed content (`layout::subcompose`), recorded by
    /// [`crate::runtime::composer::Composer::park_subcomposition`] at the moment it parked the composition.
    ///
    /// It is one of the two ways `Composer::compose`'s compose-end seeding recognizes a node that has to
    /// be re-measured (the other asks the policy, which covers a node that was just REBUILT and has not
    /// measured since). `layout()` clears `dirty` and `layout_dirty` on every node and re-derives them
    /// from `layout_dirty_keys`, and a node whose slot was Clean arrives with `dirty = false`, so a
    /// subcomposing node has no other way to say "measure me" — and a folded frame would let
    /// `materialize` clear the parent's `children` and leave the adopted subtree detached (measured
    /// while writing the facility: the adopted child disappeared on the second frame).
    ///
    /// It is set by the node that parked, never by counting: "how many subcompositions were parked while
    /// this node measured" is also true for every ANCESTOR of a subcomposing node, and seeding those put
    /// the frame's whole tree into `layout_dirty_keys` (measured: an 800-row tree's root carried this
    /// flag, and one row's update re-measured — and re-subcomposed — all 800).
    pub(crate) subcomposed: bool,
    /// The arena index of this node's subcomposed child, if its policy composed one
    /// (`layout::subcompose`). Kept out of the descriptor-driven `children` bookkeeping on purpose: the
    /// adopted subtree has NO descriptor, so materialize's "clear and rebuild from descriptors" would
    /// drop it, and the shape check ("does the cached child count match the descriptors?") would
    /// refuse the reuse path every frame. Both arms therefore treat this index as separate: the child
    /// is detached before the rebuild and re-attached after it.
    pub(crate) subcomposed_child: Option<usize>,
    /// The measurements of the adopted subtree, relative to its root, in pre-order. The
    /// subcomposition measures itself in its own `layout()`; when materialize reuses the parent and
    /// detaches/re-attaches the child, those measurements would be lost (the adopted child has no
    /// policy of its own to re-measure with), so they are cached here and replayed on re-attach.
    /// Measured while building this: without it the adopted subtree comes back at 0x0 and paints
    /// nothing.
    pub(crate) subcomposed_measurements: Vec<(usize, Size)>,
}

impl LayoutNode {
    /// Reports `line` at `value` px from this node's top edge — the measure side of an alignment
    /// line. Whoever measures the node calls this; a line has one position per measurement.
    pub fn set_alignment_line(&mut self, line: AlignmentLine, value: f32) {
        match self.alignment_lines.iter_mut().find(|(l, _)| *l == line) {
            Some(slot) => slot.1 = value,
            None => self.alignment_lines.push((line, value)),
        }
    }

    /// Where this node reports `line`, or `None` when it does not report that line at all — Compose's
    /// `AlignmentLine.Unspecified`.
    pub fn alignment_line(&self, line: AlignmentLine) -> Option<f32> {
        self.alignment_lines.iter().find(|(l, _)| *l == line).map(|(_, v)| *v)
    }

    /// Whether this node's text still matches the snapshot taken when it was last materialized. A
    /// node with no text matches by definition; `None` (never materialized, or materialized before
    /// the text arrived) does NOT — which forces the caller's re-measure, the conservative direction.
    pub(crate) fn text_snapshot_matches(&self) -> bool {
        if !(self.has_text_content || self.has_richtext_content) {
            return true;
        }
        self.last_text
            .as_ref()
            .is_some_and(|snap| text_content_matches(&self.modifier, snap))
    }

    /// Takes the snapshot again when the text moved, so the next frame's [`Self::text_snapshot_matches`]
    /// compares against what this frame materialized. Allocates only on a change: `text_snapshot`
    /// clones the content string, and a settled text node clones nothing per frame.
    pub(crate) fn refresh_text_snapshot(&mut self) {
        if !(self.has_text_content || self.has_richtext_content) {
            // A node with no text holds no snapshot. `text_snapshot_matches` answers `true` for it (a
            // compare it is not part of), so the clear has to happen before that early return — and it
            // has to happen: a snapshot surviving a text→box change would let the node, changed back to
            // Text with the same content, compare against text from before the paragraph was cleared.
            self.last_text = None;
            return;
        }
        if self.text_snapshot_matches() {
            return;
        }
        self.last_text = text_snapshot(&self.modifier);
    }

    /// Restores the layout state a REBUILT node needs from the node its key named — the degraded path
    /// in `materialize` that has no node to reuse and folds the old measurement instead of measuring
    /// again (`dirty = false` is what allows that fold).
    ///
    /// The three fields are the whole of it, and the first two MUST travel together: after `place()`
    /// `measured_size` holds the size the PARENT was told, so restoring it without the content box
    /// resurrects a flight's placeholder as the node's own box (review R3-F3, which found this third
    /// restore site missed when the field was added to the cache).
    ///
    /// Deliberately NOT restored: `modifier` (the node must keep this frame's build — an old one would
    /// overwrite it and lose a change like a button's new label), `position` (this frame's layout
    /// places the node), `focused` (written by focus routing outside compose and layout — the fourth
    /// counterexample in `docs/benchmarks.md`'s tenth attempt) and `slot_key` (the caller sets it).
    pub(crate) fn restore_layout(&mut self, from: &LayoutNode) {
        self.measured_size = from.measured_size;
        self.flight_content_size = from.flight_content_size;
        self.cached_constraints = from.cached_constraints;
        // Another layout product, so it travels with the rest: the measure writes it from the node's own
        // `minimum_interactive_size`, exactly like `measured_size`, and a stale one would inset the paint of
        // a node that has not measured yet this frame.
        self.minimum_interactive_content = from.minimum_interactive_content;
        self.minimum_interactive_painted = from.minimum_interactive_painted;
        self.content_shift = from.content_shift;
        self.dirty = false;
    }
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        if let Some(f) = self.on_remove.take() { f(); }
    }
}

impl LayoutNode {
    /// Creates a node. `measure_policy` is an index into `NodeArena.policies` (the caller obtains it
    /// from `arena.alloc_policy(...)`); leaves pass `None`.
    pub fn new(modifier: Modifier, measure_policy: Option<usize>) -> Self {
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            has_text_content: modifier_has_text(&modifier),
            has_richtext_content: modifier_has_richtext(&modifier),
            has_image_content: modifier_has_image(&modifier),
            modifier,
            measured_size: Size::ZERO,
            position: Offset::ZERO,
            alignment_lines: Vec::new(),
            minimum_interactive_content: None,
            minimum_interactive_painted: None,
            content_shift: None,
            content_box_override: None,
            resolved_padding: (0.0, 0.0, 0.0, 0.0),
            children: Vec::new(),
            measure_policy,
            focused: false,
            on_remove: None,
            dirty: true,
            layout_dirty: false,
            cached_constraints: None,
            subcomposed: false,
            subcomposed_child: None,
            subcomposed_measurements: Vec::new(),
            layout_direction: LayoutDirection::Ltr,
            children_have_z: false,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, scroll_viewport_width: 0.0, scroll_content_height: 0.0, scroll_content_width: 0.0, scroll_reverse: false, parent_id: None,
            last_global_position: None,
            focus_reported: false,
            focus_group_memory: None,
            item_paint: None,
            content_epoch: 0,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            display_focused: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            focus_color: std::cell::Cell::new(crate::graphics::Color::from_argb(204, 77, 153, 255)),
            composing_color: std::cell::Cell::new(crate::graphics::Color::TRANSPARENT),
            transition: None,
            paint: PaintDisposition::InTree,
            flight_measure: None,
            flight_content_size: None,
            last_text: None,
        }
    }

    /// Adds a child: the arena allocates its index first, then it is attached to `children`.
    pub fn add_child(&mut self, child_idx: usize) {
        // `parent_id` is set by NodeArena when the child is attached (it needs the parent's id).
        self.children.push(child_idx);
    }

    /// Creates a leaf node (no children).
    pub fn leaf(modifier: Modifier) -> Self {
        LayoutNode::new(modifier, None)
    }

    /// Creates a container node (children plus a measure policy).
    /// Whether this is a leaf.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty() && self.measure_policy.is_none()
    }
}

impl LayoutNode {
    /// Where this node's pixels come from on THIS frame — the single question the render walk asks.
    ///
    /// A flight end that has been lifted into the transition layer is folded in live rather than read
    /// from the stored field: the writer sets `elevated` together with the frame's visuals, and a caller
    /// that flips it and renders immediately (as the A/B render tests do) must see the effect on the very
    /// next render, not after the next poll. Everything else (scope-overlay chrome, placeholder copies)
    /// is decided once per frame by `refresh_paint_dispositions` and stored here.
    pub(crate) fn paint_disposition(&self) -> PaintDisposition {
        if self.transition.as_ref().is_some_and(|t| t.elevated) {
            return PaintDisposition::InLayer;
        }
        self.paint
    }

    /// The box this node's own layout/paint occupies. While a flight reports a
    /// placeholder size, `measured_size` carries the size the PARENT observes
    /// (Compose `PlaceHolderSize`) and this returns the size the content was
    /// really measured at instead — render, clip, radii and hit testing all
    /// work in that frame. `None` (no flight) means the two are identical.
    pub(crate) fn content_box(&self) -> Size {
        self.flight_content_size.unwrap_or(self.measured_size)
    }
}

impl Default for LayoutNode {
    fn default() -> Self {
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier: Modifier::new(),
            measured_size: Size::ZERO,
            position: Offset::ZERO,
            alignment_lines: Vec::new(),
            minimum_interactive_content: None,
            minimum_interactive_painted: None,
            content_shift: None,
            content_box_override: None,
            resolved_padding: (0.0, 0.0, 0.0, 0.0),
            has_image_content: false,
            children: Vec::new(),
            measure_policy: None,
            has_text_content: false,
            has_richtext_content: false,
            focused: false,
            on_remove: None,
            dirty: true,
            layout_dirty: false,
            cached_constraints: None,
            subcomposed: false,
            subcomposed_child: None,
            subcomposed_measurements: Vec::new(),
            layout_direction: LayoutDirection::Ltr,
            children_have_z: false,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, scroll_viewport_width: 0.0, scroll_content_height: 0.0, scroll_content_width: 0.0, scroll_reverse: false, parent_id: None,
            last_global_position: None,
            focus_reported: false,
            focus_group_memory: None,
            item_paint: None,
            content_epoch: 0,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            display_focused: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            focus_color: std::cell::Cell::new(crate::graphics::Color::from_argb(204, 77, 153, 255)),
            composing_color: std::cell::Cell::new(crate::graphics::Color::TRANSPARENT),
            transition: None,
            paint: PaintDisposition::InTree,
            flight_measure: None,
            flight_content_size: None,
            last_text: None,
        }
    }
}

// ── NodeArena: the layout-tree node pool (an index tree whose nodes survive recomposition) ──

/// The hasher for maps keyed by `slot_key`.
///
/// Every key in these maps has already been through the composer's `mix_key` (fnv with two multiplies)
/// or is a composable's source hash, so they are not the sequential integers a std `HashMap` is tuned
/// against — and they are not attacker-supplied either: the map is asked "which node had this key".
/// What it must not do is spend SipHash's rounds on a `u64`. These maps are filled with an entry per
/// node and read back once per node, every frame, and together they were the largest item left in an
/// idle layout (`docs/benchmarks.md`).
///
/// The hash is `(key ^ (key >> 32)) * FIB`. The multiply (Fibonacci hashing, one instruction) spreads
/// the key; the xor before it is not decoration — a multiply propagates bits upward only, so the LOW
/// bits of a product are a function of the low bits of its input, and hashbrown indexes with the low
/// bits (the top 7 go to the control byte). Folding the high half down first gives both ends of the
/// result a dependency on the whole key. Measured against a strong hash (sha256, standing in for
/// SipHash) on the key patterns this framework produces — a row per base, one base with sequential
/// counters, nested mixes — the worst bucket in 256 is 8-9, against the strong hash's 9-10. (Checked
/// rather than assumed because a bad hasher is silent: slower, not wrong.)
#[derive(Default)]
pub(crate) struct SlotKeyHasher(u64);

impl std::hash::Hasher for SlotKeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    /// Fallback for a non-`u64` key. Nothing uses it, and it is deliberately honest rather than a
    /// no-op: a future map that keys by something else should still hash, not collapse every key onto
    /// one bucket.
    fn write(&mut self, bytes: &[u8]) {
        let mut h = self.0 ^ 0xcbf29ce484222325;
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        self.0 = h;
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (n ^ (n >> 32)).wrapping_mul(0x9E3779B97F4A7C15);
    }
}

/// A map keyed by `slot_key` (see [`SlotKeyHasher`]).
pub(crate) type SlotKeyMap<V> =
    std::collections::HashMap<u64, V, std::hash::BuildHasherDefault<SlotKeyHasher>>;

/// A set of `slot_key`s (see [`SlotKeyHasher`]). The per-frame live-key set is the reason it exists:
/// at 800 rows it is ~4000 inserts, and with the std hasher that set alone measured 194 µs of an idle
/// frame — the walk, the hashing and the growth of a `HashSet` nobody sized up front.
pub(crate) type SlotKeySet =
    std::collections::HashSet<u64, std::hash::BuildHasherDefault<SlotKeyHasher>>;

/// A set of arena node indices, kept as a bit vector.
///
/// `materialize` marks every node it claims (the frame's reuse decisions) and the compose tail asks
/// about every key it drains, so both sides touch this once per node per frame. Arena indices are
/// dense (`0..nodes.len()`), which makes "is this node marked" a bit test instead of a hash: the
/// `HashSet<usize>` this replaced cost a SipHash per insert AND per lookup, in a path that runs 4000
/// times a frame (`docs/benchmarks.md`).
///
/// `clear` drops the length instead of zeroing every word — the storage is reused, and a shorter
/// vector simply answers `false` for indices it no longer covers.
#[derive(Clone, Default)]
pub(crate) struct NodeMarks {
    pub(crate) words: Vec<u64>,
}

impl NodeMarks {
    /// Marks `idx`; returns whether it was NOT marked before (the visit-once idiom the shared-element
    /// detach uses to walk a subtree).
    pub(crate) fn insert(&mut self, idx: usize) -> bool {
        let word = idx / 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let bit = 1u64 << (idx % 64);
        let was_set = self.words[word] & bit != 0;
        self.words[word] |= bit;
        !was_set
    }

    pub(crate) fn contains(&self, idx: usize) -> bool {
        let word = idx / 64;
        word < self.words.len() && self.words[word] & (1u64 << (idx % 64)) != 0
    }

    pub(crate) fn clear(&mut self) {
        self.words.clear();
    }

    /// Marks everything `other` marks. Used to move a subtree's claim from a scratch set into the
    /// frame's reuse set once the claim is known to be good.
    pub(crate) fn merge(&mut self, other: &NodeMarks) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (dst, src) in self.words.iter_mut().zip(other.words.iter()) {
            *dst |= *src;
        }
    }
}

/// The layout-tree node pool: every LayoutNode lives in `nodes` and the tree is expressed through
/// indices (`children: Vec<usize>`). The nodes PERSIST across recomposition — the composition diff
/// updates their contents instead of rebuilding the objects.
pub struct NodeArena {
    pub(crate) nodes: Vec<LayoutNode>,
    pub(crate) policies: Vec<Box<dyn MeasurePolicy>>,
    pub(crate) free_policies: Vec<usize>,
    pub(crate) free: Vec<usize>,
    pub(crate) root: Option<usize>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), policies: Vec::new(), free_policies: Vec::new(), free: Vec::new(), root: None }
    }

    /// Allocates a measure policy in the pool (reusing a recycled slot first) and returns its index
    /// for `LayoutNode::measure_policy`.
    pub fn alloc_policy(&mut self, policy: Box<dyn MeasurePolicy>) -> usize {
        if let Some(idx) = self.free_policies.pop() {
            self.policies[idx] = policy;
            idx
        } else {
            self.policies.push(policy);
            self.policies.len() - 1
        }
    }

    /// Recycles the policy slot a node held (when a structural change removes it — a low-frequency
    /// leak guard).
    pub(crate) fn recycle_policy(&mut self, idx: usize) {
        if let Some(p) = self.nodes[idx].measure_policy.take() {
            self.free_policies.push(p);
        }
    }

    /// Makes room for `additional` more nodes in one step.
    ///
    /// `alloc` pushes into `nodes`, and a `Vec` that grows 0 → 4000 does it by doubling: ~12
    /// reallocations, each MOVING every node already in it. A `LayoutNode` is a large struct (its
    /// modifier plus a dozen cells), so on the frame that builds a whole tree that is megabytes of
    /// `memcpy` — measured as the single largest item in the cold frame's materialize
    /// (`docs/benchmarks.md`). The count is an upper bound, which is the safe direction here: capacity
    /// that goes unused costs address space, not work, and a reserve against capacity already
    /// sufficient is a comparison.
    pub(crate) fn reserve_nodes(&mut self, additional: usize) {
        self.nodes.reserve(additional);
    }

    /// Allocates or reuses a slot (the free list first) and returns its index.
    pub fn alloc(&mut self, node: LayoutNode) -> usize {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = node;
            idx
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    /// Frees a node and its subtree (recursively, firing `on_remove`).
    /// `skip` holds the nodes this frame reused: they are already attached to this frame's tree, so
    /// freeing one would recurse into that tree, form a cycle and overflow the stack — they must be
    /// skipped. `visited` is a defensive cycle check (a malformed tree terminates recursion).
    pub(crate) fn free_node_skip(
        &mut self,
        idx: usize,
        skip: &NodeMarks,
        visited: &mut NodeMarks,
    ) {
        if !visited.insert(idx) { return; }
        if skip.contains(idx) { return; }
        // Defensive: a node being freed must not belong to this frame's tree (reused nodes are in
        // `skip`, new ones are absent from `prev_node_by_key`; breaking that invariant would silently
        // free a live node).
        debug_assert!(
            !self.nodes[idx].children.iter().any(|&c| c == idx),
            "a cycle in the tree (self-reference) — free must terminate"
        );
        let children = std::mem::take(&mut self.nodes[idx].children);
        for c in children {
            self.free_node_skip(c, skip, visited);
        }
        self.recycle_policy(idx);
        if let Some(f) = self.nodes[idx].on_remove.take() { f(); }
        self.nodes[idx] = LayoutNode::default();
        self.free.push(idx);
    }

    /// Frees the whole tree from the root (`free_node` recursion plus `on_remove`) and clears the
    /// root.
    pub fn free_root(&mut self) {
        if let Some(r) = self.root.take() {
            self.free_node(r);
        }
    }

    /// Frees a node and its subtree (recursively, firing `on_remove`).
    pub fn free_node(&mut self, idx: usize) {
        let children = std::mem::take(&mut self.nodes[idx].children);
        for c in children {
            self.free_node(c);
        }
        self.recycle_policy(idx);
        if let Some(f) = self.nodes[idx].on_remove.take() { f(); }
        self.nodes[idx] = LayoutNode::default();
        self.free.push(idx);
    }

    /// Attaches a child to its parent (the child is already in the pool — `start_node` allocated it;
    /// this only links the index and sets `parent_id`).
    pub fn add_child(&mut self, parent: usize, child_idx: usize) {

        self.nodes[parent].children.push(child_idx);
        self.nodes[child_idx].parent_id = Some(self.nodes[parent].id);
    }

    pub fn get(&self, idx: usize) -> &LayoutNode { &self.nodes[idx] }
    pub fn get_mut(&mut self, idx: usize) -> &mut LayoutNode { &mut self.nodes[idx] }
    pub fn root(&self) -> Option<&LayoutNode> { self.root.map(|i| &self.nodes[i]) }
    pub fn root_mut(&mut self) -> Option<&mut LayoutNode> { self.root.map(|i| &mut self.nodes[i]) }
    pub fn root_idx(&self) -> Option<usize> { self.root }
    pub fn set_root(&mut self, idx: usize) { self.root = Some(idx); }

    /// A read-only recursive walk — `fn(arena, idx)`.
    pub fn for_each(&self, mut f: impl FnMut(&NodeArena, usize)) {
        if let Some(r) = self.root {
            self.for_each_rec(r, &mut f);
        }
    }
    pub(crate) fn for_each_rec(&self, idx: usize, f: &mut impl FnMut(&NodeArena, usize)) {
        f(self, idx);
        let children = self.nodes[idx].children.clone();
        for c in children {
            self.for_each_rec(c, f);
        }
    }
}

