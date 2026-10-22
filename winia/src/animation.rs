//! 动画系统 — 对齐 Jetpack Compose 动画三层 API
//!
//! ## 层级
//! - **高层**: `animate_*_as_state` — 单值动画，开箱即用
//! - **中层**: `update_transition` — 多属性协同动画
//! - **底层**: `Animatable`（内部实现，pub(crate)——由高层 API 驱动）
//!
//! ## 核心
//! - 默认 `SpringSpec`（物理弹簧），可选 `TweenSpec`（补间）
//! - 与 `State<T>` 集成 → 自动触发重组
//! - 帧驱动在 `app.rs` 的 `AboutToWait` 中更新

pub mod interpolator;
pub mod visibility;

pub use visibility::{
    ChannelValues, ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};

use crate::runtime::state::{State, StateId};
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════
// 活跃动画管理（全局注册表，避开 Composer 字段修改）
// ═══════════════════════════════════════════════════════════

#[allow(unused_imports)] // the file-scope `#[test]` functions below build interpolators in an Arc
use std::sync::Arc;
use std::sync::{Mutex, LazyLock};

// ═══════════════════════════════════════════════════════════
// The clock
// ═══════════════════════════════════════════════════════════

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

/// 动画实例 trait（擦除类型后存储在全局列表）
pub trait AnimationInstance: Send {
    fn update(&mut self) -> bool;
    fn state_id(&self) -> StateId;
    /// 类型安全的精确目标比较（跨类型返回 false）
    fn same_target(&self, target: &dyn std::any::Any) -> bool;
    /// 当前速度（px/s）——供 retarget 速度延续（P2-9）
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
/// Color 动画列表（与 f32 动画分开，避免类型擦除）
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

// ═══════════════════════════════════════════════════════════
// InfiniteTransition — 无限循环动画（对标 Compose rememberInfiniteTransition）
// ═══════════════════════════════════════════════════════════

/// 重复模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    /// 结束回到起点重来
    Restart,
    /// 往返（from→to→from）
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
    fn value_at<T: AnimatableValue>(&self, from: &T, to: &T, t: f32) -> T {
        match self {
            DurationSpec::Tween(tween) => from.lerp(to, tween.interpolator.interpolate(t)),
            DurationSpec::Keyframes(keyframes) => from.lerp(to, interpolate_keyframes(&keyframes.frames, t)),
            DurationSpec::Snap(_) => to.clone(),
        }
    }

    /// The base curve's factor at `t`, for a loop that interpolates from/to itself.
    fn factor(&self, t: f32) -> f32 {
        match self {
            DurationSpec::Tween(tween) => tween.interpolator.interpolate(t),
            DurationSpec::Keyframes(keyframes) => interpolate_keyframes(&keyframes.frames, t),
            DurationSpec::Snap(_) => 1.0,
        }
    }

    /// For [`active_animations_report`].
    fn kind(&self) -> String {
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
    /// 循环周期内的取值曲线（None = 线性 from→to）；
    /// 对标 Compose `infiniteRepeatable(tween{...}/keyframes{...})`
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
    /// 周期内按 tween 曲线取值（对标 Compose `infiniteRepeatable(tween(...))`）
    pub fn restart_tween(duration: Duration, tween: TweenSpec) -> Self {
        Self {
            duration,
            mode: RepeatMode::Restart,
            base: Some(DurationSpec::Tween(tween)),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
    }
    /// 周期内按 keyframes 曲线取值（对标 Compose `infiniteRepeatable(keyframes{...})`）——
    /// frames = (周期内进度 0..1, 值 0..1，段间插值器)
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

/// 无限循环动画实例（永远运行，直到被移除）——泛型统一（f32/Color 共用）。
struct Infinite<T: AnimatableValue> {
    state: crate::runtime::state::Visual<T>,
    from: T,
    to: T,
    spec: InfiniteRepeatableSpec,
    start: Instant,
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
        true // 永远运行
    }
    fn same_target(&self, _target: &dyn std::any::Any) -> bool { false }
    fn state_id(&self) -> StateId { self.state.state_id() }
    fn last_velocity(&self) -> f32 { 0.0 } // 无限循环无速度延续语义
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

/// 注册一个无限循环动画（f32/Color 等 AnimatableValue 共用——泛型表，无独立第三表）
pub fn push_infinite<T: AnimatableValue + Send + Sync + 'static>(
    state: State<T>, from: T, to: T, spec: InfiniteRepeatableSpec,
) {
    push_infinite_visual(state.into_visual(), from, to, spec);
}

/// `push_infinite` 的 Visual-handle 入口（InfiniteTransition 已持有 Visual
/// 时避免 State round-trip）。
pub fn push_infinite_visual<T: AnimatableValue + Send + Sync + 'static>(
    visual: crate::runtime::state::Visual<T>, from: T, to: T, spec: InfiniteRepeatableSpec,
) {
    let sid = visual.state_id();
    if has_animation_for_state(sid) { return; } // 跨列表去重
    let anim = Infinite { state: visual, from, to, spec, start: now() };
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
}

/// 注册一个动画到全局活跃列表
/// Register a `Motion<T>` in the global active list (called by `animate_*_as_state`)
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

/// `push_animatable` 的 Animating-handle 入口（调用方已持有 Animating
/// 时避免 State round-trip；语义与 `push_animatable` 完全一致）。
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
        // 当前值已等于目标：仅当无进行中动画（或动画目标相同）时才可直接返回。
        // 若存在目标不同的旧动画，必须取消它——否则旧动画会继续把值拉向旧目标
        // （快速切换 + Backchannel 场景：peek 恰好等于新目标，旧弹簧 22→2 存活，
        // 最终把容器写回 2 而状态已是 true）。
        let conflicting = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().any(|a| a.state_id() == sid && !a.same_target(&target))
        };
        if !conflicting && !restart {
            return;
        }
        // 取消旧动画后继续走注册路径：from == target，零位移动画的每一帧都精确写回 target，
        // 避免“取消后直接返回”在极端时序下残留中间值/1 帧回弹。
        // (A Spring completes on its first update; a Tween/Keyframes motion holds the value for its
        // whole duration. The value is the target either way.)
        cancel_animation_by_id(sid);
    }
    let inherited_velocity: RetargetVelocity;
    let spec = normalise_target_spec(spec);
    // 同 `push_animatable_with_velocity_and_done`：只有声明不支持 Spring 的类型才降级
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
        // 同 state 同目标运行中 → 跳过（防每帧重启/双驱动）。
        // The imperative path (`restart`) replaces it instead: re-targeting with a new spec is a
        // deliberate request, and silence there is a bug, not a guard.
        if !restart && list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) { return; }
        // 同 state 不同目标 → 移除旧动画（用户中途改目标——旧动画继续会与
        // 新目标竞争，导致值卡在旧目标路径上）；P2-9：移除前继承旧速度
        // （Spring/Decay 被打断时新动画从当前速度继续，物理连续）
        // Each component inherits its own velocity — see `RetargetVelocity`.
        inherited_velocity = list.iter().find(|a| a.state_id() == sid)
            .map(|a| a.retarget_velocity()).unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state_id() != sid);
    } // 锁释放，下面 anim.update() 不持锁执行用户代码
    let mut anim = Motion::from_animating(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    // 立即执行首次更新，避免等下一帧 flash
    // A motion that completes on its first update is not registered: its writes and callbacks already
    // happened, and a finished entry would keep `is_animating`/`has_animation_for_state` true for a frame.
    if anim.update() {
        ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    }
    // 唤醒事件循环启动推进轮次（渲染中注册——渲染后 Wait 休眠会卡住动画）
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
        // 与 push_animatable 相同：存在目标不同的旧颜色动画时必须取消，
        // 否则旧动画会把值继续拉向旧目标
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
    // 唤醒事件循环启动推进轮次（同 push_animatable）
    crate::runtime::state::wake_loop();
}

/// 指数衰减动画规格（对标 Compose exponentialDecay）——无目标值，
/// 从初始速度衰减到自然停止（fling/惯性滚动核心）。
///
/// 解析式：`value(t) = from + v0/friction·(1 - e^(-friction·t))`
/// 完成：速度 `|v0·e^(-friction·t)| < threshold` → 写极限值（精确停靠）。
///
/// ⚠️ 仅标量语义：内部经 `AnimatableValue::to_f32/from_f32`，向量类型
/// （Offset/Size）会退化为范数方向——Decay 只应用于 f32（滚动偏移等）。
#[derive(Clone, Debug, PartialEq)]
pub struct DecaySpec {
    /// 摩擦系数（越大停得越快；对标 Compose 默认 4.2）
    pub friction: f32,
    /// 速度阈值（低于即视为停止，px/s 量级）
    pub threshold: f32,
}

