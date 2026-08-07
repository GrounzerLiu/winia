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
use std::sync::{Arc, LazyLock};
use std::time::Instant;

/// 水波纹扩散时长（ms）——对标 Compose ripple 的 PressAnimationSpec（~225ms）
pub(crate) const RIPPLE_EXPAND_MS: f32 = 225.0;
/// 释放后淡出时长（ms）
pub(crate) const RIPPLE_FADE_MS: f32 = 180.0;
/// 波纹不透明度（按下期间）
pub(crate) const RIPPLE_OPACITY: f32 = 0.24;

/// 单个波纹层（参考旧版 D:\winia ripple.rs 的分层设计）：
/// 每次按下产生一层，中心 = 按压点（场景坐标），扩散进度 0→1；
/// 释放后标记 fading，透明度淡出到 0 后由 update_ripples 清理。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RippleLayer {
    /// 层 id（按下递增——多指预留；当前单指针模型）
    pub(crate) id: u64,
    /// 按压点（场景坐标——渲染时换算节点本地坐标）
    pub(crate) center: (f32, f32),
    /// 扩散进度 0..1（225ms easeOutCubic）
    pub(crate) progress: f32,
    /// 当前透明度（按下 0.24；释放后 180ms 淡出到 0）
    pub(crate) opacity: f32,
    /// 释放后淡出中
    pub(crate) fading: bool,
}

impl RippleLayer {
    fn new(id: u64, center: (f32, f32)) -> Self {
        Self { id, center, progress: 0.0, opacity: RIPPLE_OPACITY, fading: false }
    }
}

/// 全局活动波纹（按下注册，淡出结束清理）——驱动事件循环持续重绘
static ACTIVE_RIPPLES: LazyLock<Mutex<Vec<MutableInteractionSource>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static LAST_RIPPLE_TICK: LazyLock<Mutex<Instant>> =
    LazyLock::new(|| Mutex::new(Instant::now()));

/// 每轮事件循环调用：清理已结束的波纹，返回是否仍有波纹在动画中
/// （有则请求重绘——扩散/淡出期间保持帧驱动）。
pub(crate) fn update_ripples() -> bool {
    let now = Instant::now();
    let dt = now.duration_since(*LAST_RIPPLE_TICK.lock());
    *LAST_RIPPLE_TICK.lock() = now;
    let mut list = ACTIVE_RIPPLES.lock();
    for s in list.iter() {
        s.advance_ripples(dt);
    }
    list.retain(|s| s.has_active_ripples());
    !list.is_empty()
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
        }
    }

    // ── 发射（对标 Compose emit/tryEmit）──

    /// 按下（PressInteraction.Press 等价——位置取 (0,0)）
    pub fn emit_press(&self) {
        self.emit_press_at((0.0, 0.0));
    }

    /// 按下并记录位置（对标 Compose `PressInteraction.Press(pressPosition)`——
    /// 新建一层水波纹从按压点扩散；同时注册全局波纹驱动重绘）
    pub fn emit_press_at(&self, pos: (f32, f32)) {
        let id = self.next_layer_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.ripple_layers.lock().push(RippleLayer::new(id, pos));
        self.pressed.set(true);
        ACTIVE_RIPPLES.lock().push(self.clone());
    }

    /// 释放（PressInteraction.Release 等价——含拖拽越界取消）：
    /// 当前层进入淡出（参考 ripple.rs：释放后 opacity 动画到 0 再清理）
    pub fn emit_release(&self) {
        let mut layers = self.ripple_layers.lock();
        for l in layers.iter_mut() {
            if !l.fading {
                l.fading = true;
            }
        }
        self.pressed.set(false);
    }

    /// 推进波纹动画（事件循环每轮调用）：扩散进度 + 淡出透明度，完成后移除层
    pub(crate) fn advance_ripples(&self, dt: std::time::Duration) {
        let dt_ms = dt.as_secs_f32() * 1000.0;
        let mut layers = self.ripple_layers.lock();
        for l in layers.iter_mut() {
            if l.fading {
                l.opacity = (l.opacity - dt_ms / RIPPLE_FADE_MS * RIPPLE_OPACITY).max(0.0);
            } else {
                l.progress = (l.progress + dt_ms / RIPPLE_EXPAND_MS).min(1.0);
            }
        }
        // 清理条件：透明度归零（淡出完成）即移除；扩散中即使 progress=1 也保留到释放
        layers.retain(|l| !l.fading || l.opacity > 0.0);
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
    }

    /// 失去焦点（FocusInteraction.Unfocus 等价）
    pub fn emit_unfocus(&self) {
        self.focused.set(false);
    }

    /// 指针悬停进入（HoverInteraction.Enter 等价）
    pub fn emit_hover_enter(&self) {
        self.hovered.set(true);
    }

    /// 指针悬停离开（HoverInteraction.Exit 等价）
    pub fn emit_hover_exit(&self) {
        self.hovered.set(false);
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
    fn test_ripple_layers_advance_and_fade() {
        use std::time::Duration;
        let s = MutableInteractionSource::new();
        // 未按下：无层
        assert!(!s.has_active_ripples());
        // 按下：产生一层（中心=按压点）
        s.emit_press_at((12.0, 34.0));
        assert!(s.is_pressed());
        let layers = s.ripple_layers();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].center, (12.0, 34.0));
        assert!(!layers[0].fading);
        // 推进扩散：progress 增长到 1（225ms 完成）
        s.advance_ripples(Duration::from_millis(113));
        let mid = s.ripple_layers()[0];
        assert!(mid.progress > 0.0 && mid.progress < 1.0, "扩散中 progress 应介于 0..1");
        s.advance_ripples(Duration::from_millis(120));
        assert_eq!(s.ripple_layers()[0].progress, 1.0);
        // 释放：进入淡出
        s.emit_release();
        assert!(!s.is_pressed());
        assert!(s.ripple_layers()[0].fading);
        // 淡出推进：透明度下降，结束后层被清理
        s.advance_ripples(Duration::from_millis(90));
        let mid = s.ripple_layers();
        assert_eq!(mid.len(), 1);
        assert!(mid[0].opacity > 0.0 && mid[0].opacity < RIPPLE_OPACITY);
        s.advance_ripples(Duration::from_millis(100));
        assert!(!s.has_active_ripples(), "淡出完成后层应被清理");
    }
}
