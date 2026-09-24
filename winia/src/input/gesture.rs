//! 手势状态机——对标 Compose `detectTapGestures` / `detectDragGestures`。
//!
//! 纯逻辑（不依赖 winit/skia）：app.rs 把 pointer down/move/up 事件喂进来，
//! 状态机输出 `GestureAction`（tap / double-tap / long-press / drag 系列），
//! 由 app.rs 执行对应的 Modifier 回调。
//!
//! 语义（对齐 Compose）：
//! - **touch slop**：按下后移动超过阈值才判定为拖拽（tap 取消）
//! - **双击**：两次 tap 间隔 < 300ms 且位置差 < 50px → double-tap
//! - **延迟 tap**（Compose 语义）：节点注册 onDoubleTap 时，onTap 延迟到双击
//!   窗口结束（300ms）再触发；窗口内第二次按下同节点 → 取消第一次 tap（只发
//!   DoubleTap）；超时或按下其他节点 → 补发 Tap。`PendingTap` 承载该状态。
//! - **长按**：down 持续到 500ms 且未超过 slop → 在**到点当下**触发（对齐 Compose
//!   `detectTapGestures`：`onLongPress` 在按住期间到达，不等抬手）。由事件循环轮询
//!   `poll_long_press` 驱动，`WaitUntil` 保证 idle 时也会在那个时刻醒来；抬手只是
//!   结束手势，不会再发一次长按，也不再发 tap。
//! - **drag capture**：drag 开始后事件跟随手势节点（指针移出组件仍接收）
//! - 拖拽增量 delta = 当前位置 - 上次位置

use std::time::Instant;

/// 触摸滑动阈值（逻辑像素）——超过即判定为拖拽/取消 tap
pub(crate) const TOUCH_SLOP: f32 = 8.0;
/// 双击时间窗（毫秒）
pub(crate) const DOUBLE_TAP_TIMEOUT_MS: u128 = 300;
/// 双击位置容差（逻辑像素）
pub(crate) const DOUBLE_TAP_SLOP: f32 = 50.0;
/// 长按判定时长（毫秒）
pub(crate) const LONG_PRESS_TIMEOUT_MS: u128 = 500;
/// The same timeout as a `Duration`, which is what the deadline arithmetic wants.
pub(crate) const LONG_PRESS_TIMEOUT: std::time::Duration =
    std::time::Duration::from_millis(LONG_PRESS_TIMEOUT_MS as u64);

/// Which way a gesture (or the scroll container it might belong to) moves.
///
/// A press can be claimed by two owners at once — an inner drag component (a swipeable row, a
/// slider) and the scroll container it sits in — and the finger's dominant axis decides which
/// one keeps the gesture. Compose arbitrates in the same place: its drag detectors each wait for
/// the touch slop along their own orientation, so the direction the finger moves first decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollAxis {
    Horizontal,
    Vertical,
}

impl ScrollAxis {
    /// Dominant axis of a displacement from the press point, or `None` while it is still
    /// undecided: below the touch slop (nothing has started yet) or on an exact diagonal (either
    /// could win — wait for another move instead of guessing).
    pub(crate) fn classify(dx: f32, dy: f32) -> Option<Self> {
        let (ax, ay) = (dx.abs(), dy.abs());
        if ax.max(ay) < TOUCH_SLOP {
            None
        } else if ax > ay {
            Some(Self::Horizontal)
        } else if ay > ax {
            Some(Self::Vertical)
        } else {
            None
        }
    }
}