impl DecaySpec {
    /// 快速失败：friction<=0 会导致极限位移无限大（v0/friction）或永不停止——
    /// 配置错误立即暴露（`max(0.001)` 是 update 里的双保险，不替代校验）
    pub fn new(friction: f32, threshold: f32) -> Self {
        assert!(friction > 0.0, "DecaySpec::new: friction 必须 > 0（收到 {friction}）");
        assert!(threshold > 0.0, "DecaySpec::new: threshold 必须 > 0（收到 {threshold}）");
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

/// `exponential_decay(friction)`——对标 Compose `exponentialDecay(frictionMultiplier)`。
/// 摩擦越大停得越快（默认 4.2）。
pub fn exponential_decay(friction: f32) -> DecaySpec {
    DecaySpec::new(friction, 0.1)
}

/// 便捷注册指数衰减（fling/惯性滚动）：`push_decay(state, v0, exponential_decay(4.2))`。
/// 语义：同 state 已有动画 → 取代（新 fling 接管，与 retarget 一致）。
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
    // 唤醒事件循环启动推进轮次（同 push_animatable）
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

/// fling 惯性滚动：指数衰减 + 边界 clamp（撞边界立即停——对齐 Compose fling
/// 在 scrollBy 消耗完时停止）。clamp 每帧求值（读最新滚动极限——布局期回写）。
/// 同 state 已有动画 → 取代（新 fling 接管）。
pub fn push_fling(
    state: State<f32>,
    initial_velocity: f32,
    spec: DecaySpec,
    clamp: impl Fn(f32) -> f32 + Send + Sync + 'static,
    on_finish: impl FnOnce() + Send + 'static,
) {
    push_fling_internal(state, initial_velocity, spec, clamp, None, on_finish);
}

/// 惯性滚动并报告撞到 clamp 边界时的瞬时剩余速度。
///
/// `on_boundary` 最多调用一次；自然衰减到零时不会调用。nested-scroll 使用
/// 该回调把 child 未消费的速度交给祖先的 post-fling 链。
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

/// `animateIntAsState`（对标 Compose）——target 变化时自动从当前值动画到新值，
/// 返回的 State 直接用于渲染（组合期 `get()` 或绘制期 `peek()`）。
///
/// ```ignore
/// let animated = animate_int_as_state(ctx, count.get(), TweenSpec::new(300, EaseOutQuad::new()));
/// Text::new(format!("{}", animated.get()));
/// ```
///
/// 内部：`remember` 保存动画 State + 每次调用比较 target——变化即
/// `push_animatable`（target 由调用方 `get()` 注册依赖 → 变化触发重跑）。
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

/// `animateValueAsState`（对标 Compose 泛型版）——任意 `AnimatableValue` 的
/// target 动画（f32/i32/Color；Offset/Size 的 Spring 按分量跑，不会降级）。
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

/// The spec's name, for [`active_animations_report`]: what kind of motion this is, with the one number
/// that says how long it lasts.
fn spec_kind(spec: &AnimationSpec) -> String {
    match spec {
        AnimationSpec::Spring(s) => format!(
            "spring(stiffness={:.0}, damping={:.2}, threshold={:.2})",
            s.stiffness, s.damping_ratio, s.threshold
        ),
        AnimationSpec::Tween(t) => format!("tween({}ms)", t.duration.as_millis()),
        AnimationSpec::Keyframes(k) => {
            format!("keyframes({}ms, {} frames)", k.duration.as_millis(), k.frames.len())
        }
        AnimationSpec::Repeatable(r) => format!(
            "repeatable({}x {}, base={})",
            r.iterations,
            match r.mode {
                RepeatMode::Restart => "restart",
                RepeatMode::Reverse => "reverse",
            },
            r.base.kind()
        ),
        AnimationSpec::Snap(s) => format!("snap(delay={}ms)", s.delay.as_millis()),
        AnimationSpec::Decay(d) => format!("decay(friction={:.2})", d.friction),
    }
}

/// 更新所有活跃动画，返回是否有动画还在运行
pub fn update_animations() -> bool {
    // 锁内取出动画，锁外执行 update（避免锁内执行用户代码导致死锁）
    let anims = std::mem::take(&mut *ACTIVE_ANIMATIONS.lock().unwrap());
    let mut still = Vec::new();
    for mut a in anims {
        if a.update() { still.push(a); }
    }
    let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
    // 去重：锁外新 push 的动画优先，丢弃 still 中同 state 的旧动画
    for a in still {
        if !list.iter().any(|x| x.state_id() == a.state_id()) {
            list.push(a);
        }
    }
    // Color 动画
    let canims = std::mem::take(&mut *ACTIVE_COLOR_ANIMATIONS.lock().unwrap());
    let mut cstill = Vec::new();
    for mut c in canims {
        if c.update() { cstill.push(c); }
    }
    let mut clist = ACTIVE_COLOR_ANIMATIONS.lock().unwrap();
    for c in cstill {
        if !clist.iter().any(|x| x.state.state_id() == c.state.state_id()) {
            clist.push(c);
        }
    }
    !list.is_empty() || !clist.is_empty()
}

/// 从所有动画列表移除指定 state 的动画（InfiniteTransition::dispose 用）
pub fn remove_animation_by_state(state_id: StateId) {
    ACTIVE_ANIMATIONS.lock().unwrap().retain(|a| a.state_id() != state_id);
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().retain(|a| a.state.state_id() != state_id);
}

/// 清空全部活跃动画（测试隔离用——跨测试清理全局表）
#[cfg(test)]
pub(crate) fn clear_all_animations() {
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
}

/// 指定 state 是否已在任一动画列表（跨列表去重，防双倍推进）
pub fn has_animation_for_state(state_id: StateId) -> bool {
    ACTIVE_ANIMATIONS.lock().unwrap().iter().any(|a| a.state_id() == state_id)
        || ACTIVE_COLOR_ANIMATIONS.lock().unwrap().iter().any(|a| a.state.state_id() == state_id)
}

/// 取消指定 state 的进行中动画（值保持当前，不再被动画覆盖）。
///
/// 注意：`State::set` **不**取消动画——动画是独立系统，set 后下一帧
/// update_animations 仍会把动画值写回。要"立即停下并设为目标值"请先
/// `cancel_animation(&state)` 再 `state.set(v)`（或直接用 Snap push）。
pub fn cancel_animation<T: 'static>(state: &State<T>) {
    cancel_animation_by_id(state.state_id());
}

/// Cancel by raw StateId (for `Animating`/`Visual` handles that share the
/// same signal but are not `State<T>`).
pub fn cancel_animation_by_id(sid: crate::runtime::state::StateId) {
    ACTIVE_ANIMATIONS.lock().unwrap().retain(|a| a.state_id() != sid);
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().retain(|a| a.state.state_id() != sid);
}

/// The interactions that carry an elevation — the `Interaction` kinds `ElevationDefaults` knows about
/// (`material3/internal/Elevation.kt:75-108`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationInteraction {
    Press,
    Drag,
    Hover,
    Focus,
}

/// The spec material3 animates an elevation change with — one rule for every component that has a per-state
/// elevation (`Button`, `Card`, and anything else that follows `animateElevation`).
///
/// Compose keys it on the PAIR of interactions, not on the direction of the value
/// (`material3/internal/Elevation.kt:50-61`):
/// - moving TO an interaction (`to` is `Some`): 120 ms `FastOutSlowIn`, whatever the interaction is
///   (`DefaultIncomingSpec`, `:113`);
/// - moving back to the DEFAULT state (`to` is `None`, `from` is `Some`): 150 ms of
///   `CubicBezierEasing(0.40, 0.00, 0.60, 1.00)` (`DefaultOutgoingSpec`, `:115`), except leaving HOVER, which is
///   the same curve in 120 ms (`HoveredOutgoingSpec`, `:117`);
/// - neither (`None`, `None`): Compose SNAPS to the value (`:60`), which is what `None` asks of the caller.
///
/// A previous `(current, target, was_hovered)` form compared VALUES instead, which gets two reachable cases
/// wrong: hovering then pressing took the hovered curve where Compose takes `FastOutSlowIn`, and releasing a
/// press while still hovering was read as an outgoing change even though `HoverInteraction.Enter` is still the
/// latest interaction.
pub fn elevation_spec_for_pair(
    from: Option<ElevationInteraction>,
    to: Option<ElevationInteraction>,
) -> Option<AnimationSpec> {
    use interpolator::{fast_out_slow_in, CubicBezier};
    let outgoing_easing = || CubicBezier::new(0.40, 0.00, 0.60, 1.00);
    match (from, to) {
        (_, Some(_)) => Some(AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(120),
            fast_out_slow_in(),
        ))),
        (Some(ElevationInteraction::Hover), None) => Some(AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(120),
            outgoing_easing(),
        ))),
        (Some(_), None) => Some(AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(150),
            outgoing_easing(),
        ))),
        (None, None) => None,
    }
}

/// Whether an animation is still running for ONE state.
///
/// [`is_animating`] answers for the process; a caller that needs to know when its own animation
/// finished asks about the state it animates. The value alone cannot answer that: a spring does land
/// on its exact target once every component is at rest, but a value sitting on the target looks the
/// same when the animation never ran or was cancelled, and a spec that keeps running (Repeatable) is
/// indistinguishable from a settled one. `AnimatedSize` asks this AND compares the value, which is
/// what Compose's `animateTo` returning `Finished` amounts to.
pub fn is_animating_state(id: crate::runtime::state::StateId) -> bool {
    ACTIVE_ANIMATIONS.lock().unwrap().iter().any(|a| a.state_id() == id)
        || ACTIVE_COLOR_ANIMATIONS.lock().unwrap().iter().any(|a| a.state.state_id() == id)
}

/// The value a state's running animation is heading for — Compose's `Animatable.targetValue`, `None`
/// when nothing is running for that state. Checked in both registries, so a colour animation answers
/// too.
pub fn animation_target<T: Clone + 'static>(id: crate::runtime::state::StateId) -> Option<T> {
    if let Some(found) = ACTIVE_ANIMATIONS
        .lock()
        .unwrap()
        .iter()
        .find(|a| a.state_id() == id)
        .and_then(|a| a.target_value())
        .and_then(|t| t.downcast_ref::<T>())
    {
        return Some(found.clone());
    }
    ACTIVE_COLOR_ANIMATIONS
        .lock()
        .unwrap()
        .iter()
        .find(|a| a.state.state_id() == id)
        .and_then(|a| a.target_value())
        .and_then(|t| t.downcast_ref::<T>())
        .cloned()
}

