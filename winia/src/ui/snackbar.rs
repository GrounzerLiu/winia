//! Snackbar——底部瞬时提示条（对标 Compose Material3 `Snackbar`/`SnackbarHost`）。
//!
//! 机制（对齐 Compose material3）：
//! - `SnackbarHostState`：持有当前显示的 `SnackbarData`（`State<Option<...>>`），
//!   `show()` 命令式弹出（Compose 的 `suspend showSnackbar` 降级为命令式——
//!   winia 无协程语义；action 回调仍可用）
//! - `SnackbarHost`：观察 hostState，`AnimatedVisibility` 控制进出场
//!   （fade + 底部滑入——Compose 默认 `slideInVertically + fadeIn`）
//! - `Snackbar`：视觉组件（inverse_surface 圆角条 + message + action +
//!   dismiss 按钮——M3 配色：`inverseSurface` 底、`inverseOnSurface` 文字、
//!   `inversePrimary` action）
//!
//! 与 Compose 的差异（设计取舍）：
//! - `showSnackbar` 的 suspend 语义（等待 action 点击/超时返回结果）降级为
//!   命令式 `show(data)`——调用方直接传 action 回调；无 await 结果
//! - 自动消失用 hostState 内部定时（`remember_coroutine_scope` spawn）——
//!   `SnackbarDuration` 控制（Short 4s / Long 10s / Indefinite 手动 dismiss）
//! - 单条显示（Compose 有队列——winia 新 show 覆盖当前）

use std::sync::Arc;
use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::theme::WiniaTheme;

/// Snackbar 显示时长（对标 Compose `SnackbarDuration`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnackbarDuration {
    /// 短暂（4s 自动消失——Compose Short=4000ms）
    Short,
    /// 较长（10s 自动消失——Compose Long=10000ms）
    Long,
    /// 无限期（手动 dismiss——Compose Indefinite）
    Indefinite,
    /// 自定义时长（毫秒）——测试/特殊场景
    Custom(u64),
}

impl SnackbarDuration {
    fn millis(&self) -> Option<u64> {
        match self {
            SnackbarDuration::Short => Some(4000),
            SnackbarDuration::Long => Some(10000),
            SnackbarDuration::Indefinite => None,
            SnackbarDuration::Custom(ms) => Some(*ms),
        }
    }
}

/// Snackbar 数据（对标 Compose `SnackbarData`——message/actionLabel/duration/
/// dismissAction）
#[derive(Clone)]
pub struct SnackbarData {
    /// 提示文本
    pub message: String,
    /// action 按钮文本（None = 无 action 按钮）
    pub action_label: Option<String>,
    /// 显示时长
    pub duration: SnackbarDuration,
    /// 是否显示 dismiss（×）按钮（对标 Compose `withDismissAction`）
    pub with_dismiss_action: bool,
    /// action 点击回调（对标 Compose `performAction`——Compose 在 showSnackbar
    /// 挂起返回后由调用方处理；winia 降级为直接回调）
    pub on_action: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// PartialEq 手动实现：on_action 回调不参与相等（State::set 需要 PartialEq——
/// 同 message/action/duration 视为同数据，回调变化不触发重组）
impl PartialEq for SnackbarData {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message
            && self.action_label == other.action_label
            && self.duration == other.duration
            && self.with_dismiss_action == other.with_dismiss_action
    }
}

/// Debug 手动实现：on_action 回调不参与（Arc<dyn Fn> 无 Debug）
impl std::fmt::Debug for SnackbarData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnackbarData")
            .field("message", &self.message)
            .field("action_label", &self.action_label)
            .field("duration", &self.duration)
            .field("with_dismiss_action", &self.with_dismiss_action)
            .finish()
    }
}

impl SnackbarData {
    /// 创建（默认 Short + 无 action + 无 dismiss）
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action_label: None,
            duration: SnackbarDuration::Short,
            with_dismiss_action: false,
            on_action: None,
        }
    }

    /// 设置 action 按钮（文本 + 点击回调）
    pub fn action(mut self, label: impl Into<String>, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.action_label = Some(label.into());
        self.on_action = Some(Arc::new(on_click));
        self
    }

    /// 设置时长
    pub fn duration(mut self, d: SnackbarDuration) -> Self {
        self.duration = d;
        self
    }

    /// 显示 dismiss 按钮
    pub fn with_dismiss_action(mut self) -> Self {
        self.with_dismiss_action = true;
        self
    }
}

