use super::*;

// ── MeasurePolicy trait ──

/// The measure and layout policy — Compose's MeasurePolicy.
///
/// A type implementing this trait owns one container's layout logic.
/// A type implementing this trait owns one container's layout logic.
pub trait MeasurePolicy: std::fmt::Debug {
/// The measure phase: given the constraints, return this node's size and the children's placements.
/// Arena flavour: `children` holds child indices and nodes/policies are split borrows (the policy
/// lives in the policies pool, the node in nodes, so field-level borrows cannot clash). An
/// implementation measures its children through `measure_node(nodes, policies, c, cc)`.
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>);

/// The layout phase: given the assigned size, place the children.
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]);

    /// Whether this policy composes content DURING measurement (`layout::subcompose`).
    ///
    /// Such a node must be re-measured whenever it is materialized: its content is composed inside
    /// the measurement, so a folded size freezes the content at the previous frame's parameters. The
    /// node's own `subcomposed` flag is only set BY a real measurement, so it cannot answer this for a
    /// node that was just rebuilt — asking the policy is what covers that case.
    fn subcomposes(&self) -> bool {
        false
    }

    // ── Intrinsic measurement (Compose's four `MeasurePolicy` intrinsic functions) ──
    //
    // Every method has a DEFAULT, exactly like Compose, and the default is Compose's own: "have
    // default implementations that make a best effort attempt to calculate the intrinsic
    // measurements by reusing the measure method". Concretely: measure the children with the
    // queried axis unbounded and the other axis fixed to the value the parent asked about, and
    // report the resulting size. That is an approximation for layouts whose geometry depends on
    // the space they are given; a policy that knows better (Row/Column, whose `weight` handling
    // needs the same arithmetic as its measure phases) overrides these.
    //
    // The children handed to that `measure` are SUBSTITUTED, exactly as Compose substitutes them:
    // each child measurement answers the corresponding intrinsic query instead of measuring
    // (`approx_measure` + `synthetic_intrinsic_measure`). That is what lets a policy with side
    // effects in its `measure` be asked an intrinsic question through a defaulted ancestor without
    // running it, and it is why Compose's animating nodes can inherit
    // `LayoutModifierNodeWithPassThroughIntrinsics`. A policy whose own `measure` has side effects
    // still overrides all four — "There should be no side-effects from implementers of
    // `maxIntrinsicWidth`" (`ui/layout/MeasurePolicy.kt:133`) — so that answering is exact and
    // never enters the approximation at all.

    /// The smallest width this content can be laid out at, given it will be `height` tall.
    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, f32::MAX, 0.0, height),
            IntrinsicQuery::MinWidth,
        )
        .width
    }

    /// The width this content takes with nothing wrapped, given it will be `height` tall.
    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, f32::MAX, 0.0, height),
            IntrinsicQuery::MaxWidth,
        )
        .width
    }

    /// The smallest height this content can be laid out at, given it will be `width` wide.
    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, width, 0.0, f32::MAX),
            IntrinsicQuery::MinHeight,
        )
        .height
    }

    /// The height this content takes with nothing wrapped, given it will be `width` wide.
    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, width, 0.0, f32::MAX),
            IntrinsicQuery::MaxHeight,
        )
        .height
    }

    /// The lines this container reports, when it decides them itself — Compose's
    /// `layout(alignmentLines = …)` parameter.
    ///
    /// `None`, the default, means "inherit the children's": `measure_node_inner` merges every child's lines
    /// through each line's own merger. `Some(lines)` REPLACES that set, which is what a container whose
    /// overlay child must not move its own line needs — `BadgedBox` reports the ANCHOR's baselines so a
    /// badge's text cannot shift the line a Row aligns a whole row of icons by
    /// (`material3/Badge.kt:97-100`, "to avoid default baseline calculations from including by any badge
    /// content").
    ///
    /// Called after the children are placed, so a policy may read their positions as well as their lines.
    fn alignment_lines(
        &self,
        _nodes: &[LayoutNode],
        _children: &[usize],
    ) -> Option<Vec<(AlignmentLine, f32)>> {
        None
    }
}


