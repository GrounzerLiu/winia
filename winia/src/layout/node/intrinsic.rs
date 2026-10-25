use super::*;

// ── Intrinsic measurement (Compose's intrinsic protocol) ──

/// One of Compose's four intrinsic queries: which extreme, on which axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicQuery {
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
}

impl IntrinsicQuery {
    /// The query a `Modifier.width(IntrinsicSize)` request asks for.
    pub(crate) fn for_width(size: IntrinsicSize) -> Self {
        match size {
            IntrinsicSize::Min => IntrinsicQuery::MinWidth,
            IntrinsicSize::Max => IntrinsicQuery::MaxWidth,
        }
    }

    /// The query a `Modifier.height(IntrinsicSize)` request asks for.
    pub(crate) fn for_height(size: IntrinsicSize) -> Self {
        match size {
            IntrinsicSize::Min => IntrinsicQuery::MinHeight,
            IntrinsicSize::Max => IntrinsicQuery::MaxHeight,
        }
    }

    /// Whether the query is about the horizontal axis.
    pub fn is_width(self) -> bool {
        matches!(self, IntrinsicQuery::MinWidth | IntrinsicQuery::MaxWidth)
    }
}

/// The scope a policy answers an intrinsic query in — Compose's `IntrinsicMeasureScope`.
///
/// A policy either approximates ([`IntrinsicCtx::approx_measure`], the same "reuse `measure`"
/// approximation Compose's defaults use) or asks a child directly
/// ([`IntrinsicCtx::child_intrinsic`], which inverts that child's own modifier chain).
///
/// Neither path writes the frame's measurement state. `child_intrinsic` never touches
/// [`measure_node`], and `approx_measure` runs the policy's own `measure` with its children
/// SUBSTITUTED (every descendant measurement answers an intrinsic query —
/// `synthetic_intrinsic_measure`), so no descendant is really measured, no `measured_size` /
/// `cached_constraints` / `dirty` is written, and no state a descendant's `measure` would mutate is
/// touched. That substitution is Compose's `DefaultIntrinsicMeasurable`; without it the
/// approximation really measures the subtree, which is a different thing from what Compose does.
pub struct IntrinsicCtx<'a> {
    pub(crate) nodes: &'a mut Vec<LayoutNode>,
    pub(crate) policies: &'a [Box<dyn MeasurePolicy>],
    pub(crate) policy_idx: usize,
}

impl<'a> IntrinsicCtx<'a> {
    /// Compose's `IntrinsicMeasureScope.measure`: run the policy's own `measure` with the probe
    /// constraints and report the size it produces.
    ///
    /// The query is part of the signature because Compose hands the policy a list of
    /// `DefaultIntrinsicMeasurable` wrappers instead of the real children
    /// (`ui/layout/MeasurePolicy.kt:100-107`), and that substitution is what the probe query drives
    /// here: while this call runs, every descendant `measure_node` answers an intrinsic query rather
    /// than measuring (`synthetic_intrinsic_measure`). Without it a defaulted container would really
    /// measure its subtree — which is how an intrinsic query above a plain `Stack` used to reach an
    /// `AnimatedSize`'s animation and mutate it.
    pub fn approx_measure(
        &mut self,
        children: &[usize],
        constraints: Constraints,
        query: IntrinsicQuery,
    ) -> Size {
        let _probe = IntrinsicProbeGuard::enter(query);
        let policy: &dyn MeasurePolicy = &*self.policies[self.policy_idx];
        policy.measure(self.nodes, self.policies, children, constraints).0
    }

    /// Ask a CHILD node for one of its intrinsic measurements, through its own modifier chain.
    /// `other` is the value of the axis that is not being queried (the height for a width query).
    pub fn child_intrinsic(&mut self, child: usize, query: IntrinsicQuery, other: f32) -> f32 {
        intrinsic_size_of(self.nodes, self.policies, child, query, other)
    }

    /// The child's `layoutWeight`. Read-only: a container whose intrinsic size needs the weight
    /// arithmetic (Row/Column) prices weighted children itself, because a weighted child has no
    /// intrinsic size of its own — the parent hands it a share of the available space.
    pub fn child_weight(&self, child: usize) -> f32 {
        self.nodes[child].modifier.get_layout_weight().unwrap_or(0.0)
    }
}

