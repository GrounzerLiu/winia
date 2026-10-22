use super::*;

/// How a loop repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    /// Start over from the beginning after reaching the end.
    Restart,
    /// Ping-pong (from → to → from).
    Reverse,
}

/// How a loop's `initial_start_offset` is applied — Compose's `StartOffsetType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartOffsetType {
    /// Hold the loop's first value for the offset, then run it as usual (`StartOffsetType.Delay`).
    #[default]
    Delay,
    /// Skip the offset's worth of the first cycle, so the loop starts part-way in
    /// (`StartOffsetType.FastForward`).
    FastForward,
}

/// A spec with a DURATION — what a repeat loop can cycle over: `tween`, `keyframes`, `snap`.
///
/// Compose puts this in the type system (`DurationBasedAnimationSpec<T>`, the parameter of
/// `repeatable` / `infiniteRepeatable`). winia accepted any `AnimationSpec` and sorted the impossible
/// ones out at run time with a `debug_assert` plus a silent 300 ms linear fallback — a release-build
/// surprise instead of a compile error. A spring has no cycle length and a decay has no cycle at all,
/// so neither can be built here.
#[derive(Clone, Debug)]
pub enum DurationSpec {
    Tween(TweenSpec),
    Keyframes(KeyframesSpec),
    Snap(SnapSpec),
}

impl From<TweenSpec> for DurationSpec {
    fn from(spec: TweenSpec) -> Self { Self::Tween(spec) }
}

impl From<KeyframesSpec> for DurationSpec {
    fn from(spec: KeyframesSpec) -> Self { Self::Keyframes(spec) }
}

impl From<SnapSpec> for DurationSpec {
    fn from(spec: SnapSpec) -> Self { Self::Snap(spec) }
}

impl DurationSpec {
    /// One cycle's length.
    pub fn duration(&self) -> Duration {
        match self {
            DurationSpec::Tween(t) => t.duration,
            DurationSpec::Keyframes(k) => k.duration,
            DurationSpec::Snap(s) => s.delay,
        }
    }

    /// The value at `t` in `0.0..=1.0` of a cycle. A `Snap` is already at its end value.
    pub(super) fn value_at<T: AnimatableValue>(&self, from: &T, to: &T, t: f32) -> T {
        match self {
            DurationSpec::Tween(tween) => from.lerp(to, tween.interpolator.interpolate(t)),
            DurationSpec::Keyframes(keyframes) => from.lerp(to, interpolate_keyframes(&keyframes.frames, t)),
            DurationSpec::Snap(_) => to.clone(),
        }
    }

    /// The base curve's factor at `t`, for a loop that interpolates from/to itself.
    pub(super) fn factor(&self, t: f32) -> f32 {
        match self {
            DurationSpec::Tween(tween) => tween.interpolator.interpolate(t),
            DurationSpec::Keyframes(keyframes) => interpolate_keyframes(&keyframes.frames, t),
            DurationSpec::Snap(_) => 1.0,
        }
    }

    /// For [`active_animations_report`].
    pub(super) fn kind(&self) -> String {
        match self {
            DurationSpec::Tween(t) => format!("tween({}ms)", t.duration.as_millis()),
            DurationSpec::Keyframes(k) => format!("keyframes({}ms, {} frames)", k.duration.as_millis(), k.frames.len()),
            DurationSpec::Snap(s) => format!("snap(delay={}ms)", s.delay.as_millis()),
        }
    }
}

/// An infinite repeat spec (`InfiniteRepeatableSpec`).
///
/// `RepeatMode` is covered (`restart` and `reverse`), a per-cycle curve is available through
/// `restart_tween`/`restart_keyframes`, and `with_start_offset` is Compose's
/// `initialStartOffset = StartOffset(offsetMillis, StartOffsetType.Delay | FastForward)`.
#[derive(Debug, Clone)]
pub struct InfiniteRepeatableSpec {
    pub duration: Duration,
    pub(crate) mode: RepeatMode,
    /// The value curve within one cycle (None = linear from → to);
    /// Compose's `infiniteRepeatable(tween{...}/keyframes{...})`.
    pub(crate) base: Option<DurationSpec>,
    pub(crate) start_offset: Duration,
    pub(crate) start_offset_type: StartOffsetType,
}