/// 手势状态机输出动作
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GestureAction {
    /// 按下（down 立即触发）
    Press((f32, f32)),
    /// 点按（up 且未移动超过 slop 且时长 < 长按阈值）
    Tap((f32, f32)),
    /// 双击（第二次 tap 命中时间/位置窗）
    DoubleTap((f32, f32)),
    /// 长按（down 持续超阈值且未移动）
    LongPress((f32, f32)),
    /// 拖拽开始（首次超过 slop）
    DragStart((f32, f32)),
    /// 拖拽移动（当前位置, 增量）
    DragMove((f32, f32), (f32, f32)),
    /// 拖拽结束（up）
    DragEnd,
    /// 拖拽取消（cancel——up 前被系统打断）
    DragCancel,
    /// 无动作（事件被消费但不产生回调）
    None,
}

/// 手势跟踪状态——每轮 down 创建，up/cancel 销毁
pub(crate) struct GestureTracker {
    /// 手势节点 id（回调路由目标）
    pub(crate) node_id: u64,
    /// 按下位置
    down_pos: (f32, f32),
    /// 上一次 move 位置
    last_pos: (f32, f32),
    /// 按下时刻
    down_time: Instant,
    /// 是否超过 touch slop（tap 已取消）
    slop_passed: bool,
    /// 拖拽是否已开始（on_drag_start 只触发一次）
    dragging: bool,
    /// 该节点是否声明了 drag 回调（决定 slop 后走 drag 还是静默取消 tap）
    has_drag: bool,
    /// 双击状态：上一次 tap 的时刻/位置
    last_tap_time: Option<Instant>,
    last_tap_pos: Option<(f32, f32)>,
    /// Has this gesture's long press already fired? It fires AT the deadline now, so the release must
    /// neither fire it again nor fall through to a tap — the press was consumed.
    long_press_fired: bool,
}

/// 延迟 tap（节点注册 `on_double_tap` 时——Compose `detectTapGestures` 语义）。
/// on_up 判定为 Tap 后不立即触发，挂到 PerWindow；双击窗口内第二次 down
/// 命中同节点 → 取消（不补发）；超时 → 补发；其他节点按下不影响（保留到
/// deadline 由事件循环补发——Compose 各 pointerInput 的延迟 onTap 相互独立）。
#[derive(Clone)]
pub(crate) struct PendingTap {
    /// 手势节点 slot_key（跨重组稳定——补发时按当前布局树解析节点）
    pub(crate) slot_key: u64,
    /// 手势节点 id（同节点判定——窗口内第二次 down 是否取消第一次 tap）
    pub(crate) node_id: u64,
    /// 第一次 tap 的场景坐标（补发时换算组件本地坐标）
    pub(crate) pos: (f32, f32),
    /// Arena the tap was recorded in: `None` = the main tree, `Some(overlay id)` = that popup's
    /// layer. The tap has to be re-fired there, and `pos` is layer-local for a popup.
    pub(crate) overlay_id: Option<u64>,
    /// 补发截止时刻（now + DOUBLE_TAP_TIMEOUT_MS）
    pub(crate) deadline: std::time::Instant,
}

impl PendingTap {
    pub(crate) fn new(slot_key: u64, node_id: u64, pos: (f32, f32), overlay_id: Option<u64>) -> Self {
        Self {
            slot_key,
            node_id,
            pos,
            overlay_id,
            deadline: std::time::Instant::now()
                + std::time::Duration::from_millis(DOUBLE_TAP_TIMEOUT_MS as u64),
        }
    }
}

/// 新 down 对单个 pending tap 的处置（纯逻辑——app.rs 消费，便于单测）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingTapAction {
    /// 已超时：补发第一次 tap
    Fire,
    /// 双击窗口内同节点第二次按下：取消（等 up 判定双击，只发 DoubleTap）
    Cancel,
    /// 其他节点按下：保留（等 deadline 补发——不提前也不取消）
    Keep,
}

/// 单个 pending tap 在"新的 down 到来"时的处置规则。
pub(crate) fn pending_tap_on_down(
    t: &PendingTap,
    now: std::time::Instant,
    down_node: u64,
) -> PendingTapAction {
    if t.deadline <= now {
        PendingTapAction::Fire
    } else if t.node_id == down_node {
        PendingTapAction::Cancel
    } else {
        PendingTapAction::Keep
    }
}