/// How fast the state's animation is moving right now (0.0 when nothing runs for it) — what Compose's
/// `Animatable.stop` hands back so a fling can pick the motion up where it left off.
pub fn animation_velocity(id: crate::runtime::state::StateId) -> f32 {
    ACTIVE_ANIMATIONS
        .lock()
        .unwrap()
        .iter()
        .find(|a| a.state_id() == id)
        .map(|a| a.last_velocity())
        .or_else(|| {
            ACTIVE_COLOR_ANIMATIONS
                .lock()
                .unwrap()
                .iter()
                .find(|a| a.state.state_id() == id)
                .map(|a| a.last_velocity())
        })
        .unwrap_or(0.0)
}

/// Why a motion ended — Compose's `AnimationEndReason`, plus the one case Compose does not have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionEndReason {
    /// The motion arrived: a spring settled on its target, a decay came to rest, a tween or keyframes ran
    /// out its clock. Compose's `AnimationEndReason.Finished`.
    Finished,
    /// A value range ended it — the value was written exactly at a bound (`set_bounds`). Compose's
    /// `AnimationEndReason.BoundReached`.
    BoundReached,
    /// The engine's overrun guard cut it short: a spring or decay that had still not settled after five
    /// seconds. winia only — Compose sizes its springs by an estimated duration, so it has nothing to cut.
    Forced,
}

/// What a completion callback is handed: the reason and the velocity the motion had at that moment —
/// Compose's `AnimationResult` (`endReason` plus the end state's velocity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionEnd {
    pub reason: MotionEndReason,
    pub velocity: f32,
}

/// A cached runtime check for a tracing environment variable — `WINIA_ANIM_SIZE_TRACE=1`, say.
///
/// The per-component traces used to be `#[cfg(test)]`-gated, which meant an example could never print
/// what its own animation did: the demo binary is not built with the test cfg, so the block was not even
/// compiled in (measured — `WINIA_ANIM_SIZE_TRACE=1 cargo run --example animated_size_demo` printed
/// nothing). Reading the variable once and caching it makes the switch a runtime one while keeping the
/// check cheap enough for a per-frame layout path: an atomic load plus a name compare, measured at 25 ns
/// per call against 346 ns for the mutex-and-hash-map version this started as (1,000,000 calls each,
/// debug build).
pub fn trace_flag(name: &'static str) -> bool {
    // Each known trace variable owns an atomic, and the steady-state check is one relaxed load plus a name
    // compare: no lock, no allocation, nothing to pay for a trace that is off. `AnimatedSize` asks this on
    // every layout pass, so anything heavier here would be a per-frame cost. A new trace variable is a row
    // in the table.
    const UNKNOWN: u8 = 0;
    const OFF: u8 = 1;
    const ON: u8 = 2;
    static TRACES: [(&str, std::sync::atomic::AtomicU8); 1] =
        [("WINIA_ANIM_SIZE_TRACE", std::sync::atomic::AtomicU8::new(UNKNOWN))];
    let Some((_, cell)) = TRACES.iter().find(|(n, _)| *n == name) else {
        return false;
    };
    match cell.load(std::sync::atomic::Ordering::Relaxed) {
        ON => true,
        OFF => false,
        _ => {
            let on = std::env::var(name)
                .map(|v| !v.is_empty() && v != "0")
                .unwrap_or(false);
            cell.store(if on { ON } else { OFF }, std::sync::atomic::Ordering::Relaxed);
            on
        }
    }
}

/// Keep a state's running motion inside `[min, max]` — Compose's `Animatable.updateBounds` for a motion
/// that is already in flight. `None` clears the range. Harmless when nothing runs for the state, but a
/// motion pushed afterwards starts without bounds: `Animatable::set_bounds` is the version that carries
/// them forward.
pub fn set_animation_bounds(id: crate::runtime::state::StateId, bounds: Option<(f32, f32)>) {
    for a in ACTIVE_ANIMATIONS.lock().unwrap().iter_mut() {
        if a.state_id() == id {
            a.set_value_bounds(bounds);
        }
    }
    for a in ACTIVE_COLOR_ANIMATIONS.lock().unwrap().iter_mut() {
        if a.state.state_id() == id {
            a.set_value_bounds(bounds);
        }
    }
}

/// Everything animating right now, one line per motion — what the debug server's `al` command prints.
///
/// There is no other way to ask which animation owns a value: a value that is moving looks the same
/// whoever moves it, and the registry is the only place that knows. Each line carries the state id, the
/// spec (with its numbers), where the motion came from and what it targets, how long it has been running
/// and its current velocity, so a stuck or unexpected animation can be identified from outside the
/// process. Compose has no equivalent.
pub fn active_animations_report() -> Vec<String> {
    let mut lines: Vec<String> = ACTIVE_ANIMATIONS
        .lock()
        .unwrap()
        .iter()
        .map(|a| a.describe())
        .collect();
    lines.extend(
        ACTIVE_COLOR_ANIMATIONS
            .lock()
            .unwrap()
            .iter()
            .map(|a| a.describe()),
    );
    lines
}

/// 是否有动画在运行（用于控制事件循环 Poll/Wait）
pub fn is_animating() -> bool {
    !ACTIVE_ANIMATIONS.lock().unwrap().is_empty()
        || !ACTIVE_COLOR_ANIMATIONS.lock().unwrap().is_empty()
}

// ═══════════════════════════════════════════════════════════
// Motion — the engine's own animated value (the public handle is `Animatable`)
// ═══════════════════════════════════════════════════════════

/// One value the engine can move: the state it writes, the motion in progress, and the callbacks.
/// Renamed from `Animatable` when the public Compose-shaped handle took that name.
pub(crate) struct Motion<T: Clone + 'static> {
    state: crate::runtime::state::Animating<T>,
    anim_state: Option<AnimationState<T>>,
    /// 动画完成回调（done 帧触发一次，take 后释放）
    on_finish: Option<Box<dyn FnOnce(MotionEnd) + Send>>,
    /// fling 撞到 clamp 边界时回调一次，参数为该帧尚未消费的瞬时速度。
    on_boundary: Option<Box<dyn FnOnce(f32) + Send>>,
    /// fling 边界 clamp（Decay 专用）：每帧求值（读最新滚动极限）；
    /// 值被 clamp 改变 → 立即完成（对齐 Compose：fling 消耗完即停）
    clamp: Option<Box<dyn Fn(f32) -> f32 + Send + Sync>>,
}

struct AnimationState<T> {
    from: T,
    to: T,
    start: Instant,
    spec: AnimationSpec,
    last_velocity: f32,
    last_update: Instant,
    // Spring 持续的位移（累积值，非每帧重算）
    current_displacement: f32,
    // Decay 初始速度（v0 常数——解析式需要，不被 last_velocity 覆盖）
    initial_velocity: f32,
    /// 多分量值（`Size`/`Offset`）上跑 Spring 时，每个分量一条弹簧——Compose 的
    /// `VectorizedSpringSpec` 语义。`None` 表示单分量：走下面的标量位移
    /// （`current_displacement`），标量类型与 Tween/Keyframes 等一律走它。
    lanes: Option<Vec<SpringLane>>,
    /// A fling on a multi-component value: one exponential decay per component — Compose's
    /// `DecayAnimationSpec<T>` vectorized through the type's `VectorConverter` (each axis its own
    /// velocity and its own rest threshold; the motion ends when EVERY component is at rest).
    /// `None` means the scalar path (`initial_velocity` + `from.to_f32()`).
    decay_lanes: Option<Vec<DecayLane>>,
    /// How long a Spring is meant to take, from Compose's `estimateAnimationDurationMillis` — the clock
    /// the spec runs on. For a multi-component spring it is the longest axis, exactly as
    /// `VectorizedFloatAnimationSpec.getDurationNanos` takes the maximum. `None` for every other spec,
    /// which carries its own duration.
    spring_duration: Option<Duration>,
}

/// One component's exponential decay: where it started and the velocity it started with. The motion is
/// analytic (no accumulators), so a lane needs nothing else.
struct DecayLane {
    from: f32,
    velocity: f32,
}

/// 一条分量弹簧：目标值 + 该分量的位移与速度（标量路径的字段按分量复制一份）。
struct SpringLane {
    to: f32,
    displacement: f32,
    velocity: f32,
}