impl InfiniteRepeatableSpec {
    pub fn restart(duration: Duration) -> Self {
        Self { duration, mode: RepeatMode::Restart, base: None, start_offset: Duration::ZERO, start_offset_type: StartOffsetType::Delay }
    }
    pub fn reverse(duration: Duration) -> Self {
        Self { duration, mode: RepeatMode::Reverse, base: None, start_offset: Duration::ZERO, start_offset_type: StartOffsetType::Delay }
    }
    /// Sample a tween curve within each cycle (Compose's `infiniteRepeatable(tween(...))`).
    pub fn restart_tween(duration: Duration, tween: TweenSpec) -> Self {
        Self {
            duration,
            mode: RepeatMode::Restart,
            base: Some(DurationSpec::Tween(tween)),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
    }
    /// Sample a keyframes curve within each cycle (Compose's `infiniteRepeatable(keyframes{...})`) —
    /// frames = (progress 0..1 within the cycle, value 0..1, per-segment interpolator).
    pub fn restart_keyframes(
        duration: Duration,
        frames: Vec<(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)>,
    ) -> Self {
        Self {
            duration,
            mode: RepeatMode::Restart,
            base: Some(DurationSpec::Keyframes(KeyframesSpec { duration, frames })),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
    }

    /// The curve a cycle follows — Compose's `infiniteRepeatable(tween(...)/keyframes(...))`. It works in
    /// BOTH repeat modes: a `Reverse` loop mirrors its iteration time, so the return leg is this curve running
    /// backwards (Compose's `VectorizedInfiniteRepeatableSpec`), where winia used to travel back linearly.
    pub fn with_base(mut self, base: impl Into<DurationSpec>) -> Self {
        self.base = Some(base.into());
        self
    }

    /// Compose's `initialStartOffset`: with `StartOffsetType::Delay` the loop holds its first value for
    /// `offset` before running its first cycle; with `FastForward` it starts `offset` into that cycle.
    /// The offset applies to the FIRST cycle only — later cycles follow normally, and the phase is
    /// computed from one fixed origin, so the loop never drifts.
    pub fn with_start_offset(mut self, offset: Duration, kind: StartOffsetType) -> Self {
        self.start_offset = offset;
        self.start_offset_type = kind;
        self
    }
}

/// An infinite loop instance (runs until removed) — generic over the value type (shared by f32/Color).
pub(super) struct Infinite<T: AnimatableValue> {
    pub(super) state: crate::runtime::state::Visual<T>,
    pub(super) from: T,
    pub(super) to: T,
    pub(super) spec: InfiniteRepeatableSpec,
    pub(super) start: Instant,
}

impl<T: AnimatableValue + Send + Sync + 'static> AnimationInstance for Infinite<T> {
    fn update(&mut self) -> bool {
        // Compose's `initialStartOffset`: `Delay` holds the loop's first value for the offset, then the
        // first cycle runs; `FastForward` begins that many into the first cycle. Both are applied to the
        // FIRST cycle only. The phase is taken from one fixed origin (no per-cycle restart), so the loop
        // cannot drift and an offset is never re-applied.
        let elapsed = now().saturating_duration_since(self.start);
        let period = self.spec.duration.as_secs_f32().max(0.001);
        let elapsed = match self.spec.start_offset_type {
            StartOffsetType::Delay => match elapsed.checked_sub(self.spec.start_offset) {
                Some(rest) => rest,
                None => {
                    // Still waiting: hold the loop's first value.
                    self.state.set(self.from.clone());
                    return true;
                }
            },
            StartOffsetType::FastForward => elapsed + self.spec.start_offset,
        };
        match self.spec.mode {
            RepeatMode::Restart => {
                let phase = (elapsed.as_secs_f32() % period) / period;
                let eased = loop_factor(self.spec.base.as_ref(), RepeatMode::Restart, 0, phase);
                self.state.set(self.from.lerp(&self.to, eased));
            }
            RepeatMode::Reverse => {
                // The cycle is 2x the duration: the first half travels from → to, the second to → from.
                let cycle = (elapsed.as_secs_f32() / period) as u32;
                let phase = (elapsed.as_secs_f32() % period) / period;
                let eased = loop_factor(self.spec.base.as_ref(), RepeatMode::Reverse, cycle, phase);
                self.state.set(self.from.lerp(&self.to, eased));
            }
        }
        true // Runs forever.
    }
    fn same_target(&self, _target: &dyn std::any::Any) -> bool { false }
    fn state_id(&self) -> StateId { self.state.state_id() }
    fn last_velocity(&self) -> f32 { 0.0 } // A loop has no velocity-continuity semantics.
    fn describe(&self) -> String {
        format!(
            "state={:?} spec=infinite({}ms, {}, offset {}ms {}) value={:.3}->{:.3}",
            self.state.state_id(),
            self.spec.duration.as_millis(),
            match self.spec.mode {
                RepeatMode::Restart => "restart",
                RepeatMode::Reverse => "reverse",
            },
            self.spec.start_offset.as_millis(),
            match self.spec.start_offset_type {
                StartOffsetType::Delay => "delay",
                StartOffsetType::FastForward => "fast-forward",
            },
            self.from.to_f32(),
            self.to.to_f32(),
        )
    }
}

/// Register an infinite loop (shared by f32/Color and other `AnimatableValue`s — one generic
/// registry, no separate third table).
pub fn push_infinite<T: AnimatableValue + Send + Sync + 'static>(
    state: State<T>, from: T, to: T, spec: InfiniteRepeatableSpec,
) {
    push_infinite_visual(state.into_visual(), from, to, spec);
}