/// Answer an intrinsic query with the largest of the children's own answers — the side-effect-free
/// shape Compose uses where a layout's measure must not be consulted
/// (`LayoutModifierNodeWithPassThroughIntrinsics`, `animation/AnimationModifier.kt:259-280`, and the
/// max-over-measurables fold of a container, `AnimatedContent.kt:939-957`).
///
/// Any policy whose `measure` has SIDE EFFECTS must answer the four queries itself. The trait's
/// defaults answer them by running `measure` with the queried axis unbounded (above: Compose's own
/// default, and Compose's contract is explicit that intrinsics must be pure — "There should be no
/// side-effects from implementers of `maxIntrinsicWidth`", `ui/layout/MeasurePolicy.kt:133`), so a
/// policy that starts an animation, re-targets one, or consumes a first-measurement flag turns an
/// intrinsic query into a state change. Measured before the helper existed: an intrinsic query above
/// a `TabRow` made the indicator's target infinite and its placed x `NaN`, and one above an
/// `AnimatedSize` drove its animated size to the probe's value, re-arming the spring every frame.
pub fn max_child_intrinsic(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    query: IntrinsicQuery,
    other: f32,
) -> f32 {
    children
        .iter()
        .map(|&child| ctx.child_intrinsic(child, query, other))
        .fold(0.0, f32::max)
}

/// Intrinsic measurement OF a node — the answer a parent gets when it asks this node for one of its
/// intrinsic sizes.
///
/// Inverts this node's own modifier chain in the order [`measure_node_inner`] applies it, then asks
/// the content. `other` is the value on the axis that is NOT queried; some leaves (text) need it in
/// order to answer at all.
/// The offset `wrapContent*` shifts its content by. Only the axes the modifier names are aligned:
/// Compose's `wrapContentWidth` callback returns `IntOffset(align.align(...), 0)` and the height
/// form mirrors it, so a single-axis wrap must not centre the other axis.
pub(crate) fn wrap_alignment_offset(
    align: crate::layout::ContentAlignment,
    content: crate::unit::Size,
    wrapper: crate::unit::Size,
    direction: crate::layout::LayoutDirection,
    width: bool,
    height: bool,
) -> (f32, f32) {
    let (mut dx, mut dy) = align.anchor(content, wrapper, direction);
    if !width {
        dx = 0.0;
    }
    if !height {
        dy = 0.0;
    }
    (dx, dy)
}

/// The box range an intrinsic answer is clamped into, replayed in CHAIN ORDER from the innermost
/// modifier outward: Compose nests one layout node per call, so each call constrains the answer the
/// inner one produced. `fillMax` and `wrapContent` do not override the intrinsic methods upstream
/// (they are measure-time acts), so they are skipped; `size`/`requiredSize` pin both ends,
/// `sizeIn` clamps both, and the min/max family (including `defaultMinSize`) moves one side.
pub(crate) fn intrinsic_axis_range(m: &crate::modifier::Modifier, horizontal: bool) -> (f32, f32) {
    let mut lo = 0.0f32;
    let mut hi = f32::MAX;
    for step in m.size_steps().iter().rev() {
        match *step {
            crate::modifier::SizeStep::Size { width, height }
            | crate::modifier::SizeStep::RequiredSize { width, height } => {
                if let Some(v) = if horizontal { width } else { height } {
                    lo = v;
                    hi = v;
                }
            }
            crate::modifier::SizeStep::MinWidth(v) if horizontal => {
                lo = lo.max(v);
                hi = hi.max(v);
            }
            crate::modifier::SizeStep::MaxWidth(v) if horizontal => {
                lo = lo.min(v);
                hi = hi.min(v);
            }
            crate::modifier::SizeStep::MinHeight(v) if !horizontal => {
                lo = lo.max(v);
                hi = hi.max(v);
            }
            crate::modifier::SizeStep::MaxHeight(v) if !horizontal => {
                lo = lo.min(v);
                hi = hi.min(v);
            }
            crate::modifier::SizeStep::SizeIn {
                min_width,
                min_height,
                max_width,
                max_height,
                ..
            } => {
                let (min, max) = if horizontal {
                    (min_width, max_width)
                } else {
                    (min_height, max_height)
                };
                let tmax = max.unwrap_or(f32::MAX).max(0.0);
                let tmin = min.unwrap_or(0.0).clamp(0.0, tmax);
                lo = lo.clamp(tmin, tmax);
                hi = hi.clamp(tmin, tmax);
            }
            crate::modifier::SizeStep::DefaultMin { min_width, min_height } => {
                if let Some(v) = if horizontal { min_width } else { min_height } {
                    lo = lo.max(v);
                    hi = hi.max(v);
                }
            }
            _ => {}
        }
    }
    (lo, hi)
}

