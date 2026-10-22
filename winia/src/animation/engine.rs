use super::*;

pub(super) fn spec_kind(spec: &AnimationSpec) -> String {
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

