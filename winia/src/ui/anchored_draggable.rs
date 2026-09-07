//! `anchored_draggable`——垂直/水平锚点拖拽（对标 Compose `AnchoredDraggable`）。
//!
//! 机制（移植自 androidx `AnchoredDraggable.kt`，裁剪到 winia 基建）：
//! - **锚点**（`DraggableAnchors<T>`）：一组 `值 → 位置(px)` 映射（如
//!   BottomSheet 的 `Hidden at 800f / Expanded at 0f`）
//! - **`AnchoredDraggableState<T>`**：持有 `offset`（当前拖拽位移）+ `current_value`/
//!   `settled_value`/`target_value` + 锚点。拖拽时 `on_drag` 增量更新 offset
//!   （clamp 到 min/max 锚点）；拖拽结束 `settle`/`settle_with_velocity` 按
//!   **位置阈值**（默认 50%）或**速度阈值**吸附到目标锚点；`animate_to`/
//!   `snap_to` 程序化切换。
//! - **用法**：`offset` 由使用方渲染（如 graphics_layer translation / Modifier.offset）；
//!   `on_drag`/`on_drag_end` 由使用方接到节点的拖拽事件上（见 BottomSheet）。
//!
//! 与 Compose 的差异（winia 降级）：
//! - 无 suspend/协程——动画用 `push_animatable`（tween）/`push_fling_with_boundary`
//!   （decay）驱动，非挂起
//! - 拖拽结束无原生 velocity——`DragOnMove` 提供 delta，状态内部用
//!   `(最近 delta, 时间戳)` 估算末速度；超 100ms 未动的样本过期为 0
//!   （`last_velocity` hold-still expiry，对标 VelocityTracker 样本 horizon）
//! - `confirm_value_change` 同步回调（无挂起确认）

use std::collections::BTreeMap;
use std::time::Instant;
use crate::core::state::State;

/// Velocity sample horizon (ms): drag deltas older than this are treated as
/// hold-still (velocity 0) — cf. Compose VelocityTracker horizon (~100ms).
const VELOCITY_EXPIRY_MS: u128 = 100;

// ═══════════════ DraggableAnchors ═══════════════

/// 锚点集合：值 → 位置(px) 映射。位置按**值**存，查询按位置找最近值。
/// 用 BTreeMap 保持值排序（确定性 closest 查询）。
#[derive(Clone, Debug, PartialEq)]
pub struct DraggableAnchors<T: Clone + PartialEq + Eq + Ord> {
    /// 值 → 位置（有序——BTreeMap 按 T 排序）
    by_value: BTreeMap<T, f32>,
}

impl<T: Clone + PartialEq + Eq + Ord> DraggableAnchors<T> {
    /// 从 (值, 位置) 列表构建
    pub fn new(pairs: impl IntoIterator<Item = (T, f32)>) -> Self {
        let by_value: BTreeMap<T, f32> = pairs.into_iter().collect();
        Self { by_value }
    }

    /// 值 → 位置；不存在返回 NaN
    pub fn position_of(&self, value: &T) -> f32 {
        self.by_value.get(value).copied().unwrap_or(f32::NAN)
    }

    /// 是否有该值锚点
    pub fn has_position_for(&self, value: &T) -> bool {
        self.by_value.contains_key(value)
    }

    /// 最小锚点位置（无锚点返回 NaN）
    pub fn min_position(&self) -> f32 {
        self.by_value.values().copied().fold(f32::NAN, f32::min)
    }

    /// 最大锚点位置（无锚点返回 NaN）
    pub fn max_position(&self) -> f32 {
        self.by_value.values().copied().fold(f32::NAN, f32::max)
    }

