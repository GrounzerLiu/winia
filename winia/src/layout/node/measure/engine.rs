use super::*;

// ── The recursive measure engine ──

/// Measures a node recursively: applies the constraints the modifier chain asks for and calls the
/// children's measure policies.
///
/// Arena flavour: `nodes` is the node pool, `policies` the policy pool and `idx` this node's index.
/// The children are reached through `nodes[idx].children` and measured recursively.
/// Applies layout invalidation: a DFS over the tree marks the nodes whose slot key is in
/// `dirty_keys`, and marks every ANCESTOR of those nodes.
///
/// The ancestor half is the conservative part the framework wants: a parent's placement depends on a
/// child's size, so a child that must re-measure drags its whole ancestor chain with it. The
/// DESCENDANT half is deliberately NOT here — a hit node re-measures, and that re-measure reaches its
/// children through the ordinary fold check (`cached_constraints != constraints`), so marking the
/// subtree as well only re-measures unchanged nodes.
///
/// That is not hypothetical. The walk used to pass `hit || ancestor_dirty` down to the children, which
/// marks a hit node's ENTIRE subtree, and it cost a factor of 50 on a single update: one row's change
/// in an 800-row tree whose ROOT was among the hits produced 800 subcompositions and a one-row frame
/// of 74915 µs, against ≈1600 µs with the subtree marking removed and no hit on the root
/// (`docs/benchmarks.md` has the tables).
pub(crate) fn apply_layout_dirty(nodes: &mut [LayoutNode], root_idx: usize, dirty_keys: &std::collections::HashSet<u64>) {
    pub(crate) fn walk(nodes: &mut [LayoutNode], idx: usize, dirty_keys: &std::collections::HashSet<u64>) -> bool {
        let hit = dirty_keys.contains(&nodes[idx].slot_key);
        if hit {
            nodes[idx].layout_dirty = true;
        }
        // Index reads avoid a clone on a hot path; each one is a temporary borrow, so the recursive
        // writes are not blocked.
        let mut child_hit = false;
        let n = nodes[idx].children.len();
        for i in 0..n {
            let child = nodes[idx].children[i];
            if walk(nodes, child, dirty_keys) {
                child_hit = true;
            }
        }
        if child_hit {
            nodes[idx].layout_dirty = true;
        }
        hit || child_hit
    }
    walk(nodes, root_idx, dirty_keys);
}


impl FlightMeasureFrame {
    pub(crate) const IDLE: Self = Self { content: None, reported: None };
    pub(crate) fn is_idle(&self) -> bool {
        self.content.is_none() && self.reported.is_none()
    }
}