/// Visual-handle entry of `push_infinite` (skips the State round-trip when an
/// InfiniteTransition already holds the Visual).
pub fn push_infinite_visual<T: AnimatableValue + Send + Sync + 'static>(
    visual: crate::runtime::state::Visual<T>, from: T, to: T, spec: InfiniteRepeatableSpec,
) {
    let sid = visual.state_id();
    if has_animation_for_state(sid) { return; } // Dedup across lists.
    let anim = Infinite { state: visual, from, to, spec, start: now() };
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
}

/// Register a `Motion<T>` in the global active list (called by `animate_*_as_state`).
pub fn push_animatable<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(state: State<T>, target: T, spec: AnimationSpec) {
    push_animatable_handle(state.into_animating(), target, spec);
}

/// The imperative form: a motion already heading for this exact target is REPLACED instead of
/// deduped, so asking again with a different spec takes effect.
///
/// What `Animatable::animate_to` needs. Compose's `animateTo` always runs the animation it is handed
/// — the dedup in the per-frame `animate*AsState` path exists because THAT caller pushes the same
/// target every recomposition, and silencing a deliberate re-target is not a job it should be doing.
pub fn push_animatable_restart<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
) {
    push_animatable_handle_inner(state.into_animating(), target, spec, true);
}

/// [`push_animatable`] starting from an explicit velocity rather than inheriting one from a replaced
/// animation.
///
/// What a snapping fling needs. Compose's `SnapFlingBehavior` finishes by calling `animateWithTarget`
/// on `animationState.copy(value = 0f)` (`SnapFlingBehavior.kt:150-158`) — the VALUE is reset to the
/// current offset but the FLING's velocity is carried into the snap spring, so the gesture's momentum
/// continues into the settle instead of the list stopping dead and starting again. Without it the
/// snap reads as two separate motions, which is exactly what a hand-off through a stopped animation
/// looks like.
pub fn push_animatable_with_velocity<
    T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static,
>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    velocity: f32,
) {
    push_animatable_with_velocity_and_done(state, target, spec, velocity, || {});
}

/// Is an animation registered for `sid` whose target differs from `target`? A caller that finds the
/// value already AT its target must still cancel such an animation: the value passes through the new
/// target mid-flight, and leaving the old one registered lets it pull the value on to the OLD target
/// (measured: told to stay at 0.557, the value sailed on to 0.969). The same helper backs
/// `push_animatable_handle`, `push_animatable_with_done` and `push_animatable_with_velocity_and_done`.
fn has_conflicting_animation(sid: StateId, target: &dyn std::any::Any) -> bool {
    ACTIVE_ANIMATIONS
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.state_id() == sid && !a.same_target(target))
}

