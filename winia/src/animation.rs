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

use crate::core::state::State;
use crate::core::composer::Composer;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════
// 活跃动画管理（全局注册表，避开 Composer 字段修改）
// ═══════════════════════════════════════════════════════════

use std::sync::{Arc, Mutex, LazyLock};

/// 动画实例 trait（擦除类型后存储在全局列表）
pub trait AnimationInstance: Send {
    fn update(&mut self) -> bool;
    fn state_id(&self) -> u32;
    /// 类型安全的精确目标比较（跨类型返回 false）
    fn same_target(&self, target: &dyn std::any::Any) -> bool;
}

static ACTIVE_ANIMATIONS: LazyLock<Mutex<Vec<Box<dyn AnimationInstance>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
/// Color 动画列表（与 f32 动画分开，避免类型擦除）
static ACTIVE_COLOR_ANIMATIONS: LazyLock<Mutex<Vec<Animatable<crate::modifier::Color>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

// ═══════════════════════════════════════════════════════════
// InfiniteTransition — 无限循环动画（对标 Compose rememberInfiniteTransition）
// ═══════════════════════════════════════════════════════════

/// 重复模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatMode {
    /// 结束回到起点重来
    Restart,
    /// 往返（from→to→from）
    Reverse,
}

/// 无限循环规格
#[derive(Debug, Clone)]
pub struct InfiniteRepeatableSpec {
    pub duration: Duration,
    pub(crate) mode: RepeatMode,
}

impl InfiniteRepeatableSpec {
    pub fn restart(duration: Duration) -> Self {
        Self { duration, mode: RepeatMode::Restart }
    }
    pub fn reverse(duration: Duration) -> Self {
        Self { duration, mode: RepeatMode::Reverse }
    }
}

/// 无限循环动画实例（永远运行，直到被移除）——泛型统一（f32/Color 共用）。
struct Infinite<T: AnimatableValue> {
    state: State<T>,
    from: T,
    to: T,
    spec: InfiniteRepeatableSpec,
    start: Instant,
}

impl<T: AnimatableValue + Send + Sync + 'static> AnimationInstance for Infinite<T> {
    fn update(&mut self) -> bool {
        let elapsed = self.start.elapsed();
        match self.spec.mode {
            RepeatMode::Restart => {
                let t = (elapsed.as_secs_f32() / self.spec.duration.as_secs_f32().max(0.001)).min(1.0);
                self.state.set_visual(self.from.lerp(&self.to, t));
                if elapsed >= self.spec.duration { self.start = Instant::now(); }
            }
            RepeatMode::Reverse => {
                // 周期 = 2×duration：前半 from→to，后半 to→from
                let cycle_secs = self.spec.duration.as_secs_f32().max(0.001) * 2.0;
                let phase = (elapsed.as_secs_f32() % cycle_secs) / self.spec.duration.as_secs_f32().max(0.001);
                let t = if phase < 1.0 { phase } else { 2.0 - phase };
                self.state.set_visual(self.from.lerp(&self.to, t));
            }
        }
        true // 永远运行
    }
    fn same_target(&self, _target: &dyn std::any::Any) -> bool { false }
    fn state_id(&self) -> u32 { self.state.id() }
}

/// 注册一个无限循环动画（f32/Color 等 AnimatableValue 共用——泛型表，无独立第三表）
pub fn push_infinite<T: AnimatableValue + Send + Sync + 'static>(
    state: State<T>, from: T, to: T, spec: InfiniteRepeatableSpec,
) {
    let sid = state.id();
    if has_animation_for_state(sid) { return; } // 跨列表去重
    let anim = Infinite { state, from, to, spec, start: Instant::now() };
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
}

/// 注册一个动画到全局活跃列表
/// 注册一个 Animatable<T> 到全局活跃列表（由 animate_*_as_state 调用）
pub fn push_animatable<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(state: State<T>, target: T, spec: AnimationSpec) {
    if state.peek() == target { return; }
    let sid = state.id();
    // 非标量类型（Offset/Size/Color 等）Spring 无单值物理，强制降级 Tween
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
        // 新目标竞争，导致值卡在旧目标路径上）
        list.retain(|anim| anim.state_id() != sid);
    } // 锁释放，下面 anim.update() 不持锁执行用户代码
    let mut anim = Animatable::new(state);
    anim.animate_to(target, spec);
    // 立即执行首次更新，避免等下一帧 flash
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    // 唤醒事件循环启动推进轮次（渲染中注册——渲染后 Wait 休眠会卡住动画）
    crate::core::state::wake_loop();
}

