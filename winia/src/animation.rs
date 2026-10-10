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
}

static ACTIVE_ANIMATIONS: LazyLock<Mutex<Vec<Box<dyn AnimationInstance>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
/// Color 动画列表（与 f32 动画分开，避免类型擦除）
static ACTIVE_COLOR_ANIMATIONS: LazyLock<Mutex<Vec<Animatable<crate::graphics::Color>>>> =
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
    pub(crate) base: Option<AnimationSpec>,
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
            base: Some(AnimationSpec::Tween(tween)),
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
            base: Some(AnimationSpec::Keyframes(KeyframesSpec { duration, frames })),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
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
        let elapsed = self.start.elapsed();
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
                let t = phase;
                // 曲线取值：Tween/Keyframes base 时用曲线进度（对标 Compose
                // infiniteRepeatable(keyframes{...})——周期内按关键帧插值）；None = 线性
                let eased = match &self.spec.base {
                    Some(AnimationSpec::Tween(tw)) => tw.interpolator.interpolate(t),
                    Some(AnimationSpec::Keyframes(kf)) => interpolate_keyframes(&kf.frames, t),
                    _ => t,
                };
                self.state.set(self.from.lerp(&self.to, eased));
            }
            RepeatMode::Reverse => {
                // 周期 = 2×duration：前半 from→to，后半 to→from
                let phase = (elapsed.as_secs_f32() % (period * 2.0)) / period;
                let t = if phase < 1.0 { phase } else { 2.0 - phase };
                self.state.set(self.from.lerp(&self.to, t));
            }
        }
        true // 永远运行
    }
    fn same_target(&self, _target: &dyn std::any::Any) -> bool { false }
    fn state_id(&self) -> StateId { self.state.state_id() }
    fn last_velocity(&self) -> f32 { 0.0 } // 无限循环无速度延续语义
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
    let anim = Infinite { state: visual, from, to, spec, start: Instant::now() };
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
}

/// 注册一个动画到全局活跃列表
/// 注册一个 Animatable<T> 到全局活跃列表（由 animate_*_as_state 调用）
pub fn push_animatable<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(state: State<T>, target: T, spec: AnimationSpec) {
    push_animatable_handle(state.into_animating(), target, spec);
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
    let spec = normalise_multi_component_decay(spec, &handle.peek());
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
    let mut anim = Animatable::from_animating(handle);
    anim.start_with_velocity(target, spec, RetargetVelocity::scalar(velocity));
    anim.on_finish(done);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    crate::runtime::state::wake_loop();
}

/// `push_animatable` 的 Animating-handle 入口（调用方已持有 Animating
/// 时避免 State round-trip；语义与 `push_animatable` 完全一致）。
pub fn push_animatable_handle<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(state: crate::runtime::state::Animating<T>, target: T, spec: AnimationSpec) {
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
        if !conflicting {
            return;
        }
        // 取消旧动画后继续走注册路径：零位移动画会立即完成并精确写回 target，
        // 避免“取消后直接返回”在极端时序下残留中间值/1 帧回弹。
        cancel_animation_by_id(sid);
    }
    let inherited_velocity: RetargetVelocity;
    let spec = normalise_multi_component_decay(spec, &state.peek());
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
        // 同 state 同目标运行中 → 跳过（防每帧重启/双驱动）
        if list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) { return; }
        // 同 state 不同目标 → 移除旧动画（用户中途改目标——旧动画继续会与
        // 新目标竞争，导致值卡在旧目标路径上）；P2-9：移除前继承旧速度
        // （Spring/Decay 被打断时新动画从当前速度继续，物理连续）
        // Each component inherits its own velocity — see `RetargetVelocity`.
        inherited_velocity = list.iter().find(|a| a.state_id() == sid)
            .map(|a| a.retarget_velocity()).unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state_id() != sid);
    } // 锁释放，下面 anim.update() 不持锁执行用户代码
    let mut anim = Animatable::from_animating(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    // 立即执行首次更新，避免等下一帧 flash
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    // 唤醒事件循环启动推进轮次（渲染中注册——渲染后 Wait 休眠会卡住动画）
    crate::runtime::state::wake_loop();
}

/// A `Decay` reaching the TARGET-based entry points (`push_animatable*`, which carry a target and no
/// velocity) on a value with several components becomes `Snap`: this engine's target-based decay branch
/// works on one scalar (`state.from.to_f32() + v0/friction*(…)`) and rebuilds the value through
/// `AnimatableValue::from_f32`, which for `Size`, `Offset` and `Color` is a placeholder — a square, or
/// (measured, before this guard) an opaque GREY at the old Oklab lightness, written on the first frame
/// because the push paths supply a zero initial velocity so the decay reports itself finished at once.
///
/// A real vector fling takes a velocity OF THE VALUE'S OWN TYPE, and that path exists:
/// `push_decay_vector(state, Offset::new(100.0, -60.0), spec)` (or
/// `Animatable::animate_decay_with_velocity`) gives every component its own decay, the way Compose's
/// `Animatable<T>.animateDecay(initialVelocity: T, decayAnimationSpec: DecayAnimationSpec<T>)` does.
/// Scalar decays (the fling/deceleration path) are untouched.
fn normalise_multi_component_decay<T: AnimatableValue>(spec: AnimationSpec, value: &T) -> AnimationSpec {
    if matches!(spec, AnimationSpec::Decay(_)) {
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        if value.write_components(&mut components) > 1 {
            return AnimationSpec::Snap(SnapSpec::default());
        }
    }
    spec
}

/// 注册一个 Animatable<Color> 到全局活跃列表（由 animate_color_as_state 调用）
///
/// The spec is honoured as written, springs included: a `Color` is a four-component value (Oklab
/// `alpha, L, a, b`), so `Animatable::start_with_velocity` gives it one spring per component, the way
/// Compose's `Color.VectorConverter` does. Until this, a colour spring was silently replaced by a
/// 300 ms linear tween — the caller asked for a spring and got a different motion (measured on
/// `Tab`'s content colour, whose `SpringSpec` never ran). A `Decay` is the one spec that still cannot
/// run here: see `normalise_multi_component_decay`.
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
    let spec = normalise_multi_component_decay(spec, &state.peek());
    let mut anim = Animatable::new(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    anim.update();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().push(anim);
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
#[derive(Clone, Debug)]
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
    let mut anim = Animatable::new(state);
    anim.animate_decay(initial_velocity, spec);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
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
    let mut anim = Animatable::new(state);
    anim.animate_decay_with_velocity(velocity, spec);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
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
    let mut anim = Animatable::new(state);
    anim.set_clamp(clamp);
    if let Some(on_boundary) = on_boundary {
        anim.on_boundary(on_boundary);
    }
    anim.on_finish(on_finish);
    anim.animate_decay(initial_velocity, spec);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
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
/// 注意：动画未完成时再次 push 同 state（retarget）会移除旧动画——
/// **旧回调被静默丢弃且不调用**（新动画持有新回调）。需要"每次动画完成都触发"
/// 的语义请用 `Animatable::on_finish` 实例级管理，或自行在回调里重新注册。
pub fn push_animatable_with_done<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce() + Send + 'static,
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
        if !has_conflicting_animation(sid, &target) {
            return;
        }
        cancel_animation_by_id(sid);
    }
    // Same preparation as `push_animatable_handle`: a multi-component value cannot decay, and a motion
    // replacing a running one inherits its velocity so a retarget stays continuous.
    let inherited_velocity: RetargetVelocity;
    let spec = normalise_multi_component_decay(spec, &state.peek());
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
        if list.iter().any(|anim| anim.state_id() == sid && anim.same_target(&target)) { return; }
        inherited_velocity = list
            .iter()
            .find(|a| a.state_id() == sid)
            .map(|a| a.retarget_velocity())
            .unwrap_or(RetargetVelocity::ZERO);
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Animatable::new(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    anim.on_finish(done);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    crate::runtime::state::wake_loop();
}