/// [`push_animatable_with_velocity`] reporting completion, for a caller that has to hold a flag up
/// for as long as the motion lasts.
///
/// When the value is ALREADY at `target`: a conflicting animation is cancelled and a zero-displacement
/// motion is registered (which completes at once and reports through `done`); with nothing conflicting
/// to cancel, `done` is called directly and no animation is registered. A caller pushing the same
/// target every frame therefore gets a callback on every frame — use `push_animatable_with_done` for
/// that shape, which reports only when a motion it registered finishes.
pub fn push_animatable_with_velocity_and_done<
    T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static,
>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    velocity: f32,
    done: impl FnOnce() + Send + 'static,
) {
    let handle = state.into_animating();
    if handle.peek() == target {
        if !has_conflicting_animation(handle.state_id(), &target) {
            // Already there: no motion to run, so report completion rather than leaving the caller waiting.
            done();
            return;
        }
        // Cancel the stale one and fall through: the zero-displacement motion that follows lands on the
        // exact target and reports completion itself.
        cancel_animation_by_id(handle.state_id());
    }
    let sid = handle.state_id();
    let spec = normalise_target_spec(spec);
    // A type declaring `supports_spring() == false` (`SharedBounds`, say) has no per-component
    // springs to run, so a Spring is downgraded to the default Tween rather than falling into the
    // single scalar displacement that travels along the value's norm.
    let spec = if T::supports_spring() {
        spec
    } else {
        match spec {
            AnimationSpec::Spring(_) => AnimationSpec::Tween(TweenSpec::default()),
            other => other,
        }
    };
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        if list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) {
            // The same motion is already running and owns its own callback; this one would never fire.
            done();
            return;
        }
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Motion::from_animating(handle);
    anim.start_with_velocity(target, spec, RetargetVelocity::scalar(velocity));
    anim.on_finish(done);
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    crate::runtime::state::wake_loop();
}

/// Animating-handle entry of `push_animatable` (skips the State round-trip when the caller
/// already holds an Animating; identical semantics).
pub fn push_animatable_handle<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(state: crate::runtime::state::Animating<T>, target: T, spec: AnimationSpec) {
    push_animatable_handle_inner(state, target, spec, false);
}

fn push_animatable_handle_inner<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: crate::runtime::state::Animating<T>,
    target: T,
    spec: AnimationSpec,
    restart: bool,
) {
    let sid = state.state_id();
    if state.peek() == target {
        // The value already equals the target: return only when no animation is running (or the
        // running one has the same target). A stale animation with a different target must be
        // cancelled — otherwise it keeps pulling the value towards the old target (fast toggling +
        // Backchannel: peek happens to equal the new target, the old spring 22→2 stays alive, and
        // it finally writes the container back to 2 while the state is already true).
        let conflicting = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().any(|a| a.state_id() == sid && !a.same_target(&target))
        };
        if !conflicting && !restart {
            return;
        }
        // After cancelling the old animation, continue down the registration path: from == target,
        // so a zero-displacement motion writes the exact target every frame — this avoids the
        // residual intermediate value / one-frame rebound that an early return after cancelling
        // can leave under extreme timing.
        // (A Spring completes on its first update; a Tween/Keyframes motion holds the value for its
        // whole duration. The value is the target either way.)
        cancel_animation_by_id(sid);
    }
    let inherited_velocity: RetargetVelocity;
    let spec = normalise_target_spec(spec);
    // As in `push_animatable_with_velocity_and_done`: only a type that declares no Spring support
    // is downgraded.
    let spec = if T::supports_spring() {
        spec
    } else {
        match spec {
            AnimationSpec::Spring(_) => AnimationSpec::Tween(TweenSpec::default()),
            other => other,
        }
    };
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        // Same state, same target already running → skip (no per-frame restart or double drive).
        // The imperative path (`restart`) replaces it instead: re-targeting with a new spec is a
        // deliberate request, and silence there is a bug, not a guard.
        if !restart && list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) { return; }
        // Same state, different target → remove the old animation (the user retargeted mid-flight;
        // letting the old one run would compete with the new target and strand the value on the old
        // path). P2-9: inherit the old velocity before removing it (an interrupted Spring/Decay
        // continues from its current velocity, physically continuous).
        // Each component inherits its own velocity — see `RetargetVelocity`.
        inherited_velocity = list.iter().find(|a| a.state_id() == sid)
            .map(|a| a.retarget_velocity()).unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state_id() != sid);
    } // Lock released; the anim.update() below runs user code without holding it.
    let mut anim = Motion::from_animating(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    // Run the first update immediately so the value does not flash for a frame.
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    // Wake the event loop for a tick round (registering during a render would otherwise stall the
    // animation while the post-render Wait sleeps).
    crate::runtime::state::wake_loop();
}

