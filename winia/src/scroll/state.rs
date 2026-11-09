use crate::fling::FlingBehavior as _;
#[cfg(test)]
use crate::fling::SnapInfo;

#[derive(Debug, Clone)]
pub struct ScrollState {
    /// The current offset.
    pub offset: crate::runtime::state::State<f32>,
    /// Whether a scroll is in progress.
    pub is_scroll_in_progress: crate::runtime::state::State<bool>,
    /// How far a fling may travel: written back by the layout as `content height - viewport height`, or
    /// `f32::MAX` while that is still unknown.
    ///
    /// A REAL `0` therefore means the content fits and the container cannot move at all. It used to be
    /// read as "unknown" too (`if limit > 0.0 { limit } else { f32::MAX }`), so a container whose content
    /// exactly fitted still flinged: a drag was correctly clamped by the node's own `content - viewport`,
    /// but the momentum after the release was not (measured on a three-item menu: a drag that moved
    /// nothing left the offset at 74).
    pub(crate) fling_limit: crate::runtime::state::State<f32>,
    /// A scroll-activity pulse (P1-3: what lights the scrollbar at a boundary - the offset stops
    /// changing there, so change detection cannot see the wheel. Dispatch bumps this counter for a
    /// wheel event that HIT but consumed nothing, and the scrollbar lights its fade on a change. u64
    /// and monotonic, so every set differs and every set notifies - no `PartialEq` dedup to worry
    /// about).
    pub(crate) scroll_pulse: crate::runtime::state::State<u64>,
    /// The overscroll effect this state resolved for an ambient factory, kept per factory identity so
    /// a rebuild hands the scroll container the SAME effect (a fresh one would drop an in-flight pull).
    pub(crate) overscroll_cache:
        std::sync::Arc<std::sync::Mutex<Vec<(u64, crate::overscroll::OverscrollEffect)>>>,
    /// The offset `last_scrolled_forward` saw last, so it can answer "which way" without a write hook.
    pub(crate) last_read_offset: std::sync::Arc<std::sync::Mutex<f32>>,
    /// The snapping configuration, or `None` for a list that does not snap. Written back by a PAGED
    /// list's measure (`LazyList::snap_paging`) and read by [`ScrollState::fling_with_boundary`].
    pub(crate) snap: crate::runtime::state::Backchannel<Option<crate::fling::SnapInfo>>,
    /// Where a page step is currently heading, if one is in flight. A second notch has to step from
    /// the page the first one targeted: reading the live offset mid-spring lands inside the same page
    /// and the notch is lost (four fast notches moved three pages).
    pub(crate) step_goal: std::sync::Arc<std::sync::Mutex<Option<f32>>>,
    /// The strategy this container flings with, or `None` for the built-in
    /// [`SnapFlingBehavior`](crate::fling::SnapFlingBehavior): a paged container settles on a page,
    /// anything else decays. A component assembles this from its own builder parameter; a caller
    /// holding a bare `ScrollState` can set it directly.
    pub fling_behavior: Option<std::sync::Arc<dyn crate::fling::FlingBehavior>>,
}

/// Whether a spring spec can produce a finite motion at all. A NaN anywhere in it poisons the value
/// the same way a NaN target does, and the scroll offset is the one state a container cannot recover
/// from - so a plan is checked before it is executed, not trusted.
fn spring_is_finite(spec: &crate::animation::SpringSpec) -> bool {
    spec.stiffness.is_finite()
        && spec.damping_ratio.is_finite()
        && spec.mass.is_finite()
        && spec.threshold.is_finite()
}

/// The landing position of a snapping fling — the math lives in [`crate::fling::snap_target`], which is
/// also what `SnapFlingBehavior` plans with, so a test here and a running pager cannot disagree. This
/// wrapper only exists to keep the unit tests below working against a `SnapInfo`.
#[cfg(test)]
pub(crate) fn snap_target(current: f32, velocity: f32, snap: &SnapInfo, limit: f32) -> f32 {
    crate::fling::snap_target(current, velocity, snap, limit)
}

