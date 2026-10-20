//! The fling strategy: the motion between a release and the moment a scroll comes to rest.
//!
//! Compose models this as `TargetedFlingBehavior` (`foundation/gestures/TargetedFlingBehavior.kt`),
//! whose `performFling` is a SUSPENDING function that drives the scroll itself — it calls
//! `ScrollScope.scrollBy` every frame, and returns the velocity it could not use so nested scroll has
//! something to pass on. winia's animation engine is push-based instead: a motion is registered
//! against a `State<f32>` and the frame clock ticks it, so a strategy can DESCRIBE a motion but cannot
//! run one. This module is that description: [`FlingBehavior::plan`] answers "which motion, to
//! where", and [`ScrollState::fling_with_boundary`] executes it.
//!
//! What that costs: a strategy cannot invent a per-frame curve the engine does not have (a piecewise
//! animation, a motion that reads the layout mid-flight). What it keeps: the two things a caller
//! actually reaches for — where a fling lands, and what it is launched with — plus the ability to
//! fall back to free momentum instead of snapping.
//!
//! [`ScrollState::fling_with_boundary`]: crate::modifier::ScrollState::fling_with_boundary

use crate::animation::{DecaySpec, SpringSpec};

/// The friction every fling in this tree decays with: the scroll state, `lazy_column.rs` and
/// `top_app_bar.rs` all pass 4.2 to `exponential_decay`. The built-in strategy estimates "how many
/// pages would this flick carry" from the same number, so the estimate and the motion cannot disagree.
pub(crate) const FLING_FRICTION: f32 = 4.2;

/// Everything a fling strategy is allowed to look at.
///
/// `#[non_exhaustive]`: the framework may hand a strategy more context later, and that must not be a
/// breaking change for every implementation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct FlingContext {
    /// Where the container is right now, in scroll px.
    pub offset: f32,
    /// The release velocity, px/s (signed: negative is towards the start).
    pub velocity: f32,
    /// How far the container may scroll (`content - viewport`), or `f32::MAX` while the layout has
    /// not measured it yet. A plan is clamped to this, so a strategy may overshoot deliberately.
    pub limit: f32,
    /// How the container pages, or `None` for one that does not snap at all.
    pub snap: Option<SnapInfo>,
    /// The density the container composed under, for a strategy that thinks in dp.
    pub density: f32,
}

/// The settle spring a snapping container runs unless a caller overrides it: material3 hands
/// `snapFlingBehavior` `MotionSchemeKeyTokens.DefaultEffects` (`DatePicker.kt:744`), which resolves
/// through `MotionScheme.kt:276` → `defaultEffectsSpec()` (`:152-156`) to
/// `spring(dampingRatio = SpringDefaultEffectsDamping, stiffness = SpringDefaultEffectsStiffness)`,
/// and `StandardMotionTokens.kt:22-23` puts those at **1.0 and 1600.0** (the expressive scheme is the
/// same).
pub(crate) fn default_snap_settle() -> SpringSpec {
    SpringSpec {
        stiffness: SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
        ..SpringSpec::default()
    }
}

/// The paging configuration: the record a snapping container's measure writes back and a fling
/// strategy reads. It is ONE type for both sides — a strategy cannot decide with numbers the
/// container never wrote, and a new knob cannot be added on one side only.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SnapInfo {
    /// Distance between two snap positions — the page size plus its spacing.
    pub step: f32,
    /// The scroll position of snap position 0.
    pub anchor: f32,
    /// The fling speed at or above which the velocity picks the page (`MinFlingVelocityDp`, 400 dp/s).
    pub min_fling_velocity: f32,
    /// How far into a page a slow release has to be before it carries on (`snapPositionalThreshold`).
    pub positional_threshold: f32,
    /// The spring the settle runs on (`snapAnimationSpec`).
    pub settle: SpringSpec,
    /// How many pages one fling may carry (`PagerSnapDistance.atMost`).
    pub max_pages_per_fling: usize,
}

