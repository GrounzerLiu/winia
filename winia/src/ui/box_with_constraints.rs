//! `BoxWithConstraints` — the constraints-aware box, aligned with Compose's
//! `androidx.compose.foundation.layout.BoxWithConstraints`.
//!
//! ```ignore
//! BoxWithConstraints::new()
//!     .modifier(Modifier::new().fill_max_width())
//!     .build(ctx, |ctx, scope| {
//!         if scope.max_width() > 600.0 {
//!             Row::new().build(ctx, two_columns);
//!         } else {
//!             Column::new().build(ctx, one_column);
//!         }
//!     });
//! ```
//!
//! Compose's `BoxWithConstraints` gives its content a `BoxWithConstraintsScope` carrying the
//! constraints the box was measured with (`maxWidth`, `maxHeight`, `minWidth`, `minHeight`,
//! `constraints`, `maxDimension`, `minDimension`), so a layout can branch on the space actually
//! available instead of on the window's. Composition sees those values through a subcomposition.
//!
//! winia now HAS the measure-time subcomposition this needs (`ui::subcompose`), so the content runs
//! inside measurement with the constraints that measurement just computed — the same relation Compose
//! has, and the "one composition behind" deviation this module used to document is gone.
//!
//! `BoxWithConstraintsScope` reports the constraints the box was measured with, and the content
//! composes under them on the first frame like every later one. The box's node is marked
//! `subcomposed`, which keeps it from folding: its composed subtree lives in the arena, so a folded
//! frame would let materialize clear the parent's `children` and detach it (measured while building
//! the facility).
//!
//! # What this is not
//!
//! `subcompose` re-runs the content on every frame the box measures (which, because the box never
//! folds, is every frame), where Compose's subcomposition is skipped when nothing it depends on
//! changed. It is correct before it is cheap; making the subcomposition skip is the next round, and
//! it needs the adopted subtree keyed on the component's own key so it can be reused in place.

use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::box_layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::layout::node::{Alignment, MeasurePolicy, Placement, Size};
use crate::modifier::Modifier;

/// The scope handed to `BoxWithConstraints` content: what the box was measured with.
///
/// Mirrors Compose's `BoxWithConstraintsScope` minus the geometry objects (see the module docs).
#[derive(Clone, Copy)]
pub struct BoxWithConstraintsScope {
    constraints: Constraints,
}

impl BoxWithConstraintsScope {
    fn new(constraints: Constraints) -> Self {
        Self { constraints }
    }

    /// The constraints the box was measured with.
    pub fn constraints(&self) -> Constraints {
        self.constraints
    }

    pub fn min_width(&self) -> f32 {
        self.constraints.min_width
    }

    pub fn max_width(&self) -> f32 {
        self.constraints.max_width
    }

    pub fn min_height(&self) -> f32 {
        self.constraints.min_height
    }

    pub fn max_height(&self) -> f32 {
        self.constraints.max_height
    }

    /// The larger of the two maxima — Compose's `maxDimension`.
    pub fn max_dimension(&self) -> f32 {
        self.constraints.max_width.max(self.constraints.max_height)
    }

    /// The smaller of the two minima — Compose's `minDimension`.
    pub fn min_dimension(&self) -> f32 {
        self.constraints.min_width.min(self.constraints.min_height)
    }

    // `Dp` forms of the four bounds. The scope hands back the LAYOUT coordinate system (logical
    // pixels, what `Constraints` carries); these accessors spell the same numbers on the type for a
    // caller that wants to pass a bound straight into a `Modifier`. `Dp::to_logical` is the identity
    // here — do NOT round-trip through `to_px`, which is physical pixels (see its own docs).
    pub fn min_width_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.constraints.min_width)
    }

    pub fn max_width_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.constraints.max_width)
    }

    pub fn min_height_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.constraints.min_height)
    }

    pub fn max_height_dp(&self) -> crate::unit::Dp {
        crate::unit::Dp(self.constraints.max_height)
    }

    /// Whether the box has been measured yet: the initial backchannel value is unbounded, so an
    /// infinite maximum means "no measurement has been written". A caller that must not branch on
    /// the unbounded case on the first frame checks this.
    pub fn is_measured(&self) -> bool {
        self.constraints.max_width.is_finite() || self.constraints.max_height.is_finite()
    }
}

impl std::fmt::Debug for BoxWithConstraintsScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoxWithConstraintsScope")
            .field("constraints", &self.constraints)
            .finish()
    }
}

/// A `Box` (winia's `Stack`) whose content receives the constraints the box was measured with.
pub struct BoxWithConstraints {
    modifier: Modifier,
    alignment: Alignment,
}

