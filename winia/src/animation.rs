//! 动画系统 — 对齐 Jetpack Compose 动画三层 API
//!
//! ## 层级
//! - **高层**: `animate_*_as_state` — 单值动画，开箱即用
//! - **中层**: `update_transition` — 多属性协同动画
//! - **底层**: `Animatable` — 完全控制动画播放
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
    fn is_animating_to(&self, target: f32) -> bool;
    fn state_id(&self) -> u32;
}

static ACTIVE_ANIMATIONS: LazyLock<Mutex<Vec<Box<dyn AnimationInstance>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// 注册一个动画到全局活跃列表
pub fn push_animation(anim: Box<dyn AnimationInstance + 'static>) {
    ACTIVE_ANIMATIONS.lock().unwrap().push(anim);
}

/// 注册一个 Animatable<f32> 到全局活跃列表（由 animate_float_as_state 调用）
pub fn push_animatable(state: State<f32>, target: f32, spec: AnimationSpec) {
    let current = state.get();
    if (current - target).abs() < f32::EPSILON { return; }
    let sid = state.id();
    let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
    // 检查是否已有同目标动画运行中（同目标直接跳过，防止每帧重启）
    if list.iter().any(|anim| anim.state_id() == sid && anim.is_animating_to(target)) { return; }
    // 同一 state 但目标不同时移除旧动画（用户改变了目标值）
    list.retain(|anim| anim.state_id() != sid);
    let mut anim = Animatable::new(state);
    anim.animate_to(target, spec);
    // 立即执行首次更新，避免等下一帧 flash
    anim.update();
    list.push(Box::new(anim));
}

/// 实现 AnimationInstance for Animatable<f32>
impl AnimationInstance for Animatable<f32> {
    fn update(&mut self) -> bool {
        self.update()
    }
    fn is_animating_to(&self, target: f32) -> bool {
        self.anim_state.as_ref().map(|s| AnimatableValue::to_f32(&s.to) - target).unwrap_or(f32::INFINITY).abs() < f32::EPSILON
    }
    fn state_id(&self) -> u32 {
        self.state.id()
    }
}

/// 更新所有活跃动画，返回是否有动画还在运行
pub fn update_animations() -> bool {
    let mut list = ACTIVE_ANIMATIONS.lock().unwrap();
    let running = list.len();
    // retain_mut 是 nightly API，手动 filter
    let mut i = 0;
    while i < list.len() {
        if list[i].update() {
            i += 1;
        } else {
            list.swap_remove(i);
        }
    }
    !list.is_empty()
}

// ═══════════════════════════════════════════════════════════
// Animatable — 底层动画值（对标 Compose Animatable）
// ═══════════════════════════════════════════════════════════

/// 可动画化的单一值
pub struct Animatable<T: Clone + 'static> {
    state: State<T>,
    anim_state: Option<AnimationState<T>>,
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

impl<T: Clone + AnimatableValue + 'static> Animatable<T> {
    pub fn new(state: State<T>) -> Self {
        Self { state, anim_state: None }
    }

    /// 启动动画到目标值
    pub fn animate_to(&mut self, to: T, spec: AnimationSpec) {
        let from = self.state.get();
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
        let dt = now.duration_since(state.last_update);
        state.last_update = now;
        let (value, done) = match &state.spec {
            AnimationSpec::Spring(spec) => {
                // Spring 用 f32 数值计算
                let from_f32 = AnimatableValue::to_f32(&state.from);
                let to_f32 = AnimatableValue::to_f32(&state.to);
                let displacement = compute_spring_displacement(
                    spec.stiffness, spec.damping_ratio, spec.mass,
                    state.current_displacement, &mut state.last_velocity, dt, spec.threshold,
                );
                let spring_val = to_f32 + displacement;
                let t = ((spring_val - from_f32) / (to_f32 - from_f32).max(f32::EPSILON)).clamp(0.0, 1.0);
                let value = state.from.lerp(&state.to, t);
                (value, displacement.abs() < spec.threshold && state.last_velocity.abs() < spec.threshold)
            }
            AnimationSpec::Tween(spec) => {
                let elapsed = now - state.start;
                let t = (elapsed.as_secs_f64() / spec.duration.as_secs_f64()).min(1.0) as f32;
                let eased = (spec.interpolator)(t);
                let t = state.from.lerp(&state.to, eased);
                (t, eased >= 1.0)
            }
        };
        self.state.update(|v| *v = value);
        if done { self.anim_state = None; }
        !done
    }

    /// 立即跳转到目标值（无动画）
    pub fn snap_to(&mut self, value: T) {
        self.anim_state = None;
        self.state.update(|v| *v = value);
    }
}

// ═══════════════════════════════════════════════════════════
// Spring 物理模拟（半隐式欧拉积分）
// ═══════════════════════════════════════════════════════════

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
    pub fn animate_float(
        &mut self,
        ctx: &mut ComposeCtx,
        target_fn: impl Fn(&T) -> f32,
        _label: &'static str,
    ) -> State<f32> {
        let value = target_fn(&self.target);
        let state: State<f32> = ctx.remember(|| value);
        crate::animation::push_animatable(state.clone(), value, self.spec.clone());
        state
    }
}

#[derive(Clone)]
pub enum AnimationSpec {
    Spring(SpringSpec),
    Tween(TweenSpec),
}

#[derive(Clone)]
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
            stiffness: 1500.0,
            mass: 1.0,
            threshold: 0.01,
        }
    }
}

impl SpringSpec {
    pub fn bouncy() -> Self {
        Self { damping_ratio: 0.6, threshold: 0.5, ..Self::default() }
    }
}

#[derive(Clone)]
pub struct TweenSpec {
    pub duration: Duration,
    pub interpolator: fn(f32) -> f32,
}

impl Default for TweenSpec {
    fn default() -> Self {
        Self { duration: Duration::from_millis(300), interpolator: interpolator::linear }
    }
}

/// 可动画化的值类型
pub trait AnimatableValue: Clone {
    fn lerp(&self, to: &Self, t: f32) -> Self;
    /// 转换为 f32（Spring 物理引擎使用）
    fn to_f32(&self) -> f32;
}

impl AnimatableValue for f32 {
    fn lerp(&self, to: &f32, t: f32) -> Self { self + (to - self) * t }
    fn to_f32(&self) -> f32 { *self }
}