impl<T: Clone + PartialEq + AnimatableValue + 'static> Motion<T> {
    pub fn new(state: State<T>) -> Self {
        Self::from_animating(state.into_animating())
    }

    pub fn from_animating(state: crate::runtime::state::Animating<T>) -> Self {
        Self { state, anim_state: None, on_finish: None, on_boundary: None, clamp: None }
    }

    /// fling 边界 clamp（仅 Decay 生效）：值被 clamp 改变 → 写边界值并立即完成
    pub fn set_clamp(&mut self, f: impl Fn(f32) -> f32 + Send + Sync + 'static) {
        self.clamp = Some(Box::new(f));
    }

    /// Set or clear the clamp directly (the value-bounds path builds one closure, so it does not need the
    /// `impl Fn` form).
    fn set_clamp_opt(&mut self, clamp: Option<Box<dyn Fn(f32) -> f32 + Send + Sync>>) {
        self.clamp = clamp;
    }

    /// 注册动画完成回调（对标 Compose animate*AsState 的 finishedListener——
    /// 动画自然完成/超时强制完成时调用一次）
    pub fn on_finish(&mut self, f: impl FnOnce() + Send + 'static) {
        self.on_finish = Some(Box::new(move |_end| f()));
    }

    /// The same, with the outcome: why it ended and the velocity it had — Compose's `AnimationResult`.
    pub fn on_finish_with(&mut self, f: impl FnOnce(MotionEnd) + Send + 'static) {
        self.on_finish = Some(Box::new(f));
    }

    pub fn on_boundary(&mut self, f: impl FnOnce(f32) + Send + 'static) {
        self.on_boundary = Some(Box::new(f));
    }

    /// The per-component velocity to inherit on a retarget (see [`RetargetVelocity`]): component
    /// springs keep one velocity per axis, a scalar value has a single component.
    fn retarget_velocity(&self) -> RetargetVelocity {
        match self.anim_state.as_ref().and_then(|s| s.lanes.as_ref()) {
            Some(lanes) => {
                let mut velocity = RetargetVelocity::ZERO;
                velocity.len = lanes.len().min(MAX_ANIMATION_COMPONENTS);
                for (i, lane) in lanes.iter().take(velocity.len).enumerate() {
                    velocity.components[i] = lane.velocity;
                }
                velocity
            }
            None => RetargetVelocity::scalar(
                self.anim_state.as_ref().map(|s| s.last_velocity).unwrap_or(0.0),
            ),
        }
    }

    /// 启动动画到目标值
    pub fn animate_to(&mut self, to: T, spec: AnimationSpec) {
        // P2-9 速度延续：被打断的动画从当前速度继续（Compose 核心语义——
        // 弹簧弹到一半改目标，新动画继承旧速度，物理连续）。
        // Spring/Decay 每帧更新 last_velocity；Tween/Keyframes/Repeatable
        // 无速度语义恒 0——继承无影响（从静止重启）。
        let start_velocity = self.retarget_velocity();
        self.start_with_velocity(to, spec, start_velocity);
    }

    /// 内部启动入口：显式指定初始速度（`push_animatable` retarget 时从
    /// 被移除的旧动画继承；`animate_to` 从自身 anim_state 继承）。
    fn start_with_velocity(&mut self, to: T, spec: AnimationSpec, start_velocity: RetargetVelocity) {
        // review fix：只有 Spring/Decay 有物理速度语义——Tween/Keyframes/
        // Repeatable/Snap 被打断时从静止重启。否则"Spring→Tween→Spring"
        // 第二次打断会继承 Tween 期间的过期速度（Tween 不更新 last_velocity）。
        let start_velocity = match spec {
            AnimationSpec::Spring(_) | AnimationSpec::Decay(_) => start_velocity,
            _ => RetargetVelocity::ZERO,
        };
        let from = self.state.peek();
        let displacement = AnimatableValue::to_f32(&from) - AnimatableValue::to_f32(&to);
        // Spring 在多分量值（Size/Offset）上按分量推进：每轴一条弹簧、各自收敛——这样
        // 尺寸弹簧的两个轴都能落到目标，而不是共用一条沿范数运动的单值位移。
        let lanes: Option<Vec<SpringLane>> = match &spec {
            AnimationSpec::Spring(_) => {
                let mut from_components = [0.0; MAX_ANIMATION_COMPONENTS];
                let mut to_components = [0.0; MAX_ANIMATION_COMPONENTS];
                let from_len = from.write_components(&mut from_components);
                let to_len = to.write_components(&mut to_components);
                if from_len == to_len && to_len > 1 {
                    Some(
                        (0..to_len)
                            .map(|i| SpringLane {
                                to: to_components[i],
                                displacement: from_components[i] - to_components[i],
                                velocity: start_velocity.component(i),
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            }
            _ => None,
        };
        // Compose's clock for a spring: how long it needs to stay within one threshold of the target, with
        // the longest axis deciding for a multi-component value (`VectorizedFloatAnimationSpec
        // .getDurationNanos` takes the maximum over axes).
        let spring_duration = match &spec {
            AnimationSpec::Spring(spring) => {
                let seconds = match &lanes {
                    Some(lanes) => lanes
                        .iter()
                        .map(|lane| {
                            estimate_spring_duration_seconds(
                                spring.stiffness,
                                spring.damping_ratio,
                                lane.velocity,
                                lane.displacement,
                                spring.threshold,
                            )
                        })
                        .fold(0.0f32, f32::max),
                    None => estimate_spring_duration_seconds(
                        spring.stiffness,
                        spring.damping_ratio,
                        start_velocity.max_abs(),
                        displacement,
                        spring.threshold,
                    ),
                };
                Some(Duration::from_secs_f32(seconds.max(0.0)))
            }
            _ => None,
        };
        self.anim_state = Some(AnimationState {
            from: from.clone(),
            to,
            start: now(),
            spec,
            last_velocity: start_velocity.max_abs(),
            last_update: now(),
            current_displacement: displacement,
            initial_velocity: 0.0,
            lanes,
            decay_lanes: None,
            spring_duration,
        });
    }

    /// Start an exponential decay from an initial speed (fling / inertial scrolling) — no target value,
    /// it simply comes to rest. Analytic form `value(t) = from + v0/friction·(1 - e^(-friction·t))`, and
    /// it finishes once `|v0·e^(-friction·t)| < threshold`, parking on the exact limit.
    ///
    /// SCALAR ONLY: this goes through `AnimatableValue::to_f32/from_f32`, so a value with several
    /// components would collapse into one motion along its norm — usable for f32 (scroll offsets and
    /// the like). For a value of several components use `animate_decay_with_velocity`, whose initial
    /// velocity has the value's own type, exactly as Compose's
    /// `Animatable<T>.animateDecay(initialVelocity: T, …)` takes one.
    pub fn animate_decay(&mut self, initial_velocity: f32, spec: DecaySpec) {
        let from = self.state.peek();
        self.anim_state = Some(AnimationState {
            from: from.clone(),
            to: from.clone(), // Decay 无目标——占位（更新时用解析式）
            start: now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: 0.0,
            last_update: now(),
            current_displacement: 0.0,
            initial_velocity,
            lanes: None,
            decay_lanes: None,
            spring_duration: None,
        });
    }

    /// A fling on a value whose initial velocity is the value type itself — Compose's
    /// `Animatable<T>.animateDecay(initialVelocity: T, decayAnimationSpec: DecayAnimationSpec<T>)`,
    /// vectorized through the type's converter: every component decays with its own velocity and rests
    /// on its own threshold, and the motion ends when all of them are at rest. A value with a single
    /// component falls back to the scalar path (identical result, less bookkeeping).
    pub fn animate_decay_with_velocity(&mut self, velocity: T, spec: DecaySpec) {
        let from = self.state.peek();
        let mut velocities = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let v_len = velocity.write_components(&mut velocities);
        let from_len = from.write_components(&mut components);
        let decay_lanes = if v_len == from_len && v_len > 1 {
            Some(
                (0..v_len)
                    .map(|i| DecayLane { from: components[i], velocity: velocities[i] })
                    .collect(),
            )
        } else {
            None
        };
        let scalar_velocity = if decay_lanes.is_some() {
            0.0
        } else {
            AnimatableValue::to_f32(&velocity)
        };
        self.anim_state = Some(AnimationState {
            from: from.clone(),
            to: from.clone(),
            start: now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: scalar_velocity,
            last_update: now(),
            current_displacement: 0.0,
            initial_velocity: scalar_velocity,
            lanes: None,
            decay_lanes,
            spring_duration: None,
        });
    }

    /// 检查并更新动画值，返回是否还在动画中
    pub fn update(&mut self) -> bool {
        let Some(ref mut state) = self.anim_state else { return false; };
        let now = now();
        // 极端参数保护：Spring/Decay 超过 5s 未收敛强制完成——Spring 有 done
        // 阈值但渐近收敛可能永不达（stiffness=0）；Decay 的 friction 过小时
        // 速度衰减极慢（e^(-0.1·t) 需 ~94s 才低于阈值）。Tween/Keyframes/
        // Repeatable 有明确时长、Snap 立即完成——不被截断（用户设 >5s 时长合法）
        if now.saturating_duration_since(state.start) > Duration::from_secs(5) {
            let overrun = match &state.spec {
                AnimationSpec::Spring(_) => Some(state.to.clone()),
                // Decay：写当前解析值（极限可能因 friction 过小而超大——不跳极限）
                AnimationSpec::Decay(spec) => {
                    let t = now.saturating_duration_since(state.start).as_secs_f32();
                    let friction = spec.friction.max(0.001);
                    match state.decay_lanes.as_ref() {
                        Some(lanes) => {
                            // Same analytic value per component; a lane never overshoots its limit.
                            let decay = (-friction * t).exp();
                            let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                            let len = lanes.len();
                            for (i, lane) in lanes.iter().enumerate() {
                                components[i] = if lane.velocity.abs() < spec.threshold {
                                    lane.from + lane.velocity / friction
                                } else {
                                    lane.from + lane.velocity / friction * (1.0 - decay)
                                };
                            }
                            Some(T::from_components(&components[..len]))
                        }
                        None => {
                            let v0 = state.initial_velocity;
                            let decay = (-friction * t).exp();
                            Some(AnimatableValue::from_f32(
                                state.from.to_f32() + v0 / friction * (1.0 - decay),
                            ))
                        }
                    }
                }
                _ => None,
            };
            if let Some(final_val) = overrun {
                // The guard ends the motion, but a value RANGE still wins: writing a bounded motion's own
                // target past its bound is exactly what the bounds exist to prevent, and the normal write
                // path applies them below the guard's early return.
                let velocity = state.last_velocity;
                let (final_val, bound_hit) = match &self.clamp {
                    Some(clamp) => {
                        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                        let raw = AnimatableValue::to_f32(&final_val);
                        if final_val.write_components(&mut components) == 1 {
                            let clamped = clamp(raw);
                            if clamped != raw {
                                (AnimatableValue::from_f32(clamped), true)
                            } else {
                                (final_val, false)
                            }
                        } else {
                            (final_val, false)
                        }
                    }
                    None => (final_val, false),
                };
                self.state.set(final_val);
                self.anim_state = None;
                if bound_hit {
                    if let Some(f) = self.on_boundary.take() {
                        f(velocity);
                    }
                }
                if let Some(f) = self.on_finish.take() {
                    f(MotionEnd {
                        reason: if bound_hit {
                            MotionEndReason::BoundReached
                        } else {
                            MotionEndReason::Forced
                        },
                        velocity,
                    });
                }
                return false;
            }
        }
        let dt = now.saturating_duration_since(state.last_update);
        state.last_update = now;
        let mut boundary_velocity = None;
        let mut bound_hit = false;
        // Compose ends a spring on a CLOCK: `estimateAnimationDurationMillis` says how long it needs to
        // stay within `delta` of the target, and the spec runs exactly that long (`FloatSpringSpec
        // .getDurationNanos`, `animation-core/FloatAnimationSpec.kt:172-190`). Position alone, no velocity
        // term — the rule this engine used to stop on. Both are kept: whichever comes first ends the
        // motion, with the exact target written, so a spring that the integrator has already brought to
        // rest does not keep ticking, and one the estimate says is done does not linger.
        let spring_elapsed = now.saturating_duration_since(state.start);
        let spring_on_clock = state
            .spring_duration
            .is_some_and(|duration| spring_elapsed >= duration);
        let (value, done) = match &state.spec {
            AnimationSpec::Spring(spec) => {
                if state.lanes.is_some() {
                    // 多分量（Size/Offset）：每个分量一条弹簧，各自按可见阈值收敛，全部到位才
                    // 算完成——Compose 的 `VectorizedSpringSpec` 同样以"所有轴都结束"为结束。
                    // 先到位的分量写精确目标值，免得它在别的轴还在动时留下阈值内的残余位移。
                    let (components, len, all_done, last_velocity) = {
                        let lanes = state.lanes.as_mut().unwrap();
                        let mut components = [0.0; MAX_ANIMATION_COMPONENTS];
                        let mut all_done = true;
                        let mut last_velocity = 0.0f32;
                        for (i, lane) in lanes.iter_mut().enumerate() {
                            lane.displacement = compute_spring_displacement(
                                spec.stiffness, spec.damping_ratio, spec.mass,
                                lane.displacement, &mut lane.velocity, dt, spec.threshold,
                            );
                            if spring_at_rest(lane.displacement, lane.velocity, spec.threshold) {
                                // 同标量路径：done 时写精确目标值，而不是阈值内的近似值
                                components[i] = lane.to;
                            } else {
                                all_done = false;
                                components[i] = lane.to + lane.displacement;
                            }
                            if lane.velocity.abs() > last_velocity.abs() {
                                last_velocity = lane.velocity;
                            }
                        }
                        (components, lanes.len(), all_done, last_velocity)
                    };
                    // 打断时的速度延续取最快的那条分量（`push_animatable` 的 retarget 用）
                    state.last_velocity = last_velocity;
                    if all_done || spring_on_clock {
                        (state.to.clone(), true)
                    } else {
                        (T::from_components(&components[..len]), false)
                    }
                } else {
                    let to_f32 = AnimatableValue::to_f32(&state.to);
                    let displacement = compute_spring_displacement(
                        spec.stiffness, spec.damping_ratio, spec.mass,
                        state.current_displacement, &mut state.last_velocity, dt, spec.threshold,
                    );
                    state.current_displacement = displacement;
                    // 直接使用物理值，不做 lerp/clamp（避免超调截断导致抖动）
                    // One test, shared with the integrator: see `spring_at_rest`.
                    let done = spring_on_clock
                        || spring_at_rest(displacement, state.last_velocity, spec.threshold);
                    if done {
                        // Spring 渐近收敛：done 时位移只是"小于阈值"而非精确 0——
                        // 必须返回精确目标值，否则调用方（如 AnimatedVisibility 的
                        // exit 完成检测 progress<0.001）会因残余位移卡住/误判
                        (state.to.clone(), true)
                    } else {
                        let spring_val = to_f32 + displacement;
                        (AnimatableValue::from_f32(spring_val), false)
                    }
                }
            }
            AnimationSpec::Tween(spec) => {
                let elapsed = now.saturating_duration_since(state.start);
                // `delayMillis`: hold `from` until it elapses (Compose's `tween(delayMillis)`) — the
                // animation is still running, so the loop keeps ticking and nothing snaps.
                if elapsed < spec.delay {
                    (state.from.clone(), false)
                } else {
                let elapsed = elapsed - spec.delay;
                // duration=0 → 立即完成（0 时长 = 瞬移）；>0 走正常时间轴
                let t = if spec.duration.is_zero() {
                    1.0
                } else {
                    (elapsed.as_secs_f64() / spec.duration.as_secs_f64()).min(1.0) as f32
                };
                let eased = spec.interpolator.interpolate(t);
                if t >= 1.0 {
                    // 时间到：写精确目标——过冲插值器（Elastic/Back）eased 会提前
                    // >= 1.0，若用 eased 判定则动画提前结束于过冲值（停在越界位置）
                    (state.to.clone(), true)
                } else {
                    (state.from.lerp(&state.to, eased), false)
                }
                }
            }
            AnimationSpec::Keyframes(spec) => {
                let elapsed = now.saturating_duration_since(state.start);
                // duration=0 → 立即完成（同 Tween）
                let t = if spec.duration.is_zero() {
                    1.0
                } else {
                    (elapsed.as_secs_f64() / spec.duration.as_secs_f64()).min(1.0) as f32
                };
                if t >= 1.0 {
                    // 时间到：写精确目标（与 Tween/Spring done 语义一致——
                    // 末帧 progress<1.0 或末帧值过冲时插值结果可能≠to）
                    (state.to.clone(), true)
                } else {
                    let factor = interpolate_keyframes(&spec.frames, t);
                    let value = state.from.lerp(&state.to, factor);
                    (value, false)
                }
            }
            AnimationSpec::Repeatable(spec) => {
                // The base is a `DurationSpec`, so a cycle length always exists: the impossible
                // bases (spring, decay, a nested repeat) cannot be constructed at all.
                let base = &spec.base;
                let base_duration = base.duration();
                let elapsed = now.saturating_duration_since(state.start);
                let total = base_duration.saturating_mul(spec.iterations);
                // Compose's `initialStartOffset` (`repeatable(..., initialStartOffset = …)`): `Delay`
                // holds the first value until the offset has passed, `FastForward` begins that far into
                // the first cycle. `played` is the time inside the repeated motion, so the cycle maths
                // below stays as it was; a fast-forward past the whole run finishes at once, as it does
                // in Compose.
                let played = match spec.start_offset_type {
                    StartOffsetType::Delay => elapsed.saturating_sub(spec.start_offset),
                    StartOffsetType::FastForward => elapsed + spec.start_offset,
                };
                // total=0（Snap/0 时长）→ elapsed>=0 恒真 → 首次 update 即完成（Snap 语义）
                if played >= total {
                    // 完成值：Reverse + 偶数次时最后 cycle 结束于 from（否则结束于 to）
                    let end_val = match spec.mode {
                        RepeatMode::Restart => state.to.clone(),
                        RepeatMode::Reverse => {
                            if spec.iterations % 2 == 0 { state.from.clone() } else { state.to.clone() }
                        }
                    };
                    (end_val, true)
                } else {
                    let d = base_duration.as_secs_f64().max(0.001);
                    let cycle = played.as_secs_f64() % d;
                    let t = (cycle / d) as f32;
                    let cycle_idx = (played.as_secs_f64() / d).floor() as u32;
                    let factor = match spec.mode {
                        RepeatMode::Restart => t,
                        RepeatMode::Reverse => if cycle_idx % 2 == 0 { t } else { 1.0 - t },
                    };
                    // base 曲线求值（factor 反转对 Keyframes 即曲线倒放）
                    (base.value_at(&state.from, &state.to, factor), false)
                }
            }
            AnimationSpec::Snap(spec) => {
                // Compose's `snap(delayMillis)`: hold the value where it is for the delay, then jump.
                // The animation is still running during the hold (the frame loop keeps ticking) and
                // nothing is written to the state until the jump.
                if now.saturating_duration_since(state.start) < spec.delay {
                    (state.from.clone(), false)
                } else {
                    (state.to.clone(), true)
                }
            }
            AnimationSpec::Decay(spec) => {
                let elapsed = now.saturating_duration_since(state.start);
                let t = elapsed.as_secs_f32();
                let friction = spec.friction.max(0.001); // 除零保护（friction<=0 退化为极慢）
                if let Some(lanes) = state.decay_lanes.as_mut() {
                    // A fling on a multi-component value: one decay per component, exactly as Compose's
                    // `DecayAnimationSpec<T>` runs the float spec through the type's `VectorConverter`.
                    // Each component rests on its own threshold and parks on its exact limit; the motion
                    // is over once the last of them has. A boundary clamp is a scalar concept (it takes
                    // one `f32`), so it does not apply here — `push_fling` stays scalar-only.
                    let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let len = lanes.len();
                    let mut all_done = true;
                    let mut last_velocity = 0.0f32;
                    for (i, lane) in lanes.iter_mut().enumerate() {
                        let decay = (-friction * t).exp();
                        let vel_now = lane.velocity * decay;
                        components[i] = if vel_now.abs() < spec.threshold {
                            lane.from + lane.velocity / friction
                        } else {
                            all_done = false;
                            lane.from + lane.velocity / friction * (1.0 - decay)
                        };
                        if vel_now.abs() > last_velocity.abs() {
                            last_velocity = vel_now;
                        }
                    }
                    state.last_velocity = last_velocity; // for an interruption (velocity continuity, P2-9)
                    (T::from_components(&components[..len]), all_done)
                } else {
                let v0 = state.initial_velocity;
                let decay = (-friction * t).exp();
                let vel_now = v0 * decay;
                state.last_velocity = vel_now; // for an interruption (velocity continuity, P2-9)
                // 解析式极限（速度低于阈值时精确停靠）
                let raw = if vel_now.abs() < spec.threshold {
                    state.from.to_f32() + v0 / friction
                } else {
                    state.from.to_f32() + v0 / friction * (1.0 - decay)
                };
                let done = vel_now.abs() < spec.threshold;
                // The boundary clamp is applied once, at the write point below, for every spec.
                (AnimatableValue::from_f32(raw), done)
                }
            }
        };
        // Compose's `Animatable.updateBounds`: the value never leaves its range, and a motion that reaches
        // a bound ENDS there instead of running on against a value that cannot move (its `BoundReached`).
        // A clamp that changes the value therefore completes the motion, and a fling reports the velocity it
        // had left through `on_boundary`. Scalar values only: the range is compared on the scalar
        // projection, and for a multi-component value that projection is a placeholder.
        let (value, done) = match &self.clamp {
            Some(clamp) => {
                let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                let raw = AnimatableValue::to_f32(&value);
                if value.write_components(&mut components) == 1 {
                    let clamped = clamp(raw);
                    if clamped != raw {
                        boundary_velocity.get_or_insert(state.last_velocity);
                        bound_hit = true;
                        (AnimatableValue::from_f32(clamped), true)
                    } else {
                        (value, done)
                    }
                } else {
                    (value, done)
                }
            }
            None => (value, done),
        };
        self.state.set(value);
        if let Some(velocity) = boundary_velocity {
            if let Some(f) = self.on_boundary.take() {
                f(velocity);
            }
        }
        if done {
            let velocity = state.last_velocity;
            self.anim_state = None;
            if let Some(f) = self.on_finish.take() {
                f(MotionEnd {
                    reason: if bound_hit {
                        MotionEndReason::BoundReached
                    } else {
                        MotionEndReason::Finished
                    },
                    velocity,
                });
            }
        }
        !done
    }

}

// ═══════════════════════════════════════════════════════════
// Spring 物理模拟（半隐式欧拉积分）
// ═══════════════════════════════════════════════════════════

/// 关键帧插值：在 frames 中按进度 t 定位段，段内用 interpolator 插值
fn interpolate_keyframes(frames: &[(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)], t: f32) -> f32 {
    if frames.is_empty() { return 0.0; }
    if t <= 0.0 { return frames[0].1; }
    let last = frames.last().unwrap();
    if t >= last.0 { return last.1; }
    // t 小于首帧 progress 时取首帧值（首帧 progress 可能 > 0）
    if t < frames[0].0 { return frames[0].1; }
    for i in 0..frames.len() - 1 {
        let (p0, v0, _) = frames[i];
        let (p1, v1, interp) = frames[i + 1].clone();
        if t >= p0 && t <= p1 {
            let seg = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
            let eased = interp.interpolate(seg.clamp(0.0, 1.0));
            return v0 + (v1 - v0) * eased;
        }
    }
    last.1
}

/// Compose's `estimateAnimationDurationMillis` (`animation-core/SpringEstimation.kt:36-89`): how long a
/// spring needs before it stays within `delta` of its target, in SECONDS. Ported number for number.
///
/// Compose calls it with the displacement and velocity expressed in thresholds (`delta = 1`), which is why
/// winia's `SpringSpec::threshold` divides both. It is a POSITION-only estimate: it never asks how fast the
/// spring is still moving, which is the rule this engine used to stop on.
fn estimate_spring_duration_seconds(
    stiffness: f32,
    damping_ratio: f32,
    initial_velocity: f32,
    initial_displacement: f32,
    delta: f32,
) -> f32 {
    if damping_ratio == 0.0 {
        // No damping: it never settles. (Compose returns MAX_LONG_MILLIS.)
        return f32::INFINITY;
    }
    let stiffness = stiffness as f64;
    let zeta = damping_ratio as f64;
    let damping_coefficient = 2.0 * zeta * stiffness.sqrt();
    // Roots of x^2 + damping_coefficient*x + stiffness = 0, which may be complex.
    let partial_root = damping_coefficient * damping_coefficient - 4.0 * stiffness;
    let (partial_real, partial_imaginary) = if partial_root < 0.0 {
        (0.0, (-partial_root).sqrt())
    } else {
        (partial_root.sqrt(), 0.0)
    };
    let first_root_real = (-damping_coefficient + partial_real) * 0.5;
    let first_root_imaginary = partial_imaginary * 0.5;
    let second_root_real = (-damping_coefficient - partial_real) * 0.5;
    estimate_duration_seconds(
        zeta,
        first_root_real,
        first_root_imaginary,
        second_root_real,
        initial_velocity as f64,
        initial_displacement as f64,
        delta as f64,
    ) as f32
}

fn estimate_duration_seconds(
    damping_ratio: f64,
    first_root_real: f64,
    first_root_imaginary: f64,
    second_root_real: f64,
    initial_velocity: f64,
    initial_position: f64,
    delta: f64,
) -> f64 {
    if initial_position == 0.0 && initial_velocity == 0.0 {
        return 0.0;
    }
    let v0 = if initial_position < 0.0 { -initial_velocity } else { initial_velocity };
    let p0 = initial_position.abs();
    if damping_ratio > 1.0 {
        estimate_over_damped(first_root_real, second_root_real, p0, v0, delta)
    } else if damping_ratio < 1.0 {
        estimate_under_damped(first_root_real, first_root_imaginary, p0, v0, delta)
    } else {
        estimate_critically_damped(first_root_real, p0, v0, delta)
    }
}

/// Under-damped: the general solution is `c·e^(r·t)·cos(…)`, so `c·e^(r·t)` is its envelope and the time
/// the envelope reaches `delta` is the estimate.
fn estimate_under_damped(r: f64, r_imaginary: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c1 = p0;
    let c2 = (v0 - r * c1) / r_imaginary;
    let c = (c1 * c1 + c2 * c2).sqrt();
    (delta / c).ln() / r
}

/// Critically damped: `x(t) = (c1 + c2·t)·e^(r·t)`, solved by Newton–Raphson. The inflection argument is
/// Compose's: it decides whether the first or the last crossing of the threshold is the one that matters.
fn estimate_critically_damped(r: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c1 = p0;
    let c2 = v0 - r * c1;
    let t1 = (delta / c1).abs().ln() / r;
    let t2 = {
        // Lambert's W function, applied to `t·e^t`.
        let guess = (delta / c2).abs().ln();
        let mut t = guess;
        for _ in 0..5 {
            t = guess - (t / r).abs().ln();
        }
        t / r
    };
    let mut t_curr = match (t1.is_finite(), t2.is_finite()) {
        (false, _) => t2,
        (_, false) => t1,
        _ => t1.max(t2),
    };
    let t_inflection = -(r * c1 + c2) / (r * c2);
    let x_inflection = c1 * (r * t_inflection).exp() + c2 * t_inflection * (r * t_inflection).exp();
    let signed_delta = if t_inflection.is_nan() || t_inflection <= 0.0 {
        -delta
    } else if -x_inflection < delta {
        if c2 < 0.0 && c1 > 0.0 {
            t_curr = 0.0;
        }
        -delta
    } else {
        t_curr = -(2.0 / r) - (c1 / c2);
        delta
    };
    let mut t_delta = f64::MAX;
    let mut iterations = 0;
    while t_delta > 0.001 && iterations < 100 {
        iterations += 1;
        let t_last = t_curr;
        t_curr = newton_step(
            t_curr,
            |t| (c1 + c2 * t) * (r * t).exp() + signed_delta,
            |t| (c2 * (r * t + 1.0) + c1 * r) * (r * t).exp(),
        );
        t_delta = (t_last - t_curr).abs();
    }
    t_curr
}

/// Over-damped: `x(t) = c1·e^(r1·t) + c2·e^(r2·t)`, same Newton–Raphson treatment.
fn estimate_over_damped(r1: f64, r2: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c2 = (r1 * p0 - v0) / (r1 - r2);
    let c1 = p0 - c2;
    let t1 = (delta / c1).abs().ln() / r1;
    let t2 = (delta / c2).abs().ln() / r2;
    let mut t_curr = match (t1.is_finite(), t2.is_finite()) {
        (false, _) => t2,
        (_, false) => t1,
        _ => t1.max(t2),
    };
    let t_inflection = ((c1 * r1) / (-c2 * r2)).ln() / (r2 - r1);
    let x_inflection = c1 * (r1 * t_inflection).exp() + c2 * (r2 * t_inflection).exp();
    let signed_delta = if t_inflection.is_nan() || t_inflection <= 0.0 {
        -delta
    } else if -x_inflection < delta {
        if c2 > 0.0 && c1 < 0.0 {
            t_curr = 0.0;
        }
        -delta
    } else {
        t_curr = (-(c2 * r2 * r2) / (c1 * r1 * r1)).ln() / (r1 - r2);
        delta
    };
    if (c1 * r1 * (r1 * t_curr).exp() + c2 * r2 * (r2 * t_curr).exp()).abs() < 0.0001 {
        return t_curr;
    }
    let mut t_delta = f64::MAX;
    let mut iterations = 0;
    while t_delta > 0.001 && iterations < 100 {
        iterations += 1;
        let t_last = t_curr;
        t_curr = newton_step(
            t_curr,
            |t| c1 * (r1 * t).exp() + c2 * (r2 * t).exp() + signed_delta,
            |t| c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp(),
        );
        t_delta = (t_last - t_curr).abs();
    }
    t_curr
}

fn newton_step(x: f64, f: impl Fn(f64) -> f64, f_prime: impl Fn(f64) -> f64) -> f64 {
    x - f(x) / f_prime(x)
}

/// The interpolation factor a loop is at — Compose's `VectorizedInfiniteRepeatableSpec.getValueFromNanos`:
/// `Restart` plays the base curve forward in every iteration, and `Reverse` mirrors the iteration TIME on the
/// odd ones (`if (iteration % 2 != 0) durationNanos - iterationTime`) and evaluates the base there. Mirroring
/// the time rather than the value is what makes the return leg the same curve running backwards; interpolating
/// the return leg linearly — what winia did — is a different motion for any eased base.
fn loop_factor(base: Option<&DurationSpec>, mode: RepeatMode, cycle: u32, phase: f32) -> f32 {
    let forward = match mode {
        RepeatMode::Restart => phase,
        RepeatMode::Reverse => {
            if cycle % 2 == 0 {
                phase
            } else {
                1.0 - phase
            }
        }
    };
    match base {
        Some(base) => base.factor(forward),
        None => forward,
    }
}

/// A motion that can be asked for its value at ANY time — Compose's `TargetBasedAnimation` /
/// `DecayAnimation` and their `getValueFromNanos`.
///
/// Nothing is registered and nothing ticks: this is the same evaluation the engine performs each frame,
/// exposed as a function of time. What needs it is a gesture hand-off — freeze a motion in flight, let the
/// pointer take over, then start the remainder from where it was — and any caller that wants to ask what a
/// spec does at 137 ms without driving frames.
///
/// A spring is integrated from its start for the time asked, with the engine's own stepper, so a value here
/// matches what a running animation would have shown; each query therefore costs O(steps) rather than a
/// table lookup.
pub struct TimedAnimation<T: AnimatableValue> {
    from: T,
    /// `None` for a fling: there is no target, only the decay's own limit.
    to: Option<T>,
    spec: AnimationSpec,
    /// The starting velocity in the value's own units per second (a fling's `initial_velocity` included).
    initial_velocity: f32,
    /// The same per component, for a value that has several.
    initial_velocity_components: [f32; MAX_ANIMATION_COMPONENTS],
    components: usize,
}

impl<T: AnimatableValue + Clone + 'static> TimedAnimation<T> {
    /// A motion from `from` to `to` under any spec — Compose's `TargetBasedAnimation(animationSpec,
    /// converter, initialValue, targetValue, initialVelocity)`.
    pub fn target(from: T, to: T, spec: impl Into<AnimationSpec>) -> Self {
        Self::with_velocity(from, to, spec.into(), 0.0)
    }

    /// The same with an initial speed (units per second), which a hand-off needs: the remainder of a motion
    /// that was already moving.
    pub fn with_velocity(from: T, to: T, spec: AnimationSpec, initial_velocity: f32) -> Self {
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let components = from.write_components(&mut components);
        Self {
            from,
            to: Some(to),
            spec,
            initial_velocity,
            initial_velocity_components: [0.0; MAX_ANIMATION_COMPONENTS],
            components,
        }
    }

    /// A fling with no target — Compose's `DecayAnimation(decayAnimationSpec, converter, initialValue,
    /// initialVelocity)`. The velocity has the value's own type, so a vector flings on every component.
    pub fn fling(from: T, velocity: T, spec: DecaySpec) -> Self {
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let len = velocity.write_components(&mut components);
        Self {
            from,
            to: None,
            spec: AnimationSpec::Decay(spec),
            initial_velocity: AnimatableValue::to_f32(&velocity),
            initial_velocity_components: components,
            components: len,
        }
    }

    /// How long the motion lasts — Compose's `Animation.durationNanos`. A spring's answer is the estimate
    /// the engine itself now runs on, a fling's is the time its velocity takes to fall under the threshold,
    /// and a `Repeatable`'s is its iterations times its base.
    pub fn duration(&self) -> Duration {
        match &self.spec {
            AnimationSpec::Spring(spec) => Duration::from_secs_f32(
                estimate_spring_duration_seconds(
                    spec.stiffness,
                    spec.damping_ratio,
                    self.initial_velocity,
                    self.displacement(),
                    spec.threshold,
                )
                .max(0.0),
            ),
            AnimationSpec::Tween(spec) => spec.delay + spec.duration,
            AnimationSpec::Snap(spec) => spec.delay,
            AnimationSpec::Keyframes(spec) => spec.duration,
            AnimationSpec::Repeatable(spec) => {
                base_duration_of(&spec.base).saturating_mul(spec.iterations)
            }
            AnimationSpec::Decay(spec) => {
                let friction = spec.friction.max(0.001);
                let v0 = self.initial_velocity.abs().max(1e-6);
                // |v0·e^(-friction·t)| = threshold
                Duration::from_secs_f32((v0 / spec.threshold).ln() / friction)
            }
        }
    }

    /// The value at `elapsed` — Compose's `getValueFromNanos`.
    pub fn value_at(&self, elapsed: Duration) -> T {
        match &self.spec {
            AnimationSpec::Spring(spec) => {
                let target = self.target_of();
                if elapsed >= self.duration() {
                    // The clock the engine runs on: at the estimated end it writes the exact target.
                    return target;
                }
                if self.components > 1 {
                    let mut from_components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let mut to_components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let len = self.from.write_components(&mut from_components);
                    target.write_components(&mut to_components);
                    for (i, slot) in to_components.iter_mut().take(len).enumerate() {
                        let mut velocity = self.initial_velocity_components[i];
                        let displacement = compute_spring_displacement_with_step(
                            spec.stiffness,
                            spec.damping_ratio,
                            spec.mass,
                            from_components[i] - *slot,
                            &mut velocity,
                            elapsed,
                            spec.threshold,
                            1.0 / 480.0,
                        );
                        *slot += displacement;
                    }
                    return T::from_components(&to_components[..len]);
                }
                let mut velocity = self.initial_velocity;
                let displacement = compute_spring_displacement_with_step(
                    spec.stiffness,
                    spec.damping_ratio,
                    spec.mass,
                    self.displacement(),
                    &mut velocity,
                    elapsed,
                    spec.threshold,
                    1.0 / 480.0,
                );
                AnimatableValue::from_f32(AnimatableValue::to_f32(&target) + displacement)
            }
            AnimationSpec::Tween(spec) => {
                let target = self.target_of();
                if elapsed < spec.delay {
                    return self.from.clone();
                }
                let t = fraction_of(elapsed - spec.delay, spec.duration);
                if t >= 1.0 {
                    return target;
                }
                self.from.lerp(&target, spec.interpolator.interpolate(t))
            }
            AnimationSpec::Snap(spec) => {
                let target = self.target_of();
                if elapsed < spec.delay {
                    self.from.clone()
                } else {
                    target
                }
            }
            AnimationSpec::Keyframes(spec) => {
                let target = self.target_of();
                let t = fraction_of(elapsed, spec.duration);
                if t >= 1.0 {
                    return target;
                }
                self.from.lerp(&target, interpolate_keyframes(&spec.frames, t))
            }
            AnimationSpec::Repeatable(spec) => {
                let target = self.target_of();
                let base = base_duration_of(&spec.base);
                let total = base.saturating_mul(spec.iterations);
                if base.is_zero() || elapsed >= total {
                    return target;
                }
                let played = match spec.start_offset_type {
                    StartOffsetType::Delay => elapsed.saturating_sub(spec.start_offset),
                    StartOffsetType::FastForward => elapsed + spec.start_offset,
                };
                let cycle = (played.as_secs_f32() / base.as_secs_f32()) as u32;
                let phase = (played.as_secs_f32() % base.as_secs_f32()) / base.as_secs_f32();
                let eased = loop_factor(Some(&spec.base), spec.mode, cycle, phase);
                self.from.lerp(&target, eased)
            }
            AnimationSpec::Decay(spec) => {
                let friction = spec.friction.max(0.001);
                let t = elapsed.as_secs_f32();
                let decay = (-friction * t).exp();
                if self.components > 1 {
                    let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let len = self.from.write_components(&mut components);
                    for (i, slot) in components.iter_mut().take(len).enumerate() {
                        let v0 = self.initial_velocity_components[i];
                        *slot += if (v0 * decay).abs() < spec.threshold {
                            v0 / friction
                        } else {
                            v0 / friction * (1.0 - decay)
                        };
                    }
                    return T::from_components(&components[..len]);
                }
                let v0 = self.initial_velocity;
                let raw = if (v0 * decay).abs() < spec.threshold {
                    AnimatableValue::to_f32(&self.from) + v0 / friction
                } else {
                    AnimatableValue::to_f32(&self.from) + v0 / friction * (1.0 - decay)
                };
                AnimatableValue::from_f32(raw)
            }
        }
    }

    /// How fast the value moves at `elapsed`, in the value's own units per second. A spring and a fling
    /// answer from the physics; everything else answers by stepping 1 ms and measuring, which is what a
    /// caller holding an easing curve but no derivative can do. Compose returns a vector from
    /// `getVelocityVectorFromNanos`; this is its scalar projection.
    pub fn velocity_at(&self, elapsed: Duration) -> f32 {
        match &self.spec {
            AnimationSpec::Spring(spec) => {
                let mut velocity = self.initial_velocity;
                compute_spring_displacement_with_step(
                    spec.stiffness,
                    spec.damping_ratio,
                    spec.mass,
                    self.displacement(),
                    &mut velocity,
                    elapsed,
                    spec.threshold,
                    1.0 / 480.0,
                );
                velocity
            }
            AnimationSpec::Decay(spec) => {
                self.initial_velocity * (-spec.friction.max(0.001) * elapsed.as_secs_f32()).exp()
            }
            _ => {
                let step = Duration::from_millis(1);
                let a = self.value_at(elapsed);
                let b = self.value_at(elapsed + step);
                (AnimatableValue::to_f32(&b) - AnimatableValue::to_f32(&a)) / step.as_secs_f32()
            }
        }
    }

    /// The displacement the motion starts with (value minus target), which every spring calculation is
    /// expressed in.
    fn displacement(&self) -> f32 {
        match &self.to {
            Some(to) => AnimatableValue::to_f32(&self.from) - AnimatableValue::to_f32(to),
            None => 0.0,
        }
    }

    fn target_of(&self) -> T {
        self.to.clone().expect("this spec has a target")
    }
}

/// A `Repeatable`'s cycle length. The type system already rules out a base without one
/// (`DurationSpec`), so there is no fallback arm left to get wrong in release builds.
fn base_duration_of(base: &DurationSpec) -> Duration {
    base.duration()
}

/// The fraction of a duration that `elapsed` represents, clamped: 1.0 for a zero-length spec (a value that
/// arrives at once) and for anything past the end.
fn fraction_of(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0) as f32
    }
}