impl BoxWithConstraints {
    pub fn new() -> Self {
        Self {
            modifier: Modifier::new(),
            alignment: Alignment::Start,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = a;
        self
    }

    /// Compose signature: `BoxWithConstraints(modifier) { /* BoxWithConstraintsScope + content */ }`.
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx, BoxWithConstraintsScope) + Send + 'static,
    ) {
        // The content is handed to the policy and run INSIDE measurement, with the real constraints
        // (Compose's `SubcomposeLayout` relation). `FnOnce` in a `Mutex` because the policy takes it
        // by shared reference and each frame's subcomposition runs it exactly once.
        let key = ctx.next_key();
        let policy = ConstraintsSubcomposePolicy {
            alignment: self.alignment,
            first_measure: std::cell::Cell::new(None),
            content: std::sync::Arc::new(std::sync::Mutex::new(Some(Box::new(content)))),
        };
        ctx.start_container(key, self.modifier, policy);
        ctx.end_node();
    }
}

impl Default for BoxWithConstraints {
    fn default() -> Self {
        Self::new()
    }
}

/// The policy behind [`BoxWithConstraints`]: it subcomposes the content with the constraints it was
/// measured with, and reports what that content measured.
struct ConstraintsSubcomposePolicy {
    alignment: Alignment,
    /// What this frame's first measurement produced, together with the compose generation it belongs
    /// to. A second `layout()` in the SAME frame (the frame handler runs one when a shared flight
    /// attaches a layout override) measures this node again; composing a second time would discard the
    /// first composition — which the adoption pass has already attached — and report a size derived
    /// from a tree nobody will see (measured while wiring this: the box read [0,0] while its child read
    /// [98,48]). A measurement in a LATER frame must compose again, because the content in the
    /// subcomposition is generally a function of parameters that may have changed — measured as the
    /// opposite failure: with the guard keyed on nothing, a cap change moved the state but the box
    /// kept reporting the old one, since the subcomposition never re-composed.
    first_measure: std::cell::Cell<Option<(u64, Size)>>,
    content: std::sync::Arc<
        std::sync::Mutex<Option<Box<dyn FnOnce(&mut ComposeCtx, BoxWithConstraintsScope) + Send>>>,
    >,
}

impl std::fmt::Debug for ConstraintsSubcomposePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConstraintsSubcomposePolicy")
    }
}

impl MeasurePolicy for ConstraintsSubcomposePolicy {
    fn measure(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _policies: &[Box<dyn MeasurePolicy>],
        _children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // Already measured in THIS frame: report the same answer instead of composing again (see the
        // `first_measure` field).
        let generation = crate::ui::subcompose::compose_generation().unwrap_or(0);
        if let Some((cached_generation, size)) = self.first_measure.get() {
            if cached_generation == generation {
                return (size, Vec::new());
            }
        }
        let scope = BoxWithConstraintsScope::new(constraints);
        let content = self.content.lock().unwrap().take();
        // The subcomposed content is adopted as this node's child, so its measurement IS this node's
        // size (`Box` semantics: the box is as big as its content, clamped by the constraints).
        // Reporting anything else would leave the box at 0 while holding a sized child — measured
        // while wiring this: the box read [0,0] with a [98,48] child under it.
        let size = crate::ui::subcompose::subcompose(constraints, move |ctx| {
            if let Some(content) = content {
                content(ctx, scope);
            }
        });
        let _ = self.alignment;
        self.first_measure.set(Some((generation, size)));
        // Report the content's size AS MEASURED: the engine applies the box's own constraints to a
        // policy's result, and clamping here as well double-clamps — the subcomposition laid itself
        // out under `constraints`, while the engine clamps against the constraints the box's modifier
        // produced (a different, usually tighter, set). Measured while wiring this: clamping both ways
        // made the reported width depend on which layer ran last, and the box read 0 on one frame and
        // 98 on the next.
        (size, Vec::new())
    }

