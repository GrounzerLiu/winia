//! `sheet_state`——BottomSheet 状态（对标 Compose Material3 `SheetState`）。
//!
//! 三态：`Hidden`（隐藏）/ `PartiallyExpanded`（半展开，显示一半或内容高）/
//! `Expanded`（全展开，内容顶到顶）。底层用 [`AnchoredDraggableState`]
//! 管理拖拽位移与锚点；锚点由使用方在布局期按 `full_height`（视口高）和
//! `sheet_height`（面板内容高）经 [`SheetState::update_anchors`] 计算：
//! - `Hidden at full_height`
//! - `PartiallyExpanded at full_height - min(full_height/2, sheet_height)`
//! - `Expanded at max(0, full_height - sheet_height)`
//!
//! 提供程序化控制：`show`/`hide`/`expand`/`partial_expand`；拖拽增量经
//! `drag_delta` 喂入，结束经 `settle`/`settle_with_velocity` 吸附。

use crate::runtime::state::State;
use crate::overlay::anchored_draggable::{AnchoredDraggableState, DraggableAnchors};

/// Sheet 值（对标 Compose `SheetValue`）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetValue {
    /// 隐藏（sheet 完全滑出视口）
    Hidden,
    /// 半展开（显示一半视口或内容自身高度，取小者）
    PartiallyExpanded,
    /// 全展开（内容顶到视口顶部）
    Expanded,
}

impl PartialOrd for SheetValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SheetValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // 位置序：Hidden(大) < Partial(中) < Expanded(小) 对应 offset 递减
        // 保证 DraggableAnchors 的 BTree 按位置序最贴近物理排序，避免 Hidden 最小导致最近锚点误判
        let order = |v: &SheetValue| match v {
            SheetValue::Hidden => 0,
            SheetValue::PartiallyExpanded => 1,
            SheetValue::Expanded => 2,
        };
        order(self).cmp(&order(other))
    }
}

/// BottomSheet 状态机（对标 Compose `SheetState`）。
pub struct SheetState {
    /// 底层锚点拖拽状态
    anchored: AnchoredDraggableState<SheetValue>,
    /// 跳过半展开（sheet 只 Hidden/Expanded 两态）— State 以便 Clone 后共享
    skip_partially_expanded: State<bool>,
    /// 是否启用半展开锚点（sheet 高度 > 视口一半）
    has_partially_expanded: State<bool>,
    /// 是否有 Expanded 锚点
    has_expanded: State<bool>,
}

impl SheetState {
    /// 创建（初始值通常 `Hidden`）
    pub fn new(initial_value: SheetValue) -> Self {
        Self {
            anchored: AnchoredDraggableState::new(initial_value),
            skip_partially_expanded: State::new(false),
            has_partially_expanded: State::new(false),
            has_expanded: State::new(true),
        }
    }

    /// 是否跳过半展开（Clone 后共享，改后下次 update_anchors 生效）
    pub fn set_skip_partially_expanded(&self, v: bool) {
        self.skip_partially_expanded.set(v);
    }

    /// 设置值变更确认回调（veto 时回滚）
    pub fn set_confirm_value_change(&mut self, f: impl Fn(SheetValue) -> bool + Send + Sync + 'static) {
        self.anchored.set_confirm_value_change(move |v: &SheetValue| f(*v));
    }