    /// 距离 position 最近的锚点值；空返回 None
    pub fn closest_anchor(&self, position: f32) -> Option<T> {
        if self.by_value.is_empty() {
            return None;
        }
        self.by_value
            .iter()
            .min_by(|a, b| {
                (a.1 - position)
                    .abs()
                    .partial_cmp(&(b.1 - position).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| k.clone())
    }

    /// 沿指定方向找最近锚点。`search_upwards=true` 只考虑 position 之上（位置 ≥ position）
    /// 的锚点；`false` 只考虑之下的。无则返回 None。
    pub fn closest_anchor_directional(&self, position: f32, search_upwards: bool) -> Option<T> {
        let mut best: Option<(f32, T)> = None;
        for (k, &pos) in &self.by_value {
            let in_dir = if search_upwards { pos >= position } else { pos <= position };
            if !in_dir {
                continue;
            }
            let dist = (pos - position).abs();
            if best.as_ref().map(|(bd, _)| dist < *bd).unwrap_or(true) {
                best = Some((dist, k.clone()));
            }
        }
        best.map(|(_, k)| k)
    }

    /// 锚点数量
    pub fn len(&self) -> usize {
        self.by_value.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_value.is_empty()
    }

    /// 所有 (值, 位置)
    pub fn pairs(&self) -> impl Iterator<Item = (&T, &f32)> {
        self.by_value.iter()
    }
}

impl<T: Clone + PartialEq + Eq + Ord> Default for DraggableAnchors<T> {
    fn default() -> Self {
        Self::new(std::iter::empty())
    }
}

// ═══════════════ AnchoredDraggableState ═══════════════

/// 锚点拖拽状态机（对标 Compose `AnchoredDraggableState`）。
///
/// 持有拖拽 offset + 锚点 + 值状态。`offset` 由使用方渲染；拖拽增量由
/// 使用方经 `drag_delta` 喂入；结束经 `settle`/`settle_with_velocity` 吸附。
pub struct AnchoredDraggableState<T: Clone + PartialEq + Eq + Ord + 'static> {
    /// 当前拖拽/动画位移（px）
    offset: State<f32>,
    /// 当前经过的最近锚点值
    current_value: State<T>,
    /// 当前停靠的锚点值（动画/拖拽中保持，settle 完成才更新）
    settled_value: State<T>,
    /// 拖拽期间锁定的目标（on_drag 中 set，结束清）——target_value 优先用它
    drag_target: State<Option<T>>,
    /// 锚点
    anchors: State<DraggableAnchors<T>>,
    /// 值变更确认（veto 时回滚）——Arc 可 Clone 共享（Box 会被 Clone 重置为 |_|true 丢 veto）
    confirm_value_change: std::sync::Arc<dyn Fn(&T) -> bool + Send + Sync>,
    /// 末速度（px/s，velocity 追踪用）
    last_velocity: State<f32>,
    /// 最近一次 drag 的 (时间, delta)——velocity 估算（State 使 drag_delta 可 &self）
    last_drag: State<Option<(Instant, f32)>>,
}

impl<T: Clone + PartialEq + Eq + Ord + 'static> AnchoredDraggableState<T> {
    pub fn new(initial_value: T) -> Self {
        Self {
            offset: State::new(f32::NAN),
            current_value: State::new(initial_value.clone()),
            settled_value: State::new(initial_value),
            drag_target: State::new(None),
            anchors: State::new(DraggableAnchors::new(std::iter::empty())),
            confirm_value_change: std::sync::Arc::new(|_: &T| true),
            last_velocity: State::new(0.0),
            last_drag: State::new(None),
        }
    }

    /// 设置锚点（布局期调用——如 sheet 高度变化后重新计算）。
    /// 若 offset 未初始化，snap 到 current_value 对应位置。
    pub fn update_anchors(&self, anchors: DraggableAnchors<T>) {
        let prev = self.anchors.get();
        if prev == anchors {
            return;
        }
        self.anchors.set(anchors);
        // 初始化/重新对齐 offset
        let off = self.offset.get();
        if off.is_nan() {
            let pos = self.anchors.get().position_of(&self.current_value.get());
            if !pos.is_nan() {
                self.offset.set(pos);
            }
        }
    }

    /// 当前 offset（可能 NaN 未初始化）
    pub fn offset(&self) -> f32 {
        self.offset.get()
    }

    /// offset 的 State 引用（供布局 offset 跟随——布局位置与渲染一致）
    pub fn offset_state(&self) -> State<f32> {
        self.offset.clone()
    }