/// A `Decay` reaching a TARGET-based entry point is replaced by a 300 ms tween.
///
/// A decay coasts from a velocity and has no target, so this shape cannot express one. It used to be
/// run anyway with a zero initial velocity, and a decay at zero velocity reports itself finished on
/// the first frame: a scalar then never moved at all, and a `Size`/`Offset`/`Color` was rebuilt
/// through `AnimatableValue::from_f32` — a placeholder, i.e. a square at the origin or an opaque GREY
/// at the old Oklab lightness. The multi-component case was special-cased to `Snap` to dodge that,
/// which turned a fade into a jump; a tween is the honest fallback for both, because it interpolates
/// with `lerp` (component-wise for a vector) and actually reaches the target the caller asked for.
///
/// A real fling takes a velocity OF THE VALUE'S OWN TYPE: `push_decay` / `push_decay_vector`
/// (`Motion::animate_decay_with_velocity`), or the `Animatable::animate_decay` handle.
fn normalise_target_spec(spec: AnimationSpec) -> AnimationSpec {
    match spec {
        AnimationSpec::Decay(_) => AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(300),
            interpolator::Linear::new(),
        )),
        other => other,
    }
}

/// Register a `Motion<Color>` in the global active list (called by `animate_color_as_state`)
///
/// The spec is honoured as written, springs included: a `Color` is a four-component value (Oklab
/// `alpha, L, a, b`), so `Motion::start_with_velocity` gives it one spring per component, the way
/// Compose's `Color.VectorConverter` does. Until this, a colour spring was silently replaced by a
/// 300 ms linear tween — the caller asked for a spring and got a different motion (measured on
/// `Tab`'s content colour, whose `SpringSpec` never ran). A `Decay` is the one spec that still cannot
/// run here: see `normalise_target_spec`.
pub fn push_animatable_color(state: State<crate::graphics::Color>, target: crate::graphics::Color, spec: AnimationSpec) {
    
    if state.peek() == target {
        // As in `push_animatable`: a stale colour animation with a different target must be
        // cancelled, otherwise it keeps pulling the value towards the old target.
        let sid = state.state_id();
        let conflicting = {
            let list = ACTIVE_COLOR_ANIMATIONS.lock().unwrap();
            list.iter().any(|anim| {
                anim.state.state_id() == sid
                    && !anim.anim_state.as_ref().map(|s| s.to == target).unwrap_or(false)
            })
        };
        if !conflicting {
            return;
        }
        ACTIVE_COLOR_ANIMATIONS.lock().unwrap().retain(|anim| anim.state.state_id() != sid);
        return;
    }
    let sid = state.state_id();
    // The velocity to inherit on a retarget: each Oklab component its own, the way the main registry
    // hands a `Size`'s axes their own (see `RetargetVelocity`).
    let inherited_velocity: RetargetVelocity;
    {
        let mut list = ACTIVE_COLOR_ANIMATIONS.lock().unwrap();
        if list.iter().any(|anim| anim.state.state_id() == sid && anim.anim_state.as_ref().map(|s| s.to == target).unwrap_or(false)) { return; }
        inherited_velocity = list.iter().find(|anim| anim.state.state_id() == sid)
            .map(|anim| anim.retarget_velocity())
            .unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state.state_id() != sid);
    }
    let spec = normalise_target_spec(spec);
    let mut anim = Motion::new(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    if anim.update() {
        ACTIVE_COLOR_ANIMATIONS.lock().unwrap().push(anim);
    }
    // Wake the event loop for a tick round (same as `push_animatable`).
    crate::runtime::state::wake_loop();
}