    /// 布局期更新锚点（`full_height` = 视口高，`sheet_height` = 面板内容高）。
    /// 对标 Compose BottomSheet 的 `draggableAnchors` 计算。`&self`——
    /// on_size_changed 回调（Fn 闭包）中调用。
    pub fn update_anchors(&self, full_height: f32, sheet_height: f32) {
        let mut pairs = vec![(SheetValue::Hidden, full_height)];
        // 半展开锚点：sheet 高 > 视口一半 才启用（或强制）
        let partially_available =
            !self.skip_partially_expanded.get() && sheet_height > full_height / 2.0;
        if partially_available {
            let visible = full_height / 2.0; // 或 sheet_height 取小（防止 lift off 底部）
            pairs.push((SheetValue::PartiallyExpanded, full_height - visible.min(sheet_height)));
        }
        if sheet_height != 0.0 {
            pairs.push((SheetValue::Expanded, (full_height - sheet_height).max(0.0)));
        }
        self.has_partially_expanded.set(partially_available);
        self.has_expanded.set(sheet_height != 0.0);
        let new_anchors = DraggableAnchors::new(pairs.clone());
        let was_uninit = self.anchored.offset().is_nan();
        if was_uninit {
            // 未初始化且已 show()（current != Hidden）：先 snap 到 Hidden(fullH)，再动画到 correct(Partial/Expanded)，
            // 避免直接 animate_to 导致 Hidden→Expanded 去重不通知/动画不驱动的问题
            let should_slide = self.anchored.current_value() != SheetValue::Hidden;
            if should_slide {
                self.anchored.offset_state().set(full_height);
            }
            self.anchored.update_anchors(new_anchors);
            if should_slide {
                let correct = if self.has_partially_expanded.get() {
                    SheetValue::PartiallyExpanded
                } else {
                    SheetValue::Expanded
                };
                self.anchored.animate_to(correct);
            }
            return;
        }
        let min = new_anchors.min_position();
        let max = new_anchors.max_position();
        self.anchored.update_anchors(new_anchors);
        let off = self.anchored.offset();
        if !off.is_nan() {
            if !min.is_nan() && !max.is_nan() {
                let lo = min.min(max);
                let hi = min.max(max);
                let clamped = off.clamp(lo, hi);
                if (clamped - off).abs() > 0.5 {
                    self.anchored.offset_state().set(clamped);
                }
            }
        }
        // Compose's `draggableAnchors` block returns the new anchors AND the new target together
        // (`ModalBottomSheet.kt:299-325`): the sheet keeps the state it is in across a geometry change and
        // travels to that state's NEW position — `PartiallyExpanded -> PartiallyExpanded` while that anchor
        // exists, `Expanded -> Expanded`, `Hidden -> Hidden`. Clamping alone does not do that: after the
        // window grows, a partially expanded sheet's old offset is still inside the offset range, so it
        // stayed at the old half instead of the new one (measured through the demo: 280 -> 280 where half of
        // the taller window is 380). A motion already in flight is left alone — it is heading for the same
        // value, only its end position moved.
        if !self.anchored.is_animation_running() {
            let value = self.anchored.target_value();
            let wanted = self.anchored.position_of(&value);
            if !wanted.is_nan() && (self.anchored.offset() - wanted).abs() > 0.5 {
                self.anchored.animate_to(value);
            }
        }
    }

