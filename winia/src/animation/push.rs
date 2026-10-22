use super::*;

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
