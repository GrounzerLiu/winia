use crate::fling::FlingBehavior as _;
#[cfg(test)]
use crate::fling::SnapInfo;

#[derive(Debug, Clone)]
pub struct ScrollState {
    /// 当前偏移
    pub offset: crate::runtime::state::State<f32>,
    /// 是否正在滚动
    pub is_scroll_in_progress: crate::runtime::state::State<bool>,
    /// How far a fling may travel: written back by the layout as `content height - viewport height`, or
    /// `f32::MAX` while that is still unknown.
    ///
    /// A REAL `0` therefore means the content fits and the container cannot move at all. It used to be
    /// read as "unknown" too (`if limit > 0.0 { limit } else { f32::MAX }`), so a container whose content
    /// exactly fitted still flinged: a drag was correctly clamped by the node's own `content - viewport`,
    /// but the momentum after the release was not (measured on a three-item menu: a drag that moved
    /// nothing left the offset at 74).
    pub(crate) fling_limit: crate::runtime::state::Backchannel<f32>,
    /// 滚动活动脉冲（P1-3：边界滚轮点亮用——offset 到界无变化时脉冲检测不到，
    /// 故分发层在"命中但消费为 0"的 wheel 上自增本计数，scrollbar 侧以变化
    /// 为脉冲点亮 fade。u64 单调，set 恒变→恒通知，无需 PartialEq 去重顾虑）。
    pub(crate) scroll_pulse: crate::runtime::state::State<u64>,
    /// The snapping configuration, or `None` for a list that does not snap. Written back by a PAGED
    /// list's measure (`LazyList::snap_paging`) and read by [`ScrollState::fling_with_boundary`].
    pub(crate) snap: crate::runtime::state::Backchannel<Option<crate::fling::SnapInfo>>,
    /// Where a page step is currently heading, if one is in flight. A second notch has to step from
    /// the page the first one targeted: reading the live offset mid-spring lands inside the same page
    /// and the notch is lost (four fast notches moved three pages).
    pub(crate) step_goal: std::sync::Arc<std::sync::Mutex<Option<f32>>>,
    /// The strategy this container flings with, or `None` for the built-in
    /// [`SnapFlingBehavior`](crate::fling::SnapFlingBehavior): a paged container settles on a page,
    /// anything else decays. A component assembles this from its own builder parameter; a caller
    /// holding a bare `ScrollState` can set it directly.
    pub fling_behavior: Option<std::sync::Arc<dyn crate::fling::FlingBehavior>>,
}

/// Whether a spring spec can produce a finite motion at all. A NaN anywhere in it poisons the value
/// the same way a NaN target does, and the scroll offset is the one state a container cannot recover
/// from - so a plan is checked before it is executed, not trusted.
fn spring_is_finite(spec: &crate::animation::SpringSpec) -> bool {
    spec.stiffness.is_finite()
        && spec.damping_ratio.is_finite()
        && spec.mass.is_finite()
        && spec.threshold.is_finite()
}

/// The landing position of a snapping fling — the math lives in [`crate::fling::snap_target`], which is
/// also what `SnapFlingBehavior` plans with, so a test here and a running pager cannot disagree. This
/// wrapper only exists to keep the unit tests below working against a `SnapInfo`.
#[cfg(test)]
pub(crate) fn snap_target(current: f32, velocity: f32, snap: &SnapInfo, limit: f32) -> f32 {
    crate::fling::snap_target(current, velocity, snap, limit)
}

impl ScrollState {
    pub fn new() -> Self {
        ScrollState {
            offset: crate::runtime::state::State::new(0.0),
            is_scroll_in_progress: crate::runtime::state::State::new(false),
            fling_limit: crate::runtime::state::Backchannel::new(f32::MAX),
            scroll_pulse: crate::runtime::state::State::new(0),
            snap: crate::runtime::state::Backchannel::new(None),
            step_goal: std::sync::Arc::new(std::sync::Mutex::new(None)),
            fling_behavior: None,
        }
    }

    /// 立即滚动到指定位置
    pub fn scroll_to(&self, value: f32, max_offset: f32) {
        self.offset.set(value.clamp(0.0, max_offset));
    }

    /// 动画滚动到指定位置（对标 Compose `animateScrollTo`）——用 spring 平滑
    /// 驱动 offset；目标 clamp 到 [0, max_offset]。滚动期间
    /// `is_scroll_in_progress` 为 true，动画结束回 false。
    pub fn animate_scroll_to(&self, value: f32, max_offset: f32, spec: crate::animation::AnimationSpec) {
        let target = value.clamp(0.0, max_offset);
        if (self.offset.peek() - target).abs() < 0.5 {
            return;
        }
        self.is_scroll_in_progress.set(true);
        let off = self.offset.clone();
        let done_flag = self.is_scroll_in_progress.clone();
        crate::animation::push_animatable_with_done(
            off,
            target,
            spec,
            move || {
                let _ = done_flag.set(false);
            },
        );
    }