/// Snackbar 宿主状态（对标 Compose `SnackbarHostState`）——
/// 持有当前 Snackbar 数据 + 显示/隐藏控制 + 自动消失定时。
///
/// ```rust
/// let snackbar_host = ctx.remember(|| SnackbarHostState::new()).get();
/// // 弹出（任意位置——按钮回调等）：
/// snackbar_host.show(SnackbarData::new("已保存").action("撤销", || undo()));
/// ```
#[derive(Clone)]
pub struct SnackbarHostState {
    /// 当前显示的 snackbar（None = 隐藏）
    data: State<Option<SnackbarData>>,
    /// 显示进度（0→1 动画——时长 = duration；完成回调 dismiss）
    /// ⚠ 用 State 而非纯命令式：动画每帧 set 通知 → SnackbarHost 重组 →
    /// 视觉跟随（AnimatedVisibility 已处理进出场；此 State 仅驱动自动消失）
    progress: State<f32>,
}

impl SnackbarHostState {
    pub fn new() -> Self {
        Self { data: State::new(None), progress: State::new(0.0) }
    }

    /// 当前 Snackbar 数据（组合期读——注册依赖，变化触发重组）
    pub fn current_snackbar_data(&self) -> Option<SnackbarData> {
        self.data.get().clone()
    }

    /// 当前 Snackbar 数据（**非组合上下文**读——快照，不注册依赖）。
    /// 用于动画完成回调 / 事件回调等组合外场景（`get()` 在无 active scope
    /// 时虽安全，但语义上读快照更正确，避免未来挪入组合上下文误注册依赖）。
    pub(crate) fn peek_snackbar_data(&self) -> Option<SnackbarData> {
        self.data.peek().clone()
    }

    /// 命令式弹出 Snackbar（新 show 覆盖当前）。
    /// 对标 Compose `suspend showSnackbar`——winia 降级为命令式（无 await）。
    /// action 回调在 [`SnackbarData::action`] 中给出。
    ///
    /// 自动消失：`push_animatable_with_done(progress, 1.0, duration)`——
    /// 动画完成（时长到）回调 dismiss。⚠ 完成回调在动画 tick 执行
    /// （非组合期）——只改 State（安全）。Indefinite 不启动动画（手动关闭）。
    pub fn show(&self, data: SnackbarData) {
        // 先取 duration（data 随后 move 进 State）
        let duration_ms = data.duration.millis();
        let msg = data.message.clone();
        self.data.set(Some(data));
        // 取消旧的自动消失动画（新 show 覆盖——旧定时器不误关）
        crate::animation::cancel_animation(&self.progress);
        // ⚠ 重置计时起点：上次动画完成时 progress 已写 1.0——若不清零，
        // push_animatable_with_done 的 `peek == target` 去重会直接 return
        // （不注册 done 回调）→ 覆盖场景自动消失失效（间歇性卡住）。
        // set(0.0) 值变化 → notify → 触发重组（无碍：data 已更新）。
        self.progress.set(0.0);
        match duration_ms {
            Some(0) => {
                // Custom(0)：立即消失（0ms 语义 = 瞬显瞬隐）
                self.dismiss();
            }
            Some(ms) if ms > 0 => {
                let host = self.clone();
                crate::animation::push_animatable_with_done(
                    self.progress.clone(),
                    1.0,
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(ms),
                        crate::animation::interpolator::Linear::new(),
                    )),
                    move || {
                        // 防误关：仍显示同一 snackbar 才 dismiss（新 show 覆盖后
                        // data.message 已变）。⚠ peek 快照（动画 tick 非组合上下文）
                        if host.peek_snackbar_data().map(|d| d.message) == Some(msg) {
                            host.dismiss();
                        }
                    },
                );
            }
            _ => {} // Indefinite / Custom(0) 之外（None）——不自动消失
        }
    }

    /// 手动隐藏（dismiss 按钮 / 调用方主动关闭）——同时取消自动消失动画
    pub fn dismiss(&self) {
        crate::animation::cancel_animation(&self.progress);
        self.progress.set(0.0); // 复位计时起点（下次 show 从 0 开始）
        self.data.set(None);
    }

    /// 当前是否显示中
    pub fn is_showing(&self) -> bool {
        self.data.peek().is_some()
    }
}

impl Default for SnackbarHostState {
    fn default() -> Self { Self::new() }
}

/// Snackbar 宿主——观察 hostState 渲染 Snackbar，进出场动画
/// （fade + 底部滑入——Compose 默认 `slideInVertically + fadeIn`）。
///
/// 通常放在页面底部（Scaffold 内容区底部或 overlay 底部）。非模态——
/// 不拦截底层交互（点击穿透）。
///
/// ```rust
/// SnackbarHost::new(host.clone()).build(ctx);
/// ```
pub struct SnackbarHost {
    host_state: SnackbarHostState,
}

