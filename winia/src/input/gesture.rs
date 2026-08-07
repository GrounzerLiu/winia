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
//! - **长按**：down 持续 > 500ms 且未超过 slop（当前在 up 时判定——
//!   与 Compose 的"到时即时触发"有差异，注释注明；后续可接帧时钟精确化）
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
    /// 补发截止时刻（now + DOUBLE_TAP_TIMEOUT_MS）
    pub(crate) deadline: std::time::Instant,
}

impl PendingTap {
    pub(crate) fn new(slot_key: u64, node_id: u64, pos: (f32, f32)) -> Self {
        Self {
            slot_key,
            node_id,
            pos,
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
        }
    }

    /// 指针移动——返回动作（drag 系列或 None）
    pub(crate) fn on_move(&mut self, pos: (f32, f32)) -> GestureAction {
        let delta = (pos.0 - self.last_pos.0, pos.1 - self.last_pos.1);
        self.last_pos = pos;
        // 未超过 slop：检测是否刚越过
        if !self.slop_passed {
            let dx = pos.0 - self.down_pos.0;
            let dy = pos.1 - self.down_pos.1;
            if dx * dx + dy * dy >= TOUCH_SLOP * TOUCH_SLOP {
                self.slop_passed = true;
                if self.has_drag {
                    self.dragging = true;
                    return GestureAction::DragStart(pos);
                }
                // 无 drag 回调：slop 后 tap 取消（静默）
                return GestureAction::None;
            }
            return GestureAction::None;
        }
        // 已超过 slop 且在拖拽
        if self.dragging {
            return GestureAction::DragMove(pos, delta);
        }
        GestureAction::None
    }

    /// 指针释放——返回动作（tap 系列 / drag end / None）
    pub(crate) fn on_up(&mut self) -> GestureAction {
        if self.dragging {
            return GestureAction::DragEnd;
        }
        if self.slop_passed {
            // 移动过但未拖拽（无 drag 回调）——静默
            return GestureAction::None;
        }
        // 未移动：长按 or tap or double-tap
        let held = self.down_time.elapsed().as_millis();
        if held >= LONG_PRESS_TIMEOUT_MS {
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
        assert_eq!(t.on_move((14.0, 12.0)), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::Tap((10.0, 10.0)), "slop 内移动仍是 tap");
    }

    #[test]
    fn test_tap_cancelled_by_slop() {
        // 移动超过 slop 且无 drag 回调 → tap 取消
        let mut t = tracker(false);
        assert_eq!(t.on_move((30.0, 10.0)), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::None, "超过 slop 的 tap 应取消");
    }

    #[test]
    fn test_drag_sequence() {
        let mut t = tracker(true);
        // 未超 slop：无动作
        assert_eq!(t.on_move((15.0, 10.0)), GestureAction::None);
        // 超 slop：DragStart（首次）
        assert_eq!(t.on_move((30.0, 10.0)), GestureAction::DragStart((30.0, 10.0)));
        // 后续：DragMove(pos, delta)
        assert_eq!(
            t.on_move((50.0, 20.0)),
            GestureAction::DragMove((50.0, 20.0), (20.0, 10.0))
        );
        assert_eq!(t.on_up(), GestureAction::DragEnd);
    }

    #[test]
    fn test_drag_start_only_once() {
        let mut t = tracker(true);
        t.on_move((30.0, 10.0)); // slop
        t.on_move((40.0, 10.0));
        // 第二次大移动仍是 DragMove（start 只一次）
        assert_eq!(
            t.on_move((60.0, 10.0)),
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
        let t = PendingTap::new(42, 7, (3.0, 4.0));
        assert_eq!(t.slot_key, 42);
        assert_eq!(t.node_id, 7);
        assert_eq!(t.pos, (3.0, 4.0));
        assert!(t.deadline > std::time::Instant::now(), "deadline 应在双击窗口之后");
    }

    #[test]
    fn test_pending_tap_on_down() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // 已超时 → 补发
        let expired = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), deadline: now - Duration::from_millis(1) };
        assert_eq!(pending_tap_on_down(&expired, now, 7), PendingTapAction::Fire);
        assert_eq!(pending_tap_on_down(&expired, now, 99), PendingTapAction::Fire, "超时与其他节点无关");
        // 窗口内同节点第二次按下 → 取消
        let live = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), deadline: now + Duration::from_millis(100) };
        assert_eq!(pending_tap_on_down(&live, now, 7), PendingTapAction::Cancel);
        // 窗口内其他节点按下 → 保留（不提前补发、不取消）
        assert_eq!(pending_tap_on_down(&live, now, 99), PendingTapAction::Keep);
        // 边界：deadline == now → 视为超时补发
        let boundary = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), deadline: now };
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
        t.on_move((30.0, 10.0)); // drag start
        assert_eq!(t.on_cancel(), GestureAction::DragCancel);
    }

    #[test]
    fn test_cancel_without_drag_noop() {
        let mut t = tracker(false);
        assert_eq!(t.on_cancel(), GestureAction::None);
    }
}