    /// 惯性滚动（对标 Compose flingBehavior）：以 `velocity`(px/s) 启动指数衰减
    /// 滚动，撞到滚动极限立即停止（极限由布局期回写——LazyColumn 精确值、
    /// 普通容器由 measure_node 计算）。手动滚动（wheel/拖拽）自动取消进行中
    /// 的 fling（apply_scroll_delta 内 cancel）。滚动期间 `is_scroll_in_progress`
    /// 为 true，动画结束回 false。
    pub fn fling(&self, velocity: f32) {
        self.fling_with_boundary(velocity, |_| {});
    }

    /// Whether this container settles its flings on a page boundary — the question a CALLER has to ask
    /// before it decides whether a release is worth starting a fling for at all. Same filter as the
    /// snap branch of [`Self::fling_with_boundary`], so the two can never disagree about what "snapping"
    /// means.
    pub(crate) fn snaps(&self) -> bool {
        self.snap.peek().is_some_and(|s| s.step.is_finite() && s.step > 0.0)
    }

    /// Whether a release is worth asking a strategy about: the container snaps, or it carries a
    /// [`FlingBehavior`](crate::fling::FlingBehavior) of its own.
    ///
    /// The gesture path's 50 px/s floor is a DECAY floor (it stops the tail of a drag from banking
    /// momentum), and both of these settle a release that carries no momentum of its own - so the floor
    /// is skipped for them. Without this the custom strategy on a list with no snap spec would never
    /// see a slow release at all, which is the one release a positional snap exists for.
    pub(crate) fn wants_fling(&self) -> bool {
        self.snaps() || self.fling_behavior.is_some()
    }

    /// Move ONE snap position forward or back, and animate there.
    ///
    /// A wheel notch on a paged container should move a page, not a few pixels: a viewport-sized page
    /// is far wider than one wheel delta, so the ordinary "settle on the nearest position" returns the
    /// list to where it started and the wheel reads as broken. Compose does not special-case this (a
    /// wheel on a pager barely moves it); winia does, for paged containers only - `snap` is `Some` -
    /// so a pager and a date picker move one page per notch while a plain list keeps scrolling
    /// smoothly.
    /// Returns whether a step was started. A caller that gets `false` falls back to the pixel scroll,
    /// so a container whose snap spec has not been resolved yet is never worse off than before.
    pub(crate) fn snap_page(&self, forward: bool, reverse: bool) -> bool {
        let Some(spec) = self
            .snap
            .peek()
            .filter(|s| s.step.is_finite() && s.step > 0.0)
        else {
            return false;
        };
        let raw_limit = self.fling_limit.peek();
        let limit = if raw_limit.is_finite() && raw_limit >= 0.0 { raw_limit } else { f32::MAX };
        // `reverseLayout` mirrors the scroll direction (the pixel and fling paths do the same), so a
        // "forward" notch on a reversed container steps back.
        let forward = forward != reverse;
        // Step from where a step in flight is heading, not from the live offset: a second notch
        // arriving mid-spring otherwise reads a position inside the SAME page and re-targets it.
        let mut goal = self.step_goal.lock().unwrap_or_else(|e| e.into_inner());
        // A goal means something only while the spring that set it is still running. A drag, a fling or
        // a programmatic jump CANCELS that animation without firing its completion callback, and an
        // abandoned target would otherwise survive: the next notch would step from a position the list
        // never reached, skipping a page or refusing one at the end.
        if goal.is_some() && !crate::animation::has_animation_for_state(self.offset.state_id()) {
            *goal = None;
        }
        let base = goal.unwrap_or_else(|| self.offset.peek());
        let position = (base - spec.anchor) / spec.step;
        let index = if forward {
            position.floor() + 1.0
        } else {
            (position.ceil() - 1.0).max(0.0)
        };
        let target = spec.anchor + index * spec.step;
        // Refuse rather than clamp: `limit` is `content - viewport`, which with a trailing
        // `contentPadding` is PAST the last snap position, so clamping would scroll into the padding and
        // the settle would drag it back (Compose guards this explicitly:
        // PagerSnapLayoutInfoProvider.kt:181-187 zeroes both bounds when the list cannot scroll forward).
        if limit < f32::MAX && target > limit + 0.5 {
            return false;
        }
        let target = target.clamp(0.0, limit);
        if (target - base).abs() < 0.5 {
            return false;
        }
        *goal = Some(target);
        drop(goal);
        self.is_scroll_in_progress.set(true);
        let done = self.is_scroll_in_progress.clone();
        let goal_cell = std::sync::Arc::clone(&self.step_goal);
        crate::animation::push_animatable_with_velocity_and_done(
            self.offset.clone(),
            target,
            spec.settle_spec(),
            0.0,
            move || {
                let mut goal = goal_cell.lock().unwrap_or_else(|e| e.into_inner());
                if goal.is_some_and(|g| (g - target).abs() < 0.5) {
                    *goal = None;
                }
                done.set(false);
            },
        );
        true
    }

