use super::*;

/// Compose's `Animatable<T>`: a value you drive by hand instead of pushing a target every frame.
///
/// ```ignore
/// let pulse = ctx.remember_animatable(0.0f32);
/// pulse.animate_to(1.0, SpringSpec::default());   // start a motion      (Compose's animateTo)
/// pulse.snap_to(0.0);                             // jump, cancelling    (Compose's snapTo)
/// let v = pulse.value();                          // read it (peek — no recomposition)
/// if pulse.is_running() { … }                     // Compose's isRunning
/// let v0 = pulse.stop();                          // cancel, handing back the velocity
/// let s = pulse.as_state();                       // feed the usual modifiers
/// ```
///
/// Every call takes `&self`: the handle is a cheap clone of the `State` the slot owns, so it can be
/// captured by a click handler and read by the paint closure.
#[derive(Clone)]
pub struct Animatable<T: Clone + PartialEq + 'static> {
    pub(super) state: State<T>,
    /// The range this value may live in, kept on the slot so every motion started later inherits it.
    pub(super) bounds: crate::runtime::state::Backchannel<Option<(f32, f32)>>,
}

impl<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static> Animatable<T> {
    /// The value right now (a peek: reading it subscribes to nothing, so it cannot recompose).
    pub fn value(&self) -> T {
        self.state.peek()
    }

    /// The value the running motion is heading for — Compose's `targetValue`. `None` when idle,
    /// which is also what Compose reports before the first `animateTo`.
    pub fn target(&self) -> Option<T> {
        animation_target::<T>(self.state.state_id())
    }

    /// Whether a motion is registered for this value — Compose's `isRunning`.
    pub fn is_running(&self) -> bool {
        is_animating_state(self.state.state_id())
    }

    /// How fast the value is moving right now (per second, in the value's own unit).
    pub fn velocity(&self) -> f32 {
        animation_velocity(self.state.state_id())
    }

    /// Write the value and cancel whatever was running — Compose's `snapTo`. Cancelling is part of it:
    /// leaving a motion registered would keep pulling the value away from what the caller just wrote.
    pub fn snap_to(&self, value: T) {
        remove_animation_by_state(self.state.state_id());
        self.state.set(value);
        crate::runtime::state::wake_loop();
    }

    /// Start a motion toward `target` — Compose's `animateTo`. A motion already running is replaced,
    /// inheriting its velocity (see `push_animatable`).
    pub fn animate_to(&self, target: T, spec: impl Into<AnimationSpec>) {
        // NOT the per-frame entry point: that one dedups a same-target motion because
        // `animate*AsState` calls it on every recomposition. A caller holding an `Animatable` is
        // asking deliberately, so a same-target re-target with a new spec has to run (Compose's
        // `animateTo` always does).
        push_animatable_restart(self.state.clone(), target, spec.into());
        set_animation_bounds(self.state.state_id(), self.bounds.peek());
    }

    /// A fling whose initial velocity has the value's own type — Compose's
    /// `Animatable.animateDecay(initialVelocity, decayAnimationSpec)`. The value has no target: it decays
    /// until it rests (or until it reaches a bound, if one is set).
    pub fn animate_decay(&self, velocity: T, spec: DecaySpec) {
        push_decay_vector(self.state.clone(), velocity, spec);
        set_animation_bounds(self.state.state_id(), self.bounds.peek());
    }

    /// [`Self::animate_to`] reporting its outcome — Compose's `animateTo` block, which receives an
    /// `AnimationResult`: the callback is handed a [`MotionEnd`] saying whether the motion arrived,
    /// stopped at a bound, or was cut short by the engine's overrun guard, plus its velocity at the end.
    pub fn animate_to_with_result(
        &self,
        target: T,
        spec: impl Into<AnimationSpec>,
        done: impl FnOnce(MotionEnd) + Send + 'static,
    ) {
        push_animatable_restart_with_result(self.state.clone(), target, spec.into(), done);
        set_animation_bounds(self.state.state_id(), self.bounds.peek());
    }