// The measure counter (tests use it to prove the constant-folding and layout-invalidation paths
// really skip or run a measure). Thread-local, so parallel cargo test threads cannot disturb each
// other — a global Atomic would leak counts between tests and break their assertions.
#[cfg(test)]
thread_local! {
    pub(crate) static MEASURE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Puts back the slot key a node's measurement displaced, on the way out — including on an early
/// return or a panic, which is why it is a guard rather than a line at the end. See the arming site
/// in `measure_node_inner` for what breaks without it.
pub(crate) struct ActiveSlotKeyGuard(u64);

impl Drop for ActiveSlotKeyGuard {
    fn drop(&mut self) {
        crate::runtime::composer::set_active_slot_key(self.0);
    }
}

thread_local! {
    /// The intrinsic query an approximation is answering right now, if any — the state that turns a
    /// descendant `measure_node` into Compose's `DefaultIntrinsicMeasurable`.
    static INTRINSIC_PROBE: std::cell::Cell<Option<IntrinsicQuery>> =
        const { std::cell::Cell::new(None) };
}

pub(crate) fn intrinsic_probe_query() -> Option<IntrinsicQuery> {
    INTRINSIC_PROBE.with(|q| q.get())
}

/// Sets (and restores) the probe query for one approximation. A guard rather than two lines because
/// the policy's `measure` may panic, and a nested approximation for the other axis overwrites the
/// value — restoring the OUTER query is what keeps the enclosing fold answering its own question.
pub(crate) struct IntrinsicProbeGuard(Option<IntrinsicQuery>);

impl IntrinsicProbeGuard {
    pub(crate) fn enter(query: IntrinsicQuery) -> Self {
        Self(INTRINSIC_PROBE.with(|q| q.replace(Some(query))))
    }
}

impl Drop for IntrinsicProbeGuard {
    fn drop(&mut self) {
        INTRINSIC_PROBE.with(|q| q.set(self.0));
    }
}

/// Compose's `DefaultIntrinsicMeasurable.measure` (`ui/layout/Layout.kt:309-330`): the size a child
/// reports while an approximation is running its parent's OWN `measure`. The queried axis is the
/// child's intrinsic answer at the other axis the policy handed it, and the other axis is that
/// policy's constraint (Compose uses `LargeDimension` where it is unbounded).
///
/// Nothing is written back: no `measured_size`, no placement, no cache — the wrapper is a synthetic
/// measurable, so the frame's real measurement and, above all, any state a child's real `measure`
/// would mutate stay untouched.
pub(crate) fn synthetic_intrinsic_measure(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    query: IntrinsicQuery,
    constraints: Constraints,
) -> Size {
    if query.is_width() {
        let width = intrinsic_size_of(nodes, policies, idx, query, constraints.max_height);
        Size::new(width, constraints.max_height)
    } else {
        let height = intrinsic_size_of(nodes, policies, idx, query, constraints.max_width);
        Size::new(constraints.max_width, height)
    }
}

pub(crate) fn measure_node(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // Inside an intrinsic approximation, a child measurement IS an intrinsic query — see
    // `synthetic_intrinsic_measure`. This is what keeps a policy with side effects in its `measure`
    // (an animating container, a first-measurement flag) out of an intrinsic probe even when the
    // policy that reaches it keeps the trait's default intrinsic implementation.
    if let Some(query) = intrinsic_probe_query() {
        return (
            synthetic_intrinsic_measure(nodes, policies, idx, query, constraints),
            Vec::new(),
        );
    }
    // The fold check lives in `measure_node_inner` (it must also arm the node's slot key etc.); this
    // wrapper only adds the "which node is measuring" marker that a subcomposing policy needs, and
    // leaves folded calls with no marker at all — a policy that folds did not run, so a stale marker
    // would invite a subcomposition nobody asked for.
    //
    // A subcomposing node folds like any other here: its composition is re-run when the COMPOSITION
    // changed (`dirty` from the descriptor), and that is what re-measures it — folding an unchanged one
    // is what keeps an idle frame cheap. `layout_dirty` still forces it, which is how a state read
    // during the measurement (`first_measure`, the flight override) reaches it.
    if !nodes[idx].dirty && !nodes[idx].layout_dirty && nodes[idx].cached_constraints == Some(constraints) {
        return (nodes[idx].measured_size, Vec::new());
    }
    let displaced = crate::layout::subcompose::swap_measuring_node(Some(idx));
    let sub_before = crate::layout::subcompose::subcomposition_count();
    let out = measure_node_inner(nodes, policies, idx, constraints);
    crate::layout::subcompose::swap_measuring_node(displaced);
    // Remember whether this node's policy composed anything, so the next frame does not fold it.
    nodes[idx].subcomposed = crate::layout::subcompose::subcomposition_count() > sub_before;
    out
}

/// Compose's `AspectRatioNode.findSize` (`AspectRatio.kt:163-232`): build the four candidates from
/// the incoming range — maximum width, maximum height, minimum width, minimum height (the
/// `match_height_first` form swaps the width/height priority) — and take the first that
/// SATISFIES the range; then repeat the same four UNENFORCED; then give up (`None`, which leaves
/// the constraints alone: Compose measures the child with them and keeps its size).
///
/// Two deliberate differences from Compose. The winner is CLAMPED into the incoming range, because
/// Compose's derivation lives in a layout node that reports the child's size while winia feeds the
/// result straight into `Constraints::fixed` — the clamp is what keeps a tight `size()`
/// authoritative. And a bounded maximum is `< f32::MAX`, winia's unbounded sentinel.
fn aspect_ratio_size(c: &Constraints, ratio: f32, match_height_first: bool) -> Option<Size> {
    let bounded = |v: f32| v < f32::MAX;
    let max_w = bounded(c.max_width).then(|| Size::new(c.max_width, c.max_width / ratio));
    let max_h = bounded(c.max_height).then(|| Size::new(c.max_height * ratio, c.max_height));
    let min_w = Size::new(c.min_width, c.min_width / ratio);
    let min_h = Size::new(c.min_height * ratio, c.min_height);
    let order = if match_height_first {
        [max_h, max_w, Some(min_h), Some(min_w)]
    } else {
        [max_w, max_h, Some(min_w), Some(min_h)]
    };
    let satisfied = |s: &Size| {
        s.width >= c.min_width
            && s.width <= c.max_width
            && s.height >= c.min_height
            && s.height <= c.max_height
    };
    for enforce in [true, false] {
        for cand in order.iter().flatten() {
            let usable = cand.width.is_finite()
                && cand.height.is_finite()
                && cand.width > 0.0
                && cand.height > 0.0;
            if !usable || (enforce && !satisfied(cand)) {
                continue;
            }
            return Some(Size::new(
                cand.width.clamp(c.min_width, c.max_width),
                cand.height.clamp(c.min_height, c.max_height),
            ));
        }
    }
    None
}

pub(crate) fn measure_node_inner(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // Replay stub: a clean-skipped node has no measure policy and must never be measured again —
    // without one it would take the leaf branch, return 0 and write that 0 into the node's cache,
    // which collapses it irreversibly. A stub only appears while the slot really is clean; if the
    // constraints changed, the slot is dirty next frame and Enter rebuilds it.
    // Constant folding: when the node is clean, has no layout invalidation and the constraints are
    // the same, the previous result is reused (the two-stage dependency: a layout animation value
    // re-measures without recomposing).
    // A subcomposing node is not special-cased here either — see the wrapper's note.
    if !nodes[idx].dirty && !nodes[idx].layout_dirty && nodes[idx].cached_constraints == Some(constraints) {
        return (nodes[idx].measured_size, Vec::new());
    }

    // Only a real measure counts (a constant-fold hit does not, which is what lets the tests verify
    // the folding path).
    #[cfg(test)]
    MEASURE_COUNT.with(|c| c.set(c.get() + 1));
    // A real measure may change the painted content even when the box is unchanged. The item
    // exit-picture cache uses this epoch to invalidate a stale ghost.
    nodes[idx].content_epoch = nodes[idx].content_epoch.wrapping_add(1);

    // Set ACTIVE_SLOT_KEY = this node's slot so a State::get() inside a dynamic
    // size closure registers its dependency on THIS node (value change → node
    // dirty → recompose + re-measure).
    //
    // The key is RESTORED when this measure returns, and that half is load-bearing: this node's
    // children each arm their own key on the way in, so without a restore the key stays on whatever
    // descendant measured last, and the PARENT's own post-child reads (a policy's bookkeeping after
    // its children loop, `ResizeMode` work, a dynamic size resolved after laying children out) are
    // attributed to a descendant's slot instead. A dependency on a descendant's slot is dropped the
    // moment that descendant goes away — a lazy row recycled, an `if` branch closed — and the parent
    // silently stops tracking the state. Measured before this guard existed: a parent's post-child
    // read landed on a slot that was neither the parent's nor its last child's
    // (`test_layout_dep_of_a_parent_post_child_read_lands_on_the_parent`).
    let displaced_key = crate::runtime::composer::active_slot_key();
    crate::runtime::composer::set_active_slot_key(nodes[idx].slot_key);
    let _restore_key = ActiveSlotKeyGuard(displaced_key);

    // Flight layout contract (Compose `ResizeMode` / `PlaceHolderSize`), read
    // AFTER the fold check: a folded node must neither arm its slot key nor
    // replace its recorded layout dependencies (the fold is what keeps other
    // layout dependencies alive — see `test_layout_dep_survives_const_fold`).
    // Reading here also registers the frame state as a layout dependency, so a
    // tick re-measures this node and, through `apply_layout_dirty`, its
    // ancestors — never a recomposition.
    let flight = nodes[idx].flight_measure.clone().map(|f| f.frame.get());

    // A line belongs to the measurement that produced it: clearing here means a node that stops
    // reporting one (text that became empty, a leaf that became a container) cannot leave a stale
    // position for a Row/Column to align by. Folded nodes never reach this, so they keep theirs.
    nodes[idx].alignment_lines.clear();
    // The inset belongs to the same measurement, for the same reason: a node that stops carrying
    // `Modifier::minimum_interactive_size` — removed from the chain, or disabled with
    // `minimum_interactive_size(0.0)` — must not keep the offset from when it did, or the paint insets a box
    // that was never grown (a 40 dp container drawn at 32). The write further down only happens while the
    // element is in force, so clearing here is what makes its absence meaningful.
    nodes[idx].minimum_interactive_content = None;
    nodes[idx].minimum_interactive_painted = None;
    nodes[idx].content_shift = None;
    // Same rule for the content box a leaf paints itself into: a node that stops carrying
    // `wrapContent` or `paddingFrom` must not keep the previous frame's override (it is only
    // written while those elements are in force).
    nodes[idx].content_box_override = None;

    // Apply the layout constraints the modifier carries (through the query methods)
    let mut inner_constraints = constraints;

    // Size-shaping steps come first, in CHAIN ORDER — Compose's rule, because each Compose
    // modifier is its own node that `constrain`s what the node before it produced. That is what
    // decides a conflict: `.widthIn(max = 200).widthIn(min = 300)` is 200 while the reverse is 300.
    // The walk also covers `size`/`requiredSize`/`fillMax*`, so a `wrapContent` relaxes the range
    // for whatever it wraps, wherever the chain spells it.
    //
    //   `Size` — `size`/`width`/`height` (enforceIncoming): the requested axis is clamped into the
    //   current range and then fixes both bounds. `RequiredSize` writes over the range.
    //   `Fill` — Compose's `FillNode`: a bounded maximum becomes `max * fraction` (rounded).
    //   `MinWidth`/`MaxWidth` — `widthIn(min = …)` / `widthIn(max = …)`.
    //   `SizeIn` — ONE node, both bounds against the same input range; `enforce_incoming` separates
    //   `sizeIn` from `required*In` (`Size.kt:846-888`).
    //   `DefaultMin` — `defaultMinSize`, only where the incoming minimum is 0.
    //   `Wrap` — `wrapContent*`: relax the minimum (and maximum when unbounded) and remember the
    //   range for the post-measure wrapper.
    use crate::modifier::SizeStep;
    struct WrapStep {
        align: crate::layout::ContentAlignment,
        /// Which axes the wrap node actually aligns. `wrap_content_width` fixes the horizontal
        /// alignment and leaves the vertical offset at 0 (Compose's alignmentCallback returns
        /// `IntOffset(align.align(...), 0)`); the height form mirrors it.
        width: bool,
        height: bool,
        min_width: f32,
        min_height: f32,
        max_width: f32,
        max_height: f32,
    }
    let mut wrap_steps: Vec<WrapStep> = Vec::new();
    // The constraints an `width/height(IntrinsicSize)` request saw at its chain position. A fill
    // OUTSIDE the request has already tightened them (the request honours them); a fill INSIDE has
    // not (the request decides first, the fill then fills the tight result).
    let mut intrinsic_incoming_width: Option<(f32, f32)> = None;
    let mut intrinsic_incoming_height: Option<(f32, f32)> = None;
    // Window insets are resolved AT THEIR CHAIN POSITION (see `SizeStep::InsetPadding`): the walk
    // consumes them in order, and these accumulators feed the outer size and the placement shift.
    let mut inset_consumed = crate::window::insets::consumed();
    let (mut inset_start, mut inset_end) = (0.0f32, 0.0f32);
    let (mut inset_top, mut inset_bottom) = (0.0f32, 0.0f32);
    for step in nodes[idx].modifier.size_steps() {
        match step {
            SizeStep::Size { width, height } => {
                // Compose SizeNode (enforceIncoming): the request is clamped into the current
                // range and then fixes it.
                if let Some(v) = width {
                    let w = inner_constraints.constrain_width(v);
                    inner_constraints = inner_constraints.tighten_width(w);
                }
                if let Some(v) = height {
                    let h = inner_constraints.constrain_height(v);
                    inner_constraints = inner_constraints.tighten_height(h);
                }
            }
            SizeStep::RequiredSize { width, height } => {
                // enforceIncoming = false: the numeric axes write over the range.
                if let Some(v) = width {
                    inner_constraints.min_width = v;
                    inner_constraints.max_width = v;
                }
                if let Some(v) = height {
                    inner_constraints.min_height = v;
                    inner_constraints.max_height = v;
                }
            }
            SizeStep::Fill { width, height } => {
                // Compose FillNode: a bounded maximum becomes a fixed size at max * fraction; an
                // unbounded maximum leaves the content alone.
                if let Some(fraction) = width {
                    if inner_constraints.max_width < f32::MAX {
                        let w = (inner_constraints.max_width * fraction)
                            .round()
                            .clamp(inner_constraints.min_width, inner_constraints.max_width);
                        inner_constraints.min_width = w;
                        inner_constraints.max_width = w;
                    }
                }
                if let Some(fraction) = height {
                    if inner_constraints.max_height < f32::MAX {
                        let h = (inner_constraints.max_height * fraction)
                            .round()
                            .clamp(inner_constraints.min_height, inner_constraints.max_height);
                        inner_constraints.min_height = h;
                        inner_constraints.max_height = h;
                    }
                }
            }
            SizeStep::Intrinsic { width, height } => {
                // The request helper returns the OUTERMOST intrinsic element, so keep the FIRST
                // snapshot: a later (inner) request would otherwise be the one the clamp uses.
                if width && intrinsic_incoming_width.is_none() {
                    intrinsic_incoming_width =
                        Some((inner_constraints.min_width, inner_constraints.max_width));
                }
                if height && intrinsic_incoming_height.is_none() {
                    intrinsic_incoming_height =
                        Some((inner_constraints.min_height, inner_constraints.max_height));
                }
            }
            SizeStep::AspectRatio {
                ratio,
                match_height_first,
            } => {
                // Compose's `AspectRatioNode.measure`: derive the size from the constraints at THIS
                // position, then measure the child with `Constraints.fixed(size)`. Deriving it here
                // (rather than resizing the node after the policy, as this did) is what makes the
                // content — and a `match_parent_size` child — see the aspect box, and it no longer
                // DROPS the padding (the old pass replaced the node's padded size with the inner box;
                // here the padding is added on top of the derived box). The chain order is exact:
                // padding is a step too (`SizeStep::Padding`), so `.padding(8).aspect_ratio(2)`
                // derives from the DEFLATED constraints, as Compose's node order does.
                if let Some(size) = aspect_ratio_size(&inner_constraints, ratio, match_height_first) {
                    inner_constraints = Constraints::fixed(size.width, size.height);
                }
            }
            SizeStep::InsetPadding(index) => {
                if let Some((kind, controller)) = nodes[idx].modifier.inset_at(index) {
                    let window = controller.state().get();
                    let value = crate::window::insets::remaining(kind, window, inset_consumed);
                    inner_constraints =
                        inner_constraints.offset(value.horizontal_sum(), value.vertical_sum());
                    inset_start += value.left;
                    inset_end += value.right;
                    inset_top += value.top;
                    inset_bottom += value.bottom;
                    inset_consumed = crate::window::insets::consume(kind, window, inset_consumed);
                }
            }
            SizeStep::InsetSize(index) => {
                // Compose's DerivedWidth/HeightModifierNode (WindowInsetsSize.kt): the node's one axis IS
                // the unconsumed inset on that side, the child is measured at exactly that size, and the
                // CROSS axis comes from the child. It does not consume the inset.
                if let Some((kind, controller, side)) = nodes[idx].modifier.inset_size_at(index) {
                    let window = controller.state().get();
                    let value = crate::window::insets::remaining(kind, window, inset_consumed);
                    let rtl = nodes[idx].layout_direction == crate::layout::LayoutDirection::Rtl;
                    let width = match side {
                        crate::modifier::InsetSizeSide::Start => {
                            if rtl { value.right } else { value.left }
                        }
                        crate::modifier::InsetSizeSide::End => {
                            if rtl { value.left } else { value.right }
                        }
                        _ => 0.0,
                    };
                    let height = match side {
                        crate::modifier::InsetSizeSide::Top => value.top,
                        crate::modifier::InsetSizeSide::Bottom => value.bottom,
                        _ => 0.0,
                    };
                    if width > 0.0 {
                        inner_constraints = Constraints::new(
                            width,
                            width,
                            inner_constraints.min_height,
                            inner_constraints.max_height,
                        );
                    } else if height > 0.0 {
                        inner_constraints = Constraints::new(
                            inner_constraints.min_width,
                            inner_constraints.max_width,
                            height,
                            height,
                        );
                    } else {
                        // A zero inset collapses the node and places nothing (Compose returns
                        // layout(0, 0) {} in that case).
                        inner_constraints = Constraints::fixed(0.0, 0.0);
                    }
                }
            }
            SizeStep::ConsumeInsets(index) => {
                // Consume-only: no padding, it only marks the inset for what follows it.
                if let Some((kind, controller)) = nodes[idx].modifier.inset_at(index) {
                    let window = controller.state().get();
                    inset_consumed = crate::window::insets::consume(kind, window, inset_consumed);
                }
            }
            SizeStep::Padding {
                start,
                top,
                end,
                bottom,
            } => {
                // Compose's padding node deflates the constraints where it stands
                // (`foundation/layout/Padding.kt`); the chain order decides whether a size fixed
                // inside the padding sees the deflated range. The node's OUTER size adds the total
                // padding afterwards, which is why the step only shrinks what follows.
                //
                // WINDOW-INSET padding is a step of its own (`SizeStep::InsetPadding`): the inset is
                // resolved and consumed at this element's position, so a size written inside it sees
                // the deflated range exactly like element padding.
                //
                // The deflated value and the value added to the outer size are resolved separately
                // (`size_steps` vs `get_padding_horizontal/vertical`); they agree for a static
                // dimension, and a `Dimension::Dynamic` closure that is not pure would deflate by one
                // value and add another. Making the walk the single source is a later cleanup.
                inner_constraints = inner_constraints.offset(start + end, top + bottom);
            }
            SizeStep::MinWidth(v) => {
                inner_constraints.min_width =
                    v.clamp(inner_constraints.min_width, inner_constraints.max_width);
            }
            SizeStep::MaxWidth(v) => {
                inner_constraints.max_width =
                    v.clamp(inner_constraints.min_width, inner_constraints.max_width);
            }
            SizeStep::MinHeight(v) => {
                inner_constraints.min_height =
                    v.clamp(inner_constraints.min_height, inner_constraints.max_height);
            }
            SizeStep::MaxHeight(v) => {
                inner_constraints.max_height =
                    v.clamp(inner_constraints.min_height, inner_constraints.max_height);
            }
            SizeStep::SizeIn {
                min_width,
                min_height,
                max_width,
                max_height,
                enforce_incoming,
            } => {
                // Compose resolves a range into a target Constraints first: a specified max is
                // itself, an unspecified one is Infinity; a specified min is clamped into [0, max]
                // (`Size.kt:808-844`).
                // A negative max is coerced to 0 first (Compose's `fastCoerceAtLeast(0)`); feeding
                // it straight into `clamp` would panic on its `min <= max` assertion during measure.
                let target_max_w = max_width.unwrap_or(f32::MAX).max(0.0);
                let target_min_w = min_width.map(|v| v.clamp(0.0, target_max_w)).unwrap_or(0.0);
                let target_max_h = max_height.unwrap_or(f32::MAX).max(0.0);
                let target_min_h = min_height.map(|v| v.clamp(0.0, target_max_h)).unwrap_or(0.0);
                let (lo_w, hi_w) = (inner_constraints.min_width, inner_constraints.max_width);
                let (lo_h, hi_h) = (inner_constraints.min_height, inner_constraints.max_height);
                if enforce_incoming {
                    // `constraints.constrain(target)`: the target range intersected with the
                    // incoming one — the incoming range always wins where it is tighter.
                    inner_constraints.min_width = target_min_w.clamp(lo_w, hi_w);
                    inner_constraints.max_width = target_max_w.clamp(lo_w, hi_w);
                    inner_constraints.min_height = target_min_h.clamp(lo_h, hi_h);
                    inner_constraints.max_height = target_max_h.clamp(lo_h, hi_h);
                } else {
                    // `required*In`: a specified bound ignores incoming; the other end still
                    // respects it (`Size.kt:854-883`).
                    inner_constraints.min_width = if min_width.is_some() {
                        target_min_w
                    } else {
                        lo_w.min(target_max_w)
                    };
                    inner_constraints.max_width = if max_width.is_some() {
                        target_max_w
                    } else {
                        hi_w.max(target_min_w)
                    };
                    inner_constraints.min_height = if min_height.is_some() {
                        target_min_h
                    } else {
                        lo_h.min(target_max_h)
                    };
                    inner_constraints.max_height = if max_height.is_some() {
                        target_max_h
                    } else {
                        hi_h.max(target_min_h)
                    };
                }
            }
            SizeStep::DefaultMin {
                min_width,
                min_height,
            } => {
                if let Some(v) = min_width {
                    if inner_constraints.min_width == 0.0 {
                        inner_constraints.min_width = v.clamp(0.0, inner_constraints.max_width);
                    }
                }
                if let Some(v) = min_height {
                    if inner_constraints.min_height == 0.0 {
                        inner_constraints.min_height = v.clamp(0.0, inner_constraints.max_height);
                    }
                }
            }
            SizeStep::Wrap {
                width,
                height,
                align,
                unbounded,
            } => {
                wrap_steps.push(WrapStep {
                    align,
                    width,
                    height,
                    min_width: inner_constraints.min_width,
                    min_height: inner_constraints.min_height,
                    max_width: inner_constraints.max_width,
                    max_height: inner_constraints.max_height,
                });
                if width {
                    inner_constraints.min_width = 0.0;
                    if unbounded {
                        inner_constraints.max_width = f32::MAX;
                    }
                }
                if height {
                    inner_constraints.min_height = 0.0;
                    if unbounded {
                        inner_constraints.max_height = f32::MAX;
                    }
                }
            }
        }
    }

    // Open layout nodes, type A: transform the constraints in chain order, after the size steps.
    // The position: after every size/padding/insets step in the chain, before the intrinsic
    // request and before padding is accounted for. Dynamic values are evaluated inside the
    // transform (a `State::get` during measure registers a layout dependency, like
    // `Dimension::Dynamic`).
    // The chain is copied out of the Arc to avoid a borrow clash with the mutable writes that follow.
    // Early exit when there is no node: saves one collect allocation per node per measure.
    let layout_transforms: Vec<std::sync::Arc<dyn crate::modifier::LayoutModifierNode>> =
        if nodes[idx].modifier.has_layout_nodes() {
            nodes[idx].modifier.layout_nodes().cloned().collect()
        } else {
            Vec::new()
        };
    for t in &layout_transforms {
        inner_constraints = t.transform(inner_constraints);
    }

    // Step 1 (a fixed `Size`) used to be applied here as well, reading `Modifier::fixed_size()`.
    // It was redundant — `resolved_size` above already turns every `Static(Fixed)`/`Static(Dp)`/
    // `Static(Px)` axis into a number and tightens with it, `fixed_size` only reports the subset
    // where BOTH axes are static, and merging is a superset of "the first element with both static"
    // — but it was not harmless: it re-wrote the RAW request after the clamp, so a `.size(100, 600)`
    // in a 470 dp slot came out at 600 again and undid the alignment with Compose. Removed rather
    // than duplicated so there is one place that decides a node's size from its modifier.

    // 1.5 Intrinsic-size request (`Modifier.width/height(IntrinsicSize)`): that axis takes the
    // content's own intrinsic measurement. Compose does it in
    // `IntrinsicWidthNode.calculateContentConstraints` — compute the content's intrinsic size on the
    // axis, then tighten the constraints to it; with `enforceIncoming` (the `size/width/height`
    // forms) the incoming range is still the final clipper, and `requiredWidth/Height(IntrinsicSize)`
    // sets it to false (the incoming range is ignored, like requiredSize).
    // The position is AFTER the size steps: padding is one of them now (`SizeStep::Padding`)
    // and has already deflated the constraints in chain order, so what is tightened here is the
    // CONTENT size (the node's box minus its padding); the outer size adds the padding back at
    // the end (`intrinsic_size_of` answers the box INCLUDING padding, which is why it is
    // subtracted first — see the note below).
    if let Some((size, enforce_incoming)) = nodes[idx].modifier.intrinsic_width_request() {
        // The constraints the request saw at ITS chain position decide the clamp: a fill outside
        // has already tightened them (Compose's IntrinsicWidthNode measures under those), while a
        // fill inside has not (FillNode forwards the intrinsic answer, so the request decides
        // first and the inner fill then fills the tight result).
        // The walk already deflated `inner_constraints` by the chain's padding, while
        // `intrinsic_size_of` subtracts the axis padding from `other` itself — so hand it an
        // OUTER-space value (add the element padding back) and the two cancel. Without this the
        // content was asked at `width - 2 * padding`: `.padding_horizontal(8).height(IntrinsicSize::Max)`
        // in a 400 dp slot laid its content out at 368 instead of 384.
        let (pad_other_a, pad_other_b) = nodes[idx].modifier.get_padding_vertical();
        let other = inner_constraints.max_height + pad_other_a + pad_other_b;
        // `intrinsic_size_of` answers the PADDED box (its own padding bookkeeping), while the engine
        // adds the element padding to the outer size at the end of this function. Tightening to the
        // padded value and then adding the padding again counted it twice (measured before this:
        // `padding_horizontal(12).width(IntrinsicSize::Max)` over a 56 dp label came out 104 instead of
        // 80). Clamp in the request's own space, then hand the content width to the pipeline.
        let mut w = intrinsic_size_of(nodes, policies, idx, IntrinsicQuery::for_width(size), other);
        if enforce_incoming {
            w = match intrinsic_incoming_width {
                Some((lo, hi)) => w.clamp(lo, hi),
                None => inner_constraints.constrain_width(w),
            };
        }
        let (pad_s, pad_e) = nodes[idx].modifier.get_padding_horizontal();
        inner_constraints = inner_constraints.tighten_width((w - pad_s - pad_e).max(0.0));
    }
    if let Some((size, enforce_incoming)) = nodes[idx].modifier.intrinsic_height_request() {
        let (pad_other_a, pad_other_b) = nodes[idx].modifier.get_padding_horizontal();
        let other = inner_constraints.max_width + pad_other_a + pad_other_b;
        let mut h = intrinsic_size_of(nodes, policies, idx, IntrinsicQuery::for_height(size), other);
        if enforce_incoming {
            h = match intrinsic_incoming_height {
                Some((lo, hi)) => h.clamp(lo, hi),
                None => inner_constraints.constrain_height(h),
            };
        }
        let (pad_t, pad_b) = nodes[idx].modifier.get_padding_vertical();
        inner_constraints = inner_constraints.tighten_height((h - pad_t - pad_b).max(0.0));
    }

    // 2. Apply padding
    //
    // BOTH kinds of padding deflated IN THE WALK, at their chain positions (element padding through
    // `SizeStep::Padding`, window insets through `SizeStep::InsetPadding`). What is left here is the
    // bookkeeping the pipeline needs afterwards: the totals for the OUTER size and for the placement
    // shift, and the consumed set the subtree must inherit.
    let (pad_start, pad_end) = nodes[idx].modifier.get_padding_horizontal();
    let (pad_top, pad_bottom) = nodes[idx].modifier.get_padding_vertical();
    let pad_start = pad_start + inset_start;
    let pad_end = pad_end + inset_end;
    let pad_top = pad_top + inset_top;
    let pad_bottom = pad_bottom + inset_bottom;
    let _inset_guard = crate::window::insets::ConsumeScope::enter(inset_consumed);
    let pad_x = pad_start + pad_end;
    let pad_y = pad_top + pad_bottom;
    // The single source for this node's padding: the outer size and the placement shift below use
    // it, and the subcomposition adoption path reads it instead of re-resolving the modifier (which
    // drops the insets and re-evaluates a dynamic dimension).
    nodes[idx].resolved_padding = (pad_start, pad_top, pad_end, pad_bottom);

    // 2b. `Modifier::paddingFrom` relaxes the MINIMUM on its line's axis before the content is
    // measured, the way Compose measures its child with `constraints.copy(minHeight = 0)`
    // (`foundation/layout/AlignmentLine.kt:311-315`). Without it a min height handed down from above
    // is already inside the content's own size, and the padding is then added on top of it — measured:
    // a text under `min_height = 100` with `padding_from_baseline(Some(40))` came out 122.73 tall
    // instead of 100, because the 100 was counted as the content.
    //
    // The minimum is not dropped: the padding step below puts it back, and the content still sits at
    // `before` inside the taller box.
    let mut pad_from_min_width = inner_constraints.min_width;
    let mut pad_from_min_height = inner_constraints.min_height;
    for (line, _, _) in nodes[idx].modifier.get_padding_from() {
        if line.horizontal {
            pad_from_min_height = pad_from_min_height.max(inner_constraints.min_height);
            inner_constraints.min_height = 0.0;
        } else {
            pad_from_min_width = pad_from_min_width.max(inner_constraints.min_width);
            inner_constraints.min_width = 0.0;
        }
    }

    // 3. Save the viewport before the scroll step rewrites the maximum. `fillMax*` has already
    // been applied in the size walk above, at its chain position (`FillNode`, `Size.kt:713-746`).
    let viewport_height = inner_constraints.max_height;
    let viewport_width = inner_constraints.max_width;

    // 4. Check the scroll modifier: the children are measured unbounded
    if nodes[idx].modifier.vertical_scroll_state().is_some() {
        // A scroll container fills its viewport itself (fill_max_height skips an unbounded max, so it
        // is applied here)
        if nodes[idx].modifier.is_fill_max_height() && inner_constraints.max_height >= f32::MAX {
            inner_constraints.min_height = viewport_height;
        }
        // Keep the viewport height for the scroll clamp
        nodes[idx].scroll_viewport_height = viewport_height;
        // Lazy lists: the content-height State goes into a node field (apply_scroll_delta reads it).
        // It is read before measuring, so the value is the previous frame's write-back; on the first
        // frame it is 0, which degrades to the node's own height.
        if let Some(ch) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_height = ch.get();
            nodes[idx].scroll_reverse = nodes[idx].modifier.is_lazy_scroll_reverse();
        }
        // A lazy container is NOT rewritten to unbounded: its policy sets the children's constraints
        // itself (an infinite height) and keeps the finite max_height so it can see the real viewport
        // — only then is the internal max_offset = content_h - vh right (measured: falling back to
        // vh = 600 against a real viewport of ~500 overshot a jump to the end and pushed item 999 out
        // of the bottom). A plain scroll container is still rewritten (its content
        // wrap）。
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_height = f32::MAX;
        }
    }
    if nodes[idx].modifier.horizontal_scroll_state().is_some() {
        // A scroll container fills its viewport itself (fill_max_width skips an unbounded max, so it
        // is applied here)
        if nodes[idx].modifier.is_fill_max_width() && inner_constraints.max_width >= f32::MAX {
            inner_constraints.min_width = viewport_width;
        }
        // Keep the viewport width for the scroll clamp
        nodes[idx].scroll_viewport_width = viewport_width;
        // Reversed horizontal scrolling (RTL mirrors the render translate: offset 0 shows the end of
        // the content, matching a vertical reverseLayout). A LazyRow's reverse_layout sets the same
        // lazy_scroll_reverse flag a vertical list uses, and it is claimed here too — otherwise a
        // reversed horizontal list would mirror its placements but not its render translate, and the
        // two ends of the offset range would mean different things. A non-lazy horizontal container
        // carries no LazyScroll element, so the || is safe.
        nodes[idx].scroll_reverse = nodes[idx].modifier.is_horizontal_scroll_reverse()
            || nodes[idx].modifier.is_lazy_scroll_reverse();
        // Lazy horizontal lists: the content-width State goes into a node field, with the same
        // meaning as the vertical one.
        if let Some(cw) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_width = cw.get();
        }
        // A lazy container is NOT rewritten to unbounded: the finite max_width lets the policy see
        // the real viewport width.
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_width = f32::MAX;
        }
    }

    // The actual measure
    // RemeasureToBounds: the child layout is re-measured with FIXED constraints
    // of the animated bounds, so content reflows (text rewraps, rows resize)
    // instead of being graphically scaled. Applied last — Compose's "animated
    // fixed constraints" override the incoming and modifier constraints alike.
    if let Some(fm) = flight {
        if let Some(c) = fm.content {
            inner_constraints = Constraints::new(c.width, c.width, c.height, c.height);
            // A scroll container reads its viewport from the constraints it was
            // measured with, so a re-measured one must re-derive them — render
            // clips and hit clamping both use these, and they would otherwise
            // stay on the natural size while the box animates.
            if nodes[idx].modifier.vertical_scroll_state().is_some() {
                nodes[idx].scroll_viewport_height = c.height;
            }
            if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                nodes[idx].scroll_viewport_width = c.width;
            }
        }
    }

    let mut result = if let Some(pidx) = nodes[idx].measure_policy {
        // Copy the child indices first: `policy.measure` borrows all of `nodes` mutably, so the
        // `nodes[idx]` borrow cannot be held across it.
        //
        // The ADOPTED subcomposition child is left out: this policy did not create it (a subcomposing
        // component appends it during adoption), and a policy that counts its children read one too
        // many. Measured on `TabRow`: `tab_count = children.len() - 2` became 4 for a three-tab row, so
        // each tab measured 488/4 = 122 instead of 488/3 ≈ 162.7, and the indicator was placed with the
        // four-tab geometry (`node_pos=(287.5,44)` where the three-tab one wants 389.2).
        let mut children = nodes[idx].children.clone();
        if let Some(sub) = nodes[idx].subcomposed_child {
            children.retain(|&c| c != sub);
        }
        let (size, placements) = policies[pidx].measure(nodes, policies, &children, inner_constraints);
        // After measuring, sync a lazy list's content height/width into the node fields (the render
        // pass's reverse translate and apply_scroll_delta depend on them). `measure_node` reads the
        // previous frame's value at the top, so a 0 on the first frame makes a reverseLayout translate
         // wrongly and pushes the whole content out of the viewport.
        let content_h_state = nodes[idx].modifier.lazy_scroll_content_height().map(|s| s.get());
        if let Some(v) = content_h_state {
            nodes[idx].scroll_content_height = v;
            nodes[idx].scroll_content_width = v;
        }
        // apply positions
        policies[pidx].place(nodes, &children, &placements);
        // ── Alignment-line inheritance ──
        //
        // A container reports the lines its children report, shifted into its own coordinates, with
        // each line's own merger combining them — Compose's rule (`ui/layout/AlignmentLine.kt:60-66`:
        // the position of a line within a layout is the merger applied over the children's values).
        // This is what lets a Row align by a baseline that lives inside a child: without it only a
        // DIRECT child's own line is visible, so `align_by_baseline` on a Column wrapping text found
        // nothing and fell back to the child's top edge.
        //
        // Done here rather than in every policy because this is the one point that has both the
        // children's lines (set during their own measure) and their positions (just placed), and a
        // policy does not even know its own node index.
        //
        // A policy may decide the set itself — Compose's `layout(alignmentLines = …)`: `Some` replaces the
        // inherited lines entirely (that is how an overlay child is kept out of its host's baseline), and
        // `None` leaves the merge below in charge.
        // A leaf's own line is untouched: it has no children, so the merge reports nothing for it.
        if let Some(lines) = policies[pidx].alignment_lines(nodes, &children) {
            for (line, value) in lines {
                nodes[idx].set_alignment_line(line, value);
            }
        } else {
            for child in &children {
                // Only the lines the child actually reports; a child that reports none contributes none.
                // `alignment_lines` is a small Vec (usually empty), so this is a cheap nested loop.
                let child_position = nodes[*child].position;
                let child_lines = nodes[*child].alignment_lines.clone();
                for (line, value) in child_lines {
                    // A horizontal line runs ACROSS the horizontal axis, so its position is a vertical
                    // offset from this node's top; a vertical line's position is horizontal.
                    let absolute = if line.horizontal {
                        child_position.y + value
                    } else {
                        child_position.x + value
                    };
                    // MERGE, not overwrite: two children reporting the same line combine through the
                    // line's own merger (`::min` for the first baseline of a block, `::max` for the
                    // last), which is what Compose's `merge` does.
                    let merged = match nodes[idx].alignment_line(line) {
                        Some(previous) => line.merge(previous, absolute),
                        None => absolute,
                    };
                    nodes[idx].set_alignment_line(line, merged);
                }
            }
        }
        // Record whether any child asks for a paint order of its own (`Modifier::z_index`). The
        // renderer and the hit test consult this before doing any ordering work, so the common case
        // (nobody sets a z) pays nothing for the feature existing.
        nodes[idx].children_have_z = children
            .iter()
            .any(|&c| nodes[c].modifier.get_z_index().is_some());
        // Apply the padding offset (RTL: start is on the right, so the children shift right)
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        let left_offset = if rtl { pad_end } else { pad_start };
        if left_offset > 0.0 || pad_top > 0.0 {
            for &c in &children {
                nodes[c].position.x += left_offset;
                nodes[c].position.y += pad_top;
            }
        }
        // Apply the per-child offset modifier (a plain offset mirrors x under RTL, as in Compose;
        // absolute_offset is exempt).
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        for &c in &children {
            if let Some((ox, oy)) = nodes[c].modifier.get_offset() {
                nodes[c].position.x += if rtl { -ox } else { ox };
                nodes[c].position.y += oy;
            }
            if let Some((ax, ay)) = nodes[c].modifier.get_absolute_offset() {
                nodes[c].position.x += ax;
                nodes[c].position.y += ay;
            }
        }
        // A non-lazy vertical scroll container: the content height is the lowest child edge (bottom
        // padding included), which feeds apply_scroll_delta's max_offset and the fling limit. This
        // fixes the fill case, where the container's own height equals the viewport and deriving
        // max_offset from measured_size therefore stayed 0 (nothing could scroll).
        if nodes[idx].modifier.vertical_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_h = 0.0f32;
            for &c in &children {
                let b = nodes[c].position.y + nodes[c].measured_size.height;
                if b > content_h { content_h = b; }
            }
            if content_h > 0.0 {
                content_h += pad_bottom;
                nodes[idx].scroll_content_height = content_h;
                // The scroll range ends when the CONTENT's bottom meets the container's own bottom, so
                // the container's height is what belongs here — not the padding-deducted viewport
                // constraint. `content_h` counts the content from the container's top edge (its children
                // sit below `pad_top`), so mixing the two double-counted the vertical padding and let a
                // menu scroll 2*padding past its end: measured with a 112x424 menu holding 20 items,
                // `content_h` was 976 against a viewport of 424-16 = 408, giving an offset of 568 where
                // 552 is the end — 16px of dead space under the last item (24 in total with its own 8dp
                // padding).
                let node_h = (size.height + pad_y).min(viewport_height + pad_y);
                let max_off = (content_h - node_h).max(0.0);
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        // A non-lazy horizontal scroll container: the content width is the rightmost child edge
        // (trailing padding included).
        if nodes[idx].modifier.horizontal_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_w = 0.0f32;
            for &c in &children {
                let right = nodes[c].position.x + nodes[c].measured_size.width;
                if right > content_w { content_w = right; }
            }
            if content_w > 0.0 {
                content_w += pad_end;
                nodes[idx].scroll_content_width = content_w;
                // Same shape as the vertical case above: the range ends at the container's own edge.
                let node_w = (size.width + pad_x).min(viewport_width + pad_x);
                let max_off = (content_w - node_w).max(0.0);
                if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        let mut outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        // Clamp a scroll container's own size back to its viewport: step 4 made the children's
        // constraints unbounded (max = f32::MAX), and without the clamp the container's height or
        // width would be dragged out by its content (a `.height(180.0)` list measured 991 high).
        // viewport_* was captured in step 3 (padding already removed, the size constraint already
        // narrowed), so the full viewport height is viewport + pad. Lazy containers skip the rewrite
        // (see step 4) and their policy already measured against the finite viewport, so this clamp
        // is a no-op for them (their content is at most the viewport). With an unbounded parent
        // (viewport = f32::MAX) the min has no effect and the content still sizes the container — a
        // scroll container without a fixed height grows with its content.
        if nodes[idx].modifier.vertical_scroll_state().is_some() && viewport_height < f32::MAX {
            outer_size.height = outer_size.height.min(viewport_height + pad_y);
        }
        if nodes[idx].modifier.horizontal_scroll_state().is_some() && viewport_width < f32::MAX {
            outer_size.width = outer_size.width.min(viewport_width + pad_x);
        }
        nodes[idx].measured_size = outer_size;
        (outer_size, placements)
    } else {
        // A leaf: use the ContentMeasurer or the default logic
        let size = if nodes[idx].has_text_content {
            // A combined measure and cache, so the Paragraph is built once
            // The parent's max_width is the layout width, which makes the text wrap inside the space
            // it has. For a scroll container inner_constraints.max_width is already f32::MAX.
            let layout_width = inner_constraints.max_width;
            let measured = measure_and_cache_text(&nodes[idx], layout_width);
            let text_size = measured.size;
            // A text leaf is what makes `Modifier::align_by_baseline` useful: it reports its text
            // baselines, measured from the node's own top, exactly as Compose's text does. Taken from
            // the paragraph just cached, so they are the baselines of the layout the size came from.
            //
            // Both of Compose's baselines: `FirstBaseline` is the paragraph's own
            // `alphabetic_baseline`, and `LastBaseline` is the last line's (`::max`, the line to align
            // by when the bottom of a wrapped block is what should line up).
            let (first, last) = {
                let paragraph = nodes[idx].cached_paragraph.borrow();
                match paragraph.as_ref() {
                    Some(para) => {
                        let first = para.alphabetic_baseline() as f32;
                        let last = para
                            .get_line_metrics()
                            .last()
                            .map(|m| m.baseline as f32)
                            .unwrap_or(first);
                        (Some(first), Some(last))
                    }
                    None => (None, None),
                }
            };
            if let Some(first) = first {
                nodes[idx].set_alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE, first);
            }
            if let Some(last) = last {
                nodes[idx].set_alignment_line(crate::layout::AlignmentLine::LAST_BASELINE, last);
            }
            // Supporting text (TextField draws it 4 dp below the container; the height reserves 20 so
            // it cannot overlap whatever follows).
            let supporting_h = supporting_text_height(&nodes[idx]);
            // The final size is clamped into the constraints (a fill_max_width has already tightened
            // them, so the text fills the width it was given). The height is the paragraph's real
            // height (wrapping included) plus the supporting line — never approximated from the
            // explicit newline count, which would crop wrapped text; a raised min_height floors it.
            let final_text_size = Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height + supporting_h),
            );
            // Compose's `onTextLayout` runs AFTER the layout is final: its `TextLayoutResult.size`
            // is `constraints.constrain(...)` of the paragraph, so a `fill_max_width` text reports
            // the filled width (measured before this fix: 12 where the box was 300). The callback
            // is fired from here because this arm is the only place that knows the constraints.
            if let Some(callback) = &measured.on_text_layout {
                callback(&crate::text::TextLayoutResult {
                    size: final_text_size,
                    text_size: measured.text_size,
                    line_count: measured.line_count,
                    did_exceed_max_lines: measured.did_exceed_max_lines,
                    first_baseline: measured.first_baseline,
                    last_baseline: measured.last_baseline,
                    // Compose's `didOverflowWidth` / `didOverflowHeight`: the laid-out text is
                    // wider or taller than the box it was measured into.
                    did_overflow_width: measured.longest_line > final_text_size.width + 0.5,
                    did_overflow_height: measured.size.height > final_text_size.height + 0.5,
                });
            }
            final_text_size
        } else if nodes[idx].has_richtext_content {
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_richtext(&nodes[idx], layout_width);
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else if nodes[idx].has_image_content {
            // An Image leaf: its intrinsic size (bitmap pixels or the SVG viewBox), clamped by the
            // constraints.
            let (iw, ih) = nodes[idx].modifier.image_intrinsic_size().unwrap_or((0.0, 0.0));
            Size::new(
                inner_constraints.constrain_width(iw),
                inner_constraints.constrain_height(ih),
            )
        } else {
            // A plain leaf
            let w = inner_constraints.constrain_width(
                if inner_constraints.has_fixed_width() {
                    inner_constraints.min_width
                } else {
                    0.0
                },
            );
            let h = inner_constraints.constrain_height(
                if inner_constraints.has_fixed_height() {
                    inner_constraints.min_height
                } else {
                    0.0
                },
            );
            Size::new(w, h)
        };

        nodes[idx].measured_size = size;
        // Add the padding back to a leaf's size (the container path does the same). Leaves used to
        // lose it: the measured size excluded the padding, so Text + padding collapsed and the content
        // hugged the edge.
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        nodes[idx].measured_size = outer_size;
        (outer_size, Vec::new())
    };

    // ── `Modifier::wrapContent*` ──
    //
    // The content was measured without the incoming minimum (and, when unbounded, without the
    // maximum). Compose's `WrapContentNode` then reports the content coerced back into the incoming
    // range and places it inside that box by `align` (`Size.kt:1024-1061`). Steps apply from the
    // innermost out — the last one recorded wraps everything measured before it.
    for wrap in wrap_steps.iter().rev() {
        // The wrap node sits INSIDE the element padding (Compose's padding node deflates before the
        // wrap reaches it), so the range it recorded is the INNER one — and `result.0` is already the
        // OUTER size (the padding was added above). Clamping the outer size against the inner range
        // made a padded node short by the padding on each side: `.padding(8).wrap_content_width` over
        // a 380 dp content in a 0..400 slot reported 392 where Compose reports 396.
        let outer = result.0;
        let inner = Size::new(
            (outer.width - pad_x).max(0.0),
            (outer.height - pad_y).max(0.0),
        );
        let wrapper_inner = Size::new(
            inner.width.clamp(wrap.min_width, wrap.max_width),
            inner.height.clamp(wrap.min_height, wrap.max_height),
        );
        let wrapper = Size::new(wrapper_inner.width + pad_x, wrapper_inner.height + pad_y);
        let (dx, dy) = wrap_alignment_offset(
            wrap.align,
            inner,
            wrapper_inner,
            nodes[idx].layout_direction,
            wrap.width,
            wrap.height,
        );
        if dx != 0.0 || dy != 0.0 {
            if nodes[idx].children.is_empty() {
                let (pad_s, pad_t, pad_e, _pad_b) = nodes[idx].resolved_padding;
                let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
                let left = if rtl { pad_e } else { pad_s };
                nodes[idx].content_box_override = Some((left + dx, pad_t + dy, inner.width, inner.height));
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.x += dx;
                    nodes[c].position.y += dy;
                }
            }
            for (line, value) in nodes[idx].alignment_lines.iter_mut() {
                *value += if line.horizontal { dy } else { dx };
            }
        }
        result.0 = wrapper;
        nodes[idx].measured_size = wrapper;
    }

    // ── Caller-declared alignment lines (`Modifier::alignment_line`) ──
    //
    // Published BEFORE the `paddingFrom` step below, because that step measures against the lines a
    // node reports — including one declared here. Compose declares them as the layout is produced
    // (`layout(w, h, alignmentLines = mapOf(…))`); a `MeasurePolicy` in winia is never told its own
    // node index, so the declaration travels on the modifier and is evaluated here, where the
    // measured size exists.
    for (line, value_of) in nodes[idx].modifier.get_alignment_line_values() {
        let value = value_of(result.0);
        nodes[idx].set_alignment_line(line, value);
    }

    // ── `Modifier::paddingFrom` ──
    //
    // Runs here, after the content is measured, because it is the content's own alignment line that
    // says how much padding the node needs — Compose's `alignmentLineOffsetMeasure`
    // (`foundation/layout/AlignmentLine.kt:304-358`):
    //
    //   linePosition = the content's line (0 when it reports none)
    //   paddingBefore = (before - line).coerceIn(0, axisMax - axis)          // 0 when unspecified
    //   paddingAfter  = (after - axis + line).coerceIn(0, axisMax - axis - paddingBefore)
    //   size on the line's axis = max(paddingBefore + axis + paddingAfter, min)
    //   content placed at paddingBefore when `before` is given, else at size - paddingAfter - axis
    //
    // `before` therefore wins when the two cannot both fit, and the padding never pushes the node past
    // the incoming maximum on that axis. The other axis is untouched.
    //
    // Two elements can be in force at once (`padding_from_baseline(top, bottom)` is one for the first
    // baseline and one for the last), and each sees what the one before it did — the same nesting
    // Compose gets from stacking two layout nodes. So the running size, the content's box and the
    // node's own lines all move along with the padding, which is what lets the second element read a
    // line the first has already shifted, and an outer `align_by` see the padded position.
    let mut leaf_box: Option<(f32, f32, f32, f32)> = None;
    for (line, before, after) in nodes[idx].modifier.get_padding_from() {
        let horizontal = line.horizontal;
        let size = result.0;
        let axis = if horizontal { size.height } else { size.width };
        let axis_max = if horizontal { inner_constraints.max_height } else { inner_constraints.max_width };
        // The minimum the padding must honour — the one from BEFORE the relaxation above, since the
        // content was measured without it.
        let min_axis = if horizontal { pad_from_min_height } else { pad_from_min_width };
        let line_position = nodes[idx].alignment_line(line).unwrap_or(0.0);
        let padding_before = (before.unwrap_or(0.0) - line_position).clamp(0.0, (axis_max - axis).max(0.0));
        let padding_after = (after.unwrap_or(0.0) - axis + line_position)
            .clamp(0.0, (axis_max - axis - padding_before).max(0.0));
        let grown = (padding_before + axis + padding_after).max(min_axis).max(axis);
        let placed_at = if before.is_some() {
            padding_before
        } else {
            grown - padding_after - axis
        };

        // The content's box inside this node, tracked across the elements. A leaf's content is
        // whatever it draws itself, inset by its `padding`; a container's is its children, which move.
        if leaf_box.is_none() && nodes[idx].children.is_empty() {
            let (pad_s, pad_t, pad_e, pad_b) = nodes[idx].resolved_padding;
            leaf_box = Some((
                if nodes[idx].layout_direction == LayoutDirection::Rtl { pad_e } else { pad_s },
                pad_t,
                (size.width - pad_s - pad_e).max(0.0),
                (size.height - pad_t - pad_b).max(0.0),
            ));
        }
        if horizontal {
            result.0 = Size::new(size.width, grown);
            if let Some((ix, iy, iw, ih)) = leaf_box.as_mut() {
                let _ = (ix, iw, ih);
                *iy += placed_at;
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.y += placed_at;
                }
            }
            for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                if l.horizontal {
                    *value += placed_at;
                }
            }
        } else {
            result.0 = Size::new(grown, size.height);
            if let Some((ix, _iy, _iw, _ih)) = leaf_box.as_mut() {
                *ix += placed_at;
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.x += placed_at;
                }
            }
            for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                if !l.horizontal {
                    *value += placed_at;
                }
            }
        }
        nodes[idx].measured_size = result.0;
    }
    if leaf_box.is_some() {
        // The last element wins: it is the outermost, so its box is where the content ended up.
        let (ix, iy, iw, ih) = leaf_box.unwrap();
        nodes[idx].content_box_override = Some((ix, iy, iw, ih));
    }

    // ── `Modifier::minimum_interactive_size` ──
    //
    // Compose's `MinimumInteractiveModifierNode.measure` (`material3/InteractiveComponentSize.kt:98-140`):
    // the box is `max(content, sizePx)` on BOTH axes, the content is placed CENTRED inside it, and two
    // lines are published — where the VISUAL content begins — so a parent can align the small component
    // by its real edge instead of by its 48 dp touch target. `sizePx <= 0` means enforcement is off
    // (Compose's "unspecified or 0.dp"), and then nothing changes at all.
    //
    // Runs after `paddingFrom` because it wraps whatever is inside it, which is the padded content by
    // then — the same order the chain of two Compose nodes would give.
    if let Some(size_px) = nodes[idx].modifier.get_minimum_interactive_size() {
        if size_px > 0.0 {
            let content = result.0;
            let width = content.width.max(size_px);
            let height = content.height.max(size_px);
            let left = ((size_px - content.width) / 2.0).max(0.0);
            let top = ((size_px - content.height) / 2.0).max(0.0);
            nodes[idx].set_alignment_line(AlignmentLine::minimum_interactive_left(), left);
            nodes[idx].set_alignment_line(AlignmentLine::minimum_interactive_top(), top);
            // The paint reads THIS field, not the lines just above: a container merges its children's
            // alignment lines (see the field's comment), so an ancestor reading the line would inset its
            // own background. Only the node the modifier belongs to ever sets this.
            //
            // The CONTENT's own size, which is what the component declared — NOT `content`, which is
            // the MEASURED box and a parent may have forced it wider or narrower (a ButtonGroup
            // squeezing an item forces the node, and then `content` is the forced size, which painted
            // an icon button as a 48 x 40 OVAL). The declared size is the inner container: read the
            // chain's own `size(...)` when it is fixed, and fall back to the measured box when it is
            // not (a text-sized button has no fixed size and is not what this insets for).
            // Only a component with a DECLARED fixed size paints a box of its own inside the touch
            // target — an IconButton declares `size(40, 40)` and the 48 dp target is padding around
            // it. A component without one (a text Button) has no box to inset: its container IS the
            // node, so publishing a content box here would paint it without its padding and, worse,
            // freeze it: the row can widen the node, and a box derived from the content would not
            // follow (measured: section 4's first item grew 75 -> 86 and its container did not move).
            // Only a FULLY declared pair is a box this can paint: `Modifier::width()`/`height()`
            // fill the other axis with `Dimension::Auto`, `fixed_size()` matches any static pair,
            // and `to_logical_px()` answers 0.0 for `Auto` (and for `Fill`) — which would publish a
            // content box with a zero side, and a zero-height `paint_rect` paints nothing.
            let declared = nodes[idx]
                .modifier
                .fixed_size()
                .map(|(w, h)| (w.to_logical_px(), h.to_logical_px()));
            if let Some((content_w, content_h)) = declared {
                nodes[idx].minimum_interactive_content = Some((content_w, content_h));
            }
            // `minimum_interactive_painted` and `content_shift` are NOT cleared here: the clear at
            // the top of this measurement covers them, and it has to run for a node that carries no
            // minimum-interactive element at all. Clearing a second time here only read as if this
            // block owned it.
            {
                let cx = (width - content.width) / 2.0;
                let cy = (height - content.height) / 2.0;
                if let Some((ix, iy, _iw, _ih)) = leaf_box.as_mut() {
                    *ix += cx;
                    *iy += cy;
                    nodes[idx].content_box_override = Some((*ix, *iy, content.width, content.height));
                } else {
                    let kids = nodes[idx].children.clone();
                    for c in kids {
                        nodes[c].position.x += cx;
                        nodes[c].position.y += cy;
                    }
                }
                // The lines published above are the VISUAL content's position inside the bigger box, so
                // they do not move with the centring — but the lines inherited from the children do, and
                // so does a line a caller declared with `Modifier::alignment_line`, whose closure saw the
                // pre-enlargement size. Shifting them keeps both in the new coordinates, which is what lets
                // an outer `align_by` land on the visual edge.
                for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                    let line = *l;
                    if line == AlignmentLine::minimum_interactive_left()
                        || line == AlignmentLine::minimum_interactive_top()
                    {
                        continue;
                    }
                    *value += if line.horizontal { cy } else { cx };
                }
                result.0 = Size::new(width, height);
                nodes[idx].measured_size = result.0;
            }
        }
    }

    // Mark the measure done and cache the constraints for the next frame
    nodes[idx].dirty = false;
    nodes[idx].layout_dirty = false;
    nodes[idx].cached_constraints = Some(constraints);
    // Report the size (Compose's onSizeChanged) once the final size is settled. The constant-fold
    // early exit does not come through here — an unchanged size needs no report — and the element
    // deduplicates again.
    let (rw, rh) = (nodes[idx].measured_size.width, nodes[idx].measured_size.height);
    nodes[idx].modifier.report_measured_size(rw, rh);

    // Flight layout contract (Compose `PlaceHolderSize`): `reported` is the size
    // the PARENT observes — `AnimatedSize` hands it the animated size so siblings
    // reflow, `ContentSize`/`JumpCut` keep the natural size so the surrounding
    // layout holds still. This node's OWN box (paint, clip, hit, text rects) must
    // keep the size the content was really measured at, so it lives in
    // `flight_content_size` instead: parents legitimately re-clobber
    // `measured_size` with the size they placed us at (`place()`).
    nodes[idx].flight_content_size = if flight.is_some() { Some(result.0) } else { None };
    if let Some(reported) = flight.and_then(|f| f.reported) {
        nodes[idx].measured_size = reported;
        result.0 = reported;
    }
    result
}