impl GestureTracker {
    pub(crate) fn new(node_id: u64, pos: (f32, f32), has_drag: bool, last_tap: Option<(Instant, (f32, f32))>) -> Self {
        Self {
            node_id,
            down_pos: pos,
            last_pos: pos,
            down_time: Instant::now(),
            slop_passed: false,
            dragging: false,
            has_drag,
            last_tap_time: last_tap.map(|(t, _)| t),
            last_tap_pos: last_tap.map(|(_, p)| p),
            long_press_fired: false,
        }
    }

    /// When this gesture's long press is due, or `None` when it can no longer happen: it already
    /// fired, the finger moved past the touch slop, or a drag took the gesture.
    ///
    /// The event loop registers this with `WaitUntil`, so the long press arrives at its deadline even
    /// when the window is idle and no further input is coming.
    pub(crate) fn long_press_deadline(&self) -> Option<Instant> {
        if self.long_press_fired || self.slop_passed || self.dragging {
            return None;
        }
        Some(self.down_time + LONG_PRESS_TIMEOUT)
    }

    /// Fire the long press if its deadline has passed — what makes `on_long_press` reach the callback
    /// while the finger is still down, instead of on release. Returns the action at most once per
    /// gesture; the caller dispatches it through the gesture's arena, like every other gesture action.
    pub(crate) fn poll_long_press(&mut self, now: Instant) -> Option<GestureAction> {
        let deadline = self.long_press_deadline()?;
        if now < deadline {
            return None;
        }
        self.long_press_fired = true;
        Some(GestureAction::LongPress(self.down_pos))
    }

    /// 指针移动——返回动作（drag 系列或 None）。
    ///
    /// `allow_drag` gates *starting* the drag only: an inner component whose axis is still being
    /// arbitrated (see `ScrollAxis`) cancels the tap as usual once the touch slop is crossed, but
    /// must not fire `on_drag_start` yet — the gesture may be about to be handed to the enclosing
    /// scroll. A drag that was held back can still start on a later move (the finger may cross the
    /// slop along a diagonal that decides nothing); once it has started, this flag is ignored, and
    /// a drag cancelled by [`Self::cancel_drag_for_arbitration`] never re-arms.
    pub(crate) fn on_move(&mut self, pos: (f32, f32), allow_drag: bool) -> GestureAction {
        let delta = (pos.0 - self.last_pos.0, pos.1 - self.last_pos.1);
        self.last_pos = pos;
        if !self.slop_passed {
            let dx = pos.0 - self.down_pos.0;
            let dy = pos.1 - self.down_pos.1;
            if dx * dx + dy * dy < TOUCH_SLOP * TOUCH_SLOP {
                return GestureAction::None;
            }
            self.slop_passed = true;
        }
        if self.dragging {
            return GestureAction::DragMove(pos, delta);
        }
        if self.has_drag && allow_drag {
            self.dragging = true;
            return GestureAction::DragStart(pos);
        }
        GestureAction::None
    }