    fn place(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _children: &[usize],
        _placements: &[Placement],
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::node::MeasurePolicy;

    /// The scope's own semantics: the four bounds, the dimension helpers, `is_measured`, and the `_dp`
    /// forms carrying the logical numbers (`Dp::to_logical` is the identity here).
    #[test]
    fn scope_reports_the_constraints_it_was_handed() {
        let unbounded = BoxWithConstraintsScope::new(Constraints::UNBOUNDED);
        assert!(!unbounded.is_measured(), "the initial value is the unbounded one");
        assert_eq!(unbounded.max_width(), f32::INFINITY);

        let measured = BoxWithConstraintsScope::new(Constraints::new(100.0, 400.0, 50.0, 300.0));
        assert!(measured.is_measured());
        assert_eq!(
            (measured.min_width(), measured.max_width(), measured.min_height(), measured.max_height()),
            (100.0, 400.0, 50.0, 300.0)
        );
        assert_eq!(measured.max_dimension(), 400.0, "the larger maximum");
        assert_eq!(measured.min_dimension(), 50.0, "the smaller minimum");
        assert_eq!(
            (
                measured.min_width_dp().to_logical(),
                measured.max_width_dp().to_logical(),
                measured.min_height_dp().to_logical(),
                measured.max_height_dp().to_logical(),
            ),
            (100.0, 400.0, 50.0, 300.0),
            "the dp forms match the raw bounds"
        );
    }

    /// The point of the rewrite, as a test: the content sees the REAL constraints **on the first
    /// frame**, which is what the `Reactive<Constraints>` version could not do (it read its initial
    /// unbounded value and only got the real one a composition later).
    #[test]
    fn the_content_sees_the_real_constraints_on_the_first_frame() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            BoxWithConstraints::new()
                .modifier(Modifier::new().size(120.0, 60.0))
                .build(ctx, move |ctx, scope| {
                    seen_in.lock().unwrap().push((
                        scope.min_width(),
                        scope.max_width(),
                        scope.is_measured(),
                    ));
                    crate::ui::Text::new("inside").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let runs = seen.lock().unwrap().clone();
        assert_eq!(runs.len(), 1, "the content ran once (in the measure pass): {runs:?}");
        assert!(
            runs[0].2,
            "the scope was measured on the first run — no unbounded first frame: {runs:?}"
        );
        assert!(runs[0].1.is_finite(), "and it carries a real maximum: {runs:?}");

        // The box lays out at its modifier's size, and its composed content is a child of it.
        let root = composer.arena.root.expect("root");
        let size = composer.arena_nodes()[root].measured_size;
        assert_eq!((size.width, size.height), (120.0, 60.0), "the box takes its modifier's size");
        assert_eq!(
            composer.arena.nodes[root].children.len(),
            1,
            "the subcomposed content was adopted under the box"
        );
    }

    /// The box's size IS its content's size, and that has to hold on the frame the content was
    /// composed on AND on the next one — a reused parent detaches and re-attaches the adopted child,
    /// and the size has to travel with it (the frame path measured [0,48] here while the child read
    /// [98,48]).
    #[test]
    fn the_box_sizes_to_its_content_on_the_first_frame_and_the_next() {
        let build = |ctx: &mut ComposeCtx| {
            BoxWithConstraints::new()
                .modifier(Modifier::new().max_width(200.0))
                .build(ctx, |ctx, _scope| {
                    crate::ui::Text::new("content").build(ctx);
                });
        };
        let mut composer = Composer::new();
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 160.0));
        let root = composer.arena.root.expect("root");
        let size = composer.arena_nodes()[root].measured_size;
        assert!(size.width > 0.0, "frame 1: the box is as wide as its content, got {size:?}");

        composer.compose(build);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 160.0));
        let size2 = composer.arena_nodes()[root].measured_size;
        assert!(size2.width > 0.0, "frame 2: still sized (the reused parent re-attaches the child), got {size2:?}");
    }

    /// A subcomposing node never folds: its subtree lives in the arena, so a folded frame would let
    /// materialize clear `children` and detach it. This is the failure that was measured while
    /// building the facility (the adopted child disappeared on the second frame), pinned here.
    #[test]
    fn the_box_is_measured_every_frame_so_its_content_is_not_detached() {
        let mut composer = Composer::new();
        let build = |ctx: &mut ComposeCtx| {
            BoxWithConstraints::new()
                .modifier(Modifier::new().size(80.0, 40.0))
                .build(ctx, |ctx, _scope| {
                    crate::ui::Text::new("content").build(ctx);
                });
        };
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(composer.arena.nodes[root].children.len(), 1, "adopted on frame 1");
        assert!(composer.arena.nodes[root].subcomposed, "the node is marked as subcomposing");

        // Frame 2: nothing dirty, same constraints — the fold must be refused for this node.
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            composer.arena.nodes[root].children.len(),
            1,
            "still exactly one adopted child after a second frame — no detach, no accumulation"
        );
    }

    // NOTE: the policy's own half (`subcompose()` measured without a composed tree around it) is
    // covered by `ui::subcompose_probe`'s direct tests. It cannot be tested here through
    // `measure_node` alone, because `subcompose()` requires the composer's layout pass to be armed
    // (`LayoutHostGuard`) — which is exactly the contract the facility documents.
}