    /// Scaffold 专用锚点（对标 Compose `BottomSheetScaffold` 的 `calculateAnchors`）。
    /// `layout_height` = 宿主可用高（窗口高或 Scaffold 减 topBar），
    /// `peek_height` = `sheetPeekHeight` 的 px 值（折叠时露头高度），
    /// `sheet_height` = 板面实测高。
    /// 锚点：
    /// - `PartiallyExpanded/Collapsed at layoutH - peek`
    /// - `Expanded at layoutH - sheetH`
    /// - `Hidden at layoutH`（可用于隐藏）
    pub fn update_anchors_scaffold(&self, layout_height: f32, peek_height: f32, sheet_height: f32) {
        let mut pairs = vec![];
        // Partial / Collapsed 始终由 peek 决定（与 sheet 高无关，对齐 compose）
        if !self.skip_partially_expanded.get() && peek_height > 0.0 {
            pairs.push((SheetValue::PartiallyExpanded, layout_height - peek_height));
        }
        let expanded_pos = if sheet_height != 0.0 {
            (layout_height - sheet_height).max(0.0)
        } else {
            f32::NAN
        };
        let has_expanded_pos = !expanded_pos.is_nan();
        if has_expanded_pos {
            let dominated = pairs.iter().any(|(_, p)| (*p - expanded_pos).abs() < 0.5);
            if !dominated {
                pairs.push((SheetValue::Expanded, expanded_pos));
            } else {
                // sheetH == peek 时 Partial 与 Expanded 重合，仍保留 Expanded 语义（expand() 不应静默失败）
                pairs.push((SheetValue::Expanded, expanded_pos));
            }
        }
        // Hidden 始终保留（供 hide() 用，scaffold 拖到隐藏）
        pairs.push((SheetValue::Hidden, layout_height));
        self.has_partially_expanded.set(pairs.iter().any(|(v, _)| *v == SheetValue::PartiallyExpanded));
        // Expanded 在重合时仍视为可用（expand() 应可达），用 has_expanded_pos 而非 pairs 去重结果
        self.has_expanded.set(has_expanded_pos);
        let new_anchors = DraggableAnchors::new(pairs);
        // Scaffold 首帧直接 snap 到初始值（通常 Partial），不做 Hidden→Partial 的首次滑入
        // 保留 Modal 的首次滑入逻辑，仅 Scaffold 走静默初始化
        let was_uninit = self.anchored.offset().is_nan();
        self.anchored.update_anchors(new_anchors);
        if was_uninit {
            return;
        }
        // The same re-target the modal path does: a geometry change moves the sheet to the NEW position of
        // the state it is in (Compose's `calculateAnchors` returns the anchors and the target together,
        // `BottomSheetScaffold.kt:272-306`), where keeping the old offset would leave a docked sheet at the
        // old peek line after the window resizes.
        if !self.anchored.is_animation_running() {
            let value = self.anchored.target_value();
            let wanted = self.anchored.position_of(&value);
            if !wanted.is_nan() && (self.anchored.offset() - wanted).abs() > 0.5 {
                self.anchored.animate_to(value);
            }
        }
    }

    // ── 状态读取 ──

    pub fn current_value(&self) -> SheetValue {
        self.anchored.current_value()
    }

    pub fn settled_value(&self) -> SheetValue {
        self.anchored.settled_value()
    }

    pub fn target_value(&self) -> SheetValue {
        self.anchored.target_value()
    }

    /// 是否可见（target 不是 Hidden）
    pub fn is_visible(&self) -> bool {
        self.target_value() != SheetValue::Hidden
    }

    /// 当前 offset（拖拽/动画位移 px）
    pub fn offset(&self) -> f32 {
        self.anchored.offset()
    }

    /// offset 的 State 引用（供 `Modifier::offset_y(state)` 布局跟随——
    /// 布局位置与渲染一致，hit_test 命中正确）
    pub fn offset_state(&self) -> crate::runtime::state::State<f32> {
        self.anchored.offset_state()
    }

    pub fn require_offset(&self) -> f32 {
        self.anchored.require_offset()
    }

    /// from→to 动画进度（0..1）
    pub fn progress(&self, from: SheetValue, to: SheetValue) -> f32 {
        self.anchored.progress(&from, &to)
    }

    /// 是否有半展开锚点
    pub fn has_partially_expanded_state(&self) -> bool {
        self.has_partially_expanded.get()
    }

    /// [`Self::has_partially_expanded_state`] without registering a dependency — the render-time read (a
    /// value that is PAINTED must not mark a slot dirty; see `AnchoredDraggableState::peek_progress`).
    pub fn peek_has_partially_expanded_state(&self) -> bool {
        self.has_partially_expanded.peek()
    }

    pub fn has_expanded_state(&self) -> bool {
        self.has_expanded.get()
    }

    /// 底层锚点拖拽状态（供拖拽事件使用）
    pub fn anchored_draggable(&self) -> &AnchoredDraggableState<SheetValue> {
        &self.anchored
    }

    // ── 程序化控制 ──

    /// 显示（动画到 Expanded，或半展开若有）
    pub fn show(&self) {
        let target = if self.has_partially_expanded.get() {
            SheetValue::PartiallyExpanded
        } else {
            SheetValue::Expanded
        };
        self.anchored.animate_to(target);
    }

