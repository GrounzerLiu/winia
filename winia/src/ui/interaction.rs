//! InteractionSource / ComponentState——对标 Compose foundation `interaction`。
//!
//! Compose 的实现是 `MutableSharedFlow<Interaction>` 事件流 + `collectIsPressedAsState`
//! 等扩展函数维护"活动交互列表"再导出布尔状态（官方源码 foundation 1.11.4：
//! `InteractionSource.kt` / `PressInteraction.kt` 等）。Winia 为单指针同步模型，
//! 用四个 `State<bool>` 等价表达（pressed/focused/hovered/dragged）：
//! - 读取（`is_*` / `state()`）走 `State::get()` → 注册组合依赖，状态变化自动重组消费者；
//! - 发射（`emit_*`）走 `State::set()` → PartialEq 去重 + notify + 唤醒事件循环。
//!
//! 差异（有意简化，文档化）：Compose 的 Press/Release/Cancel 带交互实例身份以支持
//! 多指/多交互并存；Winia 每窗口单活动指针，布尔标志足够，多指支持留待指针管道升级。

use crate::core::state::State;
use parking_lot::Mutex;
use std::sync::Arc;

/// 水波纹扩散时长——参考旧版 ripple.rs（Tween 500ms）
pub(crate) const RIPPLE_EXPAND_MS: u64 = 500;
/// 释放后淡出时长——参考旧版 ripple.rs（Tween 300ms）
pub(crate) const RIPPLE_FADE_MS: u64 = 300;
/// 波纹不透明度——参考旧版 ripple.rs（ripple_opacity 0.1）
pub(crate) const RIPPLE_OPACITY: f32 = 0.10;
/// hover/focus 状态层透明度（参考旧版 background_opacity 0.08）
pub(crate) const STATE_LAYER_HOVER: f32 = 0.08;
pub(crate) const STATE_LAYER_FOCUS: f32 = 0.12;
/// 状态层过渡时长（旧版 hover 动画 500ms）
pub(crate) const STATE_LAYER_TRANSITION_MS: u64 = 500;
/// 焦点环淡入/淡出时长（M3 focus indicator 约 150-200ms）
pub(crate) const FOCUS_INDICATOR_TRANSITION_MS: u64 = 180;

/// 单个波纹层（参考旧版 D:\winia ripple.rs 的分层设计）：
/// 每次按下产生一层，中心 = 按压点（节点本地坐标——相对节点左上角，
/// 对标 Compose pressPosition），扩散进度 0→1；
/// 释放后标记 fading，透明度淡出到 0 后自动从列表移除（动画 on_finish）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RippleLayer {
    /// 层 id（按下递增——多指预留；当前单指针模型）
    pub(crate) id: u64,
    /// 按压点（节点本地坐标——渲染时加布局原点即画布坐标）
    pub(crate) center: (f32, f32),
    /// 扩散进度 0..1（500ms Tween 驱动）
    pub(crate) progress: crate::core::state::State<f32>,
    /// 当前透明度（按下 0.10；释放后 300ms 淡出到 0）
    pub(crate) opacity: crate::core::state::State<f32>,
    /// 释放后淡出中（防重复触发淡出）
    pub(crate) fading: bool,
}

impl RippleLayer {
    fn new(id: u64, center: (f32, f32)) -> Self {
        Self {
            id,
            center,
            progress: crate::core::state::State::new(0.0),
            opacity: crate::core::state::State::new(RIPPLE_OPACITY),
            fading: false,
        }
    }
}

/// 组件交互状态快照（对标 material3 状态组合：enabled/pressed/hovered/focused/dragged）。
/// 由 `MutableInteractionSource::state(enabled)` 一次性读取（注册依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComponentState {
    pub enabled: bool,
    pub pressed: bool,
    pub hovered: bool,
    pub focused: bool,
    pub dragged: bool,
}