/// Whether a spring has come to rest — the ONE place this is decided, because the integrator and
/// `Motion::update` both need it and must never disagree.
///
/// **The two sides are not in the same unit, and that is deliberate.** The displacement is in the
/// threshold's unit; the velocity is in that unit per SECOND, so `velocity.abs() < threshold` reads as
/// "slower than a threshold per second" rather than "within a threshold". The second half is what makes
/// the animation end at a TURNING POINT of the oscillation — the first moment the amplitude has decayed
/// under the threshold — and that is what lets a bouncy spring overshoot before it settles: through most
/// of the flight, passing through the threshold band means the spring is still moving fast.
///
/// Scaling the velocity into frames (`velocity * FRAME < threshold`) reads better and is WRONG: it fires
/// on that first passage, so the bounce is cut. Measured — `components::shared_transition`'s
/// `tier0_bouncy_spring_overshoot_renders_then_settles` (damping 0.6, threshold 0.1 on a 0..1 progress)
/// went from "overshoots past 1.0 and then settles" to "settles at 0.886", i.e. the flight ended while
/// the spring was still ~11 % short of its target. Scaling it the other way (`velocity * FRAME <
/// threshold * FRAME`) is exactly the code below.
///
/// Compose's own clock is now the primary rule and this test is the backstop. Compose sizes a spring with
/// `estimateAnimationDurationMillis` — how long the analytic solution needs to come within one
/// `visibilityThreshold` of the target and stay there (`animation-core/SpringEstimation.kt:36-89`) — and runs
/// the spec for exactly that long (`FloatSpringSpec.getDurationNanos`,
/// `animation-core/FloatAnimationSpec.kt:172-190`). `Motion` computes the same estimate when a spring starts
/// and ends the motion on it, so a spring here stops when Compose's does. This test still ends a spring the
/// integrator has already brought to rest, which keeps a pathological parameter (an estimate that never
/// converges) from ticking forever.
///
/// One measured difference remains, and it is in the VALUE rather than the time: this engine integrates the
/// spring in 1/60 s steps instead of evaluating the analytic solution, so at the estimated end the integrator
/// is a fraction of a pixel short, and writing the exact target there is a step of that size. Measured on a
/// 0 → 100 move with stiffness 400 and a 1 px threshold: the clock ends it at 0.335 s against an estimate of
/// 0.3319 s with the value at 98.42, i.e. a 1.6 px step, where the analytic value at that moment would sit
/// inside the 1 px band. With ONLY this velocity test — the rule before the clock arrived — the same spring
/// ran 0.556 s, i.e. 224 ms past Compose's finish.
#[inline]
fn spring_at_rest(displacement: f32, velocity: f32, threshold: f32) -> bool {
    displacement.abs() < threshold && velocity.abs() < threshold
}