/// 实现 AnimationInstance for Animatable<f32>
impl<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static> AnimationInstance for Animatable<T> {
    fn update(&mut self) -> bool {
        Animatable::update(self)
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
        Animatable::retarget_velocity(self)
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

/// 是否有动画在运行（用于控制事件循环 Poll/Wait）
pub fn is_animating() -> bool {
    !ACTIVE_ANIMATIONS.lock().unwrap().is_empty()
        || !ACTIVE_COLOR_ANIMATIONS.lock().unwrap().is_empty()
}

// ═══════════════════════════════════════════════════════════
// Animatable — 底层动画值（对标 Compose Animatable）
// ═══════════════════════════════════════════════════════════

/// 可动画化的单一值
pub(crate) struct Animatable<T: Clone + 'static> {
    state: crate::runtime::state::Animating<T>,
    anim_state: Option<AnimationState<T>>,
    /// 动画完成回调（done 帧触发一次，take 后释放）
    on_finish: Option<Box<dyn FnOnce() + Send>>,
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

impl<T: Clone + PartialEq + AnimatableValue + 'static> Animatable<T> {
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

    /// 注册动画完成回调（对标 Compose animate*AsState 的 finishedListener——
    /// 动画自然完成/超时强制完成时调用一次）
    pub fn on_finish(&mut self, f: impl FnOnce() + Send + 'static) {
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
        let lanes = match &spec {
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
        self.anim_state = Some(AnimationState {
            from: from.clone(),
            to,
            start: Instant::now(),
            spec,
            last_velocity: start_velocity.max_abs(),
            last_update: Instant::now(),
            current_displacement: displacement,
            initial_velocity: 0.0,
            lanes,
            decay_lanes: None,
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
            start: Instant::now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: 0.0,
            last_update: Instant::now(),
            current_displacement: 0.0,
            initial_velocity,
            lanes: None,
            decay_lanes: None,
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
            start: Instant::now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: scalar_velocity,
            last_update: Instant::now(),
            current_displacement: 0.0,
            initial_velocity: scalar_velocity,
            lanes: None,
            decay_lanes,
        });
    }

    /// 检查并更新动画值，返回是否还在动画中
    pub fn update(&mut self) -> bool {
        let Some(ref mut state) = self.anim_state else { return false; };
        let now = Instant::now();
        // 极端参数保护：Spring/Decay 超过 5s 未收敛强制完成——Spring 有 done
        // 阈值但渐近收敛可能永不达（stiffness=0）；Decay 的 friction 过小时
        // 速度衰减极慢（e^(-0.1·t) 需 ~94s 才低于阈值）。Tween/Keyframes/
        // Repeatable 有明确时长、Snap 立即完成——不被截断（用户设 >5s 时长合法）
        if now.duration_since(state.start) > Duration::from_secs(5) {
            let overrun = match &state.spec {
                AnimationSpec::Spring(_) => Some(state.to.clone()),
                // Decay：写当前解析值（极限可能因 friction 过小而超大——不跳极限）
                AnimationSpec::Decay(spec) => {
                    let t = now.duration_since(state.start).as_secs_f32();
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
                self.state.set(final_val);
                self.anim_state = None;
                if let Some(f) = self.on_finish.take() {
                    f();
                }
                return false;
            }
        }
        let dt = now.duration_since(state.last_update);
        state.last_update = now;
        let mut boundary_velocity = None;
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
                    if all_done {
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
                    let done = spring_at_rest(displacement, state.last_velocity, spec.threshold);
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
                let elapsed = now - state.start;
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
                let elapsed = now - state.start;
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
                // P2-10：支持任意有明确时长的 base（Tween/Keyframes）；Snap 立即跳；
                // Spring/Decay/Repeatable 嵌套无循环长度语义 → 断言拒绝
                let base = spec.base.as_ref();
                let base_duration = match base {
                    AnimationSpec::Tween(t) => t.duration,
                    AnimationSpec::Keyframes(k) => k.duration,
                    AnimationSpec::Snap(s) => s.delay,
                    other => {
                        debug_assert!(false, "Repeatable 仅支持 Tween/Keyframes/Snap base（收到 {other:?}）");
                        Duration::from_millis(300)
                    }
                };
                let elapsed = now - state.start;
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
                    let value = match base {
                        AnimationSpec::Tween(tw) => state.from.lerp(&state.to, tw.interpolator.interpolate(factor)),
                        AnimationSpec::Keyframes(kf) => state.from.lerp(&state.to, interpolate_keyframes(&kf.frames, factor)),
                        AnimationSpec::Snap(_) => state.to.clone(),
                        other => {
                            debug_assert!(false, "Repeatable 仅支持 Tween/Keyframes/Snap base（收到 {other:?}）");
                            state.from.lerp(&state.to, factor)
                        }
                    };
                    (value, false)
                }
            }
            AnimationSpec::Snap(spec) => {
                // Compose's `snap(delayMillis)`: hold the value where it is for the delay, then jump.
                // The animation is still running during the hold (the frame loop keeps ticking) and
                // nothing is written to the state until the jump.
                if now.duration_since(state.start) < spec.delay {
                    (state.from.clone(), false)
                } else {
                    (state.to.clone(), true)
                }
            }
            AnimationSpec::Decay(spec) => {
                let elapsed = now - state.start;
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
                match &self.clamp {
                    // fling：每帧 clamp 到滚动边界——撞边界写边界值并立即完成
                    Some(clamp) => {
                        let c = clamp(raw);
                        if c != raw {
                            boundary_velocity = Some(vel_now);
                            (AnimatableValue::from_f32(c), true)
                        } else {
                            (AnimatableValue::from_f32(c), done)
                        }
                    }
                    None => (AnimatableValue::from_f32(raw), done),
                }
                }
            }
        };
        self.state.set(value);
        if let Some(velocity) = boundary_velocity {
            if let Some(f) = self.on_boundary.take() {
                f(velocity);
            }
        }
        if done {
            self.anim_state = None;
            if let Some(f) = self.on_finish.take() {
                f();
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

/// Whether a spring has come to rest — the ONE place this is decided, because the integrator and
/// `Animatable::update` both need it and must never disagree.
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
/// Compose settles by a different route, with a measurable difference in WHEN: a spring there computes
/// its own duration with `estimateAnimationDurationMillis` — the time the analytic solution needs to come
/// within one `visibilityThreshold` of the target and stay there (`animation-core/SpringEstimation.kt:36-89`
/// and the Newton solve at `:121-190`) — runs exactly that long (`FloatSpringSpec.getDurationNanos`,
/// `animation-core/FloatAnimationSpec.kt:172-190`) and then reports the exact target. That is a
/// POSITION-only test, so the velocity-gated rule below can keep integrating past the point Compose stops:
/// measured on the demo, a 140x44 → 320x120 grow with the default spec (stiffness 400, ratio 1, 1 px
/// threshold) is within 1 px of the target at ~0.43 s, where Compose's estimator stops, while the listener
/// reported the finished animation at ~0.5 s, once the velocity had also fallen under 1 px/s. Nothing
/// visible differs — both end inside a pixel of the target — but a finish time reported here can be up to
/// ~0.15 s later than Compose's.
#[inline]
fn spring_at_rest(displacement: f32, velocity: f32, threshold: f32) -> bool {
    displacement.abs() < threshold && velocity.abs() < threshold
}

/// 固定时间步长弹簧积分（accumulator 模式，最多 10 步）
fn compute_spring_displacement(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    elapsed: Duration, threshold: f32,
) -> f32 {
    const FIXED_DT: f32 = 1.0 / 60.0;
    const MAX_STEPS: u32 = 10;
    let mut total_dt = elapsed.as_secs_f32().min(FIXED_DT * MAX_STEPS as f32);
    let mut displacement = initial_displacement;
    while total_dt > 0.0 {
        let step = total_dt.min(FIXED_DT);
        let omega0 = (stiffness / mass).sqrt();
        let damping_coeff = damping_ratio * 2.0 * omega0 * mass;
        let force = -stiffness * displacement - damping_coeff * *velocity;
        *velocity += force / mass * step;
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
        let ids = ids_state.get();
        // 自动清理：组合点移除 → on_remove 触发 → 表内移除本作用域全部动画
        let key = self.next_key();
        let ids2 = std::sync::Arc::clone(&ids);
        self.start_leaf_with_remove(key, crate::modifier::Modifier::new(), Box::new(move || {
            let ids: Vec<StateId> = ids2.lock().unwrap().drain(..).collect();
            for sid in ids {
                crate::animation::remove_animation_by_state(sid);
            }
        }));
        self.end_node();
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

#[derive(Clone, Debug)]
pub enum AnimationSpec {
    Spring(SpringSpec),
    Tween(TweenSpec),
    /// 关键帧序列（对标 Compose keyframes）
    Keyframes(KeyframesSpec),
    /// 重复执行子动画（对标 Compose repeatable）
    Repeatable(RepeatableSpec),
    /// Instant jump to the target (cf. Compose `snap`), optionally after a delay
    Snap(SnapSpec),
    /// 指数衰减（对标 Compose exponentialDecay——fling/惯性滚动，无目标值）
    Decay(DecaySpec),
}

impl From<TweenSpec> for AnimationSpec {
    fn from(s: TweenSpec) -> Self { AnimationSpec::Tween(s) }
}

impl From<SpringSpec> for AnimationSpec {
    fn from(s: SpringSpec) -> Self { AnimationSpec::Spring(s) }
}

impl From<DecaySpec> for AnimationSpec {
    fn from(s: DecaySpec) -> Self { AnimationSpec::Decay(s) }
}

/// The physical constants of a spring animation (`AnimationSpec::Spring`). `PartialEq` so a component
/// can assert the spec it hands the engine is the motion-scheme token it claims to be.
#[derive(Clone, Debug, PartialEq)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    pub mass: f32,
    pub threshold: f32,
}

impl Default for SpringSpec {
    fn default() -> Self {
        Self {
            damping_ratio: 1.0,
            // Compose StiffnessLow 级别（200）：收敛 ~300-400ms，过渡平滑明显
            stiffness: 200.0,
            mass: 1.0,
            threshold: 0.01,
        }
    }
}

impl SpringSpec {
    // ── Compose Spring 常量（对标 androidx.compose.animation.core.Spring）──
    /// 阻尼比：无弹跳（1.0——临界阻尼）
    pub const DAMPING_RATIO_NO_BOUNCY: f32 = 1.0;
    /// 阻尼比：低弹跳（0.75）
    pub const DAMPING_RATIO_LOW_BOUNCY: f32 = 0.75;
    /// 阻尼比：中弹跳（0.5）
    pub const DAMPING_RATIO_MEDIUM_BOUNCY: f32 = 0.5;
    /// 阻尼比：高弹跳（0.4）
    pub const DAMPING_RATIO_HIGH_BOUNCY: f32 = 0.4;
    /// 刚度：极低（50——慢速柔和）
    pub const STIFFNESS_VERY_LOW: f32 = 50.0;
    /// 刚度：低（200）
    pub const STIFFNESS_LOW: f32 = 200.0;
    /// 刚度：中（400）
    pub const STIFFNESS_MEDIUM: f32 = 400.0;
    /// 刚度：高（1000——快速干脆）
    pub const STIFFNESS_HIGH: f32 = 1000.0;
    /// M3's `StandardMotionTokens.SpringDefaultEffectsStiffness` (1600) — the spring behind
    /// `MotionSchemeKeyTokens.DefaultEffects`.
    pub const STIFFNESS_EFFECTS_DEFAULT: f32 = 1600.0;
    /// M3's `StandardMotionTokens.SpringFastEffectsStiffness` (3800) — the spring behind
    /// `MotionSchemeKeyTokens.FastEffects`.
    pub const STIFFNESS_EFFECTS_FAST: f32 = 3800.0;
    /// Every M3 spatial spring shares this damping ratio
    /// (`StandardMotionTokens.Spring{Default,Fast,Slow}SpatialDamping = 0.9f`): a spatial move carries a
    /// slight overshoot, where an effect spring (damping 1.0) settles without one.
    pub const DAMPING_RATIO_SPATIAL: f32 = 0.9;
    /// `SpringSlowSpatialStiffness` (300) — a large move (a sheet, a full-screen expansion).
    pub const STIFFNESS_SPATIAL_SLOW: f32 = 300.0;
    /// `SpringDefaultSpatialStiffness` (700) — the ordinary move (an indicator sliding, a check drawing).
    pub const STIFFNESS_SPATIAL_DEFAULT: f32 = 700.0;
    /// `SpringFastSpatialStiffness` (1400) — a small move (a dot growing, a thumb sliding).
    pub const STIFFNESS_SPATIAL_FAST: f32 = 1400.0;

    pub fn bouncy() -> Self {
        Self { damping_ratio: 0.6, threshold: 0.1, ..Self::default() }
    }

    /// M3's `DefaultEffects` motion-scheme token: `spring(dampingRatio = 1.0, stiffness = 1600)`
    /// (`material3/MotionScheme.kt:152-156`, `tokens/StandardMotionTokens.kt:22-23`). Compose's
    /// `MotionSchemeKeyTokens.DefaultEffects.value()` resolves to exactly this spring, which is what
    /// M3 uses for a state arriving: a checkbox filling, a radio button dot appearing, a tab's
    /// content colour fading IN.
    ///
    /// The threshold is the default 0.01 — for a colour that is one eight-bit step of Oklab
    /// lightness, and for a length one hundredth of a dp.
    pub fn effects_default() -> Self {
        Self {
            damping_ratio: Self::DAMPING_RATIO_NO_BOUNCY,
            stiffness: Self::STIFFNESS_EFFECTS_DEFAULT,
            mass: 1.0,
            threshold: 0.01,
        }
    }

    /// M3's `FastEffects`: `spring(dampingRatio = 1.0, stiffness = 3800)`
    /// (`material3/MotionScheme.kt:158-162`). M3 uses it for a state LEAVING — a checkbox emptying, a
    /// tab's colour fading out — so the exit is quicker than the arrival.
    pub fn effects_fast() -> Self {
        Self {
            damping_ratio: Self::DAMPING_RATIO_NO_BOUNCY,
            stiffness: Self::STIFFNESS_EFFECTS_FAST,
            mass: 1.0,
            threshold: 0.01,
        }
    }

    /// M3's `DefaultSpatial` motion-scheme token: `spring(dampingRatio = 0.9, stiffness = 700)`
    /// (`material3/MotionScheme.kt:134-139`, `tokens/StandardMotionTokens.kt:20-21`). This is what M3
    /// moves a POSITION or a size with — a tab indicator travelling, a checkmark drawing, a segmented
    /// button's item resizing — and the 0.9 damping is deliberate: these moves overshoot slightly,
    /// unlike the effect springs above. winia used to approximate it with a no-bouncy spring.
    pub fn spatial_default() -> Self {
        Self {
            damping_ratio: Self::DAMPING_RATIO_SPATIAL,
            stiffness: Self::STIFFNESS_SPATIAL_DEFAULT,
            mass: 1.0,
            threshold: 0.01,
        }
    }

    /// M3's `FastSpatial`: `spring(dampingRatio = 0.9, stiffness = 1400)` (`MotionScheme.kt:140-145`)
    /// — a small spatial change: a dot growing, a switch thumb sliding, an icon scaling
    /// (`NavigationBar.kt:270`, `RadioButton.kt:86`, `Switch.kt:162`, `SegmentedButton.kt:391`).
    pub fn spatial_fast() -> Self {
        Self {
            damping_ratio: Self::DAMPING_RATIO_SPATIAL,
            stiffness: Self::STIFFNESS_SPATIAL_FAST,
            mass: 1.0,
            threshold: 0.01,
        }
    }

    /// M3's `SlowSpatial`: `spring(dampingRatio = 0.9, stiffness = 300)` (`MotionScheme.kt:146-151`)
    /// — a large spatial change, such as a sheet or an expanded container.
    pub fn spatial_slow() -> Self {
        Self {
            damping_ratio: Self::DAMPING_RATIO_SPATIAL,
            stiffness: Self::STIFFNESS_SPATIAL_SLOW,
            mass: 1.0,
            threshold: 0.01,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TweenSpec {
    pub duration: Duration,
    /// Wait before the tween starts — during it the value stays at `from` (cf. Compose
    /// `tween(delayMillis)`). `Duration::ZERO` is the plain tween, so a spec that never sets it
    /// behaves exactly as before.
    pub delay: Duration,
    /// 表驱动插值器（v1 预采样表——避免运行时计算过重；`Linear::new()` 为恒等）
    pub interpolator: std::sync::Arc<dyn interpolator::Interpolator>,
}

impl TweenSpec {
    /// 便捷构造：`TweenSpec::new(duration, interpolator)`——插值器自动包装为
    /// `Arc<dyn Interpolator>`（29 个表驱动插值器 + Linear 直接传入）
    pub fn new(
        duration: Duration,
        interpolator: impl Into<std::sync::Arc<dyn interpolator::Interpolator>>,
    ) -> Self {
        Self { duration, delay: Duration::ZERO, interpolator: interpolator.into() }
    }

    /// The same tween, waiting `delay` before it starts (Compose's `tween(delayMillis)`).
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl Default for TweenSpec {
    /// Compose's `tween()` with no arguments: `Duration = DefaultDurationMillis` (300 ms) and
    /// `easing = FastOutSlowInEasing` (`animation/core/AnimationSpec.kt:816-820`,
    /// `core/Easing.kt:50`). Linear interpolation is NOT the default there — a tween that eases in and
    /// out is — so `TweenSpec::new(duration, interpolator::Linear::new())` is what an explicitly linear
    /// ramp looks like.
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(300),
            delay: Duration::ZERO,
            interpolator: std::sync::Arc::new(interpolator::fast_out_slow_in()),
        }
    }
}

/// A keyframe sequence: `(progress 0..1, value 0..1, segment interpolator)`, evaluated with the value
/// applied as a FRACTION of the from→to displacement.
///
/// That fraction is a deliberate difference from Compose: `keyframes { durationMillis = 400; 1.2f at
/// 0.6f }` carries a value in the ANIMATED TYPE's units (1.2 px, say) plus an optional `Easing` per
/// keyframe, while winia's needs to stay type-generic — the same spec drives a `Color` or a `Size`,
/// where "the value in the target's units" does not exist. Per-segment easing IS available (the third
/// element of a frame, see `interpolator`); `new` fills it with `Linear`.
#[derive(Clone, Debug)]
pub struct KeyframesSpec {
    pub duration: Duration,
    pub frames: Vec<(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)>,
}

impl KeyframesSpec {
    /// 简化构造：仅 (progress, value)，段间线性
    pub fn new(duration: Duration, frames: Vec<(f32, f32)>) -> Self {
        let linear: std::sync::Arc<dyn interpolator::Interpolator> =
            std::sync::Arc::new(interpolator::Linear::new());
        let frames = frames.into_iter().map(|(p, v)| (p, v, linear.clone())).collect();
        Self { duration, frames }
    }
}

/// Compose-shaped keyframes: absolute millisecond times and values in the ANIMATED TYPE's units, with an
/// optional easing per keyframe —
///
/// ```ignore
/// // keyframes { durationMillis = 400; 0f at 0; 340f at 200; 300f at 400 } on a 100 → 300 flight
/// let spec = KeyframesOf::between(100.0, 300.0).at(0, 100.0).at(200, 340.0).at(400, 300.0).build();
/// ```
///
/// `build` normalises the band's endpoints into the fraction form the engine evaluates (see
/// `KeyframesSpec`), so the shape reads the way Compose's does while one spec still drives a `Color` or a
/// `Size`. A keyframe with no easing interpolates linearly. When the caller leaves the 0 ms keyframe out,
/// one is supplied at the `from` value — Compose's "the animation starts at the initial value".
pub struct KeyframesOf {
    from: f32,
    to: f32,
    frames: Vec<(u64, f32, Option<std::sync::Arc<dyn interpolator::Interpolator>>)>,
}

impl KeyframesOf {
    /// The band the values are measured in — the value the animation starts at and the one it targets.
    pub fn between(from: f32, to: f32) -> Self {
        Self { from, to, frames: Vec::new() }
    }

    /// `value at millis`, interpolated linearly from the previous keyframe.
    pub fn at(mut self, millis: u64, value: f32) -> Self {
        self.frames.push((millis, value, None));
        self
    }

    /// `value at millis with easing` — Compose's `value at millis with Easing`.
    pub fn at_with(
        mut self,
        millis: u64,
        value: f32,
        easing: std::sync::Arc<dyn interpolator::Interpolator>,
    ) -> Self {
        self.frames.push((millis, value, Some(easing)));
        self
    }

    pub fn build(mut self) -> KeyframesSpec {
        // Last keyframe wins for a repeated millisecond value; the order is otherwise the caller's.
        self.frames.sort_by_key(|(millis, _, _)| *millis);
        self.frames.dedup_by_key(|(millis, _, _)| *millis);
        let linear: std::sync::Arc<dyn interpolator::Interpolator> =
            std::sync::Arc::new(interpolator::Linear::new());
        let duration_ms = self.frames.last().map(|(millis, _, _)| *millis).unwrap_or(0);
        if self.frames.first().map(|(millis, _, _)| *millis).unwrap_or(0) != 0 {
            // Compose starts from the initial value when there is no keyframe at 0.
            self.frames.insert(0, (0, self.from, None));
        }
        let span = self.to - self.from;
        let frames = self
            .frames
            .into_iter()
            .map(|(millis, value, easing)| {
                let progress = if duration_ms == 0 {
                    0.0
                } else {
                    millis as f32 / duration_ms as f32
                };
                // A band with no width (from == to) has nothing to interpolate: every keyframe value
                // collapses to it, which is what the engine's `from.lerp(&to, …)` produces anyway.
                let fraction = if span.abs() <= f32::EPSILON {
                    0.0
                } else {
                    (value - self.from) / span
                };
                (progress, fraction, easing.unwrap_or_else(|| linear.clone()))
            })
            .collect();
        KeyframesSpec { duration: Duration::from_millis(duration_ms), frames }
    }
}

/// 重复执行：iterations 次后完成
#[derive(Clone, Debug)]
pub struct RepeatableSpec {
    pub iterations: u32,
    pub mode: RepeatMode,
    pub base: Box<AnimationSpec>,
    /// Compose's `initialStartOffset`: `Delay` holds the first value for this long before the first
    /// cycle, `FastForward` starts that far into it. The offset applies to the FIRST cycle only.
    pub(crate) start_offset: Duration,
    pub(crate) start_offset_type: StartOffsetType,
}

/// `snap(delayMillis = …)` — Compose's `SnapSpec`: hold the value where it is for the delay, then jump
/// straight to the target. A zero delay (the default) is the plain instant jump the engine always had.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapSpec {
    pub delay: Duration,
}

impl SnapSpec {
    pub fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

impl Default for SnapSpec {
    fn default() -> Self {
        Self { delay: Duration::ZERO }
    }
}

impl RepeatableSpec {
    pub fn new(iterations: u32, mode: RepeatMode, base: AnimationSpec) -> Self {
        Self {
            iterations,
            mode,
            base: Box::new(base),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
    }

    /// Compose's `repeatable(..., initialStartOffset = StartOffset(…))`: `StartOffsetType::Delay` holds
    /// the first value for `offset` before the first cycle, `FastForward` starts that far into it. The
    /// offset applies to the first cycle only; later cycles follow normally.
    pub fn with_start_offset(mut self, offset: Duration, kind: StartOffsetType) -> Self {
        self.start_offset = offset;
        self.start_offset_type = kind;
        self
    }
}

/// How many independent spring components one value can be split into (`Size`/`Offset` have 2,
/// `Color`'s Oklab `alpha, L, a, b` is exactly 4). The fixed array is what keeps a frame's step free
/// of allocations.
pub const MAX_ANIMATION_COMPONENTS: usize = 4;

/// A value that can be animated.
pub trait AnimatableValue: Clone + PartialEq {
    /// Mix towards `to` by `t` — the interpolation a Tween/Keyframes spec uses.
    fn lerp(&self, to: &Self, t: f32) -> Self;
    /// The scalar projection (spring de-duplication; exact only for scalar types).
    fn to_f32(&self) -> f32;
    /// Rebuild from that projection (exact only for scalar types; a placeholder for vectors).
    fn from_f32(v: f32) -> Self;
    /// Exact target comparison (the default `PartialEq`).
    fn same_target(&self, other: &Self) -> bool { self == other }
    /// Whether a Spring can run on this type. A scalar is one spring; a value that overrides
    /// [`Self::write_components`] with several components runs one spring per component, which is
    /// Compose's `VectorConverter` + `VectorizedSpringSpec`.
    fn supports_spring() -> bool { false }
    /// Write the components a Spring advances independently, returning how many: each gets its own
    /// spring and its own visibility threshold, which is how `Size` lands BOTH axes on target and how
    /// `Color` mixes in Oklab.
    ///
    /// The default does not split (a scalar needs no override). A vector must override this and
    /// [`Self::from_components`] together — its `from_f32` is not injective.
    fn write_components(&self, out: &mut [f32; MAX_ANIMATION_COMPONENTS]) -> usize {
        out[0] = self.to_f32();
        1
    }
    /// The inverse of [`Self::write_components`]: rebuild a value from its components.
    fn from_components(components: &[f32]) -> Self {
        Self::from_f32(components[0])
    }
}

impl AnimatableValue for f32 {
    fn lerp(&self, to: &f32, t: f32) -> Self { self + (to - self) * t }
    fn to_f32(&self) -> f32 { *self }
    fn from_f32(v: f32) -> Self { v }
    fn supports_spring() -> bool { true }
}

impl AnimatableValue for crate::graphics::Color {
    /// Oklab interpolation, which is what Compose's colour animations interpolate: the vector is
    /// `(alpha, L, a, b)` converted to Oklab (`ColorVectorConverter.kt:33-47`) and the four components
    /// are mixed independently, then clamped on the way back. A Tween's fraction lands here; a Spring
    /// runs one spring per component instead (see `SpringLane`).
    ///
    /// This replaces a CAM16-UCS blend (`material_colors::blend::cam16_ucs`) whose result is close but
    /// not the same curve, and whose alpha had to be interpolated by hand: Oklab is one space for all
    /// four components, and it is the one Compose uses.
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let from = self.to_oklab();
        let target = to.to_oklab();
        let mut components = [0.0f32; 4];
        for (i, slot) in components.iter_mut().enumerate() {
            *slot = from[i] + (target[i] - from[i]) * t;
        }
        Self::from_oklab(components)
    }

    /// The lightness component. Only the scalar fallbacks use it (Decay, and a Spring on a value that
    /// does not declare four components) — a colour animation goes through the lanes.
    fn to_f32(&self) -> f32 { self.to_oklab()[1] }

    /// A grey at that lightness: the scalar form is a placeholder, as the trait documents.
    fn from_f32(v: f32) -> Self { Self::from_oklab([1.0, v, 0.0, 0.0]) }

    /// Compose's four components: alpha, L, a, b.
    fn write_components(&self, out: &mut [f32; MAX_ANIMATION_COMPONENTS]) -> usize {
        out[..4].copy_from_slice(&self.to_oklab());
        4
    }

    fn from_components(components: &[f32]) -> Self {
        let mut vector = [0.0f32; 4];
        for (slot, value) in vector.iter_mut().zip(components.iter()) {
            *slot = *value;
        }
        Self::from_oklab(vector)
    }

    /// A colour is a four-component value like `Size`/`Offset`, so a Spring is a spring per component
    /// rather than a replacement motion.
    fn supports_spring() -> bool { true }
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
pub(crate) mod tests {
    // 测试串行锁：动画引擎用全局 ACTIVE_ANIMATIONS——并行测试互相干扰（push/update 竞态）
    pub(crate) static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    use super::*;
    use std::sync::Arc;
    use crate::runtime::composer::Composer;
    use std::time::Duration;

    /// 模拟一帧 16.7ms，步进 n 帧推进弹簧
    fn step_spring(
        stiffness: f32, damping: f32, threshold: f32,
        start: f32, target: f32,
        frames: usize,
    ) -> (f32, bool) {
        let mut disp = start - target;
        let mut vel = 0.0f32;
        for _ in 0..frames {
            disp = compute_spring_displacement(
                stiffness, damping, 1.0, disp, &mut vel,
                Duration::from_millis(17), threshold,
            );
        }
        let val = target + disp;
        // The production test, not a copy of it: a settle assertion has to measure the same criterion
        // the engine does, or it pins a duration nobody runs.
        let done = spring_at_rest(disp, vel, threshold);
        (val, done)
    }

    /// The rest test's second half is what keeps a bouncy spring's overshoot: passing through the
    /// threshold band while still moving fast is NOT rest. Pinned because "the velocity and the
    /// displacement are compared in different units" looks like a bug and has already been
    /// "fixed" once — that attempt cut the bounce and turned
    /// `components::shared_transition::tier0_tests::tier0_bouncy_spring_overshoot_renders_then_settles` red
    /// (see `spring_at_rest`).
    #[test]
    fn a_spring_crossing_its_target_is_not_at_rest() {
        // `SpringSpec::bouncy()`'s threshold on a 0..1 progress.
        let threshold = 0.1f32;
        assert!(
            spring_at_rest(0.05, 0.02, threshold),
            "under the threshold and nearly still: that is rest, and a bouncy spring has already settled"
        );
        assert!(
            !spring_at_rest(0.05, 3.0, threshold),
            "inside the band but moving 3 units/s: the bounce is still on its way out"
        );
        assert!(
            !spring_at_rest(1.0, 0.0, threshold),
            "a whole unit from the target is not rest however still it is"
        );
    }

    /// A spring ends on its own criterion, not on the 5 s extreme-parameter protection: from 0 to 123
    /// with `components::tab_row`'s indicator spring (damping 0.6 / stiffness 700 / threshold 0.01) the physics
    /// gives `ln(123 / 0.01) / (0.6 * sqrt(700)) ≈ 0.59 s`, and the value lands EXACTLY on the target.
    #[test]
    fn a_spring_finishes_without_waiting_for_the_extreme_parameter_protection() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let state = State::new(0.0f32);
        let spec = AnimationSpec::Spring(SpringSpec {
            damping_ratio: 0.6,
            stiffness: 700.0,
            mass: 1.0,
            threshold: 0.01,
        });
        push_animatable(state.clone(), 123.0, spec);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while is_animating() && std::time::Instant::now() < deadline {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        let finished = !is_animating();
        clear_all_animations();
        assert!(
            finished,
            "the spring must settle on its own criterion well inside 3 s (the extreme-parameter \
             protection only fires at 5 s); value={}",
            state.peek()
        );
        assert_eq!(state.peek(), 123.0, "a finished spring lands exactly on its target");
    }

    #[test]
    fn spring_converges_to_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 临界阻尼，50→300
        let (val, done) = step_spring(1500.0, 1.0, 0.1, 50.0, 300.0, 300);
        assert!(done, "spring should settle within 300 frames");
        assert!((val - 300.0).abs() < 1.0, "val={} should be near 300", val);
    }

    #[test]
    fn spring_bouncy_overshoots_then_converges() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 欠阻尼 bouncy，应超调后收敛
        let (mut val, mut done) = step_spring(1500.0, 0.6, 0.1, 50.0, 300.0, 5);
        // 早期应明显低于目标（尚未到达）或已超调
        let (final_val, final_done) = step_spring(1500.0, 0.6, 0.1, 50.0, 300.0, 300);
        assert!(final_done, "bouncy should settle");
        assert!((final_val - 300.0).abs() < 1.0, "final_val={}", final_val);
        // 记录中间某帧是否超调过（val > 300 出现过）
        let mut overshot = false;
        let mut disp = -250.0f32;
        let mut vel = 0.0f32;
        for _ in 0..60 {
            disp = compute_spring_displacement(1500.0, 0.6, 0.1, disp, &mut vel, Duration::from_millis(17), 0.1);
            if 300.0 + disp > 300.0 { overshot = true; }
        }
        assert!(overshot, "underdamped spring should overshoot");
        let _ = (val, done);
    }

    #[test]
    fn spring_reverse_animation() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 反向 300→50 也应收敛（此前 bug：分母 max(EPSILON) 卡死）
        let (val, done) = step_spring(1500.0, 1.0, 0.1, 300.0, 50.0, 300);
        assert!(done);
        assert!((val - 50.0).abs() < 1.0, "val={} should be near 50", val);
    }

    #[test]
    fn spring_dt_zero_is_safe() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // dt=0 不应 panic/产生 NaN
        let mut disp = -250.0f32;
        let mut vel = 0.0f32;
        let d = compute_spring_displacement(1500.0, 1.0, 0.1, disp, &mut vel, Duration::ZERO, 0.1);
        assert!(d.is_finite());
        let _ = disp;
    }

    #[test]
    fn tween_completes_within_duration() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut anim = Animatable::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Tween(TweenSpec::default()));
        // 模拟 400ms（每帧 10ms），应超过 300ms duration 完成
        let mut frames = 0;
        while anim.update() && frames < 60 {
            std::thread::sleep(Duration::from_millis(10));
            frames += 1;
        }
        assert!(frames < 60, "tween should finish within 600ms, took {} frames", frames);
        assert_eq!(anim.state.get(), 100.0);
    }

    /// `TweenSpec::delay` (Compose's `tween(delayMillis)`): the value holds `from` until the delay
    /// has passed, then the tween runs its full duration from there.
    #[test]
    fn tween_holds_from_until_its_delay_elapses() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut anim = Animatable::<f32>::new(State::new(0.0));
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(
                TweenSpec::new(Duration::from_millis(60), interpolator::Linear::new())
                    .delay(Duration::from_millis(120)),
            ),
        );
        // Well inside the delay: still exactly `from`, and still running.
        let mut held_at = 0.0f32;
        for _ in 0..6 {
            assert!(anim.update(), "a delayed tween is still running");
            std::thread::sleep(Duration::from_millis(10));
            held_at = anim.state.get();
        }
        assert_eq!(held_at, 0.0, "the value has not moved while the delay runs");

        // …and once the delay is past, it completes the 60 ms tween (180 ms of delay + tween).
        let mut frames = 6;
        while anim.update() && frames < 60 {
            std::thread::sleep(Duration::from_millis(10));
            frames += 1;
        }
        assert!(frames < 60, "the delayed tween finishes (took {frames} frames)");
        assert_eq!(anim.state.get(), 100.0);
    }