    /// 启动 fling，并在 child 撞到边界时把瞬时剩余速度交给调用方。
    ///
    /// A snapping container takes the snap branch instead of the decay: one spring, launched with the
    /// gesture's velocity, landing on a page. See [`Self::snaps`] and the note on the velocity floor below.
    pub fn fling_with_boundary(&self, velocity: f32, on_boundary: impl FnOnce(f32) + Send + 'static) {
        if !velocity.is_finite() {
            return;
        }
        // The strategy decides what this release turns into. `None` means the built-in one, which is
        // the behaviour this method had before the trait existed: a paged container settles on a page
        // boundary with ONE spring (launched with the gesture's velocity, so the momentum is carried
        // into the settle - Compose zeroes `calculateApproachOffset` for material3, `DatePicker.kt:749-753`,
        // and `tryApproach` then animates nothing, `SnapFlingBehavior.kt:165-176`), and anything else
        // keeps its momentum as a decay.
        let context = crate::fling::FlingContext {
            offset: self.offset.peek(),
            velocity,
            limit: self.fling_limit.peek(),
            snap: self.snap.peek(),
            density: crate::runtime::density::current_density().density,
        };
        let plan = match &self.fling_behavior {
            Some(behavior) => behavior.plan(context),
            None => crate::fling::SnapFlingBehavior.plan(context),
        };
        // The 1 px/s floor is a DECAY floor: it stops a scrollbar's trailing jitter from starting a
        // decay nothing asked for. A plan that settles somewhere has no decay to suppress, so the floor
        // is applied to the ANSWER, not to the container — a custom strategy on a list without a snap
        // spec still sees its slow release. (The gesture path's 50 px/s floor is keyed on
        // `wants_fling` for the same reason.) Compose has no floor at all: `Scrollable.kt:857-881`
        // calls `performFling` on every release.
        if velocity.abs() < 1.0 && matches!(&plan, crate::fling::FlingPlan::Decay { .. }) {
            return;
        }
        // A gesture-driven fling supersedes any page step in flight: whatever that step was heading for
        // is abandoned, and the next notch has to step from wherever the fling leaves the list.
        *self.step_goal.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.is_scroll_in_progress.set(true);
        let off = self.offset.clone();
        let limit = self.fling_limit.clone();
        let done_flag = self.is_scroll_in_progress.clone();

        match plan {
            crate::fling::FlingPlan::Spring { target, spec } => {
                // A strategy may aim past the content on purpose (a "bounce past the end" plan); the
                // container's own limit is still what the value is allowed to reach.
                let limit_now = limit.peek().max(0.0);
                let requested = target;
                let target = target.clamp(0.0, limit_now);
                if !target.is_finite() || !spring_is_finite(&spec) {
                    // A NaN plan must not reach the offset: the engine's overrun guard writes the
                    // spring's target verbatim after five seconds (animation.rs), and a NaN offset is
                    // something a container never recovers from.
                    self.is_scroll_in_progress.set(false);
                    return;
                }
                // A new fling supersedes whatever is still running. Without this, a second release
                // whose plan lands on the SAME target hits the engine's same-target dedup, which
                // reports `done` immediately and clears `is_scroll_in_progress` while the first spring
                // is still moving.
                crate::animation::cancel_animation(&self.offset);
                // A plan that asked to go PAST the container's limit cannot use that travel. A spring
                // has no leftover velocity to hand on the way a decay does, but the post-fling chain
                // still has to learn that the child reached its end - so the boundary is reported when
                // the spring lands. (A plan that lands inside the limit reports nothing: the built-in
                // one clamps itself, so a pager's behaviour here is unchanged.)
                let clamped = (target - requested).abs() > 0.5;
                let settle = done_flag.clone();
                let mut boundary = Some(on_boundary);
                crate::animation::push_animatable_with_velocity_and_done(
                    off,
                    target,
                    crate::animation::AnimationSpec::Spring(spec),
                    velocity,
                    move || {
                        settle.set(false);
                        if clamped {
                            if let Some(report) = boundary.take() {
                                report(0.0);
                            }
                        }
                    },
                );
            }
            crate::fling::FlingPlan::Decay { spec } => {
                crate::animation::push_fling_with_boundary(
                    off.clone(),
                    velocity,
                    spec,
                    {
                        let limit = limit.clone();
                        move |o| {
                            // The limit IS the truth here: `f32::MAX` while the layout has not measured
                            // the content yet, `0` when the content fits and this container cannot move.
                            o.clamp(0.0, limit.peek())
                        }
                    },
                    on_boundary,
                    move || {
                        done_flag.set(false);
                    },
                );
            }
            crate::fling::FlingPlan::None => {
                // Nothing to run: a release the strategy declined. Clearing the flag here is what a
                // caller's "is this container scrolling" check would otherwise miss.
                self.is_scroll_in_progress.set(false);
            }
        }
    }

    /// Cancel an in-flight fling - called before manual input or a programmatic jump. A page step in
    /// flight is abandoned with it, so its goal does not leak into the next notch.
    pub fn cancel_fling(&self) {
        crate::animation::cancel_animation(&self.offset);
        *self.step_goal.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl Default for ScrollState {
    fn default() -> Self { Self::new() }
}

