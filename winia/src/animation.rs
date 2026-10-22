//! Animation system — mirrors Jetpack Compose's three-layer animation API
//!
//! ## Layers
//! - **High level**: `animate_*_as_state` — single-value animation, works out of the box
//! - **Mid level**: `update_transition` — several properties animated together
//! - **Low level**: `Animatable` (internal, `pub(crate)` — driven by the high-level API)
//!
//! ## Core
//! - `SpringSpec` (a physical spring) by default, `TweenSpec` (tween) optionally
//! - Integrated with `State<T>` → recomposition is triggered automatically
//! - Frame driving is updated in `AboutToWait` in `app.rs`

pub mod interpolator;
pub mod visibility;

pub use visibility::{
    ChannelValues, ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};

use crate::runtime::state::{State, StateId};
use crate::runtime::composer::ComposeCtx;
use timed::{compute_spring_displacement, spring_at_rest};
use std::time::{Duration, Instant};

#[allow(unused_imports)] // the file-scope `#[test]` functions below build interpolators in an Arc
use std::sync::Arc;
use std::sync::{Mutex, LazyLock};

/// The time every motion in this module reads.
///
/// Production always gets `Instant::now()`. A test can install a [`TestClock`], which freezes this
/// thread's time and advances it only when the test says so — that is what lets an animation test
/// assert a TRAJECTORY instead of sleeping and hoping the machine kept up. (`TimedAnimation` already
/// evaluates a spec at an arbitrary time; this does the same for a running `Motion`.)
fn now() -> Instant {
    #[cfg(test)]
    {
        if let Some(t) = TEST_CLOCK.with(|c| *c.borrow()) {
            return t;
        }
    }
    Instant::now()
}

#[cfg(test)]
thread_local! {
    /// The installed test clock, if any. Thread-local on purpose: the animation registries are
    /// process-global and the suite is serial, but a frozen clock must never leak into another test
    /// (or another thread's test) by accident.
    static TEST_CLOCK: std::cell::RefCell<Option<Instant>> = std::cell::RefCell::new(None);
}

/// A frozen clock for tests: install it, advance it, and every motion started or updated on this
/// thread follows that timeline. Dropping it restores real time — on panic too.
#[cfg(test)]
pub(crate) struct TestClock;

#[cfg(test)]
impl TestClock {
    pub(crate) fn install() -> Self {
        TEST_CLOCK.with(|c| *c.borrow_mut() = Some(Instant::now()));
        Self
    }

    pub(crate) fn advance(&self, delta: Duration) {
        TEST_CLOCK.with(|c| {
            let mut slot = c.borrow_mut();
            let t = slot
                .as_mut()
                .expect("advanced a TestClock that was not installed");
            *t += delta;
        });
    }
}

#[cfg(test)]
impl Drop for TestClock {
    fn drop(&mut self) {
        TEST_CLOCK.with(|c| *c.borrow_mut() = None);
    }
}

/// The velocity handed to a new animation when a retarget interrupts the old one, carried PER
/// COMPONENT.
///
/// Compose passes the running velocity straight into the replacement animation
/// (`AnimationState.velocity`, `animation-core/AnimationState.kt`), and `IntSize`/`Offset` have a
/// two-dimensional `VectorConverter`, so width and height each keep their own velocity: when one
/// axis moves fast and the other sits still, only the moving one carries momentum into the new
/// animation. Storing a single scalar instead flings the fastest axis's velocity into every axis,
/// which shows up as tens of pixels of phantom overshoot whenever the target moves every frame
/// (an `AnimatedSize` whose content is animating too). Scalar values have one component.
#[derive(Clone, Copy)]
pub struct RetargetVelocity {
    components: [f32; MAX_ANIMATION_COMPONENTS],
    len: usize,
}

impl RetargetVelocity {
    /// Nothing to inherit.
    pub const ZERO: Self = Self { components: [0.0; MAX_ANIMATION_COMPONENTS], len: 0 };

    /// One scalar component: `animate_*_as_state`, a fling hand-off, any one-dimensional motion.
    pub fn scalar(velocity: f32) -> Self {
        let mut components = [0.0; MAX_ANIMATION_COMPONENTS];
        components[0] = velocity;
        Self { components, len: 1 }
    }

    /// The velocity of component `i`. With fewer components than asked for (a scalar handing over to
    /// a vector) it falls back to component 0 — the old "only one velocity is available" behaviour.
    fn component(&self, i: usize) -> f32 {
        if self.len == 0 {
            0.0
        } else if i < self.len {
            self.components[i]
        } else {
            self.components[0]
        }
    }

    /// The component with the largest magnitude — what the scalar path and `last_velocity()` report.
    fn max_abs(&self) -> f32 {
        self.components[..self.len]
            .iter()
            .copied()
            .reduce(|a, b| if b.abs() > a.abs() { b } else { a })
            .unwrap_or(0.0)
    }
}

/// An animation instance — type-erased and kept in the global lists.
pub trait AnimationInstance: Send {
    fn update(&mut self) -> bool;
    fn state_id(&self) -> StateId;
    /// Type-safe exact target comparison (returns false across types).
    fn same_target(&self, target: &dyn std::any::Any) -> bool;
    /// Current velocity (px/s) — carried over when a retarget interrupts the motion (P2-9).
    fn last_velocity(&self) -> f32;
    /// The per-component velocity to inherit on a retarget; by default `last_velocity()` is the
    /// only component.
    fn retarget_velocity(&self) -> RetargetVelocity {
        RetargetVelocity::scalar(self.last_velocity())
    }
    /// The value this motion is heading for, for callers that ask (Compose's
    /// `Animatable.targetValue`); `None` for a motion with no target (a fling).
    fn target_value(&self) -> Option<&dyn std::any::Any> {
        None
    }
    /// Keep this motion's value inside `[min, max]`, or clear the range with `None` — Compose's
    /// `Animatable.updateBounds`. Applied at the write point for every spec: a value that would leave the
    /// range is written exactly at the bound and the motion ends there.
    fn set_value_bounds(&mut self, bounds: Option<(f32, f32)>) {}
    /// One line describing this motion, for [`active_animations_report`]. Compose has no equivalent —
    /// it is here because a value that is moving looks the same whoever moves it, and nothing else can
    /// answer "who is animating this".
    fn describe(&self) -> String {
        format!("state={:?} spec=?", self.state_id())
    }
}

static ACTIVE_ANIMATIONS: LazyLock<Mutex<Vec<Box<dyn AnimationInstance>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
/// Colour animations, kept apart from the f32 ones to avoid type erasure.
static ACTIVE_COLOR_ANIMATIONS: LazyLock<Mutex<Vec<Motion<crate::graphics::Color>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Drop all animations owned by a Composer. State IDs remain globally unique,
/// so this is safe even when another Composer owns a different animation.
pub fn clear_animations_for_states(state_ids: &[StateId]) {
    if state_ids.is_empty() {
        return;
    }
    ACTIVE_ANIMATIONS.lock().unwrap().retain(|anim| !state_ids.contains(&anim.state_id()));
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().retain(|anim| !state_ids.contains(&anim.state.state_id()));
}

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod repeated_tests;
#[cfg(test)]
mod lifecycle_tests;

mod spec;

pub use spec::*;

mod api;
mod timed;

pub use api::*;
pub use timed::*;

mod motion;

use motion::*;
use motion::{interpolate_keyframes, loop_factor};

mod engine;

use engine::spec_kind;
pub use engine::*;

mod push;

#[cfg(test)]
use push::Infinite;
pub use push::*;
