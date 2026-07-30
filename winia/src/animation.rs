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
}

static ACTIVE_ANIMATIONS: LazyLock<Mutex<Vec<Box<dyn AnimationInstance>>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

/// 注册一个动画到全局活跃列表
pub fn push_animation(anim: Box<dyn AnimationInstance + 'static>) {
    ACTIVE_ANIMATIONS.lock().unwrap().push(anim);
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
        Self { damping_ratio: 0.4, ..Self::default() }
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
}

impl AnimatableValue for f32 {
    fn lerp(&self, to: &f32, t: f32) -> Self { self + (to - self) * t }
}