/// The spring integrator: fixed steps inside one frame's time (accumulator style).
fn compute_spring_displacement(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    elapsed: Duration, threshold: f32,
) -> f32 {
    compute_spring_displacement_with_step(
        stiffness,
        damping_ratio,
        mass,
        initial_displacement,
        velocity,
        elapsed,
        threshold,
        1.0 / 60.0,
    )
}

/// The integrator with its ordinary-frame step spelled out. [`TimedAnimation`] asks with a shorter one: a
/// query has no frame to keep up with, and a coarser step than a running animation uses shows up as a gap
/// between the two. Measured on a 100-unit stiffness-200 spring, worst gap against a running motion: 7.7 px
/// at the 1/60 s frame step, 1.7 px at 1/240 s, under 1.5 px at 1/480 s — the query's own integration error,
/// not a difference in what is being computed.
fn compute_spring_displacement_with_step(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    elapsed: Duration, threshold: f32,
    frame_cap: f32,
) -> f32 {
    /// The longest step an ordinary frame uses (1/60 s).
    const FRAME_CAP: f32 = 1.0 / 60.0;
    /// Semi-implicit Euler is only stable while the step stays well under the spring's own period, so the
    /// step is bounded by the spring (`1/(4·omega0)` = a sixteenth of a period) instead of a fixed 1/60 s:
    /// a soft spring keeps the frame cap, a very stiff one gets a proportionally shorter step.
    /// A frame can be arbitrarily long (a stalled loop, a resize, a suspended app). Integrating all of it
    /// keeps a spring on the same clock as a Tween, which is what Compose's time-based springs do — the cap
    /// only stops a pathological gap from costing unbounded work in one frame. `MAX_STEPS` at the bound
    /// covers ~33 s of a soft spring and ~5 s of a very stiff one; beyond that the remainders are dropped,
    /// which is the one case where a spring can still fall behind.
    const MAX_STEPS: u32 = 2000;
    let omega0 = (stiffness / mass.max(f32::EPSILON)).sqrt();
    let step_cap = if omega0 > 0.0 {
        (1.0 / (4.0 * omega0)).min(frame_cap)
    } else {
        frame_cap
    };
    let mut total_dt = elapsed.as_secs_f32().min(step_cap * MAX_STEPS as f32);
    let mut displacement = initial_displacement;
    let damping_coeff = damping_ratio * 2.0 * omega0 * mass;
    while total_dt > 0.0 {
        let step = total_dt.min(step_cap);
        let force = -stiffness * displacement - damping_coeff * *velocity;
        *velocity += force / mass.max(f32::EPSILON) * step;
        displacement += *velocity * step;
        total_dt -= step;
    }
    if spring_at_rest(displacement, *velocity, threshold) {
        *velocity = 0.0;
        return 0.0;
    }
    displacement
}