/// Exponential decay spec (Compose's `exponentialDecay`) — no target value: it decays from an
/// initial velocity to a natural stop (the core of a fling / inertial scrolling).
///
/// Analytic form: `value(t) = from + v0/friction·(1 - e^(-friction·t))`
/// Completion: velocity `|v0·e^(-friction·t)| < threshold` → write the limit value (parking exactly).
///
/// ⚠️ Scalar semantics only: this goes through `AnimatableValue::to_f32/from_f32`, so a vector type
/// (Offset/Size) degenerates into the norm direction — Decay applies to f32 only (scroll offsets and
/// the like).
#[derive(Clone, Debug, PartialEq)]
pub struct DecaySpec {
    /// Friction coefficient (higher stops sooner; Compose's default is 4.2).
    pub friction: f32,
    /// Velocity threshold (below it the motion counts as stopped; on the px/s scale).
    pub threshold: f32,
}

impl DecaySpec {
    /// Fail fast: friction<=0 makes the limit displacement infinite (v0/friction) or the motion
    /// never stop — a misconfiguration surfaces immediately (the `max(0.001)` in update is a second
    /// safety net, not a replacement for validation).
    pub fn new(friction: f32, threshold: f32) -> Self {
        assert!(friction > 0.0, "DecaySpec::new: friction must be > 0 (got {friction})");
        assert!(threshold > 0.0, "DecaySpec::new: threshold must be > 0 (got {threshold})");
        Self { friction, threshold }
    }

    /// How far a fling started at `initial_velocity` would travel: the asymptote `v0 / friction` of
    /// `value(t) = v0/friction · (1 - e^(-friction·t))`. This is where the motion goes ignoring the
    /// velocity threshold, and it is what a pager estimates "how many pages would this flick carry" from.
    pub fn target_value(&self, initial_velocity: f32) -> f32 {
        if self.friction > 0.0 { initial_velocity / self.friction } else { 0.0 }
    }
}

impl Default for DecaySpec {
    fn default() -> Self {
        Self { friction: 4.2, threshold: 0.1 }
    }
}

/// `exponential_decay(friction)` — Compose's `exponentialDecay(frictionMultiplier)`.
/// Higher friction stops sooner (default 4.2).
pub fn exponential_decay(friction: f32) -> DecaySpec {
    DecaySpec::new(friction, 0.1)
}

/// Convenience registration of an exponential decay (fling / inertial scrolling):
/// `push_decay(state, v0, exponential_decay(4.2))`. Semantics: an existing animation on the same
/// state is replaced (the new fling takes over, consistent with a retarget).
pub fn push_decay(state: State<f32>, initial_velocity: f32, spec: DecaySpec) {
    let sid = state.state_id();
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Motion::new(state);
    anim.animate_decay(initial_velocity, spec);
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    // Wake the event loop for a tick round (same as `push_animatable`).
    crate::runtime::state::wake_loop();
}

/// A fling whose initial velocity has the value's own type — Compose's
/// `Animatable<T>.animateDecay(initialVelocity: T, decayAnimationSpec: DecayAnimationSpec<T>)` for a
/// multi-component value (`Offset`, `Size`, `Color`): every component decays with the velocity it was
/// given and parks on its own limit, so `Offset(100, -60)` travels up and left independently instead of
/// collapsing to one scalar direction. The counterpart of `push_decay`, which is scalar; a boundary
/// clamp is scalar too, so this entry point has none.
pub fn push_decay_vector<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    velocity: T,
    spec: DecaySpec,
) {
    let sid = state.state_id();
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Motion::new(state);
    anim.animate_decay_with_velocity(velocity, spec);
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    crate::runtime::state::wake_loop();
}

/// Fling inertial scrolling: exponential decay plus a boundary clamp (stop at once when a bound is
/// hit — as a Compose fling stops once `scrollBy` is consumed). The clamp is evaluated every frame
/// (it reads the latest scroll limit, written back during layout). An existing animation on the same
/// state is replaced (the new fling takes over).
pub fn push_fling(
    state: State<f32>,
    initial_velocity: f32,
    spec: DecaySpec,
    clamp: impl Fn(f32) -> f32 + Send + Sync + 'static,
    on_finish: impl FnOnce() + Send + 'static,
) {
    push_fling_internal(state, initial_velocity, spec, clamp, None, on_finish);
}

/// Inertial scrolling that also reports the instantaneous leftover velocity when it hits the clamp
/// boundary.
///
/// `on_boundary` is called at most once; it is not called when the decay reaches zero naturally.
/// Nested scroll uses it to hand the velocity the child did not consume to an ancestor's
/// post-fling chain.
pub fn push_fling_with_boundary(
    state: State<f32>,
    initial_velocity: f32,
    spec: DecaySpec,
    clamp: impl Fn(f32) -> f32 + Send + Sync + 'static,
    on_boundary: impl FnOnce(f32) + Send + 'static,
    on_finish: impl FnOnce() + Send + 'static,
) {
    push_fling_internal(state, initial_velocity, spec, clamp, Some(Box::new(on_boundary)), on_finish);
}