impl SnackbarHost {
    pub fn new(host_state: SnackbarHostState) -> Self {
        Self { host_state }
    }

    /// #[composable]：观察 hostState——data 变化触发重组；渲染 Snackbar。
    /// 自动消失在 [`SnackbarHostState::show`] 内处理（push_animatable_with_done
    /// 完成回调 dismiss——无需每帧检查）。
    ///
    /// 布局：**不 fill_max_size**（避免全屏覆盖层在 hit_test 中拦截下方所有
    /// 点击）。底部定位由父容器完成——典型用法放 [`Scaffold::bottom_bar`]
    /// （winia 原生底部槽，只占条自身高度、不遮挡内容区点击），或放入
    /// 底部对齐的覆盖容器。Snackbar 条本身 `fill_max_width` → 底部全宽横条。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // 观察当前数据（注册依赖——show/dismiss 触发重组）
        let data = self.host_state.current_snackbar_data();
        // AnimatedVisibility 进出场——需要 State<bool>：
        // 内部 remember 的 visible 与 data 同步（show/dismiss → data 变 → 同步）
        let visible: State<bool> = ctx.remember(|| data.is_some());
        if visible.peek() != data.is_some() {
            visible.set(data.is_some());
        }
        // ⚠ display State：记住"当前要渲染的 snackbar"（跨重组稳定）。
        // 关键——dismiss 后 data 变 None，但 AnimatedVisibility 的 content 闭包
        // 仍会重跑（winia restartable group 每帧 Enter），若直接条件渲染组合期
        // `data`，退出动画期间内容立即消失。这里用 display 保留旧条，让 exit
        // 动画（fade + slide）期间条仍可见；下次 show 用新 data 覆盖。
        let display: State<Option<SnackbarData>> = ctx.remember(|| data.clone());
        if let Some(d) = &data {
            // show：更新显示内容
            display.set(Some(d.clone()));
        }
        let host = self.host_state.clone();
        crate::ui::animated_visibility::AnimatedVisibility::new(visible)
            // M3 Snackbar：从底部滑入（Down = translation_y +off → 0，即自下而上）
            .enter(
                crate::ui::animated_visibility::VisibilityTransition::fade_in(
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(200),
                        crate::animation::interpolator::EaseOutCubic::new(),
                    ),
                )
                .with_slide(crate::ui::animated_visibility::SlideDirection::Down),
            )
            // 退出：向下滑出 + 淡出（exit 动画期间 content 保留——用 display
            // 渲染旧条，直到 AnimatedVisibility removed 回收）
            .exit(
                crate::ui::animated_visibility::VisibilityTransition::fade_out(
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(150),
                        crate::animation::interpolator::EaseInCubic::new(),
                    ),
                )
                .with_slide(crate::ui::animated_visibility::SlideDirection::Down),
            )
            .build(ctx, |ctx| {
                // 渲染 display（保留旧条给退出动画），而非组合期 data
                if let Some(data) = display.get() {
                    let host = host.clone();
                    Snackbar::new(data)
                        .on_dismiss(move || host.dismiss())
                        .build(ctx);
                }
            });
    }
}

