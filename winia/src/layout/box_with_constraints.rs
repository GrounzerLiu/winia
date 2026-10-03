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
//! composes under them on the first frame like every later one.
//!
//! # When the box re-measures
//!
//! The box's node is marked `subcomposed` (it composes during measurement), which is what puts its slot
//! key into the frame's layout invalidation set; its composed subtree lives in the arena and is only
//! re-attached by a measurement, so a box that never measured again would lose it.
//!
//! It does NOT mean "measure every frame". The box folds like any other node when nothing about it
//! changed, and it re-measures when one of these moved:
//!
//! - the composition that builds the box ran and its `Modifier` differs from last frame's — including a
//!   `max_width` computed from a state read in a PARENT's scope, which is the case the `bwc` UI fixture
//!   drives (`Composer::node_modifier_changed`);
//! - the box's node slot was dirtied (a declared parameter or a state read inside the box itself
//!   changed), which also seeds the key from the compose end;
//! - a state read DURING the measurement changed (a layout-only dependency, `layout_dirty_keys`).
//!
//! Folding an unchanged box is what keeps a screen with many of them cheap; `docs/benchmarks.md` has the
//! per-frame numbers (`one row updated, 800 rows`: 74915 µs before, ≈1700 µs after).
//!
//! # What this is not
//!
//! The content is re-composed on every measurement, and a measurement is what the three cases above ask
//! for — the box has no way to tell "the content would compose the same" and skip the work, where
//! Compose's subcomposition is skipped when nothing it depends on changed.

use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
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
    ///
    /// The content is `Fn`, not `FnOnce`: a subcomposition composes again on a LATER frame (its
    /// parameters are a function of the constraints, which change), so the closure has to be
    /// re-runnable. Taking it by value made the second frame compose NOTHING — measured in a window:
    /// `[sub] ... cache=miss nodes=0 root=None size=0x0`, and that `0x0` then overwrote the size the
    /// adoption pass had written, so the box read `[0,0]` while its adopted child read `[192,19]`.
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl Fn(&mut ComposeCtx, BoxWithConstraintsScope) + Send + 'static,
    ) {
        // The content is handed to the policy and run INSIDE measurement, with the real constraints
        // (Compose's `SubcomposeLayout` relation). `Fn` shared behind an `Arc` because the policy takes
        // it by shared reference and every frame's subcomposition runs it once.
        let key = ctx.next_key();
        let policy = ConstraintsSubcomposePolicy {
            alignment: self.alignment,
            first_measure: std::cell::Cell::new(None),
            content: std::sync::Arc::new(content),
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
    /// The content, re-runnable: a later frame's measurement composes it again (see `build`).
    content: std::sync::Arc<dyn Fn(&mut ComposeCtx, BoxWithConstraintsScope) + Send>,
}

impl std::fmt::Debug for ConstraintsSubcomposePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConstraintsSubcomposePolicy")
    }
}

impl MeasurePolicy for ConstraintsSubcomposePolicy {
    /// The framework's only subcomposing policy: its content is composed inside `measure`, so its
    /// node must be re-measured rather than folded (see the trait method's contract).
    fn subcomposes(&self) -> bool {
        true
    }

