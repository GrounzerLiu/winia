//! 动画系统 — 值随时间平滑过渡
//!
//! 用法:
//! ```ignore
//! let offset = animate_as_state(target, Duration::from_millis(300), Easing::EaseOut);
//! // offset.get() 每帧自动插值更新
//! ```

use crate::core::state::State;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── Easing ──

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    BounceOut,
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(2),
            Easing::EaseInOut => {
                if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 }
            }
            Easing::BounceOut => {
                let (n1, d1) = (7.5625, 2.75);
                if t < 1.0 / d1 { n1 * t * t }
                else if t < 2.0 / d1 { let t = t - 1.5 / d1; n1 * t * t + 0.75 }
                else if t < 2.5 / d1 { let t = t - 2.25 / d1; n1 * t * t + 0.9375 }
                else { let t = t - 2.625 / d1; n1 * t * t + 0.984375 }
            }
        }
    }
}

// ── 动画条目 ──

struct AnimEntry {
    state: State<f32>,
    start: f32,
    target: f32,
    duration: Duration,
    easing: Easing,
    start_time: Instant,
    /// 完成回调
    on_finish: Option<Box<dyn FnOnce() + Send>>,
}

static ANIMATIONS: Mutex<Vec<AnimEntry>> = Mutex::new(Vec::new());

/// 创建一个动画化的 State<f32>
///
/// 首次调用时从 initial 开始向 target 动画。后续 target 变化时从当前值向新 target 过渡。
pub fn animate_as_state(initial: f32, target: f32, duration: Duration, easing: Easing) -> State<f32> {
    let state = State::new(initial);
    ANIMATIONS.lock().unwrap().push(AnimEntry {
        state: state.clone(),
        start: initial,
        target,
        duration,
        easing,
        start_time: Instant::now(),
        on_finish: None,
    });
    state
}

/// 改变已有动画的目标值，从当前值平滑过渡
pub fn animate_to(state: &State<f32>, target: f32, duration: Duration, easing: Easing) {
    animate_to_cb(state, target, duration, easing, None::<fn()>);
}

/// 带完成回调的 animate_to
pub fn animate_to_cb(
    state: &State<f32>,
    target: f32,
    duration: Duration,
    easing: Easing,
    on_finish: Option<impl FnOnce() + Send + 'static>,
) {
    let current = state.get();
    ANIMATIONS.lock().unwrap().push(AnimEntry {
        state: state.clone(),
        start: current,
        target,
        duration,
        easing,
        start_time: Instant::now(),
        on_finish: on_finish.map(|f| Box::new(f) as Box<dyn FnOnce() + Send>),
    });
}

/// 每帧调用：推进所有动画。返回 true 表示还有动画在运行（需要继续重绘）。
pub fn tick() -> bool {
    let mut anims = ANIMATIONS.lock().unwrap();
    let now = Instant::now();
    let mut any_active = false;

    let mut i = 0;
    while i < anims.len() {
        let elapsed = now.duration_since(anims[i].start_time);
        let progress = (elapsed.as_secs_f32() / anims[i].duration.as_secs_f32()).min(1.0);
        let t = anims[i].easing.apply(progress);
        let value = anims[i].start + (anims[i].target - anims[i].start) * t;

        anims[i].state.set(value);

        if progress >= 1.0 {
            // 动画完成，调用回调
            if let Some(cb) = anims[i].on_finish.take() {
                cb();
            }
            anims.remove(i);
        } else {
            any_active = true;
            i += 1;
        }
    }

    any_active
}