    /// Run `steps` one after another on this value — the capability Compose gets from a coroutine
    /// (`a.animateTo(first); a.animateTo(second)` inside a `LaunchedEffect`), which winia has no suspend for,
    /// so the steps are queued instead: each one starts when the motion before it ENDS, whatever the reason
    /// (arriving, a value range, or the overrun guard — a stalled queue would be worse than a moved-on one).
    ///
    /// The queue is driven by completion, so a direct `animate_to`/`snap_to` from the caller replaces the
    /// motion in flight and thereby drops the rest of the sequence: nothing else could start step two.
    pub fn animate_sequence(&self, steps: impl IntoIterator<Item = (T, AnimationSpec)>) {
        let queue: std::collections::VecDeque<(T, AnimationSpec)> = steps.into_iter().collect();
        self.run_step(std::sync::Arc::new(Mutex::new(queue)));
    }

    fn run_step(&self, queue: std::sync::Arc<Mutex<std::collections::VecDeque<(T, AnimationSpec)>>>) {
        let Some((target, spec)) = queue.lock().unwrap().pop_front() else {
            return;
        };
        let next = self.clone();
        self.animate_to_with_result(target, spec, move |_end| next.run_step(queue.clone()));
    }

    /// Keep this value inside `[min, max]` — Compose's `Animatable.updateBounds`. Applies to the motion in
    /// flight and to every motion started later through this handle: a value that would leave the range is
    /// written exactly at the bound and the motion ends there. Scalar values only — the range is compared
    /// on the value's scalar projection, which a multi-component value (Offset/Size/Color) does not have,
    /// so bounds on those are ignored.
    pub fn set_bounds(&self, min: f32, max: f32) {
        let bounds = Some((min, max));
        self.bounds.set(bounds);
        set_animation_bounds(self.state.state_id(), bounds);
    }

    /// Drop the range, so the value may travel anywhere again.
    pub fn clear_bounds(&self) {
        self.bounds.set(None);
        set_animation_bounds(self.state.state_id(), None);
    }

    /// The range in force, if any.
    pub fn bounds(&self) -> Option<(f32, f32)> {
        self.bounds.peek()
    }

    /// Cancel the running motion and return the velocity it had, so a fling can carry on — Compose's
    /// `stop`. Zero when nothing was running.
    pub fn stop(&self) -> f32 {
        let velocity = animation_velocity(self.state.state_id());
        remove_animation_by_state(self.state.state_id());
        velocity
    }

    /// The underlying `State`, for a modifier or a container that takes one.
    pub fn as_state(&self) -> State<T> {
        self.state.clone()
    }
}

impl ComposeCtx<'_> {
    /// rememberAnimatable — Compose's `remember { Animatable(initialValue) }`: one value the component
    /// drives imperatively, with its motion state carried by the slot it was remembered in.
    pub fn remember_animatable<T: Clone + PartialEq + 'static>(
        &mut self,
        initial: T,
    ) -> Animatable<T> {
        let state: State<T> = self.remember(|| initial);
        let bounds = self.remember_backchannel(|| None);
        // A motion this handle starts belongs to the slot the handle was remembered in.
        self.own_motion(state.state_id());
        Animatable { state, bounds }
    }
}

use crate::runtime::composer::ComposeCtx;

/// What `updateTransition` returns: the state being animated TO, the state it is leaving, and the
/// default spec (Compose's `Transition<T>`).
pub struct Transition<T: Clone + PartialEq + 'static> {
    target: T,
    /// The state this transition is animating FROM — Compose's `Transition.initialState`. It is what
    /// `transitionSpec = { initial, target -> ... }` chooses a spec from, so it has to be kept.
    previous: T,
    spec: AnimationSpec,
    #[allow(dead_code)]
    label: &'static str,
}

impl ComposeCtx<'_> {
    /// updateTransition — a state machine whose changes animate (cf. Compose `updateTransition`).
    ///
    /// The returned value carries BOTH ends of the pair — the state being animated from and the one
    /// being animated to — because Compose's `transitionSpec = { initial, target -> ... }` picks a spec
    /// per pair (`Transition.kt`'s `TransitionSpec`). Two Backchannels hold the pair between frames:
    /// nothing subscribes to them and they notify nobody, they are just remembered memory.
    pub fn update_transition<T: Clone + PartialEq + 'static>(
        &mut self,
        target: T,
        spec: AnimationSpec,
        label: &'static str,
    ) -> Transition<T> {
        let last = self.remember_backchannel(|| target.clone());
        let previous = self.remember_backchannel(|| target.clone());
        if last.peek() != target {
            previous.set(last.peek());
            last.set(target.clone());
        }
        Transition { target, previous: previous.peek(), spec, label }
    }
}

