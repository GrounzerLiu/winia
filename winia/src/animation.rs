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

    /// The curve a cycle follows — Compose's `infiniteRepeatable(tween(...)/keyframes(...))`. It works in
    /// BOTH repeat modes: a `Reverse` loop mirrors its iteration time, so the return leg is this curve running
    /// backwards (Compose's `VectorizedInfiniteRepeatableSpec`), where winia used to travel back linearly.
    pub fn with_base(mut self, base: AnimationSpec) -> Self {
        self.base = Some(base);
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
    let anim = Infinite { state: visual, from, to, spec, start: Instant::now() };
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
}

/// 注册一个动画到全局活跃列表
/// Register a `Motion<T>` in the global active list (called by `animate_*_as_state`)
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
    let mut anim = Motion::from_animating(handle);
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
    let mut anim = Motion::from_animating(state);
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
/// `Motion::animate_decay_with_velocity`) gives every component its own decay, the way Compose's
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

/// Register a `Motion<Color>` in the global active list (called by `animate_color_as_state`)
///
/// The spec is honoured as written, springs included: a `Color` is a four-component value (Oklab
/// `alpha, L, a, b`), so `Motion::start_with_velocity` gives it one spring per component, the way
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
    let mut anim = Motion::new(state);
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
    let mut anim = Motion::new(state);
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
    let mut anim = Motion::new(state);
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
    let mut anim = Motion::new(state);
    anim.start_with_velocity(target, spec, inherited_velocity);
    anim.on_finish_with(done);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
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
                s.start.elapsed().as_millis(),
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
            spec_kind(&r.base)
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
            start: Instant::now(),
            spec,
            last_velocity: start_velocity.max_abs(),
            last_update: Instant::now(),
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
            start: Instant::now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: 0.0,
            last_update: Instant::now(),
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
            start: Instant::now(),
            spec: AnimationSpec::Decay(spec),
            last_velocity: scalar_velocity,
            last_update: Instant::now(),
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
                let velocity = state.last_velocity;
                self.anim_state = None;
                if let Some(f) = self.on_finish.take() {
                    f(MotionEnd { reason: MotionEndReason::Forced, velocity });
                }
                return false;
            }
        }
        let dt = now.duration_since(state.last_update);
        state.last_update = now;
        let mut boundary_velocity = None;
        let mut bound_hit = false;
        // Compose ends a spring on a CLOCK: `estimateAnimationDurationMillis` says how long it needs to
        // stay within `delta` of the target, and the spec runs exactly that long (`FloatSpringSpec
        // .getDurationNanos`, `animation-core/FloatAnimationSpec.kt:172-190`). Position alone, no velocity
        // term — the rule this engine used to stop on. Both are kept: whichever comes first ends the
        // motion, with the exact target written, so a spring that the integrator has already brought to
        // rest does not keep ticking, and one the estimate says is done does not linger.
        let spring_elapsed = now.duration_since(state.start);
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
fn loop_factor(base: Option<&AnimationSpec>, mode: RepeatMode, cycle: u32, phase: f32) -> f32 {
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
        Some(AnimationSpec::Tween(tween)) => tween.interpolator.interpolate(forward),
        Some(AnimationSpec::Keyframes(keyframes)) => interpolate_keyframes(&keyframes.frames, forward),
        _ => forward,
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

/// A `Repeatable`'s cycle length: a `Tween` or `Keyframes` base has one, a `Snap` jumps after its delay.
/// A nested Spring/Decay/Repeatable has no cycle length, and the engine rejects those at push time too.
fn base_duration_of(base: &AnimationSpec) -> Duration {
    match base {
        AnimationSpec::Tween(t) => t.duration,
        AnimationSpec::Keyframes(k) => k.duration,
        AnimationSpec::Snap(s) => s.delay,
        other => {
            debug_assert!(
                false,
                "Repeatable only has a cycle length for a Tween/Keyframes/Snap base (got {other:?})"
            );
            Duration::from_millis(300)
        }
    }
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
        push_animatable(self.state.clone(), target, spec.into());
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
        push_animatable_with_result(self.state.clone(), target, spec.into(), done);
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

impl AnimationSpec {
    /// Compose's `visibilityThreshold` argument — how close a spring has to get before it counts as
    /// arrived, in the value's own unit. In Compose the threshold is a construction parameter of the
    /// `Animatable` (and the default spec is built with it: `spring(visibilityThreshold = …)`); winia's
    /// springs carry theirs on the spec, so this is the per-call override.
    ///
    /// A no-op for specs with no threshold: a `Tween`/`Keyframes` ends on its clock, and a `Decay`'s
    /// stop threshold is its own (`DecaySpec::threshold`).
    pub fn with_visibility_threshold(self, threshold: f32) -> Self {
        match self {
            AnimationSpec::Spring(spec) => AnimationSpec::Spring(SpringSpec { threshold, ..spec }),
            AnimationSpec::Repeatable(spec) => AnimationSpec::Repeatable(RepeatableSpec {
                base: Box::new(spec.base.with_visibility_threshold(threshold)),
                ..spec
            }),
            other => other,
        }
    }
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

    /// The theme's `DefaultEffects` token — `WiniaTheme::motion_scheme().default_effects`, which in the
    /// standard scheme is `spring(dampingRatio = 1.0, stiffness = 1600)`
    /// (`material3/MotionScheme.kt:152-156`, `tokens/StandardMotionTokens.kt:22-23`). Compose's
    /// `MotionSchemeKeyTokens.DefaultEffects.value()` resolves to exactly this spring, which is what
    /// M3 uses for a state arriving: a checkbox filling, a radio button dot appearing, a tab's
    /// content colour fading IN.
    ///
    /// The threshold is the default 0.01 — for a colour that is one eight-bit step of Oklab
    /// lightness, and for a length one hundredth of a dp.
    pub fn effects_default() -> Self {
        crate::theme::WiniaTheme::motion_scheme().default_effects
    }

    /// The theme's `FastEffects` token — `spring(dampingRatio = 1.0, stiffness = 3800)` in the standard
    /// scheme (the schemes share their effect springs)
    /// (`material3/MotionScheme.kt:158-162`). M3 uses it for a state LEAVING — a checkbox emptying, a
    /// tab's colour fading out — so the exit is quicker than the arrival.
    pub fn effects_fast() -> Self {
        crate::theme::WiniaTheme::motion_scheme().fast_effects
    }

    /// The theme's `SlowEffects` token — the effect spring for a change that should read as slow; damping 1.0
    /// and stiffness 800 in either scheme.
    pub fn effects_slow() -> Self {
        crate::theme::WiniaTheme::motion_scheme().slow_effects
    }

    /// The theme's `DefaultSpatial` token — `spring(dampingRatio = 0.9, stiffness = 700)` in the standard
    /// scheme, `spring(0.8, 380)` in the expressive one
    /// (`material3/MotionScheme.kt:134-139`, `tokens/StandardMotionTokens.kt:20-21`). This is what M3
    /// moves a POSITION or a size with — a tab indicator travelling, a checkmark drawing, a segmented
    /// button's item resizing — and the 0.9 damping is deliberate: these moves overshoot slightly,
    /// unlike the effect springs above. winia used to approximate it with a no-bouncy spring.
    pub fn spatial_default() -> Self {
        crate::theme::WiniaTheme::motion_scheme().default_spatial
    }

    /// The theme's `FastSpatial` token — `spring(dampingRatio = 0.9, stiffness = 1400)` in the standard
    /// scheme, `spring(0.6, 800)` in the expressive one (`MotionScheme.kt:140-145`)
    /// — a small spatial change: a dot growing, a switch thumb sliding, an icon scaling
    /// (`NavigationBar.kt:270`, `RadioButton.kt:86`, `Switch.kt:162`, `SegmentedButton.kt:391`).
    pub fn spatial_fast() -> Self {
        crate::theme::WiniaTheme::motion_scheme().fast_spatial
    }

    /// The theme's `SlowSpatial` token — `spring(dampingRatio = 0.9, stiffness = 300)` in the standard
    /// scheme, `spring(0.8, 200)` in the expressive one (`MotionScheme.kt:146-151`)
    /// — a large spatial change, such as a sheet or an expanded container.
    pub fn spatial_slow() -> Self {
        crate::theme::WiniaTheme::motion_scheme().slow_spatial
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
        let mut anim = Motion::<f32>::new(State::new(0.0));
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
        let mut anim = Motion::<f32>::new(State::new(0.0));
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
        // 6 cycles of 50 ms. Sampled at 1 ms, not 10 ms: the triangle hits its peak only inside a
        // ~2.5 ms window per half-cycle, so 10 ms sampling misses it depending on machine load and the
        // assertion below flaked once in a full run (the value seen depends on when the scheduler runs
        // this thread). Same wall time, deterministic phase coverage.
        let mut max_seen = 0.0f32;
        let mut min_seen = 10.0f32;
        for _ in 0..300 {
            inf.update();
            std::thread::sleep(Duration::from_millis(1));
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
            // The first two frames are skipped: the three states are separate pushes, so one can start a frame
            // later than another, and under load that phase offset showed up as a gap three times the budget
            // while the spring was still moving fastest. This guard is about the CURVE the colour follows, so it
            // starts once all three are running.
            if frames >= 2 {
                max_spring_gap = max_spring_gap.max((lightness - scalar.peek()).abs());
                max_tween_gap = max_tween_gap.max((lightness - tweened.peek().to_oklab()[1]).abs());
            }
            frames += 1;
        }
        let total = color.peek();
        // A floor, not a frame budget: under load the loop legitimately finishes in fewer, longer
        // steps, so demanding 20 frames was load-dependent.
        assert!(frames > 3, "the loop sampled the motion (frames={frames})");
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
        let mut anim = Motion::<f32>::new(State::new(0.0));
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
        let mut anim = Motion::<f32>::new(State::new(0.0));
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
        let mut anim = Motion::<f32>::new(State::new(0.0));
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
        let mut anim = Motion::new(st.clone());
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

    /// `target_value` is the asymptote the decay itself lands on, `v0 / friction` — the number a pager
    /// estimates "how many pages would this flick carry" from, so it must agree with the motion.
    #[test]
    fn a_decay_target_value_is_the_asymptote() {
        let spec = exponential_decay(4.2);
        assert!((spec.target_value(840.0) - 200.0).abs() < 1e-3, "840 / 4.2");
        assert!(
            (spec.target_value(-840.0) + 200.0).abs() < 1e-3,
            "signed, so a backward flick estimates backwards"
        );
        assert_eq!(spec.target_value(0.0), 0.0);
    }

    /// Decay（指数衰减）：v0=1000、friction=4.2 → 极限 = 1000/4.2 ≈ 238.1
    /// 验证：单调递增、done 后精确停靠极限值
    #[test]
    fn decay_moves_and_stops_at_limit() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
        anim.animate_to(42.0, AnimationSpec::Tween(TweenSpec::new(Duration::ZERO, interpolator::Linear::new())));
        assert!(!anim.update(), "duration=0 Tween 应首次 update 即完成");
        assert_eq!(st.peek(), 42.0, "duration=0 Tween 完成值 = to");
        // Keyframes duration=0
        let st2 = State::new(0.0f32);
        let mut anim2 = Motion::new(st2.clone());
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
        let mut anim = Motion::new(st.clone());
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

    /// Compose's `Animatable` handle — `rememberAnimatable` with `animateTo` / `snapTo` / `stop` /
    /// `isRunning` / `targetValue`. Three of those had no equivalent at all: nothing could tell a caller
    /// what a running motion was heading for, whether one was running, or cancel it and keep the
    /// velocity. `snapTo` cancelling is the part that is easy to get wrong: leaving the motion
    /// registered lets it pull the value straight back off what the caller just wrote.
    #[test]
    fn the_animatable_handle_reports_and_controls_its_motion() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        assert_eq!(a.value(), 0.0);
        assert!(!a.is_running(), "nothing runs before the first animateTo");
        assert_eq!(a.target(), None);

        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        a.animate_to(100.0, slow.clone());
        assert!(a.is_running());
        assert_eq!(a.target(), Some(100.0), "the handle reports what it is heading for");
        let mut frames = 0;
        while frames < 10 {
            update_animations();
            std::thread::sleep(Duration::from_millis(16));
            frames += 1;
        }
        let mid = a.value();
        assert!(mid > 0.0 && mid < 100.0, "the motion is on its way (at {mid})");

        let velocity = a.stop();
        assert!(!a.is_running(), "stop() cancels the motion");
        assert_eq!(a.target(), None, "and there is no target left");
        assert!(velocity.abs() > 1.0, "stop() hands back the velocity it had ({velocity})");
        let stopped = a.value();
        for _ in 0..5 {
            update_animations();
            std::thread::sleep(Duration::from_millis(16));
        }
        assert_eq!(a.value(), stopped, "and nothing moves it afterwards");

        a.animate_to(500.0, slow.clone());
        for _ in 0..4 {
            update_animations();
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(a.value() > stopped, "the second motion is running");
        a.snap_to(0.0);
        assert_eq!(a.value(), 0.0);
        assert!(!a.is_running(), "snapTo cancels too");
        for _ in 0..6 {
            update_animations();
            std::thread::sleep(Duration::from_millis(16));
        }
        assert_eq!(a.value(), 0.0, "the cancelled motion does not pull the value back");
        assert_eq!(a.as_state().peek(), 0.0, "as_state() is the same value the frame paints");
        clear_all_animations();
    }

    /// Compose's `visibilityThreshold` argument (`animateFloatAsState(..., visibilityThreshold = …)`):
    /// winia's springs carry the threshold on the spec, so this is the per-call override — a loose
    /// threshold arrives sooner, a tight one keeps travelling.
    #[test]
    fn a_looser_visibility_threshold_arrives_sooner() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let soft = SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        };
        let frames_until_done = |threshold: f32| -> u32 {
            let state = crate::runtime::state::State::new(0.0f32);
            push_animatable(
                state.clone(),
                100.0,
                AnimationSpec::Spring(soft.clone()).with_visibility_threshold(threshold),
            );
            let mut frames = 0;
            while update_animations() && frames < 600 {
                std::thread::sleep(Duration::from_millis(8));
                frames += 1;
            }
            assert_eq!(state.peek(), 100.0, "a spring still lands on its exact target");
            frames
        };
        let tight = frames_until_done(0.1);
        clear_all_animations();
        let loose = frames_until_done(40.0);
        clear_all_animations();
        assert!(
            loose < tight,
            "a 40-unit threshold settles before a 0.1-unit one (loose {loose} frames, tight {tight})"
        );
        // And the threshold really is inside the spec, where Compose puts it: nothing else changes.
        let spec = AnimationSpec::Spring(soft.clone()).with_visibility_threshold(7.5);
        match spec {
            AnimationSpec::Spring(s) => {
                assert_eq!(s.threshold, 7.5);
                assert_eq!(s.stiffness, soft.stiffness, "the rest of the spring is untouched");
            }
            other => panic!("expected a spring, got {other:?}"),
        }
    }

    /// `active_animations_report` is what the debug server's `al` command prints: it has to name the
    /// animation that owns a moving value, with the numbers needed to tell a stuck motion from a healthy
    /// one. Before it there was no way to ask — the registry was write-only from outside.
    #[test]
    fn the_active_animation_report_names_what_is_running() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        assert!(active_animations_report().is_empty(), "nothing runs before anything is pushed");

        let state = crate::runtime::state::State::new(40.0f32);
        push_animatable(state.clone(), 200.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(300), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>)));
        update_animations();
        let lines = active_animations_report();
        assert_eq!(lines.len(), 1, "one motion, one line ({lines:?})");
        let line = &lines[0];
        assert!(line.contains(&format!("state={:?}", state.state_id())), "names the state ({line})");
        assert!(line.contains("spec=tween(300ms)"), "names the spec and its duration ({line})");
        assert!(line.contains("from=40.000") && line.contains("target=200.000"), "names the ends ({line})");

        // A fling has no target: the line says decay and carries the friction instead.
        let fling = crate::runtime::state::State::new(0.0f32);
        push_decay(fling.clone(), 900.0, exponential_decay(4.2));
        update_animations();
        let lines = active_animations_report();
        assert_eq!(lines.len(), 2, "the tween and the fling ({lines:?})");
        assert!(
            lines.iter().any(|l| l.contains("spec=decay(friction=4.20)")),
            "the fling is named as a decay ({lines:?})"
        );

        // A vector motion says how many components it is driving.
        let vector = crate::runtime::state::State::new(crate::unit::Offset::new(0.0, 0.0));
        push_decay_vector(vector.clone(), crate::unit::Offset::new(100.0, -60.0), exponential_decay(4.2));
        update_animations();
        assert!(
            active_animations_report()
                .iter()
                .any(|l| l.contains("decay_components=2")),
            "the diagonal fling reports its two components"
        );

        // Nothing is running once every animation has been dropped.
        clear_all_animations();
        assert!(active_animations_report().is_empty());
    }

    /// A frame longer than `FIXED_DT * MAX_STEPS` advances a spring by only that much, while a Tween reads
    /// the wall clock and arrives — so one stall makes the two disagree and the spring quietly takes
    /// longer than the real time it was given. Compose's springs are functions of elapsed time, so they
    /// are unaffected. Measured here so the deviation is a number, not a suspicion.
    #[test]
    fn a_long_frame_does_not_leave_a_spring_behind_the_clock() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let spring = crate::runtime::state::State::new(0.0f32);
        let tween = crate::runtime::state::State::new(0.0f32);
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let soft = SpringSpec {
            stiffness: 200.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        };
        push_animatable(spring.clone(), 100.0, AnimationSpec::Spring(soft));
        push_animatable(
            tween.clone(),
            100.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), linear())),
        );
        update_animations();
        // One 400 ms frame: no update runs in between, exactly what a stalled loop looks like.
        std::thread::sleep(Duration::from_millis(400));
        update_animations();
        let (s, t) = (spring.peek(), tween.peek());
        assert_eq!(t, 100.0, "the tween reads the clock, so it arrives ({t})");
        // A stiffness-200 spring needs ~4/omega0 = 280 ms to settle; 400 ms of real time is past that, so
        // anything under ~96 means the frame was truncated rather than integrated. (The analytic value at
        // 400 ms is 97.68; six 1/60 s semi-implicit steps land at 96.88, the integrator's own error, which
        // is why the bar is a range and not the analytic number.)
        assert!(
            s > 96.0,
            "the spring integrates the whole 400 ms it was given (at {s} of 100; 71.8 means the frame was cut at 10 x 1/60 s)"
        );
        clear_all_animations();
    }

    /// Compose's `Animatable.updateBounds`: the value never leaves its range and a motion that reaches a
    /// bound ends exactly there. Without the range the spring would settle at 5.0 and the fling would
    /// travel on to 900/4.2 = 214.3.
    #[test]
    fn the_handle_keeps_its_value_inside_its_bounds() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");

        a.set_bounds(0.0, 1.0);
        assert_eq!(a.bounds(), Some((0.0, 1.0)));
        a.animate_to(
            5.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness: 300.0,
                ..SpringSpec::default()
            }),
        );
        for _ in 0..60 {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 1.0, "the value stops exactly at the upper bound");
        assert!(!a.is_running(), "and the motion ends there rather than pushing on");

        // The same for a fling: 900 units/s of decay would travel 214.3, but the range ends it at 100.
        a.set_bounds(0.0, 100.0);
        a.snap_to(0.0);
        a.animate_decay(900.0, exponential_decay(4.2));
        for _ in 0..60 {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 100.0, "the fling parks on the bound");
        assert!(!a.is_running());

        // Dropping the range lets it travel anywhere again.
        a.clear_bounds();
        assert_eq!(a.bounds(), None);
        a.animate_to(
            200.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>)),
        );
        for _ in 0..40 {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 200.0, "with no bounds the target is reached");
        clear_all_animations();
    }

    /// `animate_decay` on the handle is the vector form: an `Offset` fling decays each axis on its own
    /// limit. (Bounds are a scalar concept — see the module note on `set_bounds`.)
    #[test]
    fn the_handle_can_fling_a_vector_value() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<crate::unit::Offset>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(crate::unit::Offset::new(0.0, 0.0));
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let o = held.borrow().clone().expect("the slot remembered the animatable");
        o.animate_decay(crate::unit::Offset::new(100.0, -60.0), exponential_decay(4.2));
        let mut frames = 0;
        while update_animations() && frames < 400 {
            std::thread::sleep(Duration::from_millis(8));
            frames += 1;
        }
        let v = o.value();
        assert!(
            (v.x - 100.0 / 4.2).abs() < 0.01 && (v.y + 60.0 / 4.2).abs() < 0.01,
            "each axis parks on 100/4.2 and -60/4.2 (at {v:?})"
        );
        assert!(!o.is_running());
        clear_all_animations();
    }

    /// Compose's `AnimationResult`: the three ways a motion can end were indistinguishable through a
    /// zero-argument callback — a motion cut short by the overrun guard reported "finished" like any other,
    /// and one stopped by a value range looked the same. Measured here for all three.
    #[test]
    fn a_completion_callback_is_told_why_the_motion_ended() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<MotionEnd>::new()));

        let record = {
            let seen = seen.clone();
            move |end: MotionEnd| seen.lock().unwrap().push(end)
        };
        let quick = || AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>));
        let step = |frames: u32| {
            for _ in 0..frames {
                update_animations();
                std::thread::sleep(Duration::from_millis(6));
            }
        };

        // 1. It arrived.
        a.animate_to_with_result(100.0, quick(), record.clone());
        step(30);
        assert_eq!(seen.lock().unwrap().len(), 1, "one motion, one report");
        assert_eq!(seen.lock().unwrap()[0].reason, MotionEndReason::Finished);

        // 2. A value range stopped it: 5.0 is outside (0, 1), so the value is written at the bound.
        a.set_bounds(0.0, 1.0);
        a.animate_to_with_result(5.0, quick(), record.clone());
        step(30);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[1].reason, MotionEndReason::BoundReached, "the bound is named");
        assert_eq!(a.value(), 1.0);
        a.clear_bounds();
        a.snap_to(0.0);

        // 3. The overrun guard cut it short: a spring too soft to settle inside five seconds. This is the
        // one reason with no Compose counterpart, and the only way to reach it is to wait it out.
        a.animate_to_with_result(
            100.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness: 0.5,
                damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
                threshold: 0.001,
                mass: 1.0,
            }),
            record.clone(),
        );
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(5600) {
            update_animations();
            std::thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(seen.lock().unwrap().len(), 3, "the guard reports instead of vanishing");
        assert_eq!(
            seen.lock().unwrap()[2].reason,
            MotionEndReason::Forced,
            "and it says it was cut short rather than finished"
        );
        clear_all_animations();
    }

    /// The estimator is Compose's own code, so it is checked against a method that is not: for the
    /// critically damped case the solution of a step from rest is `x(t) = D·(1 + ω t)·e^(-ω t)`, and the
    /// crossing of `|x| = delta` is single and monotone, so a bisection finds it independently.
    #[test]
    fn the_spring_duration_estimate_matches_a_bisection() {
        for (stiffness, displacement) in [(400.0f32, 100.0f32), (200.0, 100.0), (1000.0, 20.0)] {
            let omega = (stiffness as f64).sqrt();
            let x = |t: f64| displacement as f64 * (1.0 + omega * t) * (-omega * t).exp();
            let (mut lo, mut hi) = (0.0f64, 5.0f64);
            assert!(x(hi).abs() < 1.0, "the bracket has to contain the crossing");
            for _ in 0..90 {
                let mid = 0.5 * (lo + hi);
                if x(mid).abs() > 1.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let reference = 0.5 * (lo + hi);
            let estimate =
                estimate_spring_duration_seconds(stiffness, 1.0, 0.0, displacement, 1.0) as f64;
            assert!(
                (estimate - reference).abs() <= 0.01 + reference * 0.02,
                "the port agrees with the analytic crossing ({stiffness} over {displacement}: estimate {estimate:.4}s against {reference:.4}s)"
            );
        }
        let base = estimate_spring_duration_seconds(400.0, 1.0, 0.0, 100.0, 1.0);
        assert!(
            estimate_spring_duration_seconds(400.0, 1.0, 0.0, 100.0, 0.5) > base,
            "a tighter threshold takes longer"
        );
        assert!(
            estimate_spring_duration_seconds(1600.0, 1.0, 0.0, 100.0, 1.0) < base,
            "a stiffer spring takes less"
        );
        assert!(
            estimate_spring_duration_seconds(400.0, 0.0, 0.0, 100.0, 1.0).is_infinite(),
            "an undamped spring never settles"
        );
    }

    /// A spring now ENDS on Compose's clock rather than waiting for its velocity to fall under the
    /// threshold. Measured here: the finish time tracks the estimate, and the gap between the frame before
    /// the end and the exact target it writes is small (the integrator's own tracking error, not a jump).
    /// With only the velocity gate — the rule this replaced — the same spring ran about 0.15 s longer.
    #[test]
    fn a_spring_ends_on_the_estimated_clock() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let spec = SpringSpec {
            stiffness: 400.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            mass: 1.0,
            threshold: 1.0,
        };
        let estimate = estimate_spring_duration_seconds(400.0, 1.0, 0.0, -100.0, 1.0);
        let state = crate::runtime::state::State::new(0.0f32);
        push_animatable(state.clone(), 100.0, AnimationSpec::Spring(spec));
        let start = std::time::Instant::now();
        let mut frames = 0u32;
        let mut before_end = 0.0f32;
        while update_animations() && frames < 600 {
            before_end = state.peek();
            std::thread::sleep(Duration::from_millis(8));
            frames += 1;
        }
        let took = start.elapsed().as_secs_f32();
        assert_eq!(state.peek(), 100.0, "the motion ends exactly on the target");
        assert!(
            took - estimate < 0.08,
            "it ends on the estimate rather than waiting out the velocity gate (took {took:.3}s against an estimate of {estimate:.3}s)"
        );
        let gap = 100.0 - before_end;
        assert!(
            gap < 5.0,
            "the exact target only replaces a value the integrator had already brought to within a few pixels (gap {gap:.3})"
        );
        clear_all_animations();
    }

    /// Compose sequences two motions with a coroutine — `a.animateTo(one); a.animateTo(two)` inside a
    /// `LaunchedEffect` — so the second starts when the first has finished suspending. The queued version
    /// has to do the same: step two starts only after step one ends. Measured against the alternative it
    /// replaces (pushing both targets in turn, where the second simply replaces the first): after the first
    /// leg's 150 ms the value must still be on its way BACK, not already at the second target.
    #[test]
    fn a_sequence_runs_its_steps_one_after_another() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;

        a.animate_sequence([
            (100.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(150), linear()))),
            (0.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(150), linear()))),
        ]);

        // Midway through the first leg: up, and nowhere near the second target.
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(80) {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        let rising = a.value();
        assert!(
            rising > 20.0 && rising < 95.0,
            "the first leg is still running ({rising}), so the second has not started"
        );

        // Once both legs are done the value is at the second target and the queue is empty.
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(400) {
            update_animations();
            std::thread::sleep(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 0.0, "the sequence ended on its last step");
        assert!(!a.is_running());
        clear_all_animations();
    }

    /// `TimedAnimation` is the engine's own evaluation exposed as a function of time, so what it reports has
    /// to match what a running motion shows at the same moment — otherwise a hand-off would jump. Measured
    /// against a real registered animation, sampled every frame, for a spring, a tween and a fling.
    #[test]
    fn a_timed_animation_matches_the_running_one() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let specs: Vec<(&str, AnimationSpec)> = vec![
            (
                "spring",
                AnimationSpec::Spring(SpringSpec {
                    stiffness: 200.0,
                    ..SpringSpec::default()
                }),
            ),
            (
                "tween",
                AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(300), linear())),
            ),
            ("decay", AnimationSpec::Decay(exponential_decay(4.2))),
        ];
        for (name, spec) in specs {
            clear_all_animations();
            let state = crate::runtime::state::State::new(0.0f32);
            let timed = match spec.clone() {
                AnimationSpec::Decay(decay) => TimedAnimation::fling(0.0f32, 900.0, decay),
                other => TimedAnimation::target(0.0f32, 100.0, other),
            };
            match spec.clone() {
                AnimationSpec::Decay(decay) => push_decay(state.clone(), 900.0, decay),
                other => push_animatable(state.clone(), 100.0, other),
            }
            let start = std::time::Instant::now();
            let mut worst = 0.0f32;
            while start.elapsed() < Duration::from_millis(280) {
                update_animations();
                let gap = (state.peek() - timed.value_at(start.elapsed())).abs();
                worst = worst.max(gap);
            }
            // 5 % of the travel, not a pixel: the RUNNING motion integrates with whatever step its frames
            // give it (a long frame means coarse steps and an error of its own), so the two curves differ by
            // that error rather than by a disagreement about what is being computed. Under a tight loop the
            // measured worst gap is well under this; the deterministic pins are the analytic comparison and
            // the hand-off below.
            assert!(
                worst < 5.0,
                "the timed {name} tracks the running one (worst gap {worst} over 100 units)"
            );
        }
        clear_all_animations();
    }

    /// The query's own accuracy, frame rate aside: for a critically damped spring the exact solution of a
    /// step from rest is `x(t) = D(1 + ωt)e^(-ωt)`, and the query must reproduce it — this is what the
    /// shorter integration step buys, and it is deterministic (no clock involved). The residual is the
    /// integrator's own error at 1/480 s: measured 0.7 px at the fastest point of a 100-unit move.
    #[test]
    fn a_timed_spring_matches_the_analytic_solution() {
        let stiffness = 200.0f32;
        let omega = (stiffness as f64).sqrt();
        let whole = TimedAnimation::target(
            0.0f32,
            100.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness,
                damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
                mass: 1.0,
                threshold: 1.0,
            }),
        );
        for millis in [40u64, 90, 150, 210] {
            let t = Duration::from_millis(millis);
            let seconds = t.as_secs_f64();
            let analytic = 100.0 * (1.0 - (1.0 + omega * seconds) * (-omega * seconds).exp());
            let measured = whole.value_at(t);
            assert!(
                (measured as f64 - analytic).abs() < 1.0,
                "the query reproduces the analytic spring within a pixel at {millis} ms ({measured:.3} against {analytic:.3})"
            );
        }
    }

    /// What the type exists for: freeze a spring mid-flight and continue it from where it was. A linear
    /// spring resumed with the position AND velocity it had keeps the trajectory it would have followed, so
    /// a gesture hand-off does not jump — measured against the uninterrupted motion.
    #[test]
    fn a_mid_flight_hand_off_keeps_the_trajectory() {
        let spec = SpringSpec {
            stiffness: 200.0,
            ..SpringSpec::default()
        };
        let whole = TimedAnimation::target(0.0f32, 100.0, AnimationSpec::Spring(spec.clone()));
        let at = Duration::from_millis(150);
        let frozen_value = whole.value_at(at);
        let frozen_velocity = whole.velocity_at(at);
        let resumed = TimedAnimation::with_velocity(
            frozen_value,
            100.0,
            AnimationSpec::Spring(spec),
            frozen_velocity,
        );
        let after = resumed.value_at(Duration::from_millis(100));
        let reference = whole.value_at(at + Duration::from_millis(100));
        assert!(
            (after - reference).abs() < 0.5,
            "the resumed motion continues the same trajectory (resumed {after:.3}, uninterrupted {reference:.3}, frozen at {frozen_value:.3} moving {frozen_velocity:.1}/s)"
        );
    }

    /// Compose's `RepeatMode.Reverse` mirrors the iteration TIME and evaluates the base curve there, so the
    /// return leg is the same curve running backwards. Interpolating the return leg linearly — what this
    /// engine used to do — is a different motion for any eased base, and a loop builder could not even name one
    /// (`restart_tween`/`restart_keyframes` were the only curve constructors, and both imply Restart).
    #[test]
    fn a_reverse_loop_runs_its_curve_backwards() {
        use crate::animation::interpolator::Interpolator;
        let curve = |t: f32| interpolator::fast_out_slow_in().interpolate(t);
        let tween = AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(400),
            Arc::new(interpolator::fast_out_slow_in()) as Arc<dyn Interpolator>,
        ));
        let forward = loop_factor(Some(&tween), RepeatMode::Reverse, 0, 0.25);
        let backward = loop_factor(Some(&tween), RepeatMode::Reverse, 1, 0.25);
        assert!(
            (forward - curve(0.25)).abs() < 1e-6,
            "the first leg is the curve itself ({forward} against {})",
            curve(0.25)
        );
        assert!(
            (backward - curve(0.75)).abs() < 1e-6,
            "the return leg evaluates the curve at the mirrored time ({backward} against {})",
            curve(0.75)
        );
        assert!(
            (backward - 0.75).abs() > 0.1,
            "which is not the linear mirror (linear reads 0.75, the curve reads {backward})"
        );

        // Then through the engine: a reverse loop carrying that curve, sampled on its return leg.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            state.clone(),
            0.0f32,
            1.0f32,
            InfiniteRepeatableSpec::reverse(Duration::from_millis(400)).with_base(tween),
        );
        let start = std::time::Instant::now();
        let mut on_the_return_leg = None;
        while start.elapsed() < Duration::from_millis(560) {
            update_animations();
            let t = start.elapsed().as_secs_f32();
            if (0.48..0.52).contains(&t) {
                on_the_return_leg = Some(state.peek());
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let value = on_the_return_leg.expect("the loop was sampled on its return leg");
        assert!(
            value > 0.85,
            "half a second in, the backward leg is already near the loop's start value ({value}; a linear return would read about 0.75)"
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim2 = Motion::new(st2.clone());
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
        let mut anim = Motion::new(st.clone());
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
        let mut anim = Motion::new(st.clone());
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