    /// 指针释放——返回动作（tap 系列 / drag-end / None）
    pub(crate) fn on_up(&mut self) -> GestureAction {
        if self.dragging {
            return GestureAction::DragEnd;
        }
        if self.long_press_fired {
            // The press was consumed by a long press that already fired at its deadline: no second
            // long press, and no tap either.
            return GestureAction::None;
        }
        if self.slop_passed {
            // 移动过但未拖拽（无 drag 回调）——静默
            return GestureAction::None;
        }
        // 未移动：长按 or tap or double-tap
        let held = self.down_time.elapsed().as_millis();
        if held >= LONG_PRESS_TIMEOUT_MS {
            // The deadline passed but the sweep has not run yet (the loop was busy, or the release and
            // the deadline arrived in the same batch). Fire it here so the press is not simply lost —
            // `long_press_fired` keeps it to one action per gesture either way.
            self.long_press_fired = true;
            return GestureAction::LongPress(self.down_pos);
        }
        // 双击检测：上次 tap 在时间窗内且位置接近
        if let (Some(lt), Some(lp)) = (self.last_tap_time, self.last_tap_pos) {
            let dt = Instant::now().duration_since(lt).as_millis();
            let dx = self.down_pos.0 - lp.0;
            let dy = self.down_pos.1 - lp.1;
            if dt < DOUBLE_TAP_TIMEOUT_MS && dx * dx + dy * dy <= DOUBLE_TAP_SLOP * DOUBLE_TAP_SLOP {
                // 双击命中——清空记录（第三次 tap 重新计数）
                self.last_tap_time = None;
                self.last_tap_pos = None;
                return GestureAction::DoubleTap(self.down_pos);
            }
        }
        // 记录本次 tap（供下一次双击判定）
        self.last_tap_time = Some(Instant::now());
        self.last_tap_pos = Some(self.down_pos);
        GestureAction::Tap(self.down_pos)
    }

    /// 取消（系统打断）——返回 drag cancel（拖拽中）或 None。
    /// ⚠ 当前 winit 未接入 PointerCanceled/TouchCanceled 事件——此分支为
    /// 预留 API（未来窗口失焦/触控取消时驱动）；状态机逻辑已单测覆盖。
    pub(crate) fn on_cancel(&mut self) -> GestureAction {
        if self.dragging {
            GestureAction::DragCancel
        } else {
            GestureAction::None
        }
    }

    /// 供下一次手势创建时取回双击上下文
    pub(crate) fn tap_context(&self) -> Option<(Instant, (f32, f32))> {
        self.last_tap_time.zip(self.last_tap_pos)
    }

    pub(crate) fn down_position(&self) -> (f32, f32) {
        self.down_pos
    }

    /// A tracker that believes the press started at `down_time` — the clock the deadline arithmetic
    /// reads. Test-only: production creates trackers at the real press moment.
    #[cfg(test)]
    pub(crate) fn with_down_time(node_id: u64, pos: (f32, f32), has_drag: bool, down_time: Instant) -> Self {
        let mut tracker = Self::new(node_id, pos, has_drag, None);
        tracker.down_time = down_time;
        tracker
    }

    /// Give the drag up after all (the enclosing scroll won the axis arbitration): the gesture keeps
    /// cancelling the tap but stops tracking a drag, so no `on_drag_*` callback — not even a
    /// `DragCancel` — fires for a gesture the component never received. The caller (`app::gesture_move`)
    /// keeps the axis locked for the rest of the gesture, which is what stops a later `on_move` from
    /// starting the drag again; this call itself does not latch anything.
    pub(crate) fn cancel_drag_for_arbitration(&mut self) {
        self.slop_passed = true;
        self.dragging = false;
    }
}