pub(crate) fn intrinsic_size_of(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    query: IntrinsicQuery,
    other: f32,
) -> f32 {
    let horizontal = query.is_width();
    // Read the whole chain in one borrow and let it go: the content query needs `nodes` mutably.
    let (
        lo,
        hi,
        pad_axis,
        pad_other,
        intrinsic_request,
        other_axis_scrolled,
        aspect,
    ) = {
        let m = &nodes[idx].modifier;
        let (lo, hi) = intrinsic_axis_range(m, horizontal);
        let aspect = m.aspect_ratio_constraint();
        let (pad_x, pad_y) = m.get_padding_values();
        // Compose's Scroll intrinsic block unbounds the axis that SCROLLS when it is asked about
        // the OTHER axis (a vertical scroller answers a width query with an infinite height).
        // Querying the scrolled axis itself keeps the offered cross-axis size, or a wrapped text
        // would answer the height of one long line.
        let other_axis_scrolled = if horizontal {
            m.vertical_scroll_state().is_some()
        } else {
            m.horizontal_scroll_state().is_some()
        };
        (
            lo,
            hi,
            if horizontal { pad_x } else { pad_y },
            if horizontal { pad_y } else { pad_x },
            if horizontal { m.intrinsic_width_request() } else { m.intrinsic_height_request() },
            other_axis_scrolled,
            aspect,
        )
    };

    // Padding on the OTHER axis reduces the space the content is asked about — Compose's padding
    // node offsets the constraints before the child's intrinsic is queried
    // (`foundation/layout/Padding.kt:449` + the inherited `NodeMeasuringIntrinsics` default), so a
    // `padding_horizontal(10)` under a height query must ask its content at `width - 20`, not at
    // `width`. Measured before this: a child whose intrinsic height equals the width it is given
    // answered 100 for an outer width of 100 with 10 of horizontal padding (correct: 80).
    let other = (other - pad_other).max(0.0);

    // A fixed axis needs no measurement, and it is also the node's own size: the pipeline tightens
    // the node to it and adds the padding to the OUTER size (padding is a chain step now), so
    // `size(100).padding(8)` measures 100, not 116.
    if lo == hi && lo < f32::MAX {
        return lo;
    }

    // `Modifier.width/height(IntrinsicSize)` declares an EXTREME for this axis, and Compose answers
    // both of its intrinsic queries with that extreme: `IntrinsicWidthNode.minIntrinsicWidth` and
    // `.maxIntrinsicWidth` both forward to `maxIntrinsicWidth` when the declared size is `Max`
    // (`foundation/layout/Intrinsic.kt:192-204`; the height node mirrors it at `:253-265`). Measured
    // before this: a node with `width(IntrinsicSize::Max)` answering a `MinWidth` query reported its
    // child's minimum (20) where Compose reports the declared maximum (80).
    let declared = match (intrinsic_request.map(|(size, _)| size), horizontal) {
        (Some(IntrinsicSize::Min), true) => Some(IntrinsicQuery::MinWidth),
        (Some(IntrinsicSize::Max), true) => Some(IntrinsicQuery::MaxWidth),
        (Some(IntrinsicSize::Min), false) => Some(IntrinsicQuery::MinHeight),
        (Some(IntrinsicSize::Max), false) => Some(IntrinsicQuery::MaxHeight),
        (None, _) => None,
    };
    // Compose's `AspectRatioNode` answers all four intrinsic queries from the ratio and the OTHER
    // axis (`AspectRatio.kt:123-161`), deferring to the child only when that axis is unbounded.
    // An outer `width/height(IntrinsicSize)` still wins for its own axis, which is why this branch
    // comes after `declared`.
    let aspect_other = if other_axis_scrolled { f32::MAX } else { other };
    let content = if let Some(declared) = declared {
        // `Modifier.width/height(IntrinsicSize)`: this axis IS the content's measurement, at the
        // extreme the modifier declares.
        content_intrinsic(nodes, policies, idx, declared, other)
    } else if let Some((ratio, _)) = aspect.filter(|_| aspect_other < f32::MAX) {
        let v = if horizontal { aspect_other * ratio } else { aspect_other / ratio };
        if v.is_finite() && v > 0.0 {
            v
        } else {
            content_intrinsic(nodes, policies, idx, query, aspect_other)
        }
    } else if other_axis_scrolled {
        // Asking a scroller about its CROSS axis: the scrolling axis is unbounded, so the content
        // is not squeezed by the current scroll extent (Compose's `Scroll` intrinsic block).
        content_intrinsic(nodes, policies, idx, query, f32::MAX)
    } else {
        content_intrinsic(nodes, policies, idx, query, other)
    };

    // `fillMax*` is deliberately not honoured here: Compose's `FillNode` keeps the DEFAULT
    // intrinsic behavior (only its size node and its `defaultMinSize` node override it), and
    // filling is a measure-time act that needs incoming space. Recorded in docs/intrinsic-size.md.
    //
    // The range applies to the node's BOX and the padding sits INSIDE it — the pipeline offsets the
    // constraints by the padding before the content is measured — so the padded content is what
    // gets clamped: `clamp(c, lo - pad, hi - pad) + pad` == `clamp(c + pad, lo, hi)`. Clamping the
    // content first and adding the padding afterwards lets the padding escape the clamp (measured:
    // a menu item with `min_width(112).padding_horizontal(12)` reported 136 instead of 112, because
    // its 64dp label was lifted to the minimum before the 24dp of padding was added on top).
    let lo_inner = (lo - pad_axis).max(0.0);
    let hi_inner = (hi - pad_axis).max(lo_inner);
    content.clamp(lo_inner, hi_inner) + pad_axis
}