impl ScrollState {
    pub fn new() -> Self {
        ScrollState {
            offset: crate::runtime::state::State::new(0.0),
            is_scroll_in_progress: crate::runtime::state::State::new(false),
            fling_limit: crate::runtime::state::State::new(f32::MAX),
            scroll_pulse: crate::runtime::state::State::new(0),
            overscroll_cache: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            last_read_offset: std::sync::Arc::new(std::sync::Mutex::new(0.0)),
            snap: crate::runtime::state::Backchannel::new(None),
            step_goal: std::sync::Arc::new(std::sync::Mutex::new(None)),
            fling_behavior: None,
        }
    }

    /// Which way the offset moved since the last time this was asked: `true` = toward the end.
    ///
    /// Compose publishes `ScrollableState.lastScrolledForward/Backward` and updates them on every
    /// scroll; winia's offset is written straight by the delta path and by the fling's motion, so
    /// there is no single write to hook. This SAMPLES instead: read it once per frame and it answers
    /// "did we move toward the end since the previous frame", which is what a bar that hides on scroll
    /// direction actually needs. A caller that never reads it gets no cost beyond one float.
    pub fn last_scrolled_forward(&self) -> bool {
        let current = self.offset.peek();
        let mut last = self
            .last_read_offset
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let forward = current > *last;
        *last = current;
        forward
    }

    /// How far this container can scroll, as the layout last wrote it back: `content - viewport`,
    /// `0` when the content fits, and `f32::MAX` until the first measure has measured it.
    ///
    /// Compose's `ScrollState.maxValue` (`foundation/gestures/ScrollableState.kt`); the limit used to
    /// be reachable only through the private `fling_limit`, so a caller driving a list by hand had to
    /// remember the last "content height" itself.
    ///
    /// It is a PEEK, not a subscription: the measure writes the limit back through a `Backchannel`
    /// (a State write during measure would notify in the middle of layout), so reading it alone does
    /// not re-render the reader. A readout therefore reads the offset too - the offset is a `State`
    /// and changes whenever the limit stops mattering - which is what `scroll_demo` does.
    pub fn max_value(&self) -> f32 {
        let limit = self.fling_limit.peek();
        if limit.is_finite() && limit >= 0.0 {
            limit
        } else {
            f32::MAX
        }
    }

    /// The limit as a SUBSCRIBABLE value: reading it in composition re-runs the reader when the
    /// container measures a new limit. `max_value()` peeks on purpose (logic must not subscribe),
    /// but a readout built on the peek was frozen at the first frame - measured: a demo card that
    /// read `max_value()` showed the `f32::MAX` sentinel until its own state happened to change.
    pub fn max_value_subscribed(&self) -> f32 {
        let limit = self.fling_limit.get();
        if limit.is_finite() && limit >= 0.0 {
            limit
        } else {
            f32::MAX
        }
    }

    /// Whether anything is left to scroll toward the end / toward the start.
    pub fn can_scroll_forward(&self) -> bool {
        self.offset.peek() < self.max_value() - 0.5
    }
    pub fn can_scroll_backward(&self) -> bool {
        self.offset.peek() > 0.5
    }

    /// Scroll by `delta` pixels (positive = toward the end, the sign Compose uses) and return how
    /// much was actually consumed — the movement is clamped to `[0, max_value]`, so a caller that
    /// wants to know whether it hit an end gets the answer in the return value.
    ///
    /// Compose's `ScrollableState.scrollBy` (`ScrollableState.kt:182`). Its sibling
    /// `dispatchRawDelta` is deliberately NOT mirrored: that one exists to bypass the nested-scroll
    /// dispatcher while still clamping, and this state has no dispatcher — the chain lives in
    /// `app::scroll`. A second name for the same function would only invite a caller to pick the
    /// wrong one.
    ///
    /// Unlike `scroll_to`, this CANCELS a fling or a page step in flight: the caller is taking over.
    pub fn scroll_by(&self, delta: f32) -> f32 {
        if !delta.is_finite() || delta == 0.0 {
            return 0.0;
        }
        self.cancel_fling();
        let current = self.offset.peek();
        let next = (current + delta).clamp(0.0, self.max_value());
        if next == current {
            return 0.0;
        }
        self.offset.set(next);
        next - current
    }