impl Default for SnapInfo {
    fn default() -> Self {
        Self {
            step: 1.0,
            anchor: 0.0,
            min_fling_velocity: 400.0,
            positional_threshold: 0.5,
            settle: default_snap_settle(),
            max_pages_per_fling: 1,
        }
    }
}

impl SnapInfo {
    /// The settle as the animation engine wants it.
    pub fn settle_spec(&self) -> crate::animation::AnimationSpec {
        crate::animation::AnimationSpec::Spring(self.settle.clone())
    }
}

/// The motion a fling should run.
///
/// `#[non_exhaustive]`: a plan variant may be added later, so a strategy's `match` needs a catch-all
/// arm (delegating ones usually do) and the crate keeps the freedom to grow the vocabulary.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FlingPlan {
    /// One spring, launched with the release velocity, landing on `target` (scroll px).
    Spring { target: f32, spec: SpringSpec },
    /// Free momentum: a decay that the container's own limits stop.
    Decay { spec: DecaySpec },
    /// Nothing worth animating (a release that moved nothing and carries no momentum).
    None,
}

impl FlingPlan {
    /// A settle on `target` with `spec`.
    pub fn spring(target: f32, spec: SpringSpec) -> Self {
        Self::Spring { target, spec }
    }

    /// Free momentum with `spec`.
    pub fn decay(spec: DecaySpec) -> Self {
        Self::Decay { spec }
    }
}

/// Decides what a release turns into.
///
/// The strategy is asked once per fling, from the gesture path, on the UI thread. It must be cheap
/// and side-effect free: the plan it returns is executed by the frame clock, and the same context may
/// be asked again by a later fling.
pub trait FlingBehavior: std::fmt::Debug + Send + Sync {
    /// The motion this release should run.
    fn plan(&self, fling: FlingContext) -> FlingPlan;
}

/// The built-in strategy, and the one a container uses unless it is given another: a PAGED container
/// settles on a page boundary (one page, or up to `max_pages_per_fling` of them for a hard flick),
/// and anything else keeps its momentum as a decay.
///
/// This is also what a decorator delegates to when it only wants to change one decision — call
/// `SnapFlingBehavior.plan(fling.clone())`, then adjust or replace the result.
#[derive(Clone, Copy, Debug, Default)]
pub struct SnapFlingBehavior;

impl FlingBehavior for SnapFlingBehavior {
    fn plan(&self, fling: FlingContext) -> FlingPlan {
        let Some(snap) = fling.snap.filter(|snap| snap.step.is_finite() && snap.step > 0.0) else {
            return FlingPlan::Decay {
                spec: crate::animation::exponential_decay(FLING_FRICTION),
            };
        };
        FlingPlan::Spring {
            target: snap_target(fling.offset, fling.velocity, &snap, fling.limit),
            spec: snap.settle.clone(),
        }
    }
}

