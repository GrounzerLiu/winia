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
        }
    }

    // ── 发射（对标 Compose emit/tryEmit）──

    /// 按下（PressInteraction.Press 等价）
    pub fn emit_press(&self) {
        self.pressed.set(true);
    }

    /// 释放（PressInteraction.Release 等价——含拖拽越界取消）
    pub fn emit_release(&self) {
        self.pressed.set(false);
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
}