impl ComponentState {
    /// 禁用态（enabled=false——组件不响应交互且视觉降级）
    pub const fn disabled() -> Self {
        Self { enabled: false, pressed: false, hovered: false, focused: false, dragged: false }
    }

    /// 启用且无任何活动交互
    pub const fn idle() -> Self {
        Self { enabled: true, pressed: false, hovered: false, focused: false, dragged: false }
    }

    /// 是否处于可交互状态（enabled 且未禁用视觉）
    pub fn is_interactive(&self) -> bool {
        self.enabled
    }
}

/// 可变的交互源——组件（Button/TextField/clickable/focusable/hoverable）发射
/// 交互事件，消费者读取派生状态。对标 Compose `MutableInteractionSource()`。
///
/// 必须通过 `ctx.remember(|| MutableInteractionSource::new())` 创建（与普通 State
/// 相同：创建期注册到 Composer 队列，`set()` 才能推送到重组）。
#[derive(Clone, Debug)]
pub struct MutableInteractionSource {
    pressed: State<bool>,
    hovered: State<bool>,
    focused: State<bool>,
    dragged: State<bool>,
    /// 水波纹层（每次按下新增一层——Ripple 渲染读取）
    ripple_layers: Arc<Mutex<Vec<RippleLayer>>>,
    /// 层 id 计数器
    next_layer_id: Arc<std::sync::atomic::AtomicU64>,
    /// hover/focus 状态层透明度（动画驱动——避免状态切换生硬跳变）
    hover_opacity: crate::core::state::State<f32>,
    focus_opacity: crate::core::state::State<f32>,
    /// 焦点环透明度（聚焦 0→1 淡入、失焦 1→0 淡出——独立于状态层）
    focus_indicator_alpha: crate::core::state::State<f32>,
}

/// 身份比较：同一交互源实例（跨 clone 稳定）——供 Modifier 参数相等判断
/// （与 State 的"id 或值相等"不同：两个不同源即使标志相同也不算相等）。
impl PartialEq for MutableInteractionSource {
    fn eq(&self, other: &Self) -> bool {
        self.pressed.id() == other.pressed.id()
            && self.hovered.id() == other.hovered.id()
            && self.focused.id() == other.focused.id()
            && self.dragged.id() == other.dragged.id()
    }
}

impl Eq for MutableInteractionSource {}

impl Default for MutableInteractionSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MutableInteractionSource {
    pub fn new() -> Self {
        Self {
            pressed: State::new(false),
            hovered: State::new(false),
            focused: State::new(false),
            dragged: State::new(false),
            ripple_layers: Arc::new(Mutex::new(Vec::new())),
            next_layer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            hover_opacity: crate::core::state::State::new(0.0),
            focus_opacity: crate::core::state::State::new(0.0),
            focus_indicator_alpha: crate::core::state::State::new(0.0),
        }
    }

    // ── 发射（对标 Compose emit/tryEmit）──

    /// 按下（PressInteraction.Press 等价——位置取 (0,0)）
    pub fn emit_press(&self) {
        self.emit_press_at((0.0, 0.0));
    }