/// Snackbar 视觉组件（对标 Compose Material3 `Snackbar`）——
/// inverse_surface 圆角条 + message + action/dismiss 按钮。
pub struct Snackbar {
    data: SnackbarData,
    /// dismiss 回调（Host 注入——dismiss 按钮触发）
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Snackbar {
    pub fn new(data: SnackbarData) -> Self {
        Self { data, on_dismiss: None }
    }

    /// 注入 dismiss 回调（SnackbarHost 内部用——dismiss 按钮触发隐藏）
    pub(crate) fn on_dismiss(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// #[composable]：渲染 Snackbar 外观
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        let bg = theme.inverse_surface;
        let fg = theme.inverse_on_surface;
        let accent = theme.inverse_primary;
        let data = self.data;
        let on_dismiss = self.on_dismiss;

        crate::ui::layout_components::Row::new()
            .modifier(Modifier::new()
                .fill_max_width()
                .padding_horizontal(16.0)
                .padding_vertical(10.0)
                .background(bg, Shape::RoundedRect { corner_radius: 8.0 }))
            .spacing(8.0)
            .build(ctx, |ctx| {
                // message（weight 弹性占位——action/dismiss 靠右）
                crate::ui::Text::new(data.message.as_str())
                    .font_size(14.0)
                    .color(fg)
                    .modifier(Modifier::new().layout_weight(1.0))
                    .build(ctx);
                // action 按钮（M3：inversePrimary 色文本）
                if let Some(action_label) = &data.action_label {
                    let cb = data.on_action.clone();
                    let label = action_label.clone();
                    crate::ui::Button::text()
                        .on_click(move || {
                            if let Some(cb) = &cb {
                                (cb)();
                            }
                        })
                        .build(ctx, |ctx| {
                            crate::ui::Text::new(label.as_str())
                                .font_size(14.0)
                                .color(accent)
                                .build(ctx);
                        });
                }
                // dismiss 按钮（×）——需要 host 引用（由 Host 注入）
                if data.with_dismiss_action {
                    if let Some(on_dismiss) = &on_dismiss {
                        let cb = on_dismiss.clone();
                        crate::ui::Button::text()
                            .on_click(move || (cb)())
                            .build(ctx, |ctx| {
                                crate::ui::Text::new("✕").font_size(14.0).color(fg).build(ctx);
                            });
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HostState：show/current/dismiss 状态机 + 覆盖语义
    #[test]
    fn host_state_show_dismiss_and_override() {
        let host = SnackbarHostState::new();
        assert!(!host.is_showing());
        assert!(host.current_snackbar_data().is_none());

        host.show(SnackbarData::new("msg1"));
        assert!(host.is_showing());
        assert_eq!(host.current_snackbar_data().unwrap().message, "msg1");

        // 覆盖：新 show 替换当前
        host.show(SnackbarData::new("msg2").duration(SnackbarDuration::Long));
        assert_eq!(host.current_snackbar_data().unwrap().message, "msg2");
        assert_eq!(host.current_snackbar_data().unwrap().duration, SnackbarDuration::Long);

        host.dismiss();
        assert!(!host.is_showing());
        assert!(host.current_snackbar_data().is_none());
    }

    /// SnackbarData：构建器 + PartialEq（on_action 不参与相等）
    #[test]
    fn snackbar_data_builder_and_eq() {
        let d1 = SnackbarData::new("hello")
            .action("undo", || {})
            .duration(SnackbarDuration::Long)
            .with_dismiss_action();
        assert_eq!(d1.message, "hello");
        assert_eq!(d1.action_label.as_deref(), Some("undo"));
        assert_eq!(d1.duration, SnackbarDuration::Long);
        assert!(d1.with_dismiss_action);

        // 同 message/action/duration/dismiss → 相等（on_action 回调不参与）
        let d2 = SnackbarData::new("hello")
            .action("undo", || {})
            .duration(SnackbarDuration::Long)
            .with_dismiss_action();
        assert_eq!(d1, d2);

        // message 不同 → 不等
        let d3 = SnackbarData::new("world");
        assert_ne!(d1, d3);
    }

    /// duration → 毫秒映射
    #[test]
    fn snackbar_duration_millis() {
        assert_eq!(SnackbarDuration::Short.millis(), Some(4000));
        assert_eq!(SnackbarDuration::Long.millis(), Some(10000));
        assert_eq!(SnackbarDuration::Indefinite.millis(), None);
    }

    /// SnackbarHost 组合期渲染：show 后树中出现 message + action；dismiss 后消失
    #[test]
    fn snackbar_host_renders_and_hides() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let host = SnackbarHostState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                SnackbarHost::new(host.clone()).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let Some(root) = composer.layout_root_idx() else { return Vec::new() };
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            // 复用 nav 的 collect 思路——直接遍历节点 modifier
            fn collect(nodes: &[crate::layout::node::LayoutNode], idx: usize, out: &mut Vec<String>) {
                let node = &nodes[idx];
                for el in node.modifier.elements() {
                    if let crate::modifier::ModifierElement::TextContent { content, .. } = el {
                        out.push(content.clone());
                    }
                }
                for &c in &node.children {
                    collect(nodes, c, out);
                }
            }
            collect(nodes, root, &mut out);
            out
        };

        // 初始：无 snackbar
        build(&mut composer);
        assert!(texts(&mut composer).is_empty(), "初始不应渲染 snackbar");

        // show：渲染 message + action
        host.show(SnackbarData::new("hello").action("undo", || {}));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("hello")), "应渲染 message: {t:?}");
        assert!(t.iter().any(|x| x.contains("undo")), "应渲染 action: {t:?}");

        // dismiss：消失
        host.dismiss();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(!t.iter().any(|x| x.contains("hello")), "dismiss 后应消失: {t:?}");
    }

    /// 自动消失：show 启动 duration 动画 → update_animations 推进 →
    /// 完成后 dismiss（真实时钟 + 短时长）
    #[test]
    fn host_state_auto_dismiss_after_duration() {
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let host = SnackbarHostState::new();

        // 短时长（100ms）——动画完成后自动 dismiss
        host.show(SnackbarData::new("auto").duration(SnackbarDuration::Custom(100)));
        assert!(host.is_showing(), "show 后应立即显示");

        // 推进动画（真实时钟：sleep 超过动画时长 + update）
        std::thread::sleep(Duration::from_millis(250));
        crate::animation::update_animations();
        assert!(!host.is_showing(), "时长到应自动 dismiss");
        assert!(host.current_snackbar_data().is_none());
    }

    /// Indefinite 不自动消失（需手动 dismiss）
    #[test]
    fn host_state_indefinite_does_not_auto_dismiss() {
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let host = SnackbarHostState::new();
        host.show(SnackbarData::new("sticky").duration(SnackbarDuration::Indefinite));
        assert!(host.is_showing());
        std::thread::sleep(Duration::from_millis(250));
        crate::animation::update_animations();
        assert!(host.is_showing(), "Indefinite 不应自动消失");
        host.dismiss();
        assert!(!host.is_showing());
    }

    /// 连续 show 覆盖：新 show 取消旧动画 → 只有最后一条的定时器有效
    #[test]
    fn host_state_override_rearms_timer() {
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let host = SnackbarHostState::new();
        // 3 条连续 show（模拟 demo 覆盖按钮）
        for i in 0..3 {
            host.show(SnackbarData::new(format!("消息 #{i}")).duration(SnackbarDuration::Custom(200)));
        }
        assert_eq!(host.current_snackbar_data().unwrap().message, "消息 #2");
        // 推进超过 200ms——最后一条应 dismiss
        std::thread::sleep(Duration::from_millis(400));
        crate::animation::update_animations();
        assert!(!host.is_showing(), "覆盖后最后一条 200ms 后应自动消失（实际: {:?}）", host.current_snackbar_data());
    }

    /// ⚠ 退出动画期间内容保留（display 方案的核心——第 3 轮修复）。
    /// 流程：show → 推进 enter 动画（progress 0→1）→ dismiss →
    /// 断言退出动画中内容仍在（display 保留旧条）→ 推进 exit → 断言移除。
    ///
    /// 注意：必须先把 enter 动画推进到 progress≈1，否则 dismiss 时 progress≈0
    /// 触发 removed 立即回收，走不到"退出动画中保留"的路径（这正是
    /// `snackbar_host_renders_and_hides` 没测到的缺口）。
    #[test]
    fn snackbar_host_keeps_content_during_exit_animation() {
        use std::time::Duration;
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let host = SnackbarHostState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                SnackbarHost::new(host.clone()).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let has_msg = |composer: &mut Composer| -> bool {
            let Some(root) = composer.layout_root_idx() else { return false };
            let nodes = composer.arena_nodes();
            fn collect(nodes: &[crate::layout::node::LayoutNode], idx: usize, out: &mut Vec<String>) {
                let node = &nodes[idx];
                for el in node.modifier.elements() {
                    if let crate::modifier::ModifierElement::TextContent { content, .. } = el {
                        out.push(content.clone());
                    }
                }
                for &c in &node.children { collect(nodes, c, out); }
            }
            let mut out = Vec::new();
            collect(nodes, root, &mut out);
            out.iter().any(|x| x.contains("exit-test"))
        };
        // 推进动画（真实时钟）：每帧 update + sleep，模拟事件循环驱动
        let mut advance = |composer: &mut Composer, ms: u64| {
            let steps = 8;
            for _ in 0..steps {
                crate::animation::update_animations();
                std::thread::sleep(Duration::from_millis(ms / steps as u64));
                build(composer);
            }
        };

        // show（Indefinite——避免自动消失干扰）+ 渲染
        host.show(SnackbarData::new("exit-test").duration(SnackbarDuration::Indefinite));
        build(&mut composer);
        assert!(has_msg(&mut composer), "show 后应渲染");

        // 推进 enter 动画到 progress≈1（200ms enter）
        advance(&mut composer, 250);
        assert!(has_msg(&mut composer), "enter 完成后仍显示");

        // dismiss：退出动画中——display 保留旧条，内容应仍在
        host.dismiss();
        advance(&mut composer, 50); // 退出动画刚起步（150ms exit）
        assert!(has_msg(&mut composer), "dismiss 后退出动画初期内容应保留（display 方案）");

        // 推进 exit 动画完成（>150ms）→ removed → 内容移除
        advance(&mut composer, 200);
        build(&mut composer);
        assert!(!has_msg(&mut composer), "退出动画完成后内容应移除");
    }
}