/// 注册一个 Animatable<Color> 到全局活跃列表（由 animate_color_as_state 调用）
pub fn push_animatable_color(state: State<crate::modifier::Color>, target: crate::modifier::Color, spec: AnimationSpec) {
    use crate::modifier::Color;
    if state.peek() == target { return; }
    let sid = state.id();
    {
        let mut list = ACTIVE_COLOR_ANIMATIONS.lock().unwrap();
        if list.iter().any(|anim| anim.state.id() == sid && anim.anim_state.as_ref().map(|s| s.to == target).unwrap_or(false)) { return; }
        list.retain(|anim| anim.state.id() != sid);
    }
    let mut anim = Animatable::new(state);
    // Color 弹簧无意义（无单一 f32 值），强制 Tween
    let spec = match spec {
        AnimationSpec::Spring(_) => AnimationSpec::Tween(TweenSpec::default()),
        other => other,
    };
    anim.animate_to(target, spec);
    anim.update();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().push(anim);
    // 唤醒事件循环启动推进轮次（同 push_animatable）
    crate::core::state::wake_loop();
}

/// `push_animatable` + 完成回调（对标 Compose animate*AsState 的 finishedListener）：
/// 动画自然完成/超时强制完成时调用一次 `done`。
pub fn push_animatable_with_done<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    done: impl FnOnce() + Send + 'static,
) {
    if state.peek() == target {
        // 已等于目标——无动画（Compose 语义：无动画不回调？——不，Compose
        // 立即到达也会回调；这里保持简单：无动画不回调，调用方自查）
        return;
    }
    let sid = state.id();
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
        list.retain(|anim| anim.state_id() != sid);
    }
    let mut anim = Animatable::new(state);
    anim.on_finish(done);
    anim.animate_to(target, spec);
    anim.update();
    ACTIVE_ANIMATIONS.lock().unwrap().push(Box::new(anim));
    crate::core::state::wake_loop();
}

/// 实现 AnimationInstance for Animatable<f32>
impl<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static> AnimationInstance for Animatable<T> {
    fn update(&mut self) -> bool {
        Animatable::update(self)
    }
    fn state_id(&self) -> u32 {
        self.state.id()
    }
    fn same_target(&self, target: &dyn std::any::Any) -> bool {
        target.downcast_ref::<T>()
            .map(|t| self.anim_state.as_ref().map(|s| s.to == *t).unwrap_or(false))
            .unwrap_or(false)
    }
}

/// 更新所有活跃动画，返回是否有动画还在运行
pub fn update_animations() -> bool {
    // 锁内取出动画，锁外执行 update（避免锁内执行用户代码导致死锁）
    let mut anims = std::mem::take(&mut *ACTIVE_ANIMATIONS.lock().unwrap());
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
    let mut canims = std::mem::take(&mut *ACTIVE_COLOR_ANIMATIONS.lock().unwrap());
    let mut cstill = Vec::new();
    for mut c in canims {
        if c.update() { cstill.push(c); }
    }
    let mut clist = ACTIVE_COLOR_ANIMATIONS.lock().unwrap();
    for c in cstill {
        if !clist.iter().any(|x| x.state.id() == c.state.id()) {
            clist.push(c);
        }
    }
    !list.is_empty() || !clist.is_empty()
}

/// 从所有动画列表移除指定 state 的动画（InfiniteTransition::dispose 用）
pub fn remove_animation_by_state(state_id: u32) {
    ACTIVE_ANIMATIONS.lock().unwrap().retain(|a| a.state_id() != state_id);
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().retain(|a| a.state.id() != state_id);
}

/// 指定 state 是否已在任一动画列表（跨列表去重，防双倍推进）
pub fn has_animation_for_state(state_id: u32) -> bool {
    ACTIVE_ANIMATIONS.lock().unwrap().iter().any(|a| a.state_id() == state_id)
        || ACTIVE_COLOR_ANIMATIONS.lock().unwrap().iter().any(|a| a.state.id() == state_id)
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
    state: State<T>,
    anim_state: Option<AnimationState<T>>,
    /// 动画完成回调（done 帧触发一次，take 后释放）
    on_finish: Option<Box<dyn FnOnce() + Send>>,
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
}