// ═══════════════════════════════════════════════════════════
// Animatable — the handle a component drives by hand
// ═══════════════════════════════════════════════════════════

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
    state: State<T>,
    /// The range this value may live in, kept on the slot so every motion started later inherits it.
    bounds: crate::runtime::state::Backchannel<Option<(f32, f32)>>,
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

// ═══════════════════════════════════════════════════════════
// updateTransition
// ═══════════════════════════════════════════════════════════

use crate::runtime::composer::ComposeCtx;

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

    /// 泛型值动画（对标 Compose `TransitionScope.animateValue`）——任意
    /// AnimatableValue 类型，target 变化 → 自动平滑过渡到新目标值。
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

    /// animateFloat — 浮点值动画（对标 Compose `TransitionScope.animateFloat`）
    pub fn animate_float(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> f32,
        label: &'static str,
    ) -> State<f32> {
        self.animate(ctx, target_fn, label)
    }

    /// animateColor — 颜色动画（CAM16-UCS 插值，对标 `animateColor`）
    pub fn animate_color(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::graphics::Color,
        label: &'static str,
    ) -> State<crate::graphics::Color> {
        self.animate(ctx, target_fn, label)
    }

    /// animateDp — Dp 值动画（对标 `animateDp`）
    pub fn animate_dp(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Dp,
        label: &'static str,
    ) -> State<crate::unit::Dp> {
        self.animate(ctx, target_fn, label)
    }

    /// animateSize — Size 值动画（对标 `animateSize`）
    pub fn animate_size(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> crate::unit::Size,
        label: &'static str,
    ) -> State<crate::unit::Size> {
        self.animate(ctx, target_fn, label)
    }

    /// animateOffset — Offset 值动画（对标 `animateOffset`）
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