    /// A state whose offset is an EXTERNALLY owned handle — what the saveable constructor needs,
    /// and what a caller driving the offset from outside (a shared page scroll) already does by hand.
    pub fn with_offset(offset: crate::runtime::state::State<f32>) -> Self {
        Self {
            offset,
            ..Self::new()
        }
    }

    /// A scroll state whose position survives a saveable restore: window recreate, a navigation
    /// push/pop, anything that hands a `SaveableStateRegistry` across compositions.
    ///
    /// Compose's `rememberScrollState` is saveable for the same reason - a list that jumps back to the
    /// top after a back navigation is the bug this exists to prevent. The saved value is the offset
    /// alone: for a plain container the content is the caller's composition, so the offset is the whole
    /// state (a LAZY list needs its first visible item too, which is not saved here yet).
    #[crate::composable]
    pub fn remember_saveable(ctx: &mut crate::runtime::composer::ComposeCtx, key: &str) -> Self {
        let offset = crate::runtime::saveable::remember_saveable::<f32>(ctx, key, || 0.0);
        ctx.remember(move || Self::with_offset(offset)).get()
    }

    /// The effect this state uses for an ambient factory: created on first use and reused after that,
    /// so a rebuild does not hand the container a fresh effect (which would drop a pull in flight).
    pub(crate) fn overscroll_for(
        &self,
        factory_id: u64,
        create: impl FnOnce() -> crate::overscroll::OverscrollEffect,
    ) -> crate::overscroll::OverscrollEffect {
        if let Some(cached) = self.cached_overscroll(factory_id) {
            return cached;
        }
        // Build OUTSIDE the lock. A factory is user code (and `remember_overscroll_factory`)
        // lets it be anything), so running it under the cache mutex would deadlock the day it
        // touched this state - and it costs nothing to keep user code out of a lock.
        let fresh = create();
        let mut cache = self
            .overscroll_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some((_, effect)) = cache.iter().find(|(id, _)| *id == factory_id) {
            return effect.clone();
        }
        // A small cap, not an unbounded log: a state that met several factories (a theme switch, a
        // test) should not keep every effect it ever built. Four is more than any real subtree uses.
        const MAX_CACHED_FACTORIES: usize = 4;
        if cache.len() >= MAX_CACHED_FACTORIES {
            cache.remove(0);
        }
        cache.push((factory_id, fresh.clone()));
        fresh
    }

    fn cached_overscroll(&self, factory_id: u64) -> Option<crate::overscroll::OverscrollEffect> {
        let cache = self
            .overscroll_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache
            .iter()
            .find(|(id, _)| *id == factory_id)
            .map(|(_, effect)| effect.clone())
    }

    /// Jump to `value` immediately.
    pub fn scroll_to(&self, value: f32, max_offset: f32) {
        // A non-finite target would poison the offset (NaN compares false against every limit, so
        // nothing clamps it out later and the render translates by NaN).
        if !value.is_finite() {
            return;
        }
        // Compose's `scrollTo` runs through `scrollableState.scroll`, which cancels a fling; without
        // this the decay kept writing its own trajectory and the jump was overwritten next frame
        // (measured: `scroll_to(0)` during a fling left 260.26 on the next frame).
        self.cancel_fling();
        self.offset.set(value.clamp(0.0, max_offset.max(0.0)));
    }

    /// Animate to `value` (Compose's `animateScrollTo`): a spring drives the offset, and the target is
    /// clamped to `[0, max_offset]`. `is_scroll_in_progress` is true while it runs and false once the
    /// animation finishes.
    pub fn animate_scroll_to(
        &self,
        value: f32,
        max_offset: f32,
        spec: crate::animation::AnimationSpec,
    ) {
        if !value.is_finite() {
            return;
        }
        let target = value.clamp(0.0, max_offset.max(0.0));
        if (self.offset.peek() - target).abs() < 0.5 {
            return;
        }
        self.is_scroll_in_progress.set(true);
        let off = self.offset.clone();
        let done_flag = self.is_scroll_in_progress.clone();
        crate::animation::push_animatable_with_done(off, target, spec, move || {
            let _ = done_flag.set(false);
        });
    }