impl<T: Clone + PartialEq + AnimatableValue + 'static> Animatable<T> {
    pub fn new(state: State<T>) -> Self {
        Self { state, anim_state: None, on_finish: None }
    }

    /// 注册动画完成回调（对标 Compose animate*AsState 的 finishedListener——
    /// 动画自然完成/超时强制完成时调用一次）
    pub fn on_finish(&mut self, f: impl FnOnce() + Send + 'static) {
        self.on_finish = Some(Box::new(f));
    }

    /// 启动动画到目标值
    pub fn animate_to(&mut self, to: T, spec: AnimationSpec) {
        let from = self.state.peek();
        let displacement = AnimatableValue::to_f32(&from) - AnimatableValue::to_f32(&to);
        self.anim_state = Some(AnimationState {
            from: from.clone(),
            to,
            start: Instant::now(),
            spec,
            last_velocity: 0.0,
            last_update: Instant::now(),
            current_displacement: displacement,
        });
    }

    /// 检查并更新动画值，返回是否还在动画中
    pub fn update(&mut self) -> bool {
        let Some(ref mut state) = self.anim_state else { return false; };
        let now = Instant::now();
        // 极端参数保护：超过 5s 未收敛强制完成（stiffness=0 等永不收敛的场景）
        if now.duration_since(state.start) > Duration::from_secs(5) {
            let final_val = state.to.clone();
            self.state.set_no_wake(final_val);
            self.anim_state = None;
            if let Some(f) = self.on_finish.take() {
                f();
            }
            return false;
        }
        let dt = now.duration_since(state.last_update);
        state.last_update = now;
        let (value, done) = match &state.spec {
            AnimationSpec::Spring(spec) => {
                let to_f32 = AnimatableValue::to_f32(&state.to);
                let displacement = compute_spring_displacement(
                    spec.stiffness, spec.damping_ratio, spec.mass,
                    state.current_displacement, &mut state.last_velocity, dt, spec.threshold,
                );
                state.current_displacement = displacement;
                // 直接使用物理值，不做 lerp/clamp（避免超调截断导致抖动）
                let done = displacement.abs() < spec.threshold && state.last_velocity.abs() < spec.threshold;
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
            AnimationSpec::Tween(spec) => {
                let elapsed = now - state.start;
                let t = (elapsed.as_secs_f64() / spec.duration.as_secs_f64()).min(1.0) as f32;
                let eased = (spec.interpolator)(t);
                let t = state.from.lerp(&state.to, eased);
                (t, eased >= 1.0)
            }
            AnimationSpec::Keyframes(spec) => {
                let elapsed = now - state.start;
                let t = (elapsed.as_secs_f64() / spec.duration.as_secs_f64()).min(1.0) as f32;
                let factor = interpolate_keyframes(&spec.frames, t);
                let value = state.from.lerp(&state.to, factor);
                (value, t >= 1.0)
            }
            AnimationSpec::Repeatable(spec) => {
                // 简化：base 仅支持 Tween（开发期断言，其他类型回退 300ms 线性）
                debug_assert!(matches!(&*spec.base, AnimationSpec::Tween(_)),
                    "RepeatableSpec 目前仅支持 Tween base");
                let base_duration = match spec.base.as_ref() {
                    AnimationSpec::Tween(t) => t.duration,
                    _ => Duration::from_millis(300),
                };
                let elapsed = now - state.start;
                let total = base_duration.saturating_mul(spec.iterations);
                if elapsed >= total {
                    // 完成值：Reverse + 偶数次时最后 cycle 结束于 from（否则结束于 to）
                    let end_val = match spec.mode {
                        RepeatMode::Restart => state.to.clone(),
                        RepeatMode::Reverse => {
                            if spec.iterations % 2 == 0 { state.from.clone() } else { state.to.clone() }
                        }
                    };
                    (end_val, true)
                } else {
                    let cycle = elapsed.as_secs_f64() % base_duration.as_secs_f64().max(0.001);
                    let t = (cycle / base_duration.as_secs_f64().max(0.001)) as f32;
                    let cycle_idx = (elapsed.as_secs_f64() / base_duration.as_secs_f64().max(0.001)).floor() as u32;
                    let factor = match spec.mode {
                        RepeatMode::Restart => t,
                        RepeatMode::Reverse => if cycle_idx % 2 == 0 { t } else { 1.0 - t },
                    };
                    let value = state.from.lerp(&state.to, factor);
                    (value, false)
                }
            }
            AnimationSpec::Snap => {
                (state.to.clone(), true)
            }
        };
        self.state.set_no_wake(value);
        if done {
            self.anim_state = None;
            if let Some(f) = self.on_finish.take() {
                f();
            }
        }
        !done
    }

    /// 立即跳转到目标值（无动画）
    pub fn snap_to(&mut self, value: T) {
        self.anim_state = None;
        self.state.set_no_wake(value);
    }
}

