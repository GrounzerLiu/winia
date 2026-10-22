use super::*;

// ═══════════════════════════════════════════════════════════

/// One value the engine can move: the state it writes, the motion in progress, and the callbacks.
/// Renamed from `Animatable` when the public Compose-shaped handle took that name.
pub(super) struct Motion<T: Clone + 'static> {
    pub(super) state: crate::runtime::state::Animating<T>,
    pub(super) anim_state: Option<AnimationState<T>>,
    /// 动画完成回调（done 帧触发一次，take 后释放）
    pub(super) on_finish: Option<Box<dyn FnOnce(MotionEnd) + Send>>,
    /// fling 撞到 clamp 边界时回调一次，参数为该帧尚未消费的瞬时速度。
    pub(super) on_boundary: Option<Box<dyn FnOnce(f32) + Send>>,
    /// fling 边界 clamp（Decay 专用）：每帧求值（读最新滚动极限）；
    /// 值被 clamp 改变 → 立即完成（对齐 Compose：fling 消耗完即停）
    pub(super) clamp: Option<Box<dyn Fn(f32) -> f32 + Send + Sync>>,
}

pub(super) struct AnimationState<T> {
    pub(super) from: T,
    pub(super) to: T,
    pub(super) start: Instant,
    pub(super) spec: AnimationSpec,
    pub(super) last_velocity: f32,
    pub(super) last_update: Instant,
    // Spring 持续的位移（累积值，非每帧重算）
    pub(super) current_displacement: f32,
    // Decay 初始速度（v0 常数——解析式需要，不被 last_velocity 覆盖）
    pub(super) initial_velocity: f32,
    /// 多分量值（`Size`/`Offset`）上跑 Spring 时，每个分量一条弹簧——Compose 的
    /// `VectorizedSpringSpec` 语义。`None` 表示单分量：走下面的标量位移
    /// （`current_displacement`），标量类型与 Tween/Keyframes 等一律走它。
    pub(super) lanes: Option<Vec<SpringLane>>,
    /// A fling on a multi-component value: one exponential decay per component — Compose's
    /// `DecayAnimationSpec<T>` vectorized through the type's `VectorConverter` (each axis its own
    /// velocity and its own rest threshold; the motion ends when EVERY component is at rest).
    /// `None` means the scalar path (`initial_velocity` + `from.to_f32()`).
    pub(super) decay_lanes: Option<Vec<DecayLane>>,
    /// How long a Spring is meant to take, from Compose's `estimateAnimationDurationMillis` — the clock
    /// the spec runs on. For a multi-component spring it is the longest axis, exactly as
    /// `VectorizedFloatAnimationSpec.getDurationNanos` takes the maximum. `None` for every other spec,
    /// which carries its own duration.
    pub(super) spring_duration: Option<Duration>,
}

/// One component's exponential decay: where it started and the velocity it started with. The motion is
/// analytic (no accumulators), so a lane needs nothing else.
pub(super) struct DecayLane {
    pub(super) from: f32,
    pub(super) velocity: f32,
}

/// 一条分量弹簧：目标值 + 该分量的位移与速度（标量路径的字段按分量复制一份）。
pub(super) struct SpringLane {
    pub(super) to: f32,
    pub(super) displacement: f32,
    pub(super) velocity: f32,
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
    pub(super) fn set_clamp_opt(&mut self, clamp: Option<Box<dyn Fn(f32) -> f32 + Send + Sync>>) {
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
    pub(super) fn retarget_velocity(&self) -> RetargetVelocity {
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
    pub(super) fn start_with_velocity(&mut self, to: T, spec: AnimationSpec, start_velocity: RetargetVelocity) {
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
pub(super) fn interpolate_keyframes(frames: &[(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)], t: f32) -> f32 {
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
pub(super) fn estimate_spring_duration_seconds(
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

pub(super) fn estimate_duration_seconds(
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
pub(super) fn estimate_under_damped(r: f64, r_imaginary: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c1 = p0;
    let c2 = (v0 - r * c1) / r_imaginary;
    let c = (c1 * c1 + c2 * c2).sqrt();
    (delta / c).ln() / r
}

/// Critically damped: `x(t) = (c1 + c2·t)·e^(r·t)`, solved by Newton–Raphson. The inflection argument is
/// Compose's: it decides whether the first or the last crossing of the threshold is the one that matters.
pub(super) fn estimate_critically_damped(r: f64, p0: f64, v0: f64, delta: f64) -> f64 {
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
pub(super) fn estimate_over_damped(r1: f64, r2: f64, p0: f64, v0: f64, delta: f64) -> f64 {
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

pub(super) fn newton_step(x: f64, f: impl Fn(f64) -> f64, f_prime: impl Fn(f64) -> f64) -> f64 {
    x - f(x) / f_prime(x)
}

/// The interpolation factor a loop is at — Compose's `VectorizedInfiniteRepeatableSpec.getValueFromNanos`:
/// `Restart` plays the base curve forward in every iteration, and `Reverse` mirrors the iteration TIME on the
/// odd ones (`if (iteration % 2 != 0) durationNanos - iterationTime`) and evaluates the base there. Mirroring
/// the time rather than the value is what makes the return leg the same curve running backwards; interpolating
/// the return leg linearly — what winia did — is a different motion for any eased base.
pub(super) fn loop_factor(base: Option<&DurationSpec>, mode: RepeatMode, cycle: u32, phase: f32) -> f32 {
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