    #[test]
    fn infinite_float_restart_loops() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0.0);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: 0.0, to: 10.0,
            spec: InfiniteRepeatableSpec::restart(Duration::from_millis(50)),
            start: Instant::now(),
        };
        // 跑 6 个周期（每个 50ms，用 10ms sleep 推进）
        let mut max_seen = 0.0f32;
        let mut min_seen = 10.0f32;
        for _ in 0..30 {
            inf.update();
            std::thread::sleep(Duration::from_millis(10));
            let v = state.get();
            max_seen = max_seen.max(v);
            min_seen = min_seen.min(v);
        }
        assert!(min_seen <= 0.5, "restart should return near from, min={}", min_seen);
        assert!(max_seen >= 9.5, "restart should reach near to, max={}", max_seen);
        // 无限动画永不完成
        assert!(inf.update(), "infinite should never complete");
    }

    #[test]
    fn infinite_float_reverse_oscillates() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0.4);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: 0.4, to: 1.0,
            spec: InfiniteRepeatableSpec::reverse(Duration::from_millis(50)),
            start: Instant::now(),
        };
        let mut saw_high = false;
        let mut saw_low = false;
        for _ in 0..30 {
            inf.update();
            std::thread::sleep(Duration::from_millis(10));
            let v = state.get();
            if v > 0.95 { saw_high = true; }
            if v < 0.45 { saw_low = true; }
        }
        assert!(saw_high, "reverse should reach near to=1.0");
        assert!(saw_low, "reverse should return near from=0.4");
    }

    /// M3's motion-scheme effect springs, pinned to the token values they mirror
    /// (`tokens/StandardMotionTokens.kt:22-27`): both critically damped, 1600 for an arriving state
    /// and 3800 for a leaving one.
    #[test]
    fn the_effects_springs_are_m3s_tokens() {
        let arriving = SpringSpec::effects_default();
        assert_eq!(arriving.damping_ratio, 1.0);
        assert_eq!(arriving.stiffness, 1600.0, "SpringDefaultEffectsStiffness");
        let leaving = SpringSpec::effects_fast();
        assert_eq!(leaving.damping_ratio, 1.0);
        assert_eq!(leaving.stiffness, 3800.0, "SpringFastEffectsStiffness");
        assert!(leaving.stiffness > arriving.stiffness, "leaving is quicker than arriving");
        assert_eq!(arriving.threshold, 0.01, "the default visibility threshold");
    }

    /// A `Decay` on a colour used to write an opaque grey through the scalar fallback (the fallback's
    /// `from_f32` is a grey at the old Oklab lightness) and to do it on the first frame, because the
    /// push path supplies a zero initial velocity and the decay then reports itself finished at once.
    /// Multi-component decays are normalised to `Snap`, so the colour lands on the target instead.
    #[test]
    fn a_decay_on_a_colour_lands_on_the_target_not_on_a_grey() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        super::tests::clear_all_animations();
        let from = crate::graphics::Color::from_argb(255, 33, 150, 243);
        let to = crate::graphics::Color::from_argb(255, 244, 67, 54);
        let state = State::new(from);
        push_animatable_color(state.clone(), to, AnimationSpec::Decay(DecaySpec::default()));
        let grey = crate::graphics::Color::from_oklab([1.0, from.to_oklab()[1], 0.0, 0.0]);
        let value = state.peek();
        assert_ne!(value, grey, "the decay must not paint the placeholder grey");
        assert_eq!(value, to, "and it lands exactly on the target");
        assert!(!update_animations(), "nothing is left running");
    }

    /// The same rule for the other placeholder types: a `Size` decay used to travel along the area
    /// norm through `from_f32` (a square at the origin).
    #[test]
    fn a_decay_on_a_size_lands_on_the_target_too() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        super::tests::clear_all_animations();
        let state = State::new(crate::unit::Size::new(10.0, 10.0));
        let target = crate::unit::Size::new(80.0, 20.0);
        push_animatable(state.clone(), target, AnimationSpec::Decay(DecaySpec::default()));
        assert_eq!(state.peek(), target, "the size snaps to the target, not to a square at the origin");
    }

    /// M3's three spatial tokens, pinned: one shared damping (0.9 — the slight overshoot is the point)
    /// and the three stiffnesses the components name.
    #[test]
    fn the_spatial_springs_are_m3s_tokens() {
        for (spec, stiffness, name) in [
            (SpringSpec::spatial_slow(), 300.0, "SpringSlowSpatialStiffness"),
            (SpringSpec::spatial_default(), 700.0, "SpringDefaultSpatialStiffness"),
            (SpringSpec::spatial_fast(), 1400.0, "SpringFastSpatialStiffness"),
        ] {
            assert_eq!(spec.damping_ratio, 0.9, "{name}: SpringDefaultSpatialDamping");
            assert_eq!(spec.stiffness, stiffness, "{name}");
            assert_eq!(spec.mass, 1.0);
        }
        assert!(SpringSpec::spatial_fast().stiffness > SpringSpec::spatial_default().stiffness);
        assert_eq!(
            SpringSpec::spatial_default().damping_ratio,
            SpringSpec::DAMPING_RATIO_SPATIAL
        );
        assert_ne!(
            SpringSpec::spatial_default().damping_ratio,
            SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            "the spatial springs overshoot; the effect springs do not"
        );
    }

    /// Compose interpolates a colour in Oklab — `ColorVectorConverter.convertToVector` hands the
    /// animation `AnimationVector4D(alpha, L, a, b)` (`animation/ColorVectorConverter.kt:33-47`), so a
    /// Tween's fraction is applied to those four components rather than to the sRGB bytes or to a
    /// CAM16 blend.
    #[test]
    fn color_lerp_interpolates_in_oklab() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::graphics::Color;
        let from = Color::from_argb(128, 33, 150, 243);
        let to = Color::from_argb(255, 255, 82, 82);
        let mid = from.lerp(&to, 0.5);

        let (a, b) = (from.to_oklab(), to.to_oklab());
        let mut half = [0.0f32; 4];
        for i in 0..4 {
            half[i] = a[i] + (b[i] - a[i]) * 0.5;
        }
        assert_eq!(mid, Color::from_oklab(half), "the midpoint is the component-wise Oklab midpoint");
        assert!((mid.a as i32 - 191).abs() <= 1, "alpha={} should be ~191", mid.a);
        assert!(mid.r > 100 || mid.b > 100, "mid={:?} should not be grayish", mid);
        assert_eq!(from.lerp(&to, 0.0), from);
        assert_eq!(from.lerp(&to, 1.0), to);

        // The perceptual midpoint, which the encoded-byte blend does not produce.
        assert_eq!(
            Color::BLACK.lerp(&Color::WHITE, 0.5),
            Color::from_argb(255, 99, 99, 99),
            "black -> white at 50% is Oklab L 0.5 (#636363), not #7F7F7F"
        );
    }

    /// A colour spring is a spring: one per Oklab component, so the lightness follows the same curve a
    /// scalar `f32` spring does under the same spec — and not the 300 ms linear tween the engine used
    /// to substitute for it (the property this test was written to pin).
    ///
    /// The endpoints are mid-tones on purpose: winia stores a colour as eight-bit sRGB, so the visible
    /// value moves in byte steps (near black one step is 0.067 of Oklab L; at these greys it is 0.004),
    /// which is the recorded deviation from Compose's float components.
    #[test]
    fn a_colour_spring_runs_on_every_component() {
        use std::time::Duration;
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        super::tests::clear_all_animations();

        let from = crate::graphics::Color::from_argb(255, 0x40, 0x40, 0x40);
        let to = crate::graphics::Color::from_argb(255, 0xC8, 0xC8, 0xC8);
        let (from_l, to_l) = (from.to_oklab()[1], to.to_oklab()[1]);
        let color = State::new(from);
        let scalar: State<f32> = State::new(from_l);
        let tweened = State::new(from);
        let spec = AnimationSpec::Spring(SpringSpec::default());
        push_animatable_color(color.clone(), to, spec.clone());
        push_animatable(scalar.clone(), to_l, spec);
        push_animatable_color(
            tweened.clone(),
            to,
            AnimationSpec::Tween(TweenSpec::default()),
        );

        let mut frames = 0;
        let mut max_spring_gap = 0.0f32;
        let mut max_tween_gap = 0.0f32;
        for _ in 0..2_000 {
            if !update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
            let lightness = color.peek().to_oklab()[1];
            max_spring_gap = max_spring_gap.max((lightness - scalar.peek()).abs());
            max_tween_gap = max_tween_gap.max((lightness - tweened.peek().to_oklab()[1]).abs());
            frames += 1;
        }
        let total = color.peek();
        assert!(frames > 20, "the loop sampled the motion (frames={frames})");
        assert!(
            max_spring_gap <= 0.006,
            "the colour's lightness follows the scalar spring under the same spec, one byte step \
             apart at most (largest gap={max_spring_gap})"
        );
        assert!(
            max_tween_gap > 0.03,
            "…and is NOT the linear tween the engine used to substitute for a colour spring \
             (largest gap={max_tween_gap})"
        );
        assert_eq!(total, to, "the spring lands exactly on target");
        assert!(tweened.peek() == to, "the tween does too, and in its own time");
    }

    #[test]
    fn infinite_color_reverse_cycles() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::graphics::Color;
        let state = State::new(Color::RED);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: Color::RED, to: Color::BLUE,
            spec: InfiniteRepeatableSpec::reverse(Duration::from_millis(50)),
            start: Instant::now(),
        };
        let mut saw_blue = false;
        let mut saw_red = false;
        // ⚠ 确定性相位推进（替代旧的 sleep 采样）：Reverse 周期 = 2×duration（100ms），
        // 前半（phase<1）红→蓝、后半蓝→红。拨动 inf.start 到各相位并 update，直接采样
        // 端点邻域——避免旧 sleep(10ms)×30 采样在负载下跳过仅几 ms 宽的端点窗口（flaky）。
        let set_phase = |inf: &mut Infinite<Color>, phase_ms: u64| {
            inf.start = std::time::Instant::now() - std::time::Duration::from_millis(phase_ms);
        };
        // 蓝端：phase≈25ms（t→1）；红端：phase≈0/50/75ms（t→0）
        for phase_ms in [0u64, 24, 25, 26, 49, 50, 51, 74, 75, 76] {
            set_phase(&mut inf, phase_ms);
            inf.update();
            let c = state.get();
            if c.b > 200 && c.r < 50 { saw_blue = true; }
            if c.r > 200 && c.b < 50 { saw_red = true; }
        }
        assert!(saw_blue, "should reach blue (phase 25ms: t→1)");
        assert!(saw_red, "should return to red (phase 0/50/75ms: t→0)");
    }

    #[test]
    fn composer_drop_cleans_owned_animation_without_touching_other_composer() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let owned = State::new(0.0f32);
        let foreign = State::new(0.0f32);
        push_animatable(owned.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));
        push_animatable(foreign.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));

        let composer = {
            let mut composer = Composer::new();
            composer.animation_state_ids.insert(owned.state_id());
            composer
        };
        drop(composer);
        assert!(!has_animation_for_state(owned.state_id()));
        assert!(has_animation_for_state(foreign.state_id()));
        remove_animation_by_state(foreign.state_id());
    }

    #[test]
    fn remove_animation_by_state_cleans_lists() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 推入 f32 + Color + 无限 Color 三种动画
        let s1 = State::new(0.0f32);
        let s2 = State::new(crate::graphics::Color::RED);
        let s3 = State::new(crate::graphics::Color::BLUE);
        push_animatable(s1.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));
        push_animatable_color(s2.clone(), crate::graphics::Color::GREEN, AnimationSpec::Tween(TweenSpec::default()));
        push_infinite(s3.clone(), crate::graphics::Color::BLUE, crate::graphics::Color::RED,
            InfiniteRepeatableSpec::restart(Duration::from_millis(50)));
        assert!(is_animating(), "animations should be registered");

        // 移除 s1 和 s3 对应的动画
        remove_animation_by_state(s1.state_id());
        remove_animation_by_state(s3.state_id());
        // s2 仍在
        assert!(has_animation_for_state(s2.state_id()), "s2 color animation should remain");
        remove_animation_by_state(s2.state_id());
        assert!(!has_animation_for_state(s2.state_id()), "s2 should be removed");
    }

    #[test]
    fn keyframes_interpolate_segments() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::unit::DpExt;
        // 0%→0, 50%→50, 100%→100，线性
        let spec = KeyframesSpec::new(Duration::from_millis(100), vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.0), 0.0);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.25), 0.25);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.5), 0.5);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.75), 0.75);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
        assert_eq!(interpolate_keyframes(&spec.frames, 2.0), 1.0); // 超界 clamp
    }

    #[test]
    fn repeatable_runs_iterations() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut anim = Animatable::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Repeatable(
            RepeatableSpec::new(3, RepeatMode::Restart,
                AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(40), delay: Duration::ZERO, interpolator: std::sync::Arc::new(interpolator::Linear::new()) }))));
        let mut frames = 0;
        while anim.update() && frames < 100 {
            std::thread::sleep(Duration::from_millis(10));
            frames += 1;
        }
        // 3 × 40ms = 120ms，应完成
        assert!(frames < 100, "repeatable should finish within 100 frames");
        assert_eq!(anim.state.get(), 100.0);
    }

    #[test]
    fn repeatable_reverse_even_ends_at_from() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // blocking bug：Reverse + 偶数次时最后 cycle 结束于 from，完成值不应跳变到 to
        let mut anim = Animatable::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Repeatable(
            RepeatableSpec::new(2, RepeatMode::Reverse,
                AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(40), delay: Duration::ZERO, interpolator: std::sync::Arc::new(interpolator::Linear::new()) }))));
        while anim.update() {
            std::thread::sleep(Duration::from_millis(10));
        }
        // 2 次反向：第1次 0→100，第2次 100→0，结束于 0（from）
        let final_val = anim.state.peek();
        assert!((final_val - 0.0).abs() < 1.0, "should end at from=0, got {}", final_val);
    }

    #[test]
    fn keyframes_first_frame_offset() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 首帧 progress > 0 时，t < 首帧 progress 应取首帧值
        let spec = KeyframesSpec::new(Duration::from_millis(100), vec![(0.5, 0.7), (1.0, 1.0)]);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.1), 0.7);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.5), 0.7);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.75), 0.85);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
    }

    #[test]
    fn snap_jumps_immediately() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut anim = Animatable::<f32>::new(State::new(0.0));
        anim.animate_to(42.0, AnimationSpec::Snap(SnapSpec::default()));
        assert!(!anim.update(), "snap completes in one update");
        assert_eq!(anim.state.get(), 42.0);
    }

    #[test]
    fn offset_dedup_is_exact_not_norm() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // blocking bug：两个同范数不同 Offset 不应互相误判为同目标
        use crate::unit::Offset;
        let s = State::new(Offset::new(0.0, 0.0));
        push_animatable(s.clone(), Offset::new(10.0, 0.0), AnimationSpec::Tween(TweenSpec::default()));
        // 移除第一个动画（避免跨列表拦截），再注册同范数不同目标
        remove_animation_by_state(s.state_id());
        push_animatable(s.clone(), Offset::new(0.0, 10.0), AnimationSpec::Tween(TweenSpec::default()));
        assert!(is_animating(), "new offset animation should be registered (norm collision must not block)");
        // 目标精确是 (0,10) 而非 (10,0)
        let list = ACTIVE_ANIMATIONS.lock().unwrap();
        let target_ok = list.iter().any(|a| a.same_target(&Offset::new(0.0, 10.0)));
        assert!(target_ok, "animation target should be (0,10)");
        let wrong_ok = list.iter().any(|a| a.same_target(&Offset::new(10.0, 0.0)));
        assert!(!wrong_ok, "animation should NOT match (10,0)");
    }

    #[test]
    fn offset_spring_no_longer_downgraded_to_tween() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 曾经的 blocking bug：Offset 用 Spring 会沿范数收敛到 (norm,norm) 而非目标，
        // 因此被强制降级成 Tween。现在 Offset 声明 `write_components`（x、y 各一条弹簧），
        // 两条弹簧各自落到目标——降级不再需要，这个测试守住"不再降级"。
        use crate::unit::Offset;
        let s = State::new(Offset::new(0.0, 0.0));
        push_animatable(s.clone(), Offset::new(3.0, 4.0), AnimationSpec::Spring(SpringSpec::default()));
        assert!(has_animation_for_state(s.state_id()), "Offset animation should be registered");
        // 真实帧间隔步进到结束（update() 用真实时钟，连调 dt≈0 永不推进）
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        assert_eq!(
            s.peek(),
            Offset::new(3.0, 4.0),
            "每条分量弹簧各写自己的精确目标（降级成 Tween 或沿范数跑都得不到 (3,4)）"
        );
        remove_animation_by_state(s.state_id());
        assert!(!has_animation_for_state(s.state_id()), "own animation should be removed");
    }

    #[test]
    fn size_spring_lands_each_axis_on_its_own_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Compose 的 `IntSize.VectorConverter`：宽高各一条弹簧，两条都到位动画才结束
        // （`AnimationModifier.kt:70-74`）。两轴位移差得远（宽 +10、高 −1）——单值位移
        // 会沿范数同时拉两轴，谁也落不到自己的目标上。
        use crate::unit::Size;
        let s = State::new(Size::new(50.0, 10.0));
        push_animatable(
            s.clone(),
            Size::new(60.0, 9.0),
            AnimationSpec::Spring(SpringSpec::default()),
        );
        let mut frames = 0;
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
            frames += 1;
        }
        assert!(frames > 1, "两条分量弹簧要跑若干帧，不是一次 Snap（frames={frames}）");
        assert_eq!(s.peek(), Size::new(60.0, 9.0), "宽高各写自己的精确目标");
    }

    #[test]
    fn a_retarget_carries_each_axis_its_own_velocity() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Compose's `Animatable.animateTo` hands the running velocity to the replacement animation
        // PER COMPONENT (`IntSize`'s converter is two-dimensional): interrupting while the width is
        // travelling fast must not give the height a velocity it never had. One scalar velocity
        // (the fastest axis) drags the height along by tens of pixels.
        use crate::unit::Size;
        let s = State::new(Size::new(0.0, 100.0));
        let spec = AnimationSpec::Spring(SpringSpec::default());
        push_animatable(s.clone(), Size::new(100.0, 100.0), spec.clone());
        for _ in 0..4 {
            std::thread::sleep(std::time::Duration::from_millis(8));
            update_animations();
            assert_eq!(s.peek().height, 100.0, "the height's target never moved, so it must not move");
        }
        // The width is mid-flight (and fast); retarget it. Only the width carries a velocity on.
        push_animatable(s.clone(), Size::new(20.0, 100.0), spec);
        let mut frames = 0;
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
            frames += 1;
            assert_eq!(
                s.peek().height, 100.0,
                "the height inherited the width's velocity (height={}, frames={frames})",
                s.peek().height
            );
        }
        assert!(frames > 1, "component springs take several frames (frames={frames})");
        assert_eq!(s.peek(), Size::new(20.0, 100.0));
    }

    #[test]
    fn spring_done_returns_exact_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 回归：Spring 渐近收敛——done 时 Animatable 必须写出精确目标值
        // （修复前返回 to+残余位移——AnimatedVisibility 的 exit 完成检测
        //   progress<0.001 会因残余位移卡住/误判）
        use crate::runtime::state::State;
        let st = State::new(1.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(0.0, AnimationSpec::Spring(SpringSpec::default()));
        // 步进直到完成：update() 用真实时钟（Instant::now()）——连续调用 dt≈0
        // 永不推进，需真实帧间隔（16ms ≈ 60fps）；上限 100 帧防死循环
        let mut frames = 0;
        while anim.update() && frames < 100 {
            std::thread::sleep(Duration::from_millis(16));
            frames += 1;
        }
        assert!(frames < 100, "spring 应收敛（stiffness=200 约 300-400ms）");
        assert_eq!(st.peek(), 0.0, "done 后值必须精确等于目标（修复前为残余位移）");
    }

    /// Decay（指数衰减）：v0=1000、friction=4.2 → 极限 = 1000/4.2 ≈ 238.1
    /// 验证：单调递增、done 后精确停靠极限值
    #[test]
    fn decay_moves_and_stops_at_limit() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_decay(1000.0, DecaySpec::new(4.2, 0.1));
        let mut prev = 0.0f32;
        let mut frames = 0;
        while anim.update() && frames < 300 {
            let v = st.peek();
            assert!(v >= prev - 0.01, "decay 必须单调（帧 {}：{prev} -> {v}）", frames);
            prev = v;
            std::thread::sleep(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "decay 应收敛");
        let limit = 1000.0 / 4.2;
        assert!(
            (st.peek() - limit).abs() < 0.5,
            "done 后必须精确停靠极限 {}（实际 {}）",
            limit,
            st.peek()
        );
    }

    /// P2-10：Repeatable 支持 Keyframes base——每周期走关键帧曲线
    /// （修复前回退 300ms 线性：3 次迭代 = 900ms；Keyframes base 50ms×3=150ms）
    #[test]
    fn repeatable_keyframes_base() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Repeatable(RepeatableSpec::new(
                3,
                RepeatMode::Restart,
                AnimationSpec::Keyframes(KeyframesSpec::new(
                    Duration::from_millis(50),
                    vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
                )),
            )),
        );
        let mut frames = 0;
        while anim.update() && frames < 30 {
            std::thread::sleep(Duration::from_millis(16));
            frames += 1;
        }
        assert!(frames < 20, "Keyframes base 3×50ms 应在 ~10 帧内完成（实际 {frames}——回退 300ms 线性会 >20）");
        assert_eq!(st.peek(), 100.0, "完成后必须精确停靠 to");
    }

    /// P2-10：Repeatable 支持 Snap base——立即完成
    #[test]
    fn repeatable_snap_base() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Repeatable(RepeatableSpec::new(3, RepeatMode::Restart, AnimationSpec::Snap(SnapSpec::default()))),
        );
        assert!(!anim.update(), "Snap base 应首次 update 即完成");
        assert_eq!(st.peek(), 100.0, "Snap base 完成值 = to");
    }

    /// Decay：初始速度为 0 → 立即完成，值不变
    #[test]
    fn decay_zero_velocity_done_immediately() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(42.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_decay(0.0, DecaySpec::default());
        assert!(!anim.update(), "v0=0 应立即完成");
        assert_eq!(st.peek(), 42.0, "v0=0 值不变");
    }

    /// animateIntAsState：target 变化自动动画，值单调逼近且最终精确到达
    #[test]
    fn animate_int_as_state_animates_to_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let value: std::cell::RefCell<Option<State<i32>>> = std::cell::RefCell::new(None);
        let mut recompose = |composer: &mut Composer, t: i32| {
            composer.compose(|ctx| {
                let v = crate::animation::animate_int_as_state(
                    ctx,
                    t,
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(300),
                        crate::animation::interpolator::Linear::new(),
                    ),
                );
                let _ = v.get(); // 注册依赖（目标变化驱动重跑）
                *value.borrow_mut() = Some(v.clone());
            });
        };
        recompose(&mut composer, 0);
        assert_eq!(value.borrow().as_ref().unwrap().peek(), 0, "初始 = target");

        // target 变 100 → 动画启动 → 中途值在 0..100 且递增
        recompose(&mut composer, 100);
        let mut prev = 0i32;
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            let v = value.borrow().as_ref().unwrap().peek();
            assert!(v >= prev, "i32 动画必须单调递增（{prev} -> {v}）");
            prev = v;
            frames += 1;
            if v >= 100 || frames > 60 {
                break;
            }
        }
        assert_eq!(prev, 100, "i32 动画最终精确到达 target（{frames} 帧）");
    }

    /// animateValueAsState 泛型：Color target 动画到达目标色
    #[test]
    fn animate_value_as_state_color_reaches_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let from = crate::graphics::Color::from_argb(255, 0, 0, 0);
        let to = crate::graphics::Color::from_argb(255, 255, 255, 255);
        let value: std::cell::RefCell<Option<State<crate::graphics::Color>>> = std::cell::RefCell::new(None);
        let mut recompose = |composer: &mut Composer, t: crate::graphics::Color| {
            composer.compose(|ctx| {
                let v = crate::animation::animate_value_as_state(
                    ctx,
                    t,
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(200),
                        crate::animation::interpolator::Linear::new(),
                    ),
                );
                let _ = v.get();
                *value.borrow_mut() = Some(v.clone());
            });
        };
        recompose(&mut composer, from.clone());
        assert_eq!(value.borrow().as_ref().unwrap().peek(), from, "初始 = target");

        recompose(&mut composer, to.clone());
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            let v = value.borrow().as_ref().unwrap().peek();
            frames += 1;
            if v == to || frames > 60 {
                break;
            }
        }
        assert_eq!(value.borrow().as_ref().unwrap().peek(), to, "Color 到达 target（{frames} 帧）");
    }

    /// P2-9 速度延续（实例级）：Decay 中途 animate_to(Spring)——
    /// 新动画继承旧速度（物理连续，Compose 打断语义）
    #[test]
    fn retarget_inherits_velocity() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_decay(1000.0, DecaySpec::default());
        // 步进 5 帧（20ms）——速度衰减但仍 > 0
        for _ in 0..5 {
            anim.update();
            std::thread::sleep(Duration::from_millis(20));
        }
        let old_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert!(old_vel > 100.0, "5 帧后速度应仍显著（实际 {old_vel}）");
        // 打断 → 新 Spring 目标 200——继承旧速度
        anim.animate_to(200.0, AnimationSpec::Spring(SpringSpec::default()));
        let new_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(new_vel, old_vel, "retarget 必须继承旧速度（修复前为 0 重启）");
        // 且起点 = 当前值（不跳变）
        let from = anim.anim_state.as_ref().unwrap().from;
        assert!((from - st.peek()).abs() < 0.01, "from 必须是当前值（无跳变）");
    }

    /// cancel_animation：动画进行中取消后，值不再被动画覆盖
    /// （回归：Reset 按钮 set(0) 后下一帧被 Decay 写回——reset 无效）
    #[test]
    fn cancel_animation_stops_decay() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        push_decay(st.clone(), 1000.0, DecaySpec::default());
        // 推进 5 帧——值显著移动
        for _ in 0..5 {
            super::update_animations();
            std::thread::sleep(Duration::from_millis(20));
        }
        let mid = st.peek();
        assert!(mid > 50.0, "Decay 推进后值应显著（实际 {mid}）");
        // 取消动画 + set 0（Reset 语义）
        cancel_animation(&st);
        st.set(0.0);
        // 再推进 5 帧——值必须保持 0（不被动画写回）
        for _ in 0..5 {
            super::update_animations();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(st.peek(), 0.0, "取消后值必须保持（修复前被 Decay 写回）");
    }

    /// review fix：速度残留——Spring→Tween→Spring 第二次打断必须从静止重启
    /// （Tween 期间 last_velocity 不更新，继承它会带着过期物理速度）
    #[test]
    fn retarget_after_tween_restarts_from_rest() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        // 1) Spring 跑起来（速度显著）
        anim.animate_to(100.0, AnimationSpec::Spring(SpringSpec::default()));
        for _ in 0..5 {
            anim.update();
            std::thread::sleep(Duration::from_millis(20));
        }
        let spring_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert!(spring_vel > 10.0, "Spring 推进后速度应显著（实际 {spring_vel}）");
        // 2) 打断成 Tween（review fix：非物理 spec 不继承速度——归零）
        anim.animate_to(200.0, AnimationSpec::Tween(TweenSpec::default()));
        for _ in 0..3 {
            anim.update();
            std::thread::sleep(Duration::from_millis(16));
        }
        let tween_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(tween_vel, 0.0, "Tween 必须从静止开始（review fix：不继承 Spring 速度）");
        // 3) 再打断成 Spring——必须从静止重启（修复前继承 Tween 的过期 spring_vel）
        anim.animate_to(50.0, AnimationSpec::Spring(SpringSpec::default()));
        let final_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(final_vel, 0.0, "Tween 打断后的 Spring 必须从静止重启（修复前继承过期速度 {spring_vel}）");
    }

    /// fling：指数衰减 + 边界 clamp——撞边界写边界值并立即完成（对齐 Compose
    /// fling 消耗完即停）。v0=10000 极限 2380 → 被 clamp 在 100
    #[test]
    fn fling_clamps_at_boundary_and_stops() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.set_clamp(|o| o.min(100.0));
        anim.animate_decay(10000.0, DecaySpec::new(4.2, 0.1));
        let mut frames = 0;
        while anim.update() && frames < 300 {
            std::thread::sleep(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "fling 撞边界应收敛（{frames} 帧）");
        assert_eq!(st.peek(), 100.0, "clamp 到边界值并停止");
    }

    #[test]
    fn fling_boundary_callback_reports_remaining_velocity_once() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let state = State::new(95.0f32);
        let boundary_velocity = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reported = boundary_velocity.clone();
        push_fling_with_boundary(
            state.clone(),
            10_000.0,
            DecaySpec::new(4.2, 0.1),
            |offset| offset.min(100.0),
            move |velocity| reported.lock().unwrap().push(velocity),
            || {},
        );
        for _ in 0..20 {
            if !update_animations() { break; }
            std::thread::sleep(Duration::from_millis(20));
        }
        let velocities = boundary_velocity.lock().unwrap();
        assert_eq!(state.peek(), 100.0);
        assert_eq!(velocities.len(), 1);
        assert!(velocities[0] > 0.0);
    }

    #[test]
    fn fling_clamps_below_zero() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(50.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.set_clamp(|o| o.max(0.0));
        anim.animate_decay(-500.0, DecaySpec::new(4.2, 0.1));
        let mut frames = 0;
        while anim.update() && frames < 300 {
            std::thread::sleep(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "反向 fling 应收敛（{frames} 帧）");
        assert_eq!(st.peek(), 0.0, "下限 clamp 到 0");
    }

    /// review fix：DecaySpec friction<=0 快速失败（配置错误立即暴露）
    #[test]
    #[should_panic(expected = "friction 必须 > 0")]
    fn decay_spec_rejects_nonpositive_friction() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let _ = DecaySpec::new(0.0, 0.1);
    }

    /// 限制 #6：duration=0 的 Tween/Keyframes 立即完成且值精确 = to（无除零 NaN）
    #[test]
    fn zero_duration_tween_and_keyframes_complete_immediately() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Tween duration=0
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(42.0, AnimationSpec::Tween(TweenSpec::new(Duration::ZERO, interpolator::Linear::new())));
        assert!(!anim.update(), "duration=0 Tween 应首次 update 即完成");
        assert_eq!(st.peek(), 42.0, "duration=0 Tween 完成值 = to");
        // Keyframes duration=0
        let st2 = State::new(0.0f32);
        let mut anim2 = Animatable::new(st2.clone());
        anim2.animate_to(
            42.0,
            AnimationSpec::Keyframes(KeyframesSpec::new(Duration::ZERO, vec![(0.0, 0.0), (1.0, 1.0)])),
        );
        assert!(!anim2.update(), "duration=0 Keyframes 应首次 update 即完成");
        assert_eq!(st2.peek(), 42.0, "duration=0 Keyframes 完成值 = to");
    }

    /// P2-9 速度延续（push 路径）：push_decay → push_animatable retarget——
    /// 全局表里的新动画继承旧速度
    #[test]
    fn push_retarget_inherits_velocity() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::runtime::state::State;
        let st = State::new(0.0f32);
        push_decay(st.clone(), 1000.0, DecaySpec::default());
        // 推进 5 帧（update_animations 驱动全局表）
        for _ in 0..5 {
            super::update_animations();
            std::thread::sleep(Duration::from_millis(20));
        }
        let vel_before = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().find(|a| a.state_id() == st.state_id()).map(|a| a.last_velocity()).unwrap_or(0.0)
        };
        assert!(vel_before > 100.0, "Decay 推进后速度应显著（实际 {vel_before}）");
        // retarget：同 state 推新目标（Spring）
        push_animatable(st.clone(), 200.0, AnimationSpec::Spring(SpringSpec::default()));
        let vel_after = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().find(|a| a.state_id() == st.state_id()).map(|a| a.last_velocity()).unwrap_or(0.0)
        };
        assert!(
            (vel_after - vel_before).abs() < 1.0,
            "push retarget 必须继承旧速度（before={vel_before} after={vel_after}——Spring 首步 dt≈0 引入微差）"
        );
        // 清理：等动画结束（避免残留影响其他测试）
        for _ in 0..60 {
            if !super::update_animations() { break; }
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    #[test]
    fn on_finish_fires_when_animation_completes() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // finishedListener（对标 Compose）：动画完成帧触发一次
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use crate::runtime::state::State;
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        anim.on_finish(move || {
            f2.store(true, Ordering::SeqCst);
        });
        anim.animate_to(1.0, AnimationSpec::Snap(SnapSpec::default()));
        anim.update(); // Snap 一帧完成 → 回调触发
        assert!(fired.load(Ordering::SeqCst), "动画完成应触发 on_finish");
        assert_eq!(st.peek(), 1.0, "动画值应到达目标");
    }
    /// 复现 animation_demo 滚动卡死（dup-key）：Column + vertical_scroll +
    /// Crossfade + 多 section。滚动 → Skip/Enter 交替 + 动画 notify → 每帧
    /// 2 次 compose（recompose loop）→ 节点不应泄漏/dup-key。
    #[test]
    fn scroll_with_animation_no_dup_key() {
        use std::cell::RefCell;
        let _g = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::runtime::composer::Composer::new();
        let scroll = RefCell::new(None::<crate::modifier::ScrollState>);
        let page = RefCell::new(None::<crate::runtime::state::State<u32>>);
        let frame_parity = RefCell::new(std::rc::Rc::new(std::cell::Cell::new(false)));
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);

        let scene = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(crate::compose!(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::Column::new()
                        .modifier(crate::modifier::Modifier::new()
                            .fill_max_size()
                            .vertical_scroll({
                                // 先取 clone 再判断——borrow_mut 与外层 borrow 冲突
                                let cur = scroll.borrow().clone();
                                if let Some(s) = cur {
                                    s
                                } else {
                                    let s = ctx.remember(|| crate::modifier::ScrollState::new()).get();
                                    *scroll.borrow_mut() = Some(s.clone());
                                    s
                                }
                            }))
                        .build(ctx, |ctx| {
                            for i in 0..10 {
                                crate::components::Text::new(format!("section {i}")).font_size(14.0).build(ctx);
                                crate::layout::Row::new().build(ctx, |ctx| {
                                    crate::components::Text::new(format!("内容 {i} —— 撑高内容")).build(ctx);
                                });
                            }
                            // 动画 section（每帧 notify——demo 8a/8b 等价）
                            let a = ctx.animate_float_as_state(
                                if frame_parity.borrow().get() { 200.0 } else { 40.0 },
                                crate::animation::TweenSpec::default().into(),
                            );
                            crate::components::Text::new(format!("w={}", a.peek())).build(ctx);
                            let p = ctx.remember(|| 0u32);
                            *page.borrow_mut() = Some(p.clone());
                            crate::components::crossfade::Crossfade::new(p.clone())
                                .animation(crate::animation::TweenSpec::default())
                                .build(ctx, |ctx, pg| {
                                    crate::components::Text::new(format!("page {pg}")).build(ctx);
                                });
                        });
                });
            }));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        };

        scene(&mut composer);
        let nodes1 = composer.arena.nodes.len();
        for frame in 0..30 {
            if let Some(s) = scroll.borrow().clone() {
                s.scroll_to((frame % 5) as f32 * 150.0, 2000.0);
            }
            if frame % 10 == 0 {
                if let Some(p) = page.borrow().clone() {
                    p.set((p.get() + 1) % 3);
                }
            }
            frame_parity.borrow().set(!frame_parity.borrow().get());
            for _ in 0..5 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
            scene(&mut composer);
            scene(&mut composer);
            let n = composer.arena.nodes.len();
            assert!(n <= nodes1 + 20,
                "帧 {frame}: 节点数不应持续增长（{n} vs 初始 {nodes1}）——泄漏/dup");
        }
    }

    /// `push_animatable_with_done` has to cancel a conflicting animation exactly as
    /// `push_animatable_handle` does (`animation.rs:294-309`). The value's CURRENT value can equal the
    /// NEW target while an animation toward a DIFFERENT target is still running — the value passes
    /// through it mid-flight — and returning early there leaves the stale animation pulling the value
    /// to its old target. Measured before the fix: an animation 0.2 → 1.0 was pushed, the value was
    /// read at ~0.62 and that same value pushed back as the target, and the state then sailed on to
    /// 1.0 instead of staying put.
    #[test]
    fn push_animatable_with_done_cancels_a_conflicting_animation() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.2f32);
        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        push_animatable_with_done(state.clone(), 1.0, slow.clone(), || {});
        // Let it get well clear of 0.2 but nowhere near 1.0.
        for _ in 0..12 {
            update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let mid = state.peek();
        assert!(mid > 0.3 && mid < 0.9, "the first animation is mid-flight (value {mid})");
        // Push the value it is AT right now as the new target: that is the `peek == target` branch.
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d = done.clone();
        push_animatable_with_done(state.clone(), mid, slow, move || {
            d.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        for _ in 0..30 {
            update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let end = state.peek();
        assert!(
            (end - mid).abs() < 0.05,
            "the conflicting animation must be cancelled, so the value stays at {mid} (ended at {end})"
        );
        assert!(
            done.load(std::sync::atomic::Ordering::SeqCst),
            "and the zero-displacement animation completes, so its callback runs"
        );
    }

    /// The velocity variant had the same hole from the other side: on `peek() == target` it reported
    /// completion and returned, without cancelling a conflicting motion — so the callback announced
    /// "finished" while the stale animation was still pulling the value (before this fix: it reported
    /// done at 0.55 and the value ended at 0.97).
    #[test]
    fn push_animatable_with_velocity_and_done_cancels_a_conflicting_animation_too() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.2f32);
        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        push_animatable(state.clone(), 1.0, slow.clone());
        for _ in 0..12 {
            update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let mid = state.peek();
        assert!(mid > 0.3 && mid < 0.9, "the first animation is mid-flight (value {mid})");
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d = done.clone();
        push_animatable_with_velocity_and_done(state.clone(), mid, slow, 0.0, move || {
            d.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        for _ in 0..30 {
            update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let end = state.peek();
        assert!(
            (end - mid).abs() < 0.05,
            "the conflicting animation must be cancelled, so the value stays at {mid} (ended at {end})"
        );
        assert!(
            done.load(std::sync::atomic::Ordering::SeqCst),
            "and the callback fires once the zero-displacement motion lands"
        );
    }

    /// A loop honours its `initial_start_offset` (Compose's `initialStartOffset`): `Delay` holds the
    /// loop's first value for the offset, `FastForward` begins that far into the first cycle.
    #[test]
    fn an_infinite_loop_honours_its_start_offset() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        // FastForward: a 400 ms cycle entered 300 ms in → the first step is already three quarters along.
        let fast = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            fast.clone(),
            0.0,
            1.0,
            InfiniteRepeatableSpec::restart(Duration::from_millis(400))
                .with_start_offset(Duration::from_millis(300), StartOffsetType::FastForward),
        );
        update_animations();
        let v = fast.peek();
        assert!(
            (v - 0.75).abs() < 0.1,
            "FastForward starts three quarters into the cycle (value {v})"
        );
        clear_all_animations();
        // Delay: the same cycle, but held for 300 ms before it begins.
        let slow = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            slow.clone(),
            0.0,
            1.0,
            InfiniteRepeatableSpec::restart(Duration::from_millis(400))
                .with_start_offset(Duration::from_millis(300), StartOffsetType::Delay),
        );
        update_animations();
        let held = slow.peek();
        assert!(held < 0.02, "Delay holds the first value for the offset (value {held})");
        std::thread::sleep(Duration::from_millis(430)); // 300 ms of waiting + ~130 ms into the cycle
        update_animations();
        let moving = slow.peek();
        assert!(
            moving > 0.1 && moving < 0.6,
            "then the first cycle runs from its beginning (value {moving})"
        );
        clear_all_animations();
    }

    /// `ComposeCtx::animate_float_as_state_with_done` is Compose's
    /// `animateFloatAsState(targetValue, animationSpec, label, finishedListener)`: the slot owns the
    /// state, so nothing else can drive it, and the callback fires once per motion that finishes.
    #[test]
    fn the_ctx_animate_state_with_done_reports_each_finished_motion() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let visible = crate::runtime::state::State::new(false);
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut composer = Composer::new();
        let spec = AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(120),
            std::sync::Arc::new(interpolator::Linear::new())
                as std::sync::Arc<dyn interpolator::Interpolator>,
        ));
        let mut frame = |composer: &mut Composer| {
            let on = visible.get();
            let c = counter.clone();
            let spec = spec.clone();
            composer.compose(|ctx| {
                let value = ctx.animate_float_as_state_with_done(if on { 100.0 } else { 0.0 }, spec, move || {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
                let _ = value.get();
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut run = |composer: &mut Composer, frames: usize, frame: &mut dyn FnMut(&mut Composer)| {
            for _ in 0..frames {
                frame(composer);
                update_animations();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        };
        frame(&mut composer); // the state is created AT the target: nothing runs, nothing reports
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
        visible.set(true);
        run(&mut composer, 20, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the grow reports exactly once"
        );
        // Interrupt it: the abandoned motion does not report, the replacement does.
        visible.set(false);
        run(&mut composer, 5, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a motion that was replaced must not report"
        );
        run(&mut composer, 20, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "the shrink reports once when it lands"
        );
        clear_all_animations();
    }

    /// Compose's `Animatable<T>.animateDecay(initialVelocity: T, decayAnimationSpec)` runs one float
    /// decay per component (`DecayAnimationSpec<T>`, vectorized by the type's converter). Before this,
    /// a Decay reaching a multi-component value was replaced by Snap
    /// (`normalise_multi_component_decay`), so a fling on an `Offset` never moved at all.
    #[test]
    fn a_vector_fling_decays_each_component_on_its_own() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let start = crate::unit::Offset::new(10.0, 20.0);
        let velocity = crate::unit::Offset::new(100.0, -60.0);
        let friction = 4.2f32;
        let spec = DecaySpec::new(friction, 0.1);
        let state = crate::runtime::state::State::new(start);
        push_decay_vector(state.clone(), velocity, spec);
        update_animations();
        let mid = state.peek();
        let (limit_x, limit_y) = (10.0 + 100.0 / friction, 20.0 - 60.0 / friction);
        assert!(
            mid.x > start.x && mid.x < limit_x,
            "x travels toward its own limit (at {})",
            mid.x
        );
        assert!(
            mid.y < start.y && mid.y > limit_y,
            "y travels the other way, on its own limit (at {})",
            mid.y
        );
        let mut frames = 0;
        while update_animations() && frames < 400 {
            std::thread::sleep(Duration::from_millis(8));
            frames += 1;
        }
        let end = state.peek();
        assert!(
            (end.x - limit_x).abs() < 0.01 && (end.y - limit_y).abs() < 0.01,
            "each component parks on its exact limit (ended at {end:?}, expected {limit_x}x{limit_y})"
        );
        assert!(frames < 400, "the fling ends on its own ({frames} frames)");
        clear_all_animations();
    }

    /// Compose's keyframe shape — absolute milliseconds and values in the type's units — is normalised
    /// into the engine's fraction band by `KeyframesOf`, and the resulting motion is the one Compose
    /// describes: a 100 → 300 flight through `340f at 200ms` overshoots to 340 and lands exactly on 300.
    #[test]
    fn composes_keyframe_shape_drives_the_same_motion() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let spec = KeyframesOf::between(100.0, 300.0)
            .at(0, 100.0)
            .at(200, 340.0)
            .at(400, 300.0)
            .build();
        assert_eq!(spec.duration, Duration::from_millis(400));
        assert_eq!(spec.frames.len(), 3);
        // 200 ms of a 400 ms flight is progress 0.5, and 340 is 1.2 bands up from 100.
        assert!((spec.frames[1].0 - 0.5).abs() < 1e-6, "the middle keyframe sits at half the duration");
        assert!((spec.frames[1].1 - 1.2).abs() < 1e-6, "and 340 is 1.2 bands above the 100 start");
        assert_eq!(interpolate_keyframes(&spec.frames, 0.0), 0.0);
        assert!((interpolate_keyframes(&spec.frames, 0.5) - 1.2).abs() < 1e-6);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
        // End to end through the engine: 100 → 300 overshoots at half time and lands on the target.
        let state = crate::runtime::state::State::new(100.0f32);
        push_animatable(state.clone(), 300.0, AnimationSpec::Keyframes(spec.clone()));
        update_animations();
        let mut mid = state.peek();
        let mut frames = 0;
        while update_animations() && frames < 200 {
            std::thread::sleep(Duration::from_millis(4));
            frames += 1;
            let v = state.peek();
            if v > mid {
                mid = v;
            }
        }
        assert!(
            mid > 330.0,
            "the flight overshoots toward 340 the way the keyframes say (peak {mid})"
        );
        assert_eq!(state.peek(), 300.0, "and it lands exactly on the target");
        // A keyframe list without a 0 ms entry starts at the `from` value (Compose's rule).
        let no_first = KeyframesOf::between(0.0, 1.0).at(100, 0.5).at(200, 1.0).build();
        assert_eq!(no_first.frames.first().map(|f| f.0), Some(0.0));
        assert_eq!(no_first.frames.first().map(|f| f.1), Some(0.0));
        clear_all_animations();
    }

    /// Compose's `snap(delayMillis)` holds the value for the delay and only then jumps, and Compose's
    /// `repeatable(..., initialStartOffset = StartOffset(…))` shifts the first cycle of a finite repeat
    /// the same way the infinite spec does. Both were missing: winia's `Snap` had no delay at all, and a
    /// repeat always began at its first frame.
    #[test]
    fn a_delayed_snap_and_a_delayed_repeat_both_hold_first() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        // snap(delayMillis = 250): nothing moves for 250 ms, then the target appears in one frame.
        let state = crate::runtime::state::State::new(0.0f32);
        push_animatable(state.clone(), 1.0, AnimationSpec::Snap(SnapSpec::new(Duration::from_millis(250))));
        update_animations();
        std::thread::sleep(Duration::from_millis(120));
        assert!(update_animations(), "the animation is still running during the hold");
        assert_eq!(state.peek(), 0.0, "and the value has not moved yet");
        std::thread::sleep(Duration::from_millis(180));
        assert!(!update_animations(), "after the delay the snap completes");
        assert_eq!(state.peek(), 1.0, "writing the target exactly");
        clear_all_animations();
        // repeatable(2, tween(200, linear), FastForward 150): the first frame is already 3/4 in.
        let fast = crate::runtime::state::State::new(0.0f32);
        push_animatable(
            fast.clone(),
            1.0,
            AnimationSpec::Repeatable(
                RepeatableSpec::new(
                    2,
                    RepeatMode::Restart,
                    AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), linear())),
                )
                .with_start_offset(Duration::from_millis(150), StartOffsetType::FastForward),
            ),
        );
        update_animations();
        let started = fast.peek();
        assert!(
            (started - 0.75).abs() < 0.1,
            "a fast-forwarded repeat starts three quarters into its first cycle (at {started})"
        );
        clear_all_animations();
        // The same repeat with Delay 300 ms still sits at the first value 120 ms in.
        let slow = crate::runtime::state::State::new(0.0f32);
        push_animatable(
            slow.clone(),
            1.0,
            AnimationSpec::Repeatable(
                RepeatableSpec::new(
                    2,
                    RepeatMode::Restart,
                    AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), linear())),
                )
                .with_start_offset(Duration::from_millis(300), StartOffsetType::Delay),
            ),
        );
        update_animations();
        std::thread::sleep(Duration::from_millis(120));
        update_animations();
        assert_eq!(slow.peek(), 0.0, "a delayed repeat holds its first value");
        std::thread::sleep(Duration::from_millis(300));
        update_animations();
        let moving = slow.peek();
        assert!(
            moving > 0.05 && moving < 0.95,
            "and only then does its first cycle run (at {moving})"
        );
        clear_all_animations();
    }

    #[derive(Clone, PartialEq, Debug)]
    enum Leg {
        Idle,
        Busy,
    }

    /// Compose's `transitionSpec = { initial, target -> ... }` chooses a spec from the PAIR of states
    /// (`Transition.kt`'s `TransitionSpec`), so the leg leaving `Idle` and the leg returning to it can
    /// run different curves. Before this, a `Transition` carried one spec and used it for every
    /// animation: this test fails on a single spec whichever one is chosen (a quick one finishes the
    /// slow leg too early, a slow one leaves the quick leg mid-flight).
    #[test]
    fn a_transition_chooses_a_spec_for_each_pair() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let slow = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(700), linear()));
        let quick = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), linear()));
        let leg = crate::runtime::state::State::new(Leg::Idle);
        let held: std::rc::Rc<std::cell::RefCell<Option<crate::runtime::state::State<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            let current = leg.get();
            let slow = slow.clone();
            let quick = quick.clone();
            let held = held.clone();
            composer.compose(|ctx| {
                let mut t = ctx.update_transition(current, quick.clone(), "leg");
                let value = t.animate_float_with_spec(
                    ctx,
                    |l| match l {
                        Leg::Idle => 0.0,
                        Leg::Busy => 100.0,
                    },
                    "position",
                    move |from, to| {
                        if *from == Leg::Idle && *to == Leg::Busy {
                            slow.clone()
                        } else {
                            quick.clone()
                        }
                    },
                );
                let _ = value.get();
                if held.borrow().is_none() {
                    *held.borrow_mut() = Some(value);
                }
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut run = |composer: &mut Composer, ms: u64, frame: &mut dyn FnMut(&mut Composer)| {
            let frames = (ms / 16).max(1);
            for _ in 0..frames {
                frame(composer);
                update_animations();
                std::thread::sleep(Duration::from_millis(16));
            }
        };
        run(&mut composer, 40, &mut frame);
        let value = held.borrow().clone().expect("the animation ran and held its state");
        assert_eq!(value.peek(), 0.0, "the idle state starts where it is");
        leg.set(Leg::Busy);
        run(&mut composer, 260, &mut frame);
        let outbound = value.peek();
        assert!(
            outbound > 15.0 && outbound < 85.0,
            "the Idle -> Busy leg runs the SLOW spec (value {outbound} after ~260 ms of 700)"
        );
        leg.set(Leg::Idle);
        run(&mut composer, 260, &mut frame);
        let back = value.peek();
        assert!(
            back < 0.001,
            "the Busy -> Idle leg runs the QUICK spec and is already home (value {back})"
        );
        clear_all_animations();
    }

}