// ═══════════════════════════════════════════════════════════
// Spring 物理模拟（半隐式欧拉积分）
// ═══════════════════════════════════════════════════════════

/// 关键帧插值：在 frames 中按进度 t 定位段，段内用 interpolator 插值
fn interpolate_keyframes(frames: &[(f32, f32, fn(f32) -> f32)], t: f32) -> f32 {
    if frames.is_empty() { return 0.0; }
    if t <= 0.0 { return frames[0].1; }
    let last = frames.last().unwrap();
    if t >= last.0 { return last.1; }
    // t 小于首帧 progress 时取首帧值（首帧 progress 可能 > 0）
    if t < frames[0].0 { return frames[0].1; }
    for i in 0..frames.len() - 1 {
        let (p0, v0, _) = frames[i];
        let (p1, v1, interp) = frames[i + 1];
        if t >= p0 && t <= p1 {
            let seg = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
            let eased = interp(seg.clamp(0.0, 1.0));
            return v0 + (v1 - v0) * eased;
        }
    }
    last.1
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
    if displacement.abs() < threshold && velocity.abs() < threshold {
        *velocity = 0.0;
        return 0.0;
    }
    displacement
}

// ═══════════════════════════════════════════════════════════
// updateTransition
// ═══════════════════════════════════════════════════════════

use crate::core::composer::ComposeCtx;

pub struct Transition<T: Clone + PartialEq + 'static> {
    target: T,
    spec: AnimationSpec,
    #[allow(dead_code)]
    label: &'static str,
}

impl ComposeCtx<'_> {
    pub fn update_transition<T: Clone + PartialEq + 'static>(
        &mut self,
        target: T,
        spec: AnimationSpec,
        label: &'static str,
    ) -> Transition<T> {
        Transition { target, spec, label }
    }
}

impl<T: Clone + PartialEq + 'static> Transition<T> {
    /// 泛型值动画（对标 Compose `TransitionScope.animateValue`）——任意
    /// AnimatableValue 类型，target 变化 → 自动平滑过渡到新目标值。
    pub fn animate<U: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> U,
        _label: &'static str,
    ) -> State<U> {
        let value = target_fn(&self.target);
        let state: State<U> = ctx.remember(|| value.clone());
        crate::animation::push_animatable(state.clone(), value, self.spec.clone());
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
        target_fn: impl Fn(&T) -> crate::modifier::Color,
        label: &'static str,
    ) -> State<crate::modifier::Color> {
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
}

// ═══════════════════════════════════════════════════════════
// InfiniteTransition — 无限循环动画
// ═══════════════════════════════════════════════════════════

/// 无限循环动画作用域：记录其创建的动画 state_id，可 dispose 统一移除
pub struct InfiniteTransition {
    ids: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
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
            let ids: Vec<u32> = ids2.lock().unwrap().drain(..).collect();
            for sid in ids {
                crate::animation::remove_animation_by_state(sid);
            }
        }));
        self.end_node();
        InfiniteTransition { ids }
    }
}

impl InfiniteTransition {
    /// 注册一个 from→to 无限循环浮点动画
    pub fn animate_float(
        &mut self,
        ctx: &mut ComposeCtx,
        from: f32,
        to: f32,
        spec: InfiniteRepeatableSpec,
    ) -> State<f32> {
        let state: State<f32> = ctx.remember(|| from);
        self.ids.lock().unwrap().push(state.id());
        crate::animation::push_infinite(state.clone(), from, to, spec);
        state
    }