    /// 当前 offset（要求已初始化）
    pub fn require_offset(&self) -> f32 {
        let o = self.offset.get();
        debug_assert!(!o.is_nan(), "offset 未初始化——先 update_anchors");
        o
    }

    /// 当前值（最近经过的锚点）
    pub fn current_value(&self) -> T {
        self.current_value.get()
    }

    /// 停靠值
    pub fn settled_value(&self) -> T {
        self.settled_value.get()
    }

    /// 目标值（拖拽锁定中优先，否则按 offset 最近锚点）
    pub fn target_value(&self) -> T {
        if let Some(t) = self.drag_target.get() {
            return t;
        }
        let off = self.offset.get();
        if !off.is_nan() {
            self.anchors.get().closest_anchor(off).unwrap_or_else(|| self.current_value.get())
        } else {
            self.current_value.get()
        }
    }

    /// 值 → 位置
    pub fn position_of(&self, value: &T) -> f32 {
        self.anchors.get().position_of(value)
    }

    /// 从 from 到 to 的动画进度（0..1）
    pub fn progress(&self, from: &T, to: &T) -> f32 {
        let from_pos = self.anchors.get().position_of(from);
        let to_pos = self.anchors.get().position_of(to);
        let off = self.offset.get();
        if off.is_nan() || from_pos.is_nan() || to_pos.is_nan() {
            return 1.0;
        }
        let lo = from_pos.min(to_pos);
        let hi = from_pos.max(to_pos);
        let c = off.clamp(lo, hi);
        let frac = (c - from_pos) / (to_pos - from_pos);
        if frac < 1e-6 { 0.0 } else if frac > 1.0 - 1e-6 { 1.0 } else { frac.abs() }
    }