/// The CONTENT's answer to an intrinsic query: the node's policy, or the node itself as a leaf.
pub(crate) fn content_intrinsic(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    query: IntrinsicQuery,
    other: f32,
) -> f32 {
    let mut children = nodes[idx].children.clone();
    if let Some(sub) = nodes[idx].subcomposed_child {
        children.retain(|&c| c != sub);
    }
    if let Some(pidx) = nodes[idx].measure_policy {
        if policies[pidx].subcomposes() {
            // A policy that composes its content DURING measurement cannot answer an intrinsic
            // query without composing, and Compose has no intrinsic scope for `SubcomposeLayout`
            // either. Report the last measured size on that axis instead of composing inside a
            // probe, which runs outside the frame's compose step.
            return if query.is_width() {
                nodes[idx].measured_size.width
            } else {
                nodes[idx].measured_size.height
            };
        }
        let mut ctx = IntrinsicCtx { nodes, policies, policy_idx: pidx };
        return match query {
            IntrinsicQuery::MinWidth => policies[pidx].min_intrinsic_width(&mut ctx, &children, other),
            IntrinsicQuery::MaxWidth => policies[pidx].max_intrinsic_width(&mut ctx, &children, other),
            IntrinsicQuery::MinHeight => policies[pidx].min_intrinsic_height(&mut ctx, &children, other),
            IntrinsicQuery::MaxHeight => policies[pidx].max_intrinsic_height(&mut ctx, &children, other),
        };
    }
    leaf_intrinsic(nodes, idx, query, other)
}

/// Leaf answers — mirrors the leaf branch of [`measure_node_inner`], but WITHOUT the incoming
/// constraints (`intrinsic_size_of` clamps the answer itself) and without writing the render cache.
///
/// - text: a width query is the paragraph's own intrinsic width, a height query is the height the
///   paragraph takes at the asked width (plus the supporting-text reserve the measure branch adds);
/// - an image reports its intrinsic size on both axes;
/// - anything else measures nothing, so it is 0 (the pipeline gives it its fixed axis, if any).
pub(crate) fn leaf_intrinsic(nodes: &mut Vec<LayoutNode>, idx: usize, query: IntrinsicQuery, other: f32) -> f32 {
    if nodes[idx].has_text_content {
        let Some(para) = build_text_paragraph(&nodes[idx], other) else {
            return 0.0;
        };
        return match query {
            // Both width answers are CEILED, exactly like the layout path (`measure_and_cache_text` does
            // `para.max_intrinsic_width().ceil()`): the paragraph's own width is fractional, while what the
            // text needs when it lays out is that width rounded up. Answering with the raw fraction made a
            // parent that shrinks to the intrinsic (`width(IntrinsicSize.Max)`) hand the label 28.2 and watch
            // it wrap into two lines needing 29.0 — measured in a dropdown menu item ("复制" in `label_large`).
            IntrinsicQuery::MinWidth => para.min_intrinsic_width().ceil(),
            IntrinsicQuery::MaxWidth => para.max_intrinsic_width().ceil(),
            // The paragraph was laid out at `other` (the width the parent asked about), so its
            // height is the answer. `measure_and_cache_text` would round the same way.
            IntrinsicQuery::MinHeight | IntrinsicQuery::MaxHeight => {
                para.height().ceil() + supporting_text_height(&nodes[idx])
            }
        };
    }
    if nodes[idx].has_richtext_content {
        // Rich text has no intrinsic accessor (inline drawables need the layout pass); answer with
        // the height it takes at the asked width, which is what its own measure would produce.
        let layout_width = if query.is_width() { f32::MAX } else { other };
        // The probe must not become the render cache: keep the paragraph that was there and put it
        // back afterwards (the plain-text leaf builds through `build_text_paragraph` and never
        // caches; rich text measures through the caching entry point, so restore explicitly).
        let saved = nodes[idx].cached_paragraph.borrow_mut().take();
        let size = measure_and_cache_richtext(&nodes[idx], layout_width);
        *nodes[idx].cached_paragraph.borrow_mut() = saved;
        return if query.is_width() { size.width } else { size.height };
    }
    if nodes[idx].has_image_content {
        let (iw, ih) = nodes[idx].modifier.image_intrinsic_size().unwrap_or((0.0, 0.0));
        // An image has one size; min and max agree, which is what the pipeline does for images.
        return if query.is_width() { iw } else { ih };
    }
    // A plain leaf: only a fixed axis gives it a size, and that case never reaches this far
    // (`intrinsic_size_of` returns the fixed value without asking the content).
    0.0
}