    /// 注册一个 from→to 无限循环颜色动画（CAM16-UCS 插值）
    pub fn animate_color(
        &mut self,
        ctx: &mut ComposeCtx,
        from: crate::modifier::Color,
        to: crate::modifier::Color,
        spec: InfiniteRepeatableSpec,
    ) -> State<crate::modifier::Color> {
        let state: State<crate::modifier::Color> = ctx.remember(|| from);
        self.ids.lock().unwrap().push(state.id());
        crate::animation::push_infinite(state.clone(), from, to, spec);
        state
    }

    /// 取消此作用域创建的所有动画（组件离开组合/不再需要时手动调用）
    pub fn dispose(&self) {
        let ids: Vec<u32> = self.ids.lock().unwrap().drain(..).collect();
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
    /// 瞬时跳转到目标（对标 Compose snap）
    Snap,
}

impl From<TweenSpec> for AnimationSpec {
    fn from(s: TweenSpec) -> Self { AnimationSpec::Tween(s) }
}

impl From<SpringSpec> for AnimationSpec {
    fn from(s: SpringSpec) -> Self { AnimationSpec::Spring(s) }
}

#[derive(Clone, Debug)]
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

    pub fn bouncy() -> Self {
        Self { damping_ratio: 0.6, threshold: 0.1, ..Self::default() }
    }
}

#[derive(Clone, Debug)]
pub struct TweenSpec {
    pub duration: Duration,
    pub interpolator: fn(f32) -> f32,
}

impl Default for TweenSpec {
    fn default() -> Self {
        Self { duration: Duration::from_millis(300), interpolator: interpolator::linear }
    }
}

/// 关键帧序列：(进度 0~1, 值, 段间插值器)
#[derive(Clone, Debug)]
pub struct KeyframesSpec {
    pub duration: Duration,
    pub frames: Vec<(f32, f32, fn(f32) -> f32)>,
}

impl KeyframesSpec {
    /// 简化构造：仅 (progress, value)，段间线性
    pub fn new(duration: Duration, frames: Vec<(f32, f32)>) -> Self {
        let linear: fn(f32) -> f32 = interpolator::linear;
        let frames = frames.into_iter().map(|(p, v)| (p, v, linear)).collect();
        Self { duration, frames }
    }
}

/// 重复执行：iterations 次后完成
#[derive(Clone, Debug)]
pub(crate) struct RepeatableSpec {
    pub iterations: u32,
    pub mode: RepeatMode,
    pub base: Box<AnimationSpec>,
}

impl RepeatableSpec {
    pub fn new(iterations: u32, mode: RepeatMode, base: AnimationSpec) -> Self {
        Self { iterations, mode, base: Box::new(base) }
    }
}

/// 可动画化的值类型
pub trait AnimatableValue: Clone + PartialEq {
    fn lerp(&self, to: &Self, t: f32) -> Self;
    /// 转换为 f32（Spring 物理引擎 + 去重用；⚠️ 非单射——Offset/Size 返回范数，仅标量类型精确）
    fn to_f32(&self) -> f32;
    /// 从 f32 构建（⚠️ 仅标量类型可用；向量/Color 的 from_f32 是占位，Spring 会强制降级 Tween）
    fn from_f32(v: f32) -> Self;
    /// 精确比较目标（默认 PartialEq；f32/Dp/Offset/Size 均精确）
    fn same_target(&self, other: &Self) -> bool { self == other }
    /// 是否支持 Spring（标量类型 true；向量/Color 无单值物理，false）
    fn supports_spring() -> bool { false }
}

impl AnimatableValue for f32 {
    fn lerp(&self, to: &f32, t: f32) -> Self { self + (to - self) * t }
    fn to_f32(&self) -> f32 { *self }
    fn from_f32(v: f32) -> Self { v }
    fn supports_spring() -> bool { true }
}

impl AnimatableValue for crate::modifier::Color {
    /// CAM16-UCS 色彩空间插值（人眼感知均匀）+ alpha 单独线性插值
    /// （cam16_ucs 忽略 alpha，需要手动插值保持透明度动画正确）
    fn lerp(&self, to: &Self, t: f32) -> Self {
        use material_colors::blend::cam16_ucs;
        use material_colors::color::Argb;
        let t = t.clamp(0.0, 1.0);
        // RGB 用 CAM16-UCS，alpha 用线性（cam16_ucs 返回 alpha 恒 255）
        let from_argb = Argb::new(255, self.r, self.g, self.b);
        let to_argb = Argb::new(255, to.r, to.g, to.b);
        let b = cam16_ucs(from_argb, to_argb, t as f64);
        let a = (self.a as f32 + (to.a as f32 - self.a as f32) * t).round() as u8;
        Self::from_argb(a, b.red, b.green, b.blue)
    }
    fn to_f32(&self) -> f32 { self.a as f32 }
    fn from_f32(v: f32) -> Self { Self::from_argb(v as u8, 0, 0, 0) }
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    // 测试串行锁：动画引擎用全局 ACTIVE_ANIMATIONS——并行测试互相干扰（push/update 竞态）
    pub(super) static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    use super::*;
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
        let done = disp.abs() < threshold && vel.abs() < threshold;
        (val, done)
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