fn push_fling_internal(
    state: State<f32>,
    initial_velocity: f32,
    spec: DecaySpec,
    clamp: impl Fn(f32) -> f32 + Send + Sync + 'static,
    on_boundary: Option<Box<dyn FnOnce(f32) + Send>>,
    on_finish: impl FnOnce() + Send + 'static,
) {
    let sid = state.state_id();
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Motion::new(state);
    anim.set_clamp(clamp);
    if let Some(on_boundary) = on_boundary {
        anim.on_boundary(on_boundary);
    }
    anim.on_finish(on_finish);
    anim.animate_decay(initial_velocity, spec);
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    crate::runtime::state::wake_loop();
}

/// `animateIntAsState` (Compose's) — when the target changes, animate from the current value to the
/// new one; the returned State is used directly for rendering (`get()` while composing, `peek()` while
/// drawing).
///
/// ```ignore
/// let animated = animate_int_as_state(ctx, count.get(), TweenSpec::new(300, EaseOutQuad::new()));
/// Text::new(format!("{}", animated.get()));
/// ```
///
/// Internally: `remember` stores the animated State and every call compares the target — a change
/// calls `push_animatable` (the caller's `get()` registers the dependency, so a change re-runs it).
pub fn animate_int_as_state(
    ctx: &mut ComposeCtx,
    target: i32,
    spec: impl Into<AnimationSpec>,
) -> State<i32> {
    let value: State<i32> = ctx.remember(|| target);
    if value.peek() != target {
        push_animatable(value.clone(), target, spec.into());
    }
    value
}

/// `animateValueAsState` (Compose's generic form) — target animation for any `AnimatableValue`
/// (f32/i32/Color; a Spring on Offset/Size runs per component and is never downgraded).
///
/// ```ignore
/// let animated = animate_value_as_state(ctx, color, TweenSpec::new(300, Linear::new()));
/// ```
pub fn animate_value_as_state<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    ctx: &mut ComposeCtx,
    target: T,
    spec: impl Into<AnimationSpec>,
) -> State<T> {
    let value: State<T> = ctx.remember(|| target.clone());
    if value.peek() != target {
        push_animatable(value.clone(), target, spec.into());
    }
    value
}

/// `push_animatable` + a completion callback (Compose's `animate*AsState(finishedListener)`), fired
/// once when a motion this call registered finishes naturally or is forced to by the overrun guard.
///
/// Two differences from Compose's `finishedListener`, both forced by winia's call shape:
/// * Compose fires it when `Animatable.animateTo` ends, INCLUDING the "already at the target, motion
///   completes instantly" case (`AnimationEndResult.Finished`). But Compose only calls `animateTo` when
///   the target CHANGES, so a per-frame caller has no such moment to attach to. This entry point is
///   pushed every frame, where "already there" is the steady state, so nothing fires there — only a
///   registered motion reports. A caller that uses it one-shot per target should reach for
///   `push_animatable_with_velocity_and_done`, which reports the instant case too.
/// * Compose's `label` and `visibilityThreshold` parameters have no counterpart here: winia has no
///   tooling label, and the threshold lives on the `AnimationSpec`.
///
/// Careful: pushing a new target for the same state while a motion is running REPLACES it — the old
/// motion's callback is dropped silently, never called, because the new motion carries the new one. A
/// caller that needs every completion wants per-instance `Motion::on_finish` management, or to
/// re-register from inside the callback.
pub fn push_animatable_with_done<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce() + Send + 'static,
) {
    push_animatable_with_result(state, target, spec, move |_end| done());
}

/// [`push_animatable`] reporting its outcome: why the motion ended and the velocity it had at that moment
/// — Compose's `AnimationResult` handed to the block of `Animatable.animateTo`. The reason separates a
/// motion that arrived (`Finished`) from one a value range stopped (`BoundReached`) and from one the
/// engine's overrun guard cut short (`Forced`), which the plain callback cannot tell apart.
pub fn push_animatable_with_result<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce(MotionEnd) + Send + 'static,
) {
    push_animatable_with_result_inner(state, target, spec, done, false);
}