    /// A fling (Compose's `flingBehavior`): exponential decay from `velocity` (px/s) that stops as soon
    /// as it hits the scroll limit (written back during layout - the exact value for a LazyColumn,
    /// computed by `measure_node` for a plain container). A manual scroll (wheel or drag) cancels a
    /// running fling (`apply_scroll_delta` does the cancel). `is_scroll_in_progress` is true while it
    /// runs and false once it finishes.
    pub fn fling(&self, velocity: f32) {
        self.fling_with_boundary(velocity, |_| {});
    }

    /// Whether this container settles its flings on a page boundary — the question a CALLER has to ask
    /// before it decides whether a release is worth starting a fling for at all. Same filter as the
    /// snap branch of [`Self::fling_with_boundary`], so the two can never disagree about what "snapping"
    /// means.
    pub(crate) fn snaps(&self) -> bool {
        self.snap
            .peek()
            .is_some_and(|s| s.step.is_finite() && s.step > 0.0)
    }

    /// Whether a release is worth asking a strategy about: the container snaps, or it carries a
    /// [`FlingBehavior`](crate::fling::FlingBehavior) of its own.
    ///
    /// The gesture path's 50 px/s floor is a DECAY floor (it stops the tail of a drag from banking
    /// momentum), and both of these settle a release that carries no momentum of its own - so the floor
    /// is skipped for them. Without this the custom strategy on a list with no snap spec would never
    /// see a slow release at all, which is the one release a positional snap exists for.
    pub(crate) fn wants_fling(&self) -> bool {
        self.snaps() || self.fling_behavior.is_some()
    }

    /// Move ONE snap position forward or back, and animate there.
    ///
    /// A wheel notch on a paged container should move a page, not a few pixels: a viewport-sized page
    /// is far wider than one wheel delta, so the ordinary "settle on the nearest position" returns the
    /// list to where it started and the wheel reads as broken. Compose does not special-case this (a
    /// wheel on a pager barely moves it); winia does, for paged containers only - `snap` is `Some` -
    /// so a pager and a date picker move one page per notch while a plain list keeps scrolling
    /// smoothly.
    /// Returns whether a step was started. A caller that gets `false` falls back to the pixel scroll,
    /// so a container whose snap spec has not been resolved yet is never worse off than before.
    pub(crate) fn snap_page(&self, forward: bool, reverse: bool) -> bool {
        let Some(spec) = self
            .snap
            .peek()
            .filter(|s| s.step.is_finite() && s.step > 0.0)
        else {
            return false;
        };
        let raw_limit = self.fling_limit.peek();
        let limit = if raw_limit.is_finite() && raw_limit >= 0.0 {
            raw_limit
        } else {
            f32::MAX
        };
        // `reverseLayout` mirrors the scroll direction (the pixel and fling paths do the same), so a
        // "forward" notch on a reversed container steps back.
        let forward = forward != reverse;
        // Step from where a step in flight is heading, not from the live offset: a second notch
        // arriving mid-spring otherwise reads a position inside the SAME page and re-targets it.
        let mut goal = self.step_goal.lock().unwrap_or_else(|e| e.into_inner());
        // A goal means something only while the spring that set it is still running. A drag, a fling or
        // a programmatic jump CANCELS that animation without firing its completion callback, and an
        // abandoned target would otherwise survive: the next notch would step from a position the list
        // never reached, skipping a page or refusing one at the end.
        if goal.is_some() && !crate::animation::has_animation_for_state(self.offset.state_id()) {
            *goal = None;
        }
        let base = goal.unwrap_or_else(|| self.offset.peek());
        let position = (base - spec.anchor) / spec.step;
        let index = if forward {
            position.floor() + 1.0
        } else {
            (position.ceil() - 1.0).max(0.0)
        };
        let target = spec.anchor + index * spec.step;
        // Refuse rather than clamp: `limit` is `content - viewport`, which with a trailing
        // `contentPadding` is PAST the last snap position, so clamping would scroll into the padding and
        // the settle would drag it back (Compose guards this explicitly:
        // PagerSnapLayoutInfoProvider.kt:181-187 zeroes both bounds when the list cannot scroll forward).
        if limit < f32::MAX && target > limit + 0.5 {
            return false;
        }
        let target = target.clamp(0.0, limit);
        if (target - base).abs() < 0.5 {
            return false;
        }
        *goal = Some(target);
        drop(goal);
        self.is_scroll_in_progress.set(true);
        let done = self.is_scroll_in_progress.clone();
        let goal_cell = std::sync::Arc::clone(&self.step_goal);
        crate::animation::push_animatable_with_velocity_and_done(
            self.offset.clone(),
            target,
            spec.settle_spec(),
            0.0,
            move || {
                let mut goal = goal_cell.lock().unwrap_or_else(|e| e.into_inner());
                if goal.is_some_and(|g| (g - target).abs() < 0.5) {
                    *goal = None;
                }
                done.set(false);
            },
        );
        true
    }