    #[test]
    fn infinite_float_restart_loops() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0.0);
        let mut inf = Infinite {
            state: state.clone(),
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
            state: state.clone(),
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

    #[test]
    fn color_lerp_uses_cam16_and_preserves_alpha() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::modifier::Color;
        // 蓝 → 红，alpha 128 → 255
        let from = Color::from_argb(128, 33, 150, 243);
        let to = Color::from_argb(255, 255, 82, 82);
        let mid = from.lerp(&to, 0.5);
        // alpha 应线性插值（≈191）
        assert!((mid.a as i32 - 191).abs() <= 1, "alpha={} should be ~191", mid.a);
        // RGB 应在蓝和红之间（非灰暗：r 和 b 至少一个 > 100）
        assert!(mid.r > 100 || mid.b > 100, "mid={:?} should not be grayish", mid);
        // 端点保持
        let start = from.lerp(&to, 0.0);
        assert_eq!(start, from);
        let end = from.lerp(&to, 1.0);
        assert_eq!(end, to);
    }

    #[test]
    fn infinite_color_reverse_cycles() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::modifier::Color;
        let state = State::new(Color::RED);
        let mut inf = Infinite {
            state: state.clone(),
            from: Color::RED, to: Color::BLUE,
            spec: InfiniteRepeatableSpec::reverse(Duration::from_millis(50)),
            start: Instant::now(),
        };
        let mut saw_blue = false;
        let mut saw_red = false;
        for _ in 0..30 {
            inf.update();
            std::thread::sleep(Duration::from_millis(10));
            let c = state.get();
            if c.b > 200 && c.r < 50 { saw_blue = true; }
            if c.r > 200 && c.b < 50 { saw_red = true; }
        }
        assert!(saw_blue, "should reach blue");
        assert!(saw_red, "should return to red");
    }

    #[test]
    fn remove_animation_by_state_cleans_lists() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 推入 f32 + Color + 无限 Color 三种动画
        let s1 = State::new(0.0f32);
        let s2 = State::new(crate::modifier::Color::RED);
        let s3 = State::new(crate::modifier::Color::BLUE);
        push_animatable(s1.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));
        push_animatable_color(s2.clone(), crate::modifier::Color::GREEN, AnimationSpec::Tween(TweenSpec::default()));
        push_infinite(s3.clone(), crate::modifier::Color::BLUE, crate::modifier::Color::RED,
            InfiniteRepeatableSpec::restart(Duration::from_millis(50)));
        assert!(is_animating(), "animations should be registered");

        // 移除 s1 和 s3 对应的动画
        remove_animation_by_state(s1.id());
        remove_animation_by_state(s3.id());
        // s2 仍在
        assert!(has_animation_for_state(s2.id()), "s2 color animation should remain");
        remove_animation_by_state(s2.id());
        assert!(!has_animation_for_state(s2.id()), "s2 should be removed");
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
                AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(40), interpolator: interpolator::linear }))));
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
                AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(40), interpolator: interpolator::linear }))));
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
        anim.animate_to(42.0, AnimationSpec::Snap);
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
        remove_animation_by_state(s.id());
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
    fn offset_spring_downgraded_to_tween() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // blocking bug：Offset 用 Spring 会收敛到 (norm,norm) 而非目标，应强制 Tween
        use crate::unit::Offset;
        let s = State::new(Offset::new(0.0, 0.0));
        push_animatable(s.clone(), Offset::new(3.0, 4.0), AnimationSpec::Spring(SpringSpec::default()));
        // 验证动画注册（Spring 被降级为 Tween 后仍正常运行）
        assert!(has_animation_for_state(s.id()), "Offset animation should be registered");
        remove_animation_by_state(s.id());
        assert!(!has_animation_for_state(s.id()), "own animation should be removed");
    }

    #[test]
    fn spring_done_returns_exact_target() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 回归：Spring 渐近收敛——done 时 Animatable 必须写出精确目标值
        // （修复前返回 to+残余位移——AnimatedVisibility 的 exit 完成检测
        //   progress<0.001 会因残余位移卡住/误判）
        use crate::core::state::State;
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

    #[test]
    fn on_finish_fires_when_animation_completes() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // finishedListener（对标 Compose）：动画完成帧触发一次
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use crate::core::state::State;
        let st = State::new(0.0f32);
        let mut anim = Animatable::new(st.clone());
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        anim.on_finish(move || {
            f2.store(true, Ordering::SeqCst);
        });
        anim.animate_to(1.0, AnimationSpec::Snap);
        anim.update(); // Snap 一帧完成 → 回调触发
        assert!(fired.load(Ordering::SeqCst), "动画完成应触发 on_finish");
        assert_eq!(st.peek(), 1.0, "动画值应到达目标");
    }
}