    fn measure(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _policies: &[Box<dyn MeasurePolicy>],
        _children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // Already measured in THIS frame: report the same answer instead of composing again (see the
        // `first_measure` field).
        let generation = crate::layout::subcompose::compose_generation().unwrap_or(0);
        if let Some((cached_generation, size)) = self.first_measure.get() {
            if cached_generation == generation {
                return (size, Vec::new());
            }
        }
        let scope = BoxWithConstraintsScope::new(constraints);
        let content = &self.content;
        // The subcomposed content is adopted as this node's child, so its measurement IS this node's
        // size (`Box` semantics: the box is as big as its content, clamped by the constraints).
        // Reporting anything else would leave the box at 0 while holding a sized child — measured
        // while wiring this: the box read [0,0] with a [98,48] child under it.
        let size = crate::layout::subcompose::subcompose(constraints, |ctx| {
            content(ctx, scope);
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
    use crate::runtime::composer::Composer;
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
                    crate::components::Text::new("inside").build(ctx);
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
                    crate::components::Text::new("content").build(ctx);
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

    /// An UNCHANGED box does not re-compose its content — the idle contract, and the reason a screen can
    /// carry many of these.
    ///
    /// The box folds when nothing about it moved (`dirty`, `layout_dirty` and its constraints all
    /// unchanged), and a folded box composes nothing: the content closure's run count is the observable.
    /// It is pinned because the invalidation rules around it are easy to widen by accident — the flag the
    /// compose end seeds from, the layout-invalidation walk, and the modifier comparison added for
    /// `a_cap_change_in_the_composition_reaches_the_content_the_box_composes` all run on this path, and
    /// any of them firing on an unchanged frame turns every box on the screen into per-frame work
    /// (measured at 800 boxes: 63233 µs a frame against 1038; `docs/benchmarks.md`).
    #[test]
    fn an_unchanged_box_does_not_recompose_its_content() {
        let runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let build = {
            let runs = runs.clone();
            move |ctx: &mut ComposeCtx| {
                let runs = runs.clone();
                BoxWithConstraints::new()
                    .modifier(Modifier::new().size(80.0, 40.0))
                    .build(ctx, move |ctx, _scope| {
                        runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        crate::components::Text::new("content").build(ctx);
                    });
            }
        };
        let mut composer = Composer::new();
        composer.compose(build.clone());
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            runs.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "frame 1 composes the content once"
        );

        for frame in 2..=3 {
            composer.compose(build.clone());
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            assert_eq!(
                runs.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "frame {frame}: nothing changed, so the box folded and composed no content"
            );
        }
    }

    /// The adopted subtree survives the frames the box does NOT measure.
    ///
    /// A subcomposing node's subtree lives in the arena and is re-attached by a measurement, so the one
    /// thing that must never happen is materialize dropping it while the node folds. This is the failure
    /// that was measured while building the facility (the adopted child disappeared on the second
    /// frame), pinned here — now on a frame where the box legitimately folds (nothing about it changed).
    #[test]
    fn the_box_keeps_its_adopted_child_across_a_frame_it_did_not_measure() {
        let mut composer = Composer::new();
        let build = |ctx: &mut ComposeCtx| {
            BoxWithConstraints::new()
                .modifier(Modifier::new().size(80.0, 40.0))
                .build(ctx, |ctx, _scope| {
                    crate::components::Text::new("content").build(ctx);
                });
        };
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(composer.arena.nodes[root].children.len(), 1, "adopted on frame 1");
        assert!(composer.arena.nodes[root].subcomposed, "the node is marked as subcomposing");

        // Frame 2: nothing dirty, same constraints — the box folds, and the child must still be there.
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            composer.arena.nodes[root].children.len(),
            1,
            "still exactly one adopted child after a second frame — no detach, no accumulation"
        );
    }

    /// A state read in the COMPOSITION, feeding the box's own `max_width`, has to reach the content the
    /// box composes at measure time — on the frame the state changes, not eventually.
    ///
    /// This is the shape the `bwc` UI fixture drives (`box_with_constraints_composes_its_content_at_measure_time`),
    /// reduced to one composer, so the failure is a two-second reproduction instead of a window test. It
    /// is the test that was written BECAUSE the reduced form failed while the layout-invalidation cascade
    /// was being fixed: the box is a descendant of the node that read the state, so nothing about that
    /// cascade may be what re-measures it — its own modifier changed, and that has to carry the change
    /// down. (Before the fix the UI test timed out on a click that had landed, because the content kept
    /// its first frame's text.)
    #[test]
    fn a_cap_change_in_the_composition_reaches_the_content_the_box_composes() {
        fn texts(composer: &Composer) -> Vec<String> {
            fn rec(nodes: &[crate::layout::node::LayoutNode], root: usize, out: &mut Vec<String>) {
                for el in nodes[root].modifier.elements() {
                    if let crate::modifier::ModifierElement::TextContent { content, .. } = el {
                        out.push(content.clone());
                    }
                }
                for &c in &nodes[root].children {
                    rec(nodes, c, out);
                }
            }
            let mut out = Vec::new();
            if let Some(root) = composer.arena.root {
                rec(composer.arena_nodes(), root, &mut out);
            }
            out
        }

        let cap = crate::runtime::state::State::new(200.0f32);
        let setter = cap.clone();
        let build = || {
            let cap = cap.clone();
            move |ctx: &mut ComposeCtx| {
                crate::layout::Column::new().build(ctx, |ctx| {
                    BoxWithConstraints::new()
                        .modifier(Modifier::new().max_width(cap.get()))
                        .build(ctx, |ctx, scope| {
                            crate::components::Text::new(format!("BWC max {}", scope.max_width() as i32))
                                .build(ctx);
                        });
                });
            }
        };

        let mut composer = Composer::new();
        composer.compose(build());
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(texts(&composer), ["BWC max 200"], "frame 1: the box caps the content");

        setter.set(120.0);
        composer.compose(build());
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(
            texts(&composer),
            ["BWC max 120"],
            "the narrowed cap reached the subcomposed content on the frame the state changed"
        );

        setter.set(200.0);
        composer.compose(build());
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(texts(&composer), ["BWC max 200"], "and back — both directions");
    }

    // NOTE: the policy's own half (`subcompose()` measured without a composed tree around it) is
    // covered by `ui::subcompose_probe`'s direct tests. It cannot be tested here through
    // `measure_node` alone, because `subcompose()` requires the composer's layout pass to be armed
    // (`LayoutHostGuard`) — which is exactly the contract the facility documents.
}