    /// 隐藏（动画到 Hidden）
    pub fn hide(&self) {
        self.anchored.animate_to(SheetValue::Hidden);
    }

    /// 全展开
    pub fn expand(&self) {
        self.anchored.animate_to(SheetValue::Expanded);
    }

    /// 半展开
    pub fn partial_expand(&self) {
        self.anchored.animate_to(SheetValue::PartiallyExpanded);
    }

    /// 拖拽增量（on_drag 回调）
    pub fn drag_delta(&self, delta: f32) {
        self.anchored.drag_delta(delta);
    }

    /// 拖拽结束吸附（按位置最近锚点）
    pub fn settle(&self) -> SheetValue {
        self.anchored.settle()
    }

    /// 拖拽结束带速度吸附
    pub fn settle_with_velocity(&self, velocity: f32) -> SheetValue {
        self.anchored.settle_with_velocity(velocity, None)
    }

    /// 是否动画中
    pub fn is_animation_running(&self) -> bool {
        self.anchored.is_animation_running()
    }

    /// 末速度
    pub fn last_velocity(&self) -> f32 {
        self.anchored.last_velocity()
    }
}

impl Clone for SheetState {
    fn clone(&self) -> Self {
        Self {
            anchored: self.anchored.clone(),
            skip_partially_expanded: self.skip_partially_expanded.clone(),
            has_partially_expanded: self.has_partially_expanded.clone(),
            has_expanded: self.has_expanded.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> SheetState {
        let mut s = SheetState::new(SheetValue::Hidden);
        s.update_anchors(800.0, 400.0); // 视口 800，内容 400
        s
    }

    #[test]
    fn anchors_computed_correctly() {
        let s = make_state();
        // Hidden=800, Expanded=400 (800-400), Partial 不启用(400 不高过 400)
        assert_eq!(s.anchored.position_of(&SheetValue::Hidden), 800.0);
        assert_eq!(s.anchored.position_of(&SheetValue::Expanded), 400.0);
        assert!(!s.has_partially_expanded_state());
        assert_eq!(s.current_value(), SheetValue::Hidden);
    }

    #[test]
    fn partial_anchor_when_sheet_tall() {
        let mut s = SheetState::new(SheetValue::Hidden);
        s.update_anchors(800.0, 600.0); // 内容 600 > 400 (半视口)
        assert!(s.has_partially_expanded_state());
        assert_eq!(s.anchored.position_of(&SheetValue::PartiallyExpanded), 400.0);
        assert_eq!(s.anchored.position_of(&SheetValue::Expanded), 200.0);
    }

    /// A geometry change moves the sheet to the NEW position of the state it is in, the way Compose's
    /// `draggableAnchors` block returns the anchors and the target together (`ModalBottomSheet.kt:299-325`).
    /// Clamping alone left the old offset in place: measured here, a partially expanded sheet whose window
    /// grew from 800 to 1000 stayed at 400 where half the taller window is 500.
    /// Drive the engine until the sheet stops moving, the way a frame loop would.
    fn settle(state: &SheetState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while crate::animation::update_animations() {
            std::thread::sleep(std::time::Duration::from_millis(8));
            assert!(std::time::Instant::now() < deadline, "the sheet never settled");
        }
    }

    #[test]
    fn a_taller_window_moves_the_modal_sheet_to_the_new_anchor() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut s = SheetState::new(SheetValue::Hidden);
        s.update_anchors(800.0, 600.0);
        s.show();
        settle(&s);
        // Land it on the partially expanded anchor, as a real settle would.
        s.anchored.offset_state().set(400.0);
        assert_eq!(s.offset(), 400.0);
        s.update_anchors(1000.0, 600.0);
        assert_eq!(
            s.anchored.position_of(&SheetValue::PartiallyExpanded),
            500.0,
            "the anchor follows the window"
        );
        assert_eq!(s.target_value(), SheetValue::PartiallyExpanded, "the state is kept");
        settle(&s);
        assert_eq!(s.offset(), 500.0, "the sheet travels to the new position of that state");
    }

    /// The same for the docked sheet: its anchors come from the layout height and the peek, so a resize has
    /// to move the collapsed sheet to the new peek line (`BottomSheetScaffold.kt:272-306`).
    #[test]
    fn a_taller_window_moves_the_docked_sheet_to_the_new_peek_line() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let s = SheetState::new(SheetValue::PartiallyExpanded);
        s.update_anchors_scaffold(800.0, 96.0, 600.0);
        assert_eq!(s.anchored.position_of(&SheetValue::PartiallyExpanded), 704.0);
        s.anchored.offset_state().set(704.0);
        s.update_anchors_scaffold(1000.0, 96.0, 600.0);
        assert_eq!(
            s.anchored.position_of(&SheetValue::PartiallyExpanded),
            904.0,
            "the peek line follows the layout height"
        );
        settle(&s);
        assert_eq!(s.offset(), 904.0, "the docked sheet travels with it");
    }