    /// Start a fling and hand the caller the instantaneous remaining velocity when the child hits a
    /// boundary.
    pub fn fling_with_boundary(
        &self,
        velocity: f32,
        on_boundary: impl FnOnce(f32) + Send + 'static,
    ) {
        if !velocity.is_finite() {
            return;
        }
        // The strategy decides what this release turns into. `None` means the built-in one, which is
        // the behaviour this method had before the trait existed: a paged container settles on a page
        // boundary with ONE spring (launched with the gesture's velocity, so the momentum is carried
        // into the settle - Compose zeroes `calculateApproachOffset` for material3, `DatePicker.kt:749-753`,
        // and `tryApproach` then animates nothing, `SnapFlingBehavior.kt:165-176`), and anything else
        // keeps its momentum as a decay.
        let context = crate::fling::FlingContext {
            offset: self.offset.peek(),
            velocity,
            limit: self.fling_limit.peek(),
            snap: self.snap.peek(),
            density: crate::runtime::density::current_density().density,
        };
        let plan = match &self.fling_behavior {
            Some(behavior) => behavior.plan(context),
            None => crate::fling::SnapFlingBehavior.plan(context),
        };
        // The 1 px/s floor is a DECAY floor: it stops a scrollbar's trailing jitter from starting a
        // decay nothing asked for. A plan that settles somewhere has no decay to suppress, so the floor
        // is applied to the ANSWER, not to the container — a custom strategy on a list without a snap
        // spec still sees its slow release. (The gesture path's 50 px/s floor is keyed on
        // `wants_fling` for the same reason.) Compose has no floor at all: `Scrollable.kt:857-881`
        // calls `performFling` on every release.
        if velocity.abs() < 1.0 && matches!(&plan, crate::fling::FlingPlan::Decay { .. }) {
            return;
        }
        // A gesture-driven fling supersedes any page step in flight: whatever that step was heading for
        // is abandoned, and the next notch has to step from wherever the fling leaves the list.
        *self.step_goal.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.is_scroll_in_progress.set(true);
        let off = self.offset.clone();
        let limit = self.fling_limit.clone();
        let done_flag = self.is_scroll_in_progress.clone();

        match plan {
            crate::fling::FlingPlan::Spring { target, spec } => {
                // A strategy may aim past the content on purpose (a "bounce past the end" plan); the
                // container's own limit is still what the value is allowed to reach.
                let limit_now = limit.peek().max(0.0);
                let requested = target;
                let target = target.clamp(0.0, limit_now);
                if !target.is_finite() || !spring_is_finite(&spec) {
                    // A NaN plan must not reach the offset: the engine's overrun guard writes the
                    // spring's target verbatim after five seconds (animation.rs), and a NaN offset is
                    // something a container never recovers from.
                    self.is_scroll_in_progress.set(false);
                    return;
                }
                // A new fling supersedes whatever is still running. Without this, a second release
                // whose plan lands on the SAME target hits the engine's same-target dedup, which
                // reports `done` immediately and clears `is_scroll_in_progress` while the first spring
                // is still moving.
                crate::animation::cancel_animation(&self.offset);
                // A plan that asked to go PAST the container's limit cannot use that travel. A spring
                // has no leftover velocity to hand on the way a decay does, but the post-fling chain
                // still has to learn that the child reached its end - so the boundary is reported when
                // the spring lands. (A plan that lands inside the limit reports nothing: the built-in
                // one clamps itself, so a pager's behaviour here is unchanged.)
                let clamped = (target - requested).abs() > 0.5;
                let settle = done_flag.clone();
                let mut boundary = Some(on_boundary);
                crate::animation::push_animatable_with_velocity_and_done(
                    off,
                    target,
                    crate::animation::AnimationSpec::Spring(spec),
                    velocity,
                    move || {
                        settle.set(false);
                        if clamped {
                            if let Some(report) = boundary.take() {
                                report(0.0);
                            }
                        }
                    },
                );
            }
            crate::fling::FlingPlan::Decay { spec } => {
                crate::animation::push_fling_with_boundary(
                    off.clone(),
                    velocity,
                    spec,
                    {
                        let limit = limit.clone();
                        move |o| {
                            // The limit IS the truth here: `f32::MAX` while the layout has not measured
                            // the content yet, `0` when the content fits and this container cannot move.
                            o.clamp(0.0, limit.peek())
                        }
                    },
                    on_boundary,
                    move || {
                        done_flag.set(false);
                    },
                );
            }
            crate::fling::FlingPlan::None => {
                // Nothing to run: a release the strategy declined. Clearing the flag here is what a
                // caller's "is this container scrolling" check would otherwise miss.
                self.is_scroll_in_progress.set(false);
            }
        }
    }