    /// 拖拽增量（on_drag 回调——当前位置增量 delta 的轴向分量）。
    /// 更新 offset（clamp 到 min/max 锚点），跨过半程更新 current_value。
    pub fn drag_delta(&self, delta: f32) {
        if self.anchors.get().is_empty() {
            return;
        }
        let off = self.offset.get();
        let base = if off.is_nan() { 0.0 } else { off };
        let min = self.anchors.get().min_position();
        let max = self.anchors.get().max_position();
        let new_offset = if !min.is_nan() && !max.is_nan() {
            (base + delta).clamp(min, max)
        } else {
            base + delta
        };
        self.offset.set(new_offset);
        // 修正：直接按 new_offset 查 closest，不要经 drag_target 自锁的 target_value()
        let new_target = self.anchors.get().closest_anchor(new_offset).unwrap_or_else(|| self.current_value.get());
        self.drag_target.set(Some(new_target));
        // velocity 追踪：用当前 delta/dt，非上一帧 d0（首帧亦有速度）
        let now = Instant::now();
        if let Some((t0, _)) = self.last_drag.get() {
            let dt = now.duration_since(t0).as_secs_f32().max(1e-4);
            let v = delta / dt;
            self.last_velocity.set(v.clamp(-8000.0, 8000.0));
        } else {
            // 首帧按 16ms 估算
            let v = delta / 0.016;
            self.last_velocity.set(v.clamp(-8000.0, 8000.0));
        }
        self.last_drag.set(Some((now, delta)));
        // 半程阈值更新 current_value：仅当越过 (cur+neighbor)/2 中点才切
        let cur = self.current_value.get();
        let cur_pos = self.anchors.get().position_of(&cur);
        if !cur_pos.is_nan() {
            let anchors = self.anchors.get();
            // 紧邻锚点（Compose AnchoredDraggable 的 findNeighbor），非距 new_offset 最近
            let neighbor = if new_offset >= cur_pos {
                anchors.pairs().filter(|(_, p)| **p > cur_pos)
                    .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(k, _)| k.clone())
            } else {
                anchors.pairs().filter(|(_, p)| **p < cur_pos)
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(k, _)| k.clone())
            };
            if let Some(n) = neighbor {
                let n_pos = anchors.position_of(&n);
                if !n_pos.is_nan() {
                    let mid = (cur_pos + n_pos) / 2.0;
                    let should_switch = if new_offset >= cur_pos { new_offset >= mid } else { new_offset <= mid };
                    if should_switch && n != cur && (self.confirm_value_change)(&n) {
                        self.current_value.set(n);
                    }
                }
            }
        }
    }

    /// 拖拽结束：按位置最近锚点吸附（无 velocity 版本——用 50% 位置阈值）。
    /// 返回目标值。
    pub fn settle(&self) -> T {
        let off = self.require_offset();
        let target = self
            .anchors
            .get()
            .closest_anchor(off)
            .unwrap_or_else(|| self.current_value.get());
        self.settle_to(&target);
        target
    }

    /// 拖拽结束：带 velocity 吸附（对标 Compose `settle(velocity)`——
    /// velocity ≥ 阈值走方向下一个锚点，否则按位置阈值）。
    ///
    /// - `velocity_threshold`：默认 125 px/s（Compose AnchoredDraggableMinFlingVelocity）
    /// - `positional_threshold`: 默认距离/2
    pub fn settle_with_velocity(&self, velocity: f32, positional_threshold: Option<f32>) -> T {
        let off = self.require_offset();
        let target = self.compute_target(off, velocity, positional_threshold);
        self.settle_to(&target);
        target
    }

    /// 计算吸附目标（对标 Compose `computeTarget`）
    fn compute_target(&self, current_offset: f32, velocity: f32, positional_threshold: Option<f32>) -> T {
        if self.anchors.get().is_empty() {
            return self.current_value.get();
        }
        let anchors = self.anchors.get();
        if anchors.len() <= 1 {
            return anchors.closest_anchor(current_offset).unwrap_or_else(|| self.current_value.get());
        }
        let is_moving = velocity.abs() > 0.0;
        let density = crate::unit::current_density().density;
        let velocity_threshold = 125.0 * density; // 125 dp/s → px/s
        if !is_moving {
            return anchors.closest_anchor(current_offset).unwrap_or_else(|| self.current_value.get());
        }
        let is_moving_forward = velocity > 0.0;
        if velocity.abs() >= velocity_threshold {
            // 高速度：沿方向下一个锚点
            return anchors
                .closest_anchor_directional(current_offset, is_moving_forward)
                .unwrap_or_else(|| anchors.closest_anchor(current_offset).unwrap_or_else(|| self.current_value.get()));
        }
        // 低速度：按位置阈值判断（默认距离/2）
        let left = anchors.closest_anchor_directional(current_offset, false)
            .unwrap_or_else(|| anchors.closest_anchor(current_offset).unwrap_or_else(|| self.current_value.get()));
        let right = anchors.closest_anchor_directional(current_offset, true)
            .unwrap_or_else(|| anchors.closest_anchor(current_offset).unwrap_or_else(|| self.current_value.get()));
        let left_pos = anchors.position_of(&left);
        let right_pos = anchors.position_of(&right);
        let distance = (left_pos - right_pos).abs();
        let threshold = positional_threshold.unwrap_or(distance / 2.0);
        // 移动方向的起始锚点位置
        let start_pos = if is_moving_forward { left_pos } else { right_pos };
        let rel_pos = (start_pos - current_offset).abs();
        if rel_pos >= threshold {
            if is_moving_forward { right } else { left }
        } else {
            if is_moving_forward { left } else { right }
        }
    }

    /// 吸附到指定目标（动画 tween——Compose SnapAnimationSpec）
    fn settle_to(&self, target: &T) {
        let pos = self.anchors.get().position_of(target);
        if pos.is_nan() {
            return;
        }
        // confirm：否决则回滚到当前停靠值
        if (self.confirm_value_change)(target) {
            self.animate_to_pos(pos);
            self.settled_value.set(target.clone());
            self.current_value.set(target.clone());
            self.drag_target.set(None);
        } else {
            // veto：回滚到当前停靠值（current_value/settled_value 都复位）
            let prev = self.settled_value.get();
            let prev_pos = self.anchors.get().position_of(&prev);
            if !prev_pos.is_nan() {
                self.animate_to_pos(prev_pos);
            }
            self.settled_value.set(prev.clone());
            self.current_value.set(prev);
            self.drag_target.set(None);
        }
    }

    /// 程序化吸附到目标（动画）
    pub fn animate_to(&self, target: T) {
        let pos = self.anchors.get().position_of(&target);
        if pos.is_nan() {
            // 无此锚点：仅更新值（Compose 语义）
            self.settled_value.set(target.clone());
            self.current_value.set(target);
            return;
        }
        if (self.confirm_value_change)(&target) {
            // 设置拖拽目标——target_value() 优先返回它（Compose animateTo 经
            // anchoredDrag(targetValue=...) 设 dragTarget）
            self.drag_target.set(Some(target.clone()));
            self.animate_to_pos(pos);
            self.settled_value.set(target.clone());
            self.current_value.set(target);
        }
    }

    /// 程序化瞬移到目标（无动画）
    pub fn snap_to(&self, target: T) {
        let pos = self.anchors.get().position_of(&target);
        if pos.is_nan() {
            self.settled_value.set(target.clone());
            self.current_value.set(target);
            return;
        }
        if (self.confirm_value_change)(&target) {
            self.offset.set(pos);
            self.settled_value.set(target.clone());
            self.current_value.set(target);
        }
    }

    /// 驱动 offset 动画到位置（tween）
    fn animate_to_pos(&self, pos: f32) {
        let off = self.offset.peek();
        let from = if off.is_nan() { pos } else { off };
        if from == pos {
            return;
        }
        crate::animation::push_animatable(
            self.offset.clone(),
            pos,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(250),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
    }

    /// Last fling velocity (px/s) with hold-still expiry: returns 0 when the finger
    /// has not moved for longer than [`VELOCITY_EXPIRY_MS`]. Without expiry a stale
    /// velocity (e.g. drag halfway, hold 1s, release) would fling the sheet to a far
    /// anchor on release — Compose's VelocityTracker drops old samples the same way.
    pub fn last_velocity(&self) -> f32 {
        match self.last_drag.peek() {
            Some((t, _)) if t.elapsed().as_millis() > VELOCITY_EXPIRY_MS => 0.0,
            _ => self.last_velocity.get(),
        }
    }

    /// 是否动画进行中（offset 有动画）
    pub fn is_animation_running(&self) -> bool {
        crate::animation::has_animation_for_state(self.offset.state_id())
    }

    /// 设置值变更确认回调（Arc 共享，Clone 后仍生效）
    pub fn set_confirm_value_change(&mut self, f: impl Fn(&T) -> bool + Send + Sync + 'static) {
        self.confirm_value_change = std::sync::Arc::new(f);
    }
}