/// The imperative form of `push_animatable_with_result`: same-target retargets take effect (see
/// `push_animatable_restart`).
pub fn push_animatable_restart_with_result<
    T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static,
>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce(MotionEnd) + Send + 'static,
) {
    push_animatable_with_result_inner(state, target, spec, done, true);
}

fn push_animatable_with_result_inner<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce(MotionEnd) + Send + 'static,
    restart: bool,
) {
    let sid = state.state_id();
    if state.peek() == target {
        // Already at the target. Nothing to animate, so no callback fires — this entry point is for a
        // caller that pushes every frame, where "already there" is the steady state and a callback per
        // frame would be wrong. (Compose calls its `finishedListener` for an instantaneous
        // `Animatable.animateTo`, but it only calls `animateTo` when the target CHANGES; a per-frame
        // push has no such moment to attach to.)
        //
        // A conflicting animation still has to be cancelled: the value passes through the new target
        // mid-flight, and a stale motion left registered keeps pulling it to the OLD target (measured
        // before this: told to stay at 0.557, the value sailed on to 0.969). `push_animatable_handle`
        // has always done this; this entry point was missing it.
        if !has_conflicting_animation(sid, &target) && !restart {
            return;
        }
        cancel_animation_by_id(sid);
    }
    // Same preparation as `push_animatable_handle`: a multi-component value cannot decay, and a motion
    // replacing a running one inherits its velocity so a retarget stays continuous.
    let inherited_velocity: RetargetVelocity;
    let spec = normalise_target_spec(spec);
    let spec = if T::supports_spring() {
        spec
    } else {
        match spec {
            AnimationSpec::Spring(_) => AnimationSpec::Tween(TweenSpec::default()),
            other => other,
        }
    };
    {
        let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
        if !restart && list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) { return; }
        inherited_velocity = list
            .iter()
            .find(|a| a.state_id() == sid)
            .map(|a| a.retarget_velocity())
            .unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Motion::new(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    anim.on_finish_with(done);
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    crate::runtime::state::wake_loop();
}

/// AnimationInstance for Motion<f32> (and every other value type it is instantiated with)
impl<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static> AnimationInstance for Motion<T> {
    fn update(&mut self) -> bool {
        Motion::update(self)
    }
    fn state_id(&self) -> StateId {
        self.state.state_id()
    }
    fn same_target(&self, target: &dyn std::any::Any) -> bool {
        target.downcast_ref::<T>()
            .map(|t| self.anim_state.as_ref().map(|s| s.to == *t).unwrap_or(false))
            .unwrap_or(false)
    }
    fn last_velocity(&self) -> f32 {
        self.anim_state.as_ref().map(|s| s.last_velocity).unwrap_or(0.0)
    }
    fn retarget_velocity(&self) -> RetargetVelocity {
        Motion::retarget_velocity(self)
    }
    fn set_value_bounds(&mut self, bounds: Option<(f32, f32)>) {
        self.set_clamp_opt(bounds.map(|(lo, hi)| -> Box<dyn Fn(f32) -> f32 + Send + Sync> {
            Box::new(move |v| v.clamp(lo.min(hi), hi.max(lo)))
        }));
    }
    fn target_value(&self) -> Option<&dyn std::any::Any> {
        self.anim_state
            .as_ref()
            .map(|s| &s.to as &dyn std::any::Any)
    }
    fn describe(&self) -> String {
        match &self.anim_state {
            Some(s) => format!(
                "state={:?} spec={} from={:.3} target={:.3} elapsed={}ms v={:.1}{}",
                self.state.state_id(),
                spec_kind(&s.spec),
                s.from.to_f32(),
                s.to.to_f32(),
                now().saturating_duration_since(s.start).as_millis(),
                s.last_velocity,
                match (&s.lanes, &s.decay_lanes) {
                    (Some(lanes), _) => format!(" components={}", lanes.len()),
                    (_, Some(lanes)) => format!(" decay_components={}", lanes.len()),
                    _ => String::new(),
                },
            ),
            None => format!("state={:?} idle", self.state.state_id()),
        }
    }
}