#[cfg(test)]
mod repeated_tests {
    use super::*;
    use std::sync::Arc;
    use crate::runtime::composer::Composer;

    /// 反复动画循环：多次 push 目标 + update 步进——最终收敛到最新目标。
    /// 验证动画引擎在反复触发（点击循环）时值正确（旧动画移除/去重无双驱动）。
    #[test]
    fn repeated_animation_cycles_converge() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = crate::runtime::state::State::new(40.0f32);

        // 每轮：push 目标 → sleep 超过动画时长 → update 一次（真实时间 dt）→ 检查收敛
        for (i, target) in [(1usize, 200.0f32), (2, 40.0), (3, 200.0), (4, 40.0)] {
            push_animatable(state.clone(), target, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(300), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
            }));
            std::thread::sleep(std::time::Duration::from_millis(400));
            update_animations();
            let v = state.peek();
            eprintln!("[repeat-anim] round {} target={} val={:.1}", i, target, v);
            assert!((v - target).abs() < 1.0,
                "反复动画第 {} 轮应收敛到 {}，实际 {:.1}", i, target, v);
        }
    }

    /// 动画未完成时再次 push（中途改变目标）——应切换目标（旧动画移除）
    #[test]
    fn mid_flight_retarget_switches() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = crate::runtime::state::State::new(40.0f32);
        push_animatable(state.clone(), 200.0, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(1000), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        // 中途（10 帧后）改目标 40——应切换（用真实时间 sleep 模拟帧间隔）
        std::thread::sleep(std::time::Duration::from_millis(50));
        update_animations();
        let mid = state.peek();
        assert!(mid > 40.0 && mid < 200.0, "中途应处于动画中（{}）", mid);
        push_animatable(state.clone(), 90.0, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(200), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        std::thread::sleep(std::time::Duration::from_millis(300));
        update_animations();
        // 完成判定可能差一帧（dt 偏移）——再 sleep + update 一次确保收敛
        std::thread::sleep(std::time::Duration::from_millis(250));
        update_animations();
        let v = state.peek();
        eprintln!("[retarget] mid={:.1} final={:.1}", mid, v);
        assert!((v - 90.0).abs() < 1.0, "中途改目标应收敛到 90，实际 {:.1}", v);
    }

    /// i32 插值（animate_int_as_state 的基础）：四舍五入 + spring 支持
    #[test]
    fn int_value_lerp_rounds() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::animation::AnimatableValue;
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.5), 5);
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.51), 5);
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.55), 6);
        assert_eq!(<i32 as AnimatableValue>::from_f32(3.7), 4);
        assert!(<i32 as AnimatableValue>::supports_spring(), "i32 标量应支持 Spring");
        // 动画收敛：0 → 100（Tween）
        let state = crate::runtime::state::State::new(0i32);
        push_animatable(state.clone(), 100, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(200), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        std::thread::sleep(std::time::Duration::from_millis(300));
        update_animations();
        assert_eq!(state.peek(), 100, "i32 动画应收敛到 100");
    }

    /// Transition::animate 泛型（animate_value 核心）：Color 值经 target_fn
    /// 映射 + push_animatable 驱动收敛
    #[test]
    fn transition_animate_generic_value() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::runtime::composer::Composer::new();
        let mut captured = None;
        composer.compose(|ctx| {
            let mut t = ctx.update_transition(3.0f32, AnimationSpec::Tween(TweenSpec::default()), "t");
            let s = t.animate(ctx, |v| *v * 2.0, "v");
            captured = Some(s);
        });
        assert_eq!(captured.unwrap().peek(), 6.0, "target_fn 映射应立即生效（同值跳过动画）");
    }

    #[test]
    fn transition_animate_value_named_wrapper() {
        // animate_value is the Compose-named twin of the generic animate<U>.
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut captured = None;
        composer.compose(|ctx| {
            let mut t = ctx.update_transition(10i32, AnimationSpec::Tween(TweenSpec::default()), "t");
            let s = t.animate_value(ctx, |v| *v + 5, "v");
            captured = Some(s);
        });
        assert_eq!(captured.unwrap().peek(), 15, "animate_value maps target_fn immediately");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn transition_child_follows_parent_target() {
        // create_child_transition: child target derives from parent target on every
        // call — flip the parent, the child re-maps and animates to the new value.
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        let parent = crate::runtime::state::State::new(1i32);
        let child_val = std::cell::RefCell::new(None);
        let build = |composer: &mut crate::runtime::composer::Composer,
                     child_val: &std::cell::RefCell<
            Option<crate::runtime::state::State<f32>>,
        >| {
            let p = parent.clone();
            composer.compose(|ctx| {
                let t = ctx.update_transition(p.get(), AnimationSpec::Tween(TweenSpec::default()), "p");
                let child = t.create_child_transition(|v| *v * 100);
                let mut child = child;
                let s = child.animate_float(ctx, |v| *v as f32, "c");
                *child_val.borrow_mut() = Some(s.clone());
            });
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &child_val);
        assert_eq!(child_val.borrow().clone().unwrap().peek(), 100.0, "child maps parent 1 → 100");
        // Flip parent 1 → 2: child target becomes 200 and animates there.
        parent.set(2);
        build(&mut composer, &child_val);
        for _ in 0..10 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(50));
            build(&mut composer, &child_val);
        }
        let v = child_val.borrow().clone().unwrap().peek();
        assert!(
            (v - 200.0).abs() < 5.0,
            "child should converge 100 → 200 after parent flip, got {v}"
        );
        crate::animation::clear_all_animations();
    }
}