impl<T: Clone + PartialEq + 'static> Transition<T> {
    /// The state being animated TO — Compose's `Transition.targetState`.
    pub fn target(&self) -> &T {
        &self.target
    }

    /// The state being animated FROM — Compose's `Transition.initialState`.
    pub fn previous(&self) -> &T {
        &self.previous
    }

    /// Generic value animation (cf. Compose `TransitionScope.animateValue`). Any
    /// `AnimatableValue` type works: when the target changes, the value eases to it.
    pub fn animate<U: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> U,
        label: &'static str,
    ) -> State<U> {
        let spec = self.spec.clone();
        self.animate_with_spec(ctx, target_fn, label, move |_, _| spec.clone())
    }

    /// `animateValue(transitionSpec = { initial, target -> ... })` — the per-pair spec Compose's
    /// animations all accept. The function is handed the state being animated FROM and the one being
    /// animated TO, evaluated at the moment the motion is pushed, so a forward leg and a return leg can
    /// run different curves (a slow ease out, a quick bouncy return). The transition's own `spec` stays
    /// the default for every call that does not pass one.
    pub fn animate_with_spec<U: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> U,
        _label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<U> {
        let value = target_fn(&self.target);
        let state: State<U> = ctx.remember(|| value.clone());
        let spec = spec_fn(&self.previous, &self.target);
        crate::animation::push_animatable(state.clone(), value, spec);
        state
    }

    /// Float animation (cf. Compose `TransitionScope.animateFloat`).
    pub fn animate_float(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> f32,
        label: &'static str,
    ) -> State<f32> {
        self.animate(ctx, target_fn, label)
    }

    /// Colour animation, interpolated in CAM16-UCS (cf. Compose `animateColor`).
    pub fn animate_color(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::graphics::Color,
        label: &'static str,
    ) -> State<crate::graphics::Color> {
        self.animate(ctx, target_fn, label)
    }

    /// Dp animation (cf. Compose `animateDp`).
    pub fn animate_dp(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Dp,
        label: &'static str,
    ) -> State<crate::unit::Dp> {
        self.animate(ctx, target_fn, label)
    }

    /// Size animation (cf. Compose `animateSize`).
    pub fn animate_size(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Size,
        label: &'static str,
    ) -> State<crate::unit::Size> {
        self.animate(ctx, target_fn, label)
    }

    /// Offset animation (cf. Compose `animateOffset`).
    pub fn animate_offset(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Offset,
        label: &'static str,
    ) -> State<crate::unit::Offset> {
        self.animate(ctx, target_fn, label)
    }

    /// animateValue — generic value animation (cf. Compose `TransitionScope.animateValue`).
    /// Same machinery as the typed variants above; any `AnimatableValue` works
    /// (f32/i32/Color/Dp/Offset/Size; vector Spring auto-downgrades to Tween).
    pub fn animate_value<U: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> U,
        label: &'static str,
    ) -> State<U> {
        self.animate(ctx, target_fn, label)
    }

    /// animateFloat with Compose's `transitionSpec = { initial, target -> ... }`.
    pub fn animate_float_with_spec(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> f32,
        label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<f32> {
        self.animate_with_spec(ctx, target_fn, label, spec_fn)
    }

    /// animateColor with Compose's `transitionSpec`.
    pub fn animate_color_with_spec(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::graphics::Color,
        label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<crate::graphics::Color> {
        self.animate_with_spec(ctx, target_fn, label, spec_fn)
    }

    /// animateDp with Compose's `transitionSpec`.
    pub fn animate_dp_with_spec(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Dp,
        label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<crate::unit::Dp> {
        self.animate_with_spec(ctx, target_fn, label, spec_fn)
    }

    /// animateSize with Compose's `transitionSpec`.
    pub fn animate_size_with_spec(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Size,
        label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<crate::unit::Size> {
        self.animate_with_spec(ctx, target_fn, label, spec_fn)
    }

    /// animateOffset with Compose's `transitionSpec`.
    pub fn animate_offset_with_spec(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Offset,
        label: &'static str,
        spec_fn: impl Fn(&T, &T) -> AnimationSpec,
    ) -> State<crate::unit::Offset> {
        self.animate_with_spec(ctx, target_fn, label, spec_fn)
    }

    /// createChildTransition — derived transition following this one
    /// (cf. Compose `Transition.createChildTransition(transformToChildState)`).
    ///
    /// The child maps the parent target to its own target type on every call, so it
    /// follows parent changes with the same spec. Label is inherited (labels are
    /// `&'static str` — no runtime string building; Compose synthesizes
    /// "child of <parent>" labels, we keep the parent's).
    pub fn create_child_transition<U: Clone + PartialEq + 'static>(
        &self,
        map: impl Fn(&T) -> U,
    ) -> Transition<U> {
        Transition {
            target: map(&self.target),
            previous: map(&self.previous),
            spec: self.spec.clone(),
            label: self.label,
        }
    }
}

/// The scope of an infinite (looping) transition: it records the state ids it creates, so
/// `dispose` can remove them all at once.
pub struct InfiniteTransition {
    pub(super) ids: std::sync::Arc<std::sync::Mutex<Vec<StateId>>>,
}

impl ComposeCtx<'_> {
    /// rememberInfiniteTransition — creates an infinite-transition scope.
    ///
    /// Its lifetime is tied to the composition point: when that point leaves the tree the scope is
    /// disposed automatically (the on-forget callback removes every motion from the global table, so
    /// nothing leaks or ticks without a composable). Calling `dispose()` explicitly still works, and
    /// the two paths are safe together because draining the ids is idempotent.
    pub fn remember_infinite_transition(&mut self) -> InfiniteTransition {
        let ids_state = self.remember(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        // `.peek()`: the slot is a container whose contents are pushed through the mutex; the State
        // itself is never written, so a tracked read could only add a dead dependency edge.
        let ids = ids_state.peek();
        // Automatic cleanup: when the composition point is removed, take all of this scope's
        // motions out of the global table.
        // `effect::on_forget` keeps the callback in the slot rather than on an implicit leaf node:
        // the node was a real child of the enclosing container, so a transition created inside a
        // spaced `Column`/`Row` opened a gap between its neighbours for a callback layout never used.
        let ids2 = std::sync::Arc::clone(&ids);
        crate::effect::on_forget(self, move || {
            let ids: Vec<StateId> = ids2.lock().unwrap().drain(..).collect();
            for sid in ids {
                crate::animation::remove_animation_by_state(sid);
            }
        });
        InfiniteTransition { ids }
    }
}

impl InfiniteTransition {
    /// Record a state id; the same id is kept once, because a recomposing component calls the
    /// animate_* methods again.
    fn push_id(&self, id: StateId) {
        let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    /// Register a looping float animation from `from` to `to`.
    pub fn animate_float(
        &mut self,
        ctx: &mut ComposeCtx,
        from: f32,
        to: f32,
        spec: InfiniteRepeatableSpec,
    ) -> crate::runtime::state::Visual<f32> {
        let state: State<f32> = ctx.remember(|| from);
        self.push_id(state.state_id());
        let visual = state.into_visual();
        crate::animation::push_infinite_visual(visual.clone(), from, to, spec);
        visual
    }

    /// Register a looping float animation that keeps the value's current phase.
    ///
    /// Unlike [`animate_float`](Self::animate_float), a state that has run before — and is registered
    /// again after being removed — continues from its CURRENT value instead of restarting at
    /// `default_from`. This is for looping animations whose phase must stay continuous, such as a wave
    /// offset.
    pub fn animate_float_preserving(
        &mut self,
        ctx: &mut ComposeCtx,
        default_from: f32,
        to: f32,
        spec: InfiniteRepeatableSpec,
    ) -> crate::runtime::state::Visual<f32> {
        let state: State<f32> = ctx.remember(|| default_from);
        let start = state.peek();
        let range = to - default_from;
        self.push_id(state.state_id());
        let visual = state.into_visual();
        crate::animation::push_infinite_visual(visual.clone(), start, start + range, spec);
        visual
    }

    /// Register a looping colour animation from `from` to `to` (CAM16-UCS interpolation).
    pub fn animate_color(
        &mut self,
        ctx: &mut ComposeCtx,
        from: crate::graphics::Color,
        to: crate::graphics::Color,
        spec: InfiniteRepeatableSpec,
    ) -> crate::runtime::state::Visual<crate::graphics::Color> {
        let state: State<crate::graphics::Color> = ctx.remember(|| from);
        self.push_id(state.state_id());
        let visual = state.into_visual();
        crate::animation::push_infinite_visual(visual.clone(), from, to, spec);
        visual
    }

    /// Cancel every animation this scope created (call it when the component leaves composition or
    /// no longer needs them).
    pub fn dispose(&self) {
        let ids: Vec<StateId> = self.ids.lock().unwrap().drain(..).collect();
        for sid in ids {
            crate::animation::remove_animation_by_state(sid);
        }
    }
}