    #[test]
    fn show_animates_to_expanded() {
        let s = make_state();
        s.show();
        assert_eq!(s.target_value(), SheetValue::Expanded);
        assert!(s.is_visible());
    }

    #[test]
    fn hide_animates_to_hidden() {
        let s = make_state();
        s.show();
        s.hide();
        assert_eq!(s.target_value(), SheetValue::Hidden);
        assert!(!s.is_visible());
    }

    #[test]
    fn drag_and_settle() {
        let mut s = make_state();
        s.update_anchors(800.0, 400.0);
        // 初始 offset = Hidden 位置 800
        assert_eq!(s.require_offset(), 800.0);
        // 拖拽向上（负 delta）到 700
        s.drag_delta(-100.0);
        assert_eq!(s.require_offset(), 700.0);
        // settle：700 最近 Expanded(400) 还是 Hidden(800)？距离各 300/100 → Hidden
        let t = s.settle();
        assert_eq!(t, SheetValue::Hidden);
    }

    #[test]
    fn drag_down_to_expanded() {
        let mut s = make_state();
        s.update_anchors(800.0, 400.0);
        s.drag_delta(-100.0); // 700
        s.drag_delta(-200.0); // 500 — 最近 Expanded(400) 距离 100 vs Hidden(800) 300
        let t = s.settle();
        assert_eq!(t, SheetValue::Expanded);
    }

    /// 拖拽到底部 → settle 到 Hidden（关闭语义）
    #[test]
    fn drag_to_bottom_settles_hidden() {
        let mut s = make_state();
        s.update_anchors(800.0, 400.0);
        // Expanded 后向下拖（正 dy = offset 增大 → 面板下移）
        s.show();
        s.drag_delta(200.0); // 400 + 200 = 600
        s.drag_delta(200.0); // 800 — 到底（Hidden 锚点）
        let t = s.settle();
        assert_eq!(t, SheetValue::Hidden);
        assert!(!s.is_visible());
    }

    /// 高速度向上（负 velocity）→ 沿方向下一个锚点 = Expanded
    #[test]
    fn settle_with_velocity_goes_expanded() {
        let mut s = make_state();
        s.update_anchors(800.0, 400.0);
        s.drag_delta(-50.0); // 750，位置最近 Hidden(800)
        // 高速向上甩（负速度，abs > 125 px/s 阈值）→ 方向下一个（向上=Expanded）
        let t = s.settle_with_velocity(-400.0);
        assert_eq!(t, SheetValue::Expanded);
    }

    /// 低速度 → 按位置阈值（默认距离/2）
    #[test]
    fn settle_low_velocity_uses_positional_threshold() {
        let mut s = make_state();
        s.update_anchors(800.0, 400.0);
        s.drag_delta(-50.0); // 750 — 距 Hidden(800) 50，距 Expanded(400) 350
        // 低速度（<125）→ 位置阈值：仍近 Hidden → Hidden
        let t = s.settle_with_velocity(50.0);
        assert_eq!(t, SheetValue::Hidden);
    }
}