// ═══════════════════════════════════════════════════════════
// P3-2 无限动画生命周期（T5/T6）
// ═══════════════════════════════════════════════════════════

/// T5：无限动画作用域随组合点移除自动 dispose——动画全局表清空，
/// `update_animations()` 不再空转。
#[test]
fn test_infinite_transition_auto_dispose() {
    // 全局动画表共享——串行锁（仓库既有约定，防并行测试 clear 误删）
    let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use crate::runtime::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::runtime::state::State;
    use std::time::Duration;

    // 清空全局动画表（跨测试并行隔离）
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);
    // 记录自己动画的 state_id（跨测试并行隔离——只断言自己的动画状态）
    let sid_holder = std::cell::RefCell::new(None::<StateId>);

    let build = |composer: &mut Composer,
                 holder: &std::cell::RefCell<Option<State<bool>>>,
                 sid_holder: &std::cell::RefCell<Option<StateId>>| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        // 无限动画作用域（内部挂 on_remove 自动 dispose）
                        let mut inf = ctx.remember_infinite_transition();
                        let s = inf.animate_float(
                            ctx,
                            0.0,
                            1.0,
                            InfiniteRepeatableSpec::restart(Duration::from_millis(100)),
                        );
                        *sid_holder.borrow_mut() = Some(s.state_id());
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    // 帧1：动画注册 → 活跃
    build(&mut composer, &holder, &sid_holder);
    let sid = sid_holder.borrow().unwrap();
    assert!(has_animation_for_state(sid), "帧1 应有无限动画（活跃）");

    // 帧2：show=false → 组合点移除 → on_remove → dispose → 表清空
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder, &sid_holder);
    assert!(!has_animation_for_state(sid), "组合点移除后无限动画应自动 dispose（无空转）");
}