// ═══════════════ 测试 ═══════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(has_drag: bool) -> GestureTracker {
        GestureTracker::new(1, (10.0, 10.0), has_drag, None)
    }

    #[test]
    fn test_tap_basic() {
        let mut t = tracker(false);
        assert_eq!(t.on_up(), GestureAction::Tap((10.0, 10.0)));
    }

    #[test]
    fn test_tap_moved_within_slop() {
        // 移动 < slop 仍算 tap
        let mut t = tracker(false);
        assert_eq!(t.on_move((14.0, 12.0), true), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::Tap((10.0, 10.0)), "slop 内移动仍是 tap");
    }

    #[test]
    fn test_tap_cancelled_by_slop() {
        // 移动超过 slop 且无 drag 回调 → tap 取消
        let mut t = tracker(false);
        assert_eq!(t.on_move((30.0, 10.0), true), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::None, "超过 slop 的 tap 应取消");
    }

    #[test]
    fn test_drag_sequence() {
        let mut t = tracker(true);
        // 未超 slop：无动作
        assert_eq!(t.on_move((15.0, 10.0), true), GestureAction::None);
        // 超 slop：DragStart（首次）
        assert_eq!(t.on_move((30.0, 10.0), true), GestureAction::DragStart((30.0, 10.0)));
        // 后续：DragMove(pos, delta)
        assert_eq!(
            t.on_move((50.0, 20.0), true),
            GestureAction::DragMove((50.0, 20.0), (20.0, 10.0))
        );
        assert_eq!(t.on_up(), GestureAction::DragEnd);
    }

    #[test]
    fn test_drag_start_only_once() {
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true); // slop
        t.on_move((40.0, 10.0), true);
        // 第二次大移动仍是 DragMove（start 只一次）
        assert_eq!(
            t.on_move((60.0, 10.0), true),
            GestureAction::DragMove((60.0, 10.0), (20.0, 0.0))
        );
    }

    #[test]
    fn test_double_tap() {
        // 第一次 tap
        let mut t1 = tracker(false);
        assert_eq!(t1.on_up(), GestureAction::Tap((10.0, 10.0)));
        // 第二次（携带上次 tap 上下文）
        let mut t2 = GestureTracker::new(1, (12.0, 11.0), false, t1.tap_context());
        assert_eq!(t2.on_up(), GestureAction::DoubleTap((12.0, 11.0)));
    }

    #[test]
    fn test_pending_tap_metadata() {
        let t = PendingTap::new(42, 7, (3.0, 4.0), None);
        assert_eq!(t.slot_key, 42);
        assert_eq!(t.node_id, 7);
        assert_eq!(t.pos, (3.0, 4.0));
        assert_eq!(t.overlay_id, None, "a main-tree tap carries no overlay id");
        assert!(t.deadline > std::time::Instant::now(), "deadline 应在双击窗口之后");
    }

    #[test]
    fn test_pending_tap_on_down() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // 已超时 → 补发
        let expired = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: None, deadline: now - Duration::from_millis(1) };
        assert_eq!(pending_tap_on_down(&expired, now, 7), PendingTapAction::Fire);
        assert_eq!(pending_tap_on_down(&expired, now, 99), PendingTapAction::Fire, "超时与其他节点无关");
        // 窗口内同节点第二次按下 → 取消
        let live = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: Some(3), deadline: now + Duration::from_millis(100) };
        assert_eq!(pending_tap_on_down(&live, now, 7), PendingTapAction::Cancel);
        // 窗口内其他节点按下 → 保留（不提前补发、不取消）
        assert_eq!(pending_tap_on_down(&live, now, 99), PendingTapAction::Keep);
        // 边界：deadline == now → 视为超时补发
        let boundary = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: None, deadline: now };
        assert_eq!(pending_tap_on_down(&boundary, now, 7), PendingTapAction::Fire);
    }

    #[test]
    fn test_double_tap_too_far() {
        let mut t1 = tracker(false);
        t1.on_up();
        // 位置差 > 50：不算双击
        let mut t2 = GestureTracker::new(1, (80.0, 10.0), false, t1.tap_context());
        assert_eq!(t2.on_up(), GestureAction::Tap((80.0, 10.0)), "位置过远不算双击");
    }

    #[test]
    fn test_drag_cancel() {
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true); // drag start
        assert_eq!(t.on_cancel(), GestureAction::DragCancel);
    }

    #[test]
    fn test_cancel_without_drag_noop() {
        let mut t = tracker(false);
        assert_eq!(t.on_cancel(), GestureAction::None);
    }

    #[test]
    fn a_long_press_fires_at_its_deadline_while_still_held() {
        // The behaviour change this replaced: `on_long_press` used to arrive on RELEASE, so a caller
        // could not react to a hold until the finger came up. Compose fires it at the deadline.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        let deadline = start + LONG_PRESS_TIMEOUT;
        assert_eq!(t.long_press_deadline(), Some(deadline), "the deadline is the press + the timeout");

        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT / 2), None, "not yet");
        assert_eq!(
            t.poll_long_press(deadline),
            Some(GestureAction::LongPress((10.0, 10.0))),
            "at the deadline, with the finger still down"
        );
        assert_eq!(t.poll_long_press(deadline + LONG_PRESS_TIMEOUT), None, "and only once");
    }

    #[test]
    fn a_fired_long_press_is_not_followed_by_a_tap() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        t.poll_long_press(start + LONG_PRESS_TIMEOUT);
        assert_eq!(
            t.on_up(),
            GestureAction::None,
            "the press was consumed by the long press: no second long press, and no tap"
        );
    }

    #[test]
    fn moving_past_the_slop_cancels_a_pending_long_press() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        t.on_move((40.0, 10.0), true);
        assert_eq!(t.long_press_deadline(), None, "a moving finger is not a hold");
        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT * 2), None);
    }

    #[test]
    fn a_dragging_gesture_has_no_long_press() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), true, start);
        t.on_move((40.0, 10.0), true); // drag starts
        assert_eq!(t.long_press_deadline(), None);
        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT * 2), None);
    }

    #[test]
    fn a_deadline_that_the_sweep_missed_still_fires_on_release() {
        // The event loop was busy (or the release and the deadline landed in one batch): the release
        // must not silently swallow the press. It fires here, once.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        std::thread::sleep(std::time::Duration::from_millis(LONG_PRESS_TIMEOUT_MS as u64 + 20));
        assert_eq!(t.on_up(), GestureAction::LongPress((10.0, 10.0)), "the hold still counts");
        assert!(t.long_press_fired, "and it is recorded, so nothing fires again");
    }

    #[test]
    fn a_quick_release_is_still_a_tap() {
        // The deadline machinery must not turn short presses into holds.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        assert_eq!(
            t.on_up(),
            GestureAction::Tap((10.0, 10.0)),
            "a release before the deadline is a tap"
        );
    }

    #[test]
    fn axis_classification_waits_for_a_decisive_move() {
        use ScrollAxis::{Horizontal, Vertical};
        // Below the touch slop nothing has started, so nothing can be decided.
        assert_eq!(ScrollAxis::classify(4.0, 3.0), None);
        assert_eq!(ScrollAxis::classify(0.0, 0.0), None);
        // One axis clearly ahead decides, in either sign.
        assert_eq!(ScrollAxis::classify(-20.0, 6.0), Some(Horizontal));
        assert_eq!(ScrollAxis::classify(6.0, -20.0), Some(Vertical));
        // An exact diagonal is undecided: either owner could take it, so wait for the next move.
        assert_eq!(ScrollAxis::classify(12.0, 12.0), None);
        assert_eq!(ScrollAxis::classify(-9.0, 9.0), None);
    }

    #[test]
    fn a_held_back_drag_can_still_start_on_a_later_move() {
        // The finger crosses the slop on a diagonal that decides nothing (7,5), so the component
        // is not allowed to start yet; the next move is clearly horizontal and the drag begins
        // there — a drag that was merely held back is not cancelled.
        let mut t = tracker(true);
        t.on_move((17.0, 15.0), false);
        assert_eq!(t.on_move((30.0, 15.0), true), GestureAction::DragStart((30.0, 15.0)));
        assert_eq!(
            t.on_move((40.0, 15.0), true),
            GestureAction::DragMove((40.0, 15.0), (10.0, 0.0))
        );
    }

    #[test]
    fn an_arbitration_cancel_does_not_rearm_the_drag() {
        // The scroll won: the component's drag is dead for the rest of the gesture, and the
        // release must not look like the end of a drag the component never began.
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true);
        t.cancel_drag_for_arbitration();
        assert_eq!(t.on_move((40.0, 10.0), false), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::None);
    }
}