// ═══════════════════════════════════════════════════════════
// InfiniteTransition — 无限循环动画
// ═══════════════════════════════════════════════════════════

/// 无限循环动画作用域：记录其创建的动画 state_id，可 dispose 统一移除
pub struct InfiniteTransition {
    ids: std::sync::Arc<std::sync::Mutex<Vec<StateId>>>,
}

impl ComposeCtx<'_> {
    /// rememberInfiniteTransition — 创建无限循环动画作用域。
    ///
    /// 生命周期绑定组合点：组合点被移除时自动 dispose（on_remove 触发——
    /// 从动画全局表移除，防泄漏/每帧空转）。显式 `dispose()` 仍可用，
    /// 双重触发安全（ids drain 幂等）。
    pub fn remember_infinite_transition(&mut self) -> InfiniteTransition {
        let ids_state = self.remember(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        // `.peek()`: the slot is a container whose contents are pushed through the mutex; the State
        // itself is never written, so a tracked read could only add a dead dependency edge.
        let ids = ids_state.peek();
        // 自动清理：组合点移除 → 从全局表移除本作用域全部动画。
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
    /// 记录一个 state id，重复 id 只保留一份（组件重组会反复调用 animate_*）。
    fn push_id(&self, id: StateId) {
        let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    /// 注册一个 from→to 无限循环浮点动画
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

    /// 注册一个保留当前相位的无限浮点动画。
    ///
    /// 与 [`animate_float`](Self::animate_float) 不同：如果该 state 之前已运行过、
    /// 被移除后再次注册，会从**当前值**继续，而不是重置回 `default_from`。
    /// 用于 wave offset 这类“相位必须连续”的循环动画。
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

    /// 注册一个 from→to 无限循环颜色动画（CAM16-UCS 插值）
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

    /// 取消此作用域创建的所有动画（组件离开组合/不再需要时手动调用）
    pub fn dispose(&self) {
        let ids: Vec<StateId> = self.ids.lock().unwrap().drain(..).collect();
        for sid in ids {
            crate::animation::remove_animation_by_state(sid);
        }
    }
}


// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════


#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod repeated_tests;
#[cfg(test)]
mod lifecycle_tests;

mod spec;

pub use spec::*;
