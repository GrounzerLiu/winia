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
//! winia has no subcomposition, so the constraints travel on the framework's measure-to-compose
//! channel: the box's measure policy records the constraints it was measured with, and the content
//! reads them on its next run. The channel is a `Reactive<Constraints>` — a `Backchannel` does not
//! work here, because its write never moves the signal's revision and so never wakes the content
//! (measured: the component stayed on its first value forever). See `build` for the full reasoning.
//!
//! # Deliberate deviation: the constraints are one composition behind
//!
//! Compose's scope is current **within** the frame: a `SubcomposeLayout` measure-time subcomposition
//! runs with the constraints that measure pass just computed. winia composes first and measures
//! after, so the value the content reads is the one the **previous** measure wrote:
//!
//! - The **first** composition reads the initial value (unbounded — `min 0`, `max +∞`). A caller that
//!   must not take the unbounded branch on that run asks [`BoxWithConstraintsScope::is_measured`].
//! - After that, a change of constraints **does** re-run the content: the write goes through
//!   `State::set`, so the content's read is a composition dependency and the change wakes it. The
//!   write also dedups on equality, which is what keeps the cycle finite.
//! - The box does not poll: if neither its constraints nor anything else changes, it stays idle.
//!
//! This is the same trail the architecture already documents for writers that run after layout (see
//! the layout-override note in `app.rs` and `docs/shared-element-transition.md` §3.1). Closing it
//! needs a real lookahead/subcomposition pass, which is a framework-level change, not a component.
//!
//! # Deliberate difference: the scope speaks the layout coordinate system, not `Dp`
//!
//! winia HAS `Dp` (`crate::unit::Dp`, exported by the prelude, accepted by `Modifier::size` and
//! friends). What the scope hands back is the layout coordinate system — `f32` logical pixels, what
//! `Constraints` carries — because that is what the box was measured in, and mixing the two is the trap
//! `Dp::to_px`'s own docs warn about (it returns physical pixels). The four `*_dp()` accessors spell
//! the same numbers on the type for a caller that wants to feed a bound straight into a `Modifier`;
//! `constraints()` is exposed as the framework's `Constraints` so a custom `MeasurePolicy` can take it
//! unchanged.

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
        content: impl FnOnce(&mut ComposeCtx, BoxWithConstraintsScope),
    ) {
        // The channel: a `Reactive` state, NOT a `Backchannel`. The content has to be woken when the
        // constraints change, and a `Backchannel` write does not move the signal's revision at all
        // (`set_backchannel` only overwrites the value), so nothing would ever re-read it — measured
        // while writing this component: the tree kept printing "not measured yet (frame 1)" forever.
        // `State::set` dedups on `PartialEq`, so an unchanged constraint notifies nobody and the
        // measure-write → recompose → measure cycle terminates.
        let observed: State<Constraints> = ctx.remember(|| State::new(Constraints::UNBOUNDED)).get();
        let scope = BoxWithConstraintsScope::new(observed.get());
        let key = ctx.next_key();
        let policy = ConstraintsReporter {
            observed: observed.clone(),
            alignment: self.alignment,
        };
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                content(ctx, scope);
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for BoxWithConstraints {
    fn default() -> Self {
        Self::new()
    }
}

/// A `BoxLayout` that also records the constraints it was measured with.
///
/// The recording is a `Backchannel` write: it lands silently (no notify, no wake), which is what
/// keeps a per-frame write from turning into a per-frame recomposition. The measure itself is
/// delegated to [`BoxLayout`] so the layout semantics stay the same as a plain `Stack`.
#[derive(Debug)]
struct ConstraintsReporter {
    observed: State<Constraints>,
    alignment: Alignment,
}

impl MeasurePolicy for ConstraintsReporter {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // Record what this box was measured with, for the content's next composition. The write is
        // deduped by `State::set`, so it only notifies when the constraint actually moved.
        self.observed.set(constraints);
        BoxLayout::new().alignment(self.alignment).measure(nodes, policies, children, constraints)
    }

    fn place(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        children: &[usize],
        placements: &[Placement],
    ) {
        BoxLayout::new().alignment(self.alignment).place(nodes, children, placements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::node::MeasurePolicy;

    /// The scope's own semantics. `is_measured()` is the escape hatch the module docs promise for the
    /// first-frame case, so its boundary is pinned here: unbounded means "not measured yet".
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

        // The Dp accessors carry the same numbers: layout coordinates ARE logical pixels
        // (`Dp::to_logical` is the identity), which is why `to_px` must not be used here.
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

    /// The mechanism behind the component: the measure policy writes the constraints it was given,
    /// and the value is readable afterwards (through the same `Backchannel` the component uses). This
    /// is the half that has to hold for the content to ever see a real constraint, and it is checked
    /// by measuring the policy directly with a known constraint rather than by inspecting types.
    #[test]
    fn policy_records_the_incoming_constraints() {
        let observed: State<Constraints> = State::new(Constraints::UNBOUNDED);
        let policy = ConstraintsReporter { observed: observed.clone(), alignment: Alignment::Start };
        let mut nodes = Vec::new();
        let (_size, _placements) = policy.measure(
            &mut nodes,
            &[],
            &[],
            Constraints::new(10.0, 250.0, 20.0, 125.0),
        );
        assert_eq!(
            observed.peek(),
            Constraints::new(10.0, 250.0, 20.0, 125.0),
            "the policy must record exactly the constraints it was measured with"
        );
        MeasurePolicy::place(&policy, &mut nodes, &[], &[]);
    }

    /// End to end through the public component: the box composes, its content runs inside the same
    /// group, and the content closure receives a scope (the unmeasured value on the first run — the
    /// documented one-frame trail). The assertion is that the component builds and lays out at the
    /// size its modifier gives it, i.e. it is a `Stack` with a channel, not a different layout.
    #[test]
    fn box_with_constraints_lays_out_like_a_stack_and_hands_its_content_a_scope() {
        let mut composer = Composer::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        composer.compose(|ctx| {
            BoxWithConstraints::new()
                .modifier(Modifier::new().size(120.0, 60.0))
                .build(ctx, move |ctx, scope| {
                    seen_in.lock().unwrap().push((scope.min_width(), scope.max_width(), scope.is_measured()));
                    crate::ui::Text::new("inside").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let size = composer.arena_nodes()[root].measured_size;
        assert_eq!((size.width, size.height), (120.0, 60.0), "the box takes its modifier's size");
        let runs = seen.lock().unwrap().clone();
        assert!(!runs.is_empty(), "the content ran");
        // First run necessarily reads the backchannel's initial value.
        assert_eq!(runs[0].0, 0.0);
        assert!(!runs[0].2, "unmeasured on the first run — the documented trail");
    }
}