/// Where a snapping fling should land, from where it started and how fast it was let go.
///
/// Compose's `calculateSnapOffset` for a list whose items are all one `step` wide
/// (`LazyListSnapLayoutInfoProvider.kt:66-100`) plus `calculateFinalOffset`
/// (`SnapFlingBehavior.kt:436-467`) and the pager's `calculateFinalSnappingBound`
/// (`PagerSnapLayoutInfoProvider.kt:231-305`): a release under the fling floor settles on the nearer
/// boundary unless it has crossed the positional threshold, a release above it carries the gesture.
///
/// **A release carries at most `max_pages_per_fling` pages, one by default.** That ceiling IS the feel
/// of a pager: clamping only to the list's end instead lets a hard flick bank thousands of pixels of
/// decay first, and the direction ends up decided by where the decay ran out rather than by the
/// gesture. How many pages it does carry is estimated from the decay's own asymptote — v0/friction,
/// rounded to whole pages — never fewer than the one page it was thrown toward.
pub(crate) fn snap_target(current: f32, velocity: f32, snap: &SnapInfo, limit: f32) -> f32 {
    let step = snap.step;
    // The snap positions are `k * step + anchor`: with a fixed page that does not fill the viewport,
    // `anchor` is where the first one sits. A page that fills the viewport has anchor 0.
    let position = (current - snap.anchor).max(0.0);
    let lower = (position / step).floor() * step + snap.anchor;
    let upper = (position / step).ceil() * step + snap.anchor;
    // How far the list already sits into the page it is between, and whether it rests exactly on a
    // boundary (which is not "past the threshold", whatever the threshold is - 0.0 would otherwise
    // turn a page from a release that moved nothing).
    let into_page = (position / step).fract();
    let at_rest = (position - (lower - snap.anchor)).abs() < 0.5;

    let picked = if velocity.abs() < snap.min_fling_velocity {
        if !at_rest && into_page >= snap.positional_threshold {
            upper
        } else {
            lower
        }
    } else {
        let start = lower - snap.anchor;
        let carried = (crate::animation::exponential_decay(FLING_FRICTION)
            .target_value(velocity.abs())
            / step)
            .round()
            .max(1.0);
        let pages = carried.min(snap.max_pages_per_fling.max(1) as f32) * step;
        let target = if velocity > 0.0 { start + pages } else { (start - pages).max(0.0) };
        target + snap.anchor
    };
    picked.clamp(0.0, limit.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(step: f32) -> SnapInfo {
        SnapInfo {
            step,
            anchor: 0.0,
            min_fling_velocity: 400.0,
            positional_threshold: 0.5,
            settle: default_snap_settle(),
            max_pages_per_fling: 1,
        }
    }

    fn ctx(offset: f32, velocity: f32, snap: Option<SnapInfo>) -> FlingContext {
        FlingContext { offset, velocity, limit: 2000.0, snap, density: 1.0 }
    }

    /// Without a snap spec the built-in keeps the old momentum: a decay, at the tree's friction.
    #[test]
    fn a_plain_container_plans_a_decay() {
        match SnapFlingBehavior.plan(ctx(0.0, 900.0, None)) {
            FlingPlan::Decay { spec } => {
                assert_eq!(spec.friction, FLING_FRICTION);
                assert!((spec.target_value(900.0) - 900.0 / FLING_FRICTION).abs() < 1e-3);
            }
            other => panic!("a container that does not snap decays, got {other:?}"),
        }
    }

    /// A paged container plans a spring landing on a boundary, with the settle it was given.
    #[test]
    fn a_paged_container_plans_a_spring_on_a_boundary() {
        match SnapFlingBehavior.plan(ctx(100.0, 9000.0, Some(snap(200.0)))) {
            FlingPlan::Spring { target, spec } => {
                assert_eq!(target, 200.0, "a hard forward flick carries one page");
                assert_eq!(spec.stiffness, 1600.0, "and uses the spec's settle spring");
            }
            other => panic!("a paged container settles, got {other:?}"),
        }
    }

    /// The built-in clamps its own target to the container's limit; a CUSTOM strategy may overshoot,
    /// because the executor clamps whatever a plan asks for.
    #[test]
    fn the_builtin_clamps_its_target_and_a_plan_may_overshoot() {
        let mut fling = ctx(0.0, 100.0, Some(snap(200.0)));
        fling.limit = 150.0;
        match SnapFlingBehavior.plan(fling) {
            FlingPlan::Spring { target, .. } => {
                assert!(target <= 150.0, "the built-in clamps itself, got {target}")
            }
            other => panic!("expected a spring, got {other:?}"),
        }

        let plan = FlingPlan::spring(10_000.0, SpringSpec::default());
        assert_eq!(
            plan,
            FlingPlan::Spring { target: 10_000.0, spec: SpringSpec::default() }
        );
    }
}