    /// 按下并记录位置（对标 Compose `PressInteraction.Press(pressPosition)`，
    /// `pos` 为**节点本地坐标**——
    /// 新建一层水波纹从按压点扩散；进度由动画系统驱动（500ms Tween——
    /// 参考旧版 ripple.rs），完成自动停止）
    pub fn emit_press_at(&self, pos: (f32, f32)) {
        let id = self.next_layer_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.ripple_layers.lock().push(RippleLayer::new(id, pos));
        self.pressed.set(true);
        let progress = self.ripple_layers.lock().last().unwrap().progress.clone();
        crate::animation::push_animatable(
            progress,
            1.0,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(RIPPLE_EXPAND_MS),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
    }

    /// 释放（PressInteraction.Release 等价——含拖拽越界取消）：
    /// 当前层进入淡出（参考 ripple.rs：释放后 opacity 动画到 0，on_finish 清理）
    pub fn emit_release(&self) {
        let ids: Vec<u64> = {
            let mut layers = self.ripple_layers.lock();
            layers.iter_mut()
                .filter(|l| !l.fading)
                .map(|l| {
                    l.fading = true;
                    l.id
                })
                .collect()
        };
        for id in ids {
            let opacity = self.ripple_layers.lock().iter()
                .find(|l| l.id == id)
                .map(|l| l.opacity.clone());
            if let Some(opacity) = opacity {
                let src = self.clone();
                crate::animation::push_animatable_with_done(
                    opacity,
                    0.0,
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(RIPPLE_FADE_MS),
                        crate::animation::interpolator::EaseInOutCubic::new(),
                    )),
                    move || src.remove_ripple_layer(id),
                );
            }
        }
        self.pressed.set(false);
    }

    /// 淡出完成回调：从列表移除层
    fn remove_ripple_layer(&self, id: u64) {
        self.ripple_layers.lock().retain(|l| l.id != id);
    }

    /// 是否有未结束的波纹层（事件循环驱动重绘的依据）
    pub(crate) fn has_active_ripples(&self) -> bool {
        !self.ripple_layers.lock().is_empty()
    }

    /// 波纹层快照（渲染读取——场景坐标中心）
    pub(crate) fn ripple_layers(&self) -> Vec<RippleLayer> {
        self.ripple_layers.lock().clone()
    }

    /// 获得焦点（FocusInteraction.Focus 等价）
    pub fn emit_focus(&self) {
        self.focused.set(true);
        self.animate_state_layer(&self.focus_opacity, STATE_LAYER_FOCUS);
        self.animate_focus_indicator(1.0);
    }

    /// 失去焦点（FocusInteraction.Unfocus 等价）
    pub fn emit_unfocus(&self) {
        self.focused.set(false);
        self.animate_state_layer(&self.focus_opacity, 0.0);
        self.animate_focus_indicator(0.0);
    }

    /// 指针悬停进入（HoverInteraction.Enter 等价）——状态层 500ms 淡入
    pub fn emit_hover_enter(&self) {
        self.hovered.set(true);
        self.animate_state_layer(&self.hover_opacity, STATE_LAYER_HOVER);
    }

    /// 指针悬停离开（HoverInteraction.Exit 等价）——状态层 500ms 淡出
    pub fn emit_hover_exit(&self) {
        self.hovered.set(false);
        self.animate_state_layer(&self.hover_opacity, 0.0);
    }

    /// 状态层透明度动画（hover/focus 过渡平滑——参考旧版 500ms Tween）
    fn animate_state_layer(&self, state: &crate::core::state::State<f32>, target: f32) {
        crate::animation::push_animatable(
            state.clone(),
            target,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(STATE_LAYER_TRANSITION_MS),
                crate::animation::interpolator::EaseInOutCubic::new(),
            )),
        );
    }

    /// 焦点环透明度动画（M3 focus indicator 淡入/淡出）
    fn animate_focus_indicator(&self, target: f32) {
        crate::animation::push_animatable(
            self.focus_indicator_alpha.clone(),
            target,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(FOCUS_INDICATOR_TRANSITION_MS),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
    }

    /// hover 状态层当前透明度（渲染读取——动画值）
    pub(crate) fn hover_opacity_value(&self) -> f32 {
        self.hover_opacity.peek()
    }

    /// focus 状态层当前透明度（渲染读取——动画值）
    pub(crate) fn focus_opacity_value(&self) -> f32 {
        self.focus_opacity.peek()
    }

    /// 焦点环当前透明度（渲染读取——动画值）
    pub(crate) fn focus_indicator_alpha_value(&self) -> f32 {
        self.focus_indicator_alpha.peek()
    }

    /// 拖拽开始（DragInteraction.Start 等价）
    pub fn emit_drag_start(&self) {
        self.dragged.set(true);
    }

    /// 拖拽结束/取消（DragInteraction.Stop/Cancel 等价）
    pub fn emit_drag_end(&self) {
        self.dragged.set(false);
    }

    // ── 派生状态读取（注册组合依赖——状态变化自动重组）──

    pub fn is_pressed(&self) -> bool {
        self.pressed.get()
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }

    pub fn is_dragged(&self) -> bool {
        self.dragged.get()
    }

    /// 一次读取全部状态（与 material3 的 enabled/pressed/hovered/focused 组合一致）
    pub fn state(&self, enabled: bool) -> ComponentState {
        ComponentState {
            enabled,
            pressed: self.pressed.get(),
            hovered: self.hovered.get(),
            focused: self.focused.get(),
            dragged: self.dragged.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;

    #[test]
    fn test_source_transitions() {
        let s = MutableInteractionSource::new();
        assert_eq!(s.state(true), ComponentState::idle());
        s.emit_press();
        assert!(s.is_pressed());
        assert!(!s.is_hovered());
        s.emit_hover_enter();
        assert!(s.is_hovered());
        s.emit_release();
        assert!(!s.is_pressed());
        assert!(s.is_hovered(), "release 不影响 hover");
        s.emit_hover_exit();
        assert!(!s.is_hovered());
        s.emit_focus();
        assert!(s.is_focused());
        s.emit_drag_start();
        assert!(s.is_dragged());
        s.emit_drag_end();
        s.emit_unfocus();
        assert_eq!(s.state(true), ComponentState::idle());
        assert_eq!(s.state(false).enabled, false);
    }

    #[test]
    fn test_source_emit_triggers_recompose() {
        // remember 创建的 source：读取注册依赖、发射推送到 composer pending——
        // 验证"状态变化自动重组消费者"闭环
        let mut composer = Composer::new();
        let mut holder: Option<MutableInteractionSource> = None;
        let mut reads = 0i32;
        composer.compose(|ctx| {
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let _ = src.is_pressed(); // 注册依赖（模拟 Button::build 内读取）
            holder = Some(src.clone());
            reads += 1;
        });
        let src = holder.expect("compose 应执行一次");
        src.emit_press(); // 变化 → notify → pending
        assert!(composer.has_pending_states(), "set 应推送到 composer pending 队列");
        let before = reads;
        composer.recompose(|ctx| {
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let _ = src.is_pressed();
            reads += 1;
        });
        assert!(reads > before, "读取过 is_pressed 的 slot 应被标记 dirty 并重跑");
        assert!(src.is_pressed());
    }

    #[test]
    fn test_ripple_layers_press_release() {
        let s = MutableInteractionSource::new();
        // 未按下：无层
        assert!(!s.has_active_ripples());
        // 按下：产生一层（中心=按压点）
        s.emit_press_at((12.0, 34.0));
        assert!(s.is_pressed());
        let layers = s.ripple_layers();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].center, (12.0, 34.0));
        assert_eq!(layers[0].opacity.get(), RIPPLE_OPACITY, "波纹初始透明度 0.10");
        assert!(!layers[0].fading);
        assert!(s.has_active_ripples());
        // 释放：进入淡出（进度/透明度动画由动画系统驱动——animation 测试覆盖）
        s.emit_release();
        assert!(!s.is_pressed());
        assert!(s.ripple_layers()[0].fading);
    }

    #[test]
    fn test_ripple_removed_after_fade() {
        // 淡出动画 on_finish 的清理路径：remove_ripple_layer 按 id 移除
        let s = MutableInteractionSource::new();
        s.emit_press_at((1.0, 2.0));
        let id = s.ripple_layers()[0].id;
        s.remove_ripple_layer(id);
        assert!(!s.has_active_ripples(), "淡出完成后层应被清理");
    }

    #[test]
    fn test_focus_indicator_alpha_api() {
        let s = MutableInteractionSource::new();
        assert_eq!(s.focus_indicator_alpha_value(), 0.0, "初始无焦点环");
        s.emit_focus();   // 淡入动画启动（值由全局动画系统推进）
        s.emit_unfocus(); // 淡出动画启动
    }
}