/// T6：显式 dispose + on_remove 双重触发幂等——无 panic、表不变。
#[test]
fn test_infinite_transition_manual_dispose_idempotent() {
    // 全局动画表共享——串行锁（仓库既有约定，防并行测试 clear 误删）
    let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use crate::runtime::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::runtime::state::State;
    use std::time::Duration;

    // 清空全局动画表（跨测试并行隔离）
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);
    // 保存 InfiniteTransition 引用（模拟用户持有——显式 dispose 路径）
    let inf_holder = std::cell::RefCell::new(None::<InfiniteTransition>);

    let build = |composer: &mut Composer,
                 holder: &std::cell::RefCell<Option<State<bool>>>,
                 inf_holder: &std::cell::RefCell<Option<InfiniteTransition>>| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        let mut inf = ctx.remember_infinite_transition();
                        let _s = inf.animate_float(
                            ctx,
                            0.0,
                            1.0,
                            InfiniteRepeatableSpec::restart(Duration::from_millis(100)),
                        );
                        *inf_holder.borrow_mut() = Some(inf);
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    // 帧1：注册
    build(&mut composer, &holder, &inf_holder);
    let sid = inf_holder.borrow().as_ref().unwrap().ids.lock().unwrap()[0];
    assert!(has_animation_for_state(sid), "帧1 应有无限动画");

    // 显式 dispose（用户路径）→ 表清空
    inf_holder.borrow().as_ref().unwrap().dispose();
    assert!(!has_animation_for_state(sid), "显式 dispose 后表应清空");

    // 组合点移除 → on_remove 再触发（ids 已 drain——幂等无 panic）
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder, &inf_holder);
    assert!(!has_animation_for_state(sid), "双重触发后表仍空（幂等）");
}

    /// Wiring test for the table-driven interpolators (v1's 29 curves plus `Linear`).
    #[test]
    fn tween_uses_table_interpolator() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::animation::interpolator::{EaseInQuad, Interpolator, Linear};
        // 1. The default is Compose's `tween()` easing, `FastOutSlowIn` — not a linear ramp. Its
        //    midpoint sits at 0.776 (see `interpolator::tests` for the solved reference values).
        let t = TweenSpec::default();
        assert!(
            (t.interpolator.interpolate(0.5) - 0.7756).abs() < 0.01,
            "the default easing is FastOutSlowIn, midpoint = {}",
            t.interpolator.interpolate(0.5)
        );
        // 1b. An explicitly linear ramp still is one — `TweenSpec::new(dur, Linear::new())`.
        let linear = TweenSpec::new(Duration::from_millis(300), Linear::new());
        assert_eq!(linear.interpolator.interpolate(0.5), 0.5, "an explicit Linear midpoint = 0.5");
        // 2. A table-driven curve takes effect (EaseInQuad's midpoint is below 0.5 — it eases in).
        let ease = Arc::new(EaseInQuad::new());
        assert!(ease.interpolate(0.5) < 0.5, "EaseInQuad's midpoint must be below 0.5");
        assert!(ease.interpolate(0.0) == 0.0 && ease.interpolate(1.0) == 1.0, "the ends are clamped");
        // 3. Engine wiring: an Animatable animated by a table interpolator. After 60 ms (t ≈ 0.2)
        //    EaseInQuad(0.2) = 0.04 → about 4, while linear(0.2) = 0.2 → about 20: five times apart,
        //    so a ±10 ms jitter in the sleep cannot blur the two.
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(300), delay: Duration::ZERO, interpolator: Arc::new(EaseInQuad::new()),
            }),
        );
        std::thread::sleep(Duration::from_millis(60));
        anim.update();
        let v = st.peek();
        assert!(
            v > 0.0 && v < 10.0,
            "EaseInQuad after 60 ms must be far below linear (got {}, linear is about 20)",
            v
        );
        // Control: the same 300 ms tween through its own declared interpolator reaches about 20.
        let st2 = State::new(0.0f32);
        let mut anim2 = Animatable::new(st2.clone());
        anim2.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec::new(
                Duration::from_millis(300),
                crate::animation::interpolator::Linear::new(),
            )),
        );
        std::thread::sleep(Duration::from_millis(60));
        anim2.update();
        assert!((st2.peek() - 20.0).abs() < 5.0, "linear 推进 60ms 应≈20（实际 {}）", st2.peek());
    }

    /// 回归：过冲插值器（Elastic/Back）动画完成后必须停靠精确目标——
    /// 修复前用 eased>=1.0 判定，过冲使动画提前结束于越界值
    #[test]
    fn tween_overshoot_interpolator_settles_at_target() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::animation::interpolator::{EaseOutElastic, Interpolator};
        // 曲线本身在 t≈0.98 处 eased 就 > 1.0（过冲）
        let ease = EaseOutElastic::new();
        assert!(ease.interpolate(0.98) > 1.0, "EaseOutElastic 末端应过冲（>1.0）");
        // 动画推进完整时长——必须精确停靠 100（修复前停在过冲值）
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), EaseOutElastic::new())),
        );
        let mut frames = 0;
        while anim.update() && frames < 30 {
            std::thread::sleep(Duration::from_millis(40));
            frames += 1;
        }
        assert!(frames < 30, "动画应收敛");
        assert_eq!(st.peek(), 100.0, "过冲插值器完成必须精确停靠目标（修复前停在越界值）");
    }

    /// Keyframes done 帧同样必须停靠精确目标（与 Tween/Spring 一致）——
    /// 末帧 progress<1.0 时插值结果 ≠ to
    #[test]
    fn keyframes_settles_at_target() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        // 末帧 progress=0.8（<1.0）——done 时若用插值结果将 ≠ to
        anim.animate_to(
            100.0,
            AnimationSpec::Keyframes(KeyframesSpec {
                duration: Duration::from_millis(200),
                frames: vec![
                    (0.0, 0.0, Arc::new(crate::animation::interpolator::Linear::new())),
                    (0.8, 50.0, Arc::new(crate::animation::interpolator::Linear::new())),
                ],
            }),
        );
        let mut frames = 0;
        while anim.update() && frames < 20 {
            std::thread::sleep(Duration::from_millis(40));
            frames += 1;
        }
        assert!(frames < 20, "keyframes 应收敛");
        assert_eq!(st.peek(), 100.0, "keyframes 完成必须精确停靠目标（末帧 progress<1.0 时修复前≠to）");
    }