impl<T: Clone + PartialEq + Eq + Ord + 'static> Clone for AnchoredDraggableState<T> {
    fn clone(&self) -> Self {
        Self {
            offset: self.offset.clone(),
            current_value: self.current_value.clone(),
            settled_value: self.settled_value.clone(),
            drag_target: self.drag_target.clone(),
            anchors: self.anchors.clone(),
            confirm_value_change: std::sync::Arc::clone(&self.confirm_value_change),
            last_velocity: self.last_velocity.clone(),
            last_drag: self.last_drag.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
    enum V { A, B, C }

    fn anchors() -> DraggableAnchors<V> {
        DraggableAnchors::new(vec![(V::A, 0.0), (V::B, 100.0), (V::C, 200.0)])
    }

    #[test]
    fn closest_anchor_works() {
        let a = anchors();
        assert_eq!(a.closest_anchor(0.0), Some(V::A));
        assert_eq!(a.closest_anchor(49.0), Some(V::A));
        assert_eq!(a.closest_anchor(51.0), Some(V::B));
        assert_eq!(a.closest_anchor(150.0), Some(V::B));
        assert_eq!(a.closest_anchor(199.0), Some(V::C));
        // 方向查询
        assert_eq!(a.closest_anchor_directional(50.0, true), Some(V::B)); // 上方
        assert_eq!(a.closest_anchor_directional(50.0, false), Some(V::A)); // 下方
        assert_eq!(a.closest_anchor_directional(0.0, false), Some(V::A));
    }

    #[test]
    fn update_anchors_initializes_offset() {
        let s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        assert!(!s.offset().is_nan());
        assert_eq!(s.offset(), 0.0);
        assert_eq!(s.current_value(), V::A);
    }

    #[test]
    fn drag_updates_offset_and_current_value() {
        let mut s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        // 拖到 60（B 的半程 50 之后）→ current_value 更新为 B
        s.drag_delta(60.0);
        assert_eq!(s.offset(), 60.0);
        assert_eq!(s.current_value(), V::B);
        // 继续到 160（C 半程 150 之后）→ C
        s.drag_delta(100.0);
        assert_eq!(s.offset(), 160.0);
        assert_eq!(s.current_value(), V::C);
    }

    #[test]
    fn drag_clamps_to_bounds() {
        let mut s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.drag_delta(-500.0);
        assert_eq!(s.offset(), 0.0); // clamp 到 min (A=0)
        s.drag_delta(10000.0);
        assert_eq!(s.offset(), 200.0); // clamp 到 max (C=200)
    }

    #[test]
    fn settle_uses_closest_anchor() {
        let mut s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.drag_delta(40.0); // 40 → 最近 A (距离40) vs B (距离60)
        let target = s.settle();
        assert_eq!(target, V::A);
        assert_eq!(s.settled_value(), V::A);
        // 60 → 最近 B
        s.drag_delta(60.0);
        let target = s.settle();
        assert_eq!(target, V::B);
    }

    #[test]
    fn settle_with_velocity_direction() {
        let mut s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.drag_delta(30.0); // 30 位置，最近 A
        // 高速向上（负速度 = 向前）→ 应选 A（方向下一个 = 下方无）→ 回 A
        let t = s.settle_with_velocity(-500.0, None);
        assert_eq!(t, V::A);
        // 高速向下（正速度）→ 沿方向下一个 = B
        s.drag_delta(0.0); // 重置到 30
        let t2 = s.settle_with_velocity(500.0, None);
        assert_eq!(t2, V::B);
    }

    #[test]
    fn animate_to_sets_value_and_offset_target() {
        let s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.animate_to(V::C);
        assert_eq!(s.settled_value(), V::C);
        assert_eq!(s.current_value(), V::C);
        // offset 动画进行中（target C=200）
        assert!(s.is_animation_running() || s.offset() == 200.0);
    }

    #[test]
    fn confirm_value_change_veto_rolls_back() {
        let mut s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.set_confirm_value_change(|v| *v != V::B); // veto B
        s.drag_delta(120.0); // 120 位置：最近 B(100)，确认目标 B
        let t = s.settle();
        // B 被 veto → settle_to 内回滚到 settled_value (A)
        assert_eq!(t, V::B); // settle 计算的目标是 B
        assert_eq!(s.settled_value(), V::A); // 回滚到 A
        assert_eq!(s.current_value(), V::A);
    }

    #[test]
    fn stale_drag_velocity_expires_to_zero() {
        // Bug: drag halfway, hold still, release — the release must NOT fling on the
        // stale velocity. last_velocity() expires samples older than 100ms.
        let s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.drag_delta(60.0); // fast move → nonzero tracked velocity
        assert!(s.last_velocity().abs() > 0.0, "fresh drag has velocity");
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(
            s.last_velocity(),
            0.0,
            "velocity must expire after hold-still past the horizon"
        );
    }

    #[test]
    fn settle_after_hold_still_uses_position() {
        // End-to-end of the expiry: drag toward B, hold, release → positional settle
        // (closest anchor), NOT a velocity fling along the stale direction.
        let s = AnchoredDraggableState::new(V::A);
        s.update_anchors(anchors());
        s.drag_delta(40.0); // 40: closest A (dist 40 vs B dist 60)
        std::thread::sleep(std::time::Duration::from_millis(150));
        let t = s.settle_with_velocity(s.last_velocity(), None);
        assert_eq!(t, V::A, "expired velocity → positional settle to closest anchor");
    }
}