    /// Cancel an in-flight fling - called before manual input or a programmatic jump. A page step in
    /// flight is abandoned with it, so its goal does not leak into the next notch.
    pub fn cancel_fling(&self) {
        crate::animation::cancel_animation(&self.offset);
        *self.step_goal.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod scroll_by_tests {
    use super::ScrollState;

    /// The limit comes from the layout write-back, so the test writes it the same way.
    fn state(offset: f32, limit: f32) -> ScrollState {
        let s = ScrollState::new();
        s.offset.set(offset);
        s.fling_limit.set(limit);
        s
    }

    #[test]
    fn scroll_by_moves_and_reports_what_it_consumed() {
        let s = state(0.0, 300.0);
        assert_eq!(s.scroll_by(48.0), 48.0);
        assert_eq!(s.offset.peek(), 48.0);
        assert_eq!(s.scroll_by(-20.0), -20.0);
        assert_eq!(s.offset.peek(), 28.0);
        // Zero is a no-op, not a cancellation.
        assert_eq!(s.scroll_by(0.0), 0.0);
    }

    #[test]
    fn scroll_by_clamps_at_both_ends_and_says_so() {
        let s = state(290.0, 300.0);
        assert_eq!(
            s.scroll_by(50.0),
            10.0,
            "only the last 10 px were available"
        );
        assert_eq!(s.offset.peek(), 300.0);
        assert_eq!(s.scroll_by(50.0), 0.0, "at the end nothing moves");
        assert_eq!(s.scroll_by(-400.0), -300.0);
        assert_eq!(s.offset.peek(), 0.0);
    }

    #[test]
    fn can_scroll_follows_the_offset_and_the_limit() {
        // A container whose content fits: nowhere to go.
        let tight = state(0.0, 0.0);
        assert!(!tight.can_scroll_forward());
        assert!(!tight.can_scroll_backward());

        let s = state(0.0, 300.0);
        assert!(s.can_scroll_forward());
        assert!(!s.can_scroll_backward());
        s.offset.set(300.0);
        assert!(!s.can_scroll_forward());
        assert!(s.can_scroll_backward());
    }

    /// The point of the saveable constructor: a scroll position that a back navigation would reset.
    /// `snap_page` is what a pager's wheel notch runs, and until now only the wheel tests reached it.
    /// One notch = one page, and it REFUSES rather than clamps at the ends (clamping scrolls into a
    /// trailing content padding and the settle drags it back).
    #[test]
    fn a_page_step_moves_one_page_and_refuses_at_the_end() {
        let state = state(0.0, 400.0);
        state.snap.set(Some(crate::fling::SnapInfo {
            step: 100.0,
            anchor: 0.0,
            ..crate::fling::SnapInfo::default()
        }));
        assert!(state.snap_page(true, false), "a notch starts a step");
        assert_eq!(
            crate::animation::animation_target::<f32>(state.offset.state_id()),
            Some(100.0),
            "forward goes to the next page"
        );
        crate::animation::clear_all_animations();

        // At the last page there is nothing forward: the call refuses, so the caller falls back to a
        // pixel scroll (or lets an ancestor take the delta).
        state.offset.set(400.0);
        assert!(!state.snap_page(true, false), "no page past the end");
        assert!(state.snap_page(false, false), "but back is fine");
        assert_eq!(
            crate::animation::animation_target::<f32>(state.offset.state_id()),
            Some(300.0)
        );
        crate::animation::clear_all_animations();
    }

    /// A reversed container mirrors the direction: "forward" steps BACK.
    #[test]
    fn a_reversed_container_steps_the_other_way() {
        let state = state(200.0, 400.0);
        state.snap.set(Some(crate::fling::SnapInfo {
            step: 100.0,
            anchor: 0.0,
            ..crate::fling::SnapInfo::default()
        }));
        assert!(state.snap_page(true, true));
        assert_eq!(
            crate::animation::animation_target::<f32>(state.offset.state_id()),
            Some(100.0)
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_saved_scroll_position_comes_back_in_the_next_composition() {
        use std::collections::HashMap;
        let first = crate::runtime::saveable::SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen = None;
        crate::runtime::saveable::with_saveable_registry(first.clone(), || {
            composer.compose(|ctx| seen = Some(ScrollState::remember_saveable(ctx, "page")));
        });
        let scroll: ScrollState = seen.expect("composed");
        assert_eq!(scroll.offset.peek(), 0.0, "nothing to restore yet");
        scroll.offset.set(240.0);

        let saved = first.perform_save();
        assert_eq!(
            saved.get("page"),
            Some(&crate::runtime::saveable::SavedValue::F64(240.0))
        );

        let second = crate::runtime::saveable::SaveableStateRegistry::new(saved);
        let mut next = crate::runtime::composer::Composer::new();
        let mut restored = None;
        crate::runtime::saveable::with_saveable_registry(second, || {
            next.compose(|ctx| restored = Some(ScrollState::remember_saveable(ctx, "page")));
        });
        assert_eq!(
            restored.expect("composed").offset.peek(),
            240.0,
            "the list opens where it was"
        );
    }

    /// A restored offset can outlive its content: clamping happens during measure, the way Compose
    /// does it, so the container never opens on blank space.
    /// Compose's `scrollTo` runs through `scrollableState.scroll`, which cancels a fling. Without the
    /// cancel the decay kept writing its own trajectory over the jump (measured: 260.26 on the next
    /// frame after a `scroll_to(0)`).
    #[test]
    fn a_scroll_to_cancels_a_running_fling() {
        let (_guard, clock) = crate::animation::tests::serial_clock();
        crate::animation::clear_all_animations();
        let state = ScrollState::new();
        state.fling_limit.set(1000.0);
        state.fling(4000.0);
        clock.advance(std::time::Duration::from_millis(60));
        crate::animation::update_animations();
        assert!(state.offset.peek() > 0.0, "the fling is running");

        state.scroll_to(0.0, 1000.0);
        clock.advance(std::time::Duration::from_millis(16));
        crate::animation::update_animations();
        clock.advance(std::time::Duration::from_millis(16));
        crate::animation::update_animations();
        assert_eq!(
            state.offset.peek(),
            0.0,
            "the jump sticks: the fling was cancelled"
        );
        crate::animation::clear_all_animations();
    }

    /// A NaN would poison the offset: every limit comparison against it is false, so nothing clamps
    /// it out later and the render translates by NaN.
    #[test]
    fn a_non_finite_scroll_is_ignored() {
        let state = ScrollState::new();
        state.fling_limit.set(100.0);
        state.scroll_to(f32::NAN, 100.0);
        assert_eq!(state.offset.peek(), 0.0, "NaN scroll_to is a no-op");
        state.scroll_to(10.0, 100.0);
        assert_eq!(state.scroll_by(f32::NAN), 0.0, "NaN scroll_by is a no-op");
        assert_eq!(state.offset.peek(), 10.0, "and leaves the offset alone");
        state.animate_scroll_to(
            f32::NAN,
            100.0,
            crate::animation::AnimationSpec::Tween(Default::default()),
        );
        assert!(
            !crate::animation::has_animation_for_state(state.offset.state_id()),
            "no NaN animation starts"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_restored_offset_past_the_end_is_pulled_back() {
        use std::collections::HashMap;
        let first = crate::runtime::saveable::SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen = None;
        crate::runtime::saveable::with_saveable_registry(first.clone(), || {
            composer.compose(crate::compose!(|ctx| {
                seen = Some(ScrollState::remember_saveable(ctx, "page"));
            }));
        });
        seen.unwrap().offset.set(240.0);
        let saved = first.perform_save();

        let second = crate::runtime::saveable::SaveableStateRegistry::new(saved);
        let mut next = crate::runtime::composer::Composer::new();
        let mut restored = None;
        crate::runtime::saveable::with_saveable_registry(second, || {
            next.compose(crate::compose!(|ctx| {
                let scroll = ScrollState::remember_saveable(ctx, "page");
                restored = Some(scroll.clone());
                crate::layout::Column::new()
                    .modifier(
                        crate::modifier::Modifier::new()
                            .height(100.0)
                            .vertical_scroll(scroll),
                    )
                    .build(ctx, |ctx| {
                        crate::components::Text::new("only 60 tall")
                            .modifier(crate::modifier::Modifier::new().size(100.0, 60.0))
                            .build(ctx);
                    });
            }));
        });
        next.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let scroll = restored.expect("composed");
        assert_eq!(
            scroll.offset.peek(),
            0.0,
            "the content does not scroll, so the offset is 0"
        );
    }

    #[test]
    fn last_scrolled_forward_samples_the_direction() {
        let s = state(100.0, 300.0);
        s.offset.set(140.0);
        assert!(s.last_scrolled_forward(), "moved toward the end");
        assert!(
            !s.last_scrolled_forward(),
            "nothing moved since the last read"
        );
        s.offset.set(120.0);
        assert!(!s.last_scrolled_forward(), "moved back toward the start");
    }

    #[test]
    fn an_unmeasured_limit_is_open_ended_and_then_settles() {
        let s = ScrollState::new();
        assert_eq!(
            s.max_value(),
            f32::MAX,
            "until the layout measures, everything is reachable"
        );
        assert!(s.can_scroll_forward());
        s.fling_limit.set(120.0);
        assert_eq!(s.max_value(), 120.0);
    }
}

#[cfg(test)]
mod limit_subscription_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// `max_value()` peeks (a readout built on it was frozen at the first frame: it showed the
    /// `f32::MAX` sentinel until the reader's own state happened to change). `max_value_subscribed()`
    /// re-runs the reader when the container measures a limit.
    #[test]
    fn a_measured_limit_reaches_a_subscribed_reader() {
        let scroll = ScrollState::new();
        let s = scroll.clone();
        let seen = Rc::new(Cell::new(-1.0f32));
        let seen_in = seen.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let s = s.clone();
            let seen_in = seen_in.clone();
            composer.compose(crate::compose!(|ctx| {
                crate::layout::Column::new()
                    .modifier(
                        crate::modifier::Modifier::new()
                            .size(100.0, 100.0)
                            .vertical_scroll(s.clone()),
                    )
                    .build(ctx, |ctx| {
                        let value = s.max_value_subscribed();
                        seen_in.set(value);
                        crate::components::Text::new(format!("{value:?}")).build(ctx);
                        crate::layout::Column::new()
                            .modifier(crate::modifier::Modifier::new().size(100.0, 300.0))
                            .build(ctx, |_| {});
                    });
            }));
        };
        build(&mut composer);
        let first = seen.get();
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        build(&mut composer);
        let second = seen.get();
        assert_eq!(
            first,
            f32::MAX,
            "before the first measure the limit is the sentinel"
        );
        assert!(
            second.is_finite() && second > 0.0,
            "the measured limit must reach the reader, got {second}"
        );
    }
}