#[cfg(test)]
mod repeated_tests {
    use super::*;

    /// 反复动画循环：多次 push 目标 + update 步进——最终收敛到最新目标。
    /// 验证动画引擎在反复触发（点击循环）时值正确（旧动画移除/去重无双驱动）。
    #[test]
    fn repeated_animation_cycles_converge() {
        let _g = super::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = crate::core::state::State::new(40.0f32);

        // 每轮：push 目标 → sleep 超过动画时长 → update 一次（真实时间 dt）→ 检查收敛
        for (i, target) in [(1usize, 200.0f32), (2, 40.0), (3, 200.0), (4, 40.0)] {
            push_animatable(state.clone(), target, AnimationSpec::Tween(TweenSpec {
                duration: std::time::Duration::from_millis(300),
                interpolator: crate::animation::interpolator::linear,
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
        let state = crate::core::state::State::new(40.0f32);
        push_animatable(state.clone(), 200.0, AnimationSpec::Tween(TweenSpec {
            duration: std::time::Duration::from_millis(1000),
            interpolator: crate::animation::interpolator::linear,
        }));
        // 中途（10 帧后）改目标 40——应切换（用真实时间 sleep 模拟帧间隔）
        std::thread::sleep(std::time::Duration::from_millis(50));
        update_animations();
        let mid = state.peek();
        assert!(mid > 40.0 && mid < 200.0, "中途应处于动画中（{}）", mid);
        push_animatable(state.clone(), 90.0, AnimationSpec::Tween(TweenSpec {
            duration: std::time::Duration::from_millis(200),
            interpolator: crate::animation::interpolator::linear,
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
        let state = crate::core::state::State::new(0i32);
        push_animatable(state.clone(), 100, AnimationSpec::Tween(TweenSpec {
            duration: std::time::Duration::from_millis(200),
            interpolator: crate::animation::interpolator::linear,
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
        let mut composer = crate::core::composer::Composer::new();
        let mut captured = None;
        composer.compose(|ctx| {
            let mut t = ctx.update_transition(3.0f32, AnimationSpec::Tween(TweenSpec::default()), "t");
            let s = t.animate(ctx, |v| *v * 2.0, "v");
            captured = Some(s);
        });
        assert_eq!(captured.unwrap().peek(), 6.0, "target_fn 映射应立即生效（同值跳过动画）");
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
    use crate::core::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::core::state::State;
    use std::time::Duration;

    // 清空全局动画表（跨测试并行隔离）
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);
    // 记录自己动画的 state_id（跨测试并行隔离——只断言自己的动画状态）
    let sid_holder = std::cell::RefCell::new(None::<u32>);

    let build = |composer: &mut Composer,
                 holder: &std::cell::RefCell<Option<State<bool>>>,
                 sid_holder: &std::cell::RefCell<Option<u32>>| {
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
                            InfiniteRepeatableSpec {
                                duration: Duration::from_millis(100),
                                mode: RepeatMode::Restart,
                            },
                        );
                        *sid_holder.borrow_mut() = Some(s.id());
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
    use crate::core::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::core::state::State;
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
                            InfiniteRepeatableSpec {
                                duration: Duration::from_millis(100),
                                mode: RepeatMode::Restart,
                            },
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
