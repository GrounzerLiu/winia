//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

use std::time::Instant;

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::debug_log;
use crate::layout::node::{hit_test, focus_next, focus_prev, LayoutNode};
use crate::render;
pub(crate) struct PendingWindow {
    pub width: f32,
    pub height: f32,
    pub title: String,
    pub content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>,
    pub on_close: Option<Box<dyn FnMut() + Send>>,
    pub created_id: Option<u64>,
    pub theme: Option<crate::ui::theme::ThemeColors>,
}
use skiwin::{SkiaWindowTrait, vulkan::VulkanSkiaWindow};
use skiwin::vulkan::{request_capture, take_capture};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::monitor::MonitorHandleProvider;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

/// 方向键焦点导航方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusDir { Left, Right, Up, Down }

/// 查询窗口当前所在显示器的刷新率 → 帧间隔（mHz：300000 = 300Hz）。
/// 查询失败返回 None——调用方保留旧值（创建期才回退 16ms）。
fn current_frame_interval(sw: &VulkanSkiaWindow) -> Option<std::time::Duration> {
    let mhz = sw.current_monitor()
        .and_then(|m| m.current_video_mode())
        .and_then(|v| v.refresh_rate_millihertz())
        .map(|m| m.get())?;
    Some(std::time::Duration::from_nanos(1_000_000_000_000 / mhz as u64))
}

// ── PerWindow ──

pub(crate) struct PerWindow {
    pub(crate) composer: Composer,
    pub(crate) skia_window: Option<VulkanSkiaWindow>,
    width: f32, height: f32,
    pub(crate) scale_factor: f64,
    pub(crate) focused_id: Option<u64>,
    pub(crate) content: Box<dyn Fn(&mut ComposeCtx)>,
    pub(crate) on_close: Option<Box<dyn FnMut() + Send>>,
    pub(crate) created_id: Option<u64>,
    theme: crate::ui::theme::ThemeColors,
    /// 渲染帧计数（vsync 研究——Fifo 下应 ~60fps）
    pub(crate) frame_counter: u64,
    /// 上次 request_redraw 时刻（request 节流独立计时——避免与渲染节流共用
    /// last_render_time 导致理论上的 2I 间隔减半：WM_PAINT 处理晚于 request（ε>0），
    /// 定时器按 I 唤醒时 now-last_render = I-ε < I 恒拦截）
    last_request_time: std::time::Instant,
    /// 帧间隔（屏幕刷新率对齐——窗口创建时从 monitor 获取；刷新率变化（显示器
    /// 切换）需重建窗口——当前不做动态跟踪）
    pub(crate) frame_interval: std::time::Duration,
    /// 强制渲染（resize/动画停止等必须显示的帧——跳过分支的请求链断裂修复）
    pub(crate) force_redraw: bool,
    /// 崩溃边界（P3-3）：连续渲染 panic 计数（防风暴停更）
    pub(crate) consecutive_panics: u32,
    /// 渲染已禁用（连续 panic 后停更——保留最后画面，不再自旋）
    pub(crate) render_disabled: bool,

    /// 上次渲染时间（帧率限制——Windows acquire 不阻塞 vsync，应用层节流 60fps）
    pub(crate) last_render_time: std::time::Instant,
    /// 焦点节点的 slot_key
    pub(crate) focused_slot_key: Option<u64>,
    /// 指针按下态（Compose 风格 click 检测）
    pointer_down_state: Option<PtrDownState>,
    /// 最近的 PointerKind（Move 事件继承自上一个 Down）
    last_pointer_kind: crate::modifier::PointerKind,
    /// Down 时的最内层节点 ID（后续 Move/Up 优先发给此节点，而非 hit_test）
    pointer_down_slot: Option<u64>,
    /// 手势状态机（单活动手势——down 创建，up/cancel 销毁）
    gesture: Option<crate::input::gesture::GestureTracker>,
    /// 手势回调路由：手势节点 id
    gesture_node: Option<u64>,
    /// 双击上下文（上次 tap 的节点/时刻/位置——跨手势传递；
    /// 绑定节点——不同节点的手势不共享双击计数）
    gesture_tap_ctx: Option<(u64, std::time::Instant, (f32, f32))>,
    /// 手势节点的 slot_key（跨重组稳定——node_id 会变，find_node_by_id 会失败）
    gesture_slot: Option<u64>,
    /// 顶层弹出层（独立组合单元——渲染在主树之上）
    overlays: Vec<OverlayWindow>,
    /// overlay 点击目标（down 命中 overlay 记录——up 执行 click；v1 仅 clickable）
    overlay_click: Option<(usize, (f32, f32), u64)>,
    /// 延迟 tap 列表（节点注册 on_double_tap 时——Compose 语义：onTap 延迟到
    /// 双击窗口结束；窗口内第二次 down 同节点 → 取消；超时 → 补发；不同节点
    /// 的 pending 相互独立——快速连续点击多个手势节点时各自按 deadline 补发）
    pending_taps: Vec<crate::input::gesture::PendingTap>,
    /// 上次刷新率查询时刻（Moved/ScaleFactorChanged 高频触发——300ms 去抖）
    last_refresh_check: std::time::Instant,
    /// 当前悬停节点的 slot_key（指针移入/移出时发射 Hover Enter/Exit）
    hovered_slot: Option<u64>,
    /// 当前按下交互（clickable 绑定源 + 按下节点 slot——Up/越界 slop 时释放）
    pressed_interaction: Option<(u64, crate::ui::interaction::MutableInteractionSource)>,
    /// 已发射 Focus 的节点 slot（focus 变化时对旧节点补发 Unfocus）
    focused_interaction_slot: Option<u64>,
}

/// 顶层弹出层实例——独立 Composer 组合单元（State 跨帧保持），
/// 渲染定位在主树之上（模态遮罩 + 内容）
struct OverlayWindow {
    id: u64,
    composer: crate::core::composer::Composer,
    anchor_slot: Option<u64>,
    position: crate::ui::overlay::PopupPosition,
    offset: (f32, f32),
    modal: bool,
    dismiss_on_outside: bool,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    content: Box<dyn Fn(&mut ComposeCtx)>,
    /// 渲染/命中用的屏幕位置（逻辑坐标——每帧布局后更新）
    screen_pos: (f32, f32),
}

/// Compose 风格的 click 检测中间状态
struct PtrDownState {
    node_id: u64,
    position: (f32, f32),
    time: Instant,
    /// 文本选区的起始字符位置（Down 时记录）
    selection_anchor: Option<usize>,
    /// Down 时所在的 SelectionContainer registrar（拖动跨容器时选择不切偏移空间）
    anchor_registrar: Option<crate::ui::selection_container::SelectionRegistrar>,
}

impl PerWindow {
    fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32, theme: crate::ui::theme::ThemeColors) -> Self {
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, on_close: None, created_id: None, theme, focused_slot_key: None, pointer_down_state: None, last_pointer_kind: crate::modifier::PointerKind::Mouse { button: crate::modifier::PointerButton::Primary }, pointer_down_slot: None, gesture: None, gesture_node: None, gesture_tap_ctx: None, gesture_slot: None, overlays: Vec::new(), overlay_click: None, pending_taps: Vec::new(), frame_counter: 0, last_render_time: std::time::Instant::now(), frame_interval: std::time::Duration::from_millis(16), force_redraw: false, consecutive_panics: 0, render_disabled: false, last_request_time: std::time::Instant::now(), last_refresh_check: std::time::Instant::now(), hovered_slot: None, pressed_interaction: None, focused_interaction_slot: None }
    }
    pub(crate) fn created_id(&self) -> Option<u64> { self.created_id }

    /// 焦点同步：focused_slot_key 变化时，对旧焦点节点补发 Unfocus、
    /// 新焦点节点补发 Focus（布局后调用——焦点标志已随重组刷新）。
    fn sync_focus_interaction(&mut self) {
        let new_slot = self.focused_slot_key;
        if new_slot == self.focused_interaction_slot {
            return;
        }
        if let Some(old) = self.focused_interaction_slot.take() {
            let nodes = self.composer.arena_nodes();
            if let Some(r) = self.composer.layout_root_idx() {
                if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, old) {
                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                        if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                            src.emit_unfocus();
                        }
                    }
                }
            }
        }
        if let Some(new) = new_slot {
            let nodes = self.composer.arena_nodes();
            if let Some(r) = self.composer.layout_root_idx() {
                if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, new) {
                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                        if let Some(src) = nodes[idx].modifier.focusable_interaction() {
                            src.emit_focus();
                            self.focused_interaction_slot = Some(new);
                        }
                    }
                }
            }
        }
    }

    /// 按指定焦点节点同步 IME 开关（Tab/方向键/Escape/焦点请求共用）——
    /// 框架只做机械转发：节点声明了 ime_callback 才开启，否则关闭。
    fn apply_ime_for_focus(&self, fid: Option<u64>) {
        let wants_ime = if let Some(fid) = fid {
            if let Some(r) = self.composer.layout_root_idx() {
                let nodes = self.composer.arena_nodes();
                crate::layout::node::find_node_by_id(nodes, r, fid)
                    .map(|idx| nodes[idx].ime_callback.borrow().is_some())
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        if let Some(ref sw) = self.skia_window {
            sw.set_ime_allowed(wants_ime);
        }
    }

    /// 重新查询窗口所在显示器的刷新率并更新帧间隔（跨屏跟随）。
    /// Moved/ScaleFactorChanged 高频触发——300ms 去抖；查询失败保留旧值
    /// （避免瞬时失败把高刷错误降级成 60fps）。
    fn refresh_frame_interval(&mut self) {
        const RECHECK_MIN: std::time::Duration = std::time::Duration::from_millis(300);
        let now = std::time::Instant::now();
        if now.duration_since(self.last_refresh_check) < RECHECK_MIN {
            return;
        }
        self.last_refresh_check = now;
        let Some(ref sw) = self.skia_window else { return; };
        let Some(interval) = current_frame_interval(sw) else { return; };
        if interval != self.frame_interval {
            debug_log!("[refresh] frame_interval {:?} -> {:?}", self.frame_interval, interval);
            self.frame_interval = interval;
            if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
        }
    }

    /// 补发延迟的 tap（Compose：onDoubleTap 存在时 onTap 延迟到双击窗口结束）。
    /// 超时或按下其他节点时调用——按当前布局树换算组件本地坐标。
    fn fire_pending_tap(&mut self, t: crate::input::gesture::PendingTap) {
        let nodes = self.composer.arena_nodes();
        let Some(r) = self.composer.layout_root_idx() else { return; };
        fire_gesture_action(
            nodes,
            r,
            t.slot_key,
            crate::input::gesture::GestureAction::Tap(t.pos),
        );
        if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
    }

    /// 清除焦点 + 更新缓存
    fn clear_focus(&mut self, nodes: &mut Vec<LayoutNode>, root: usize) {
        crate::layout::node::clear_focus(nodes, root);
        self.focused_id = None;
        self.focused_slot_key = None;
        if let Some(ref sw) = self.skia_window { sw.set_ime_allowed(false); }
    }

    /// 从当前焦点节点刷新 cached 字段
    fn refresh_focus(&mut self, nodes: &[LayoutNode], root: usize) {
        // 如果树中已有焦点节点，直接读取
        if let Some(fid) = crate::layout::node::get_focus_id(nodes, root) {
            self.focused_id = Some(fid);
            self.focused_slot_key = crate::layout::node::find_node_by_id(nodes, root, fid).map(|idx| nodes[idx].slot_key);
        }
        // 否则：如果之前有关焦点但树中丢失了（重组后新节点 focus=false），保留 focused_id
        // （由后续触发的 set_focus_by_id 或 pointer 事件补上树的焦点标记）
    }

    /// 方向键移动焦点（对标 Compose Desktop 方向导航）——
    /// 候选 = 目标方向半平面内的可聚焦节点，得分 = 方向距离 + 垂直偏离×2
    fn focus_move_directional(&mut self, dir: FocusDir) -> bool {
        let Some(fid) = self.focused_id else { return false; };
        let Some(r) = self.composer.layout_root_idx() else { return false; };
        let best = {
            let nodes = self.composer.arena_nodes();
            let candidates = crate::layout::node::collect_focus_candidates(nodes, r);
            let Some((_, cur_cx, cur_cy)) = candidates.iter().find(|(id, _, _)| *id == fid) else {
                return false;
            };
            let mut best: Option<(f32, u64)> = None;
            for &(id, cx, cy) in &candidates {
                if id == fid { continue; }
                let (dx, dy) = (cx - cur_cx, cy - cur_cy);
                let in_dir = match dir {
                    FocusDir::Right => dx > 0.0,
                    FocusDir::Left => dx < 0.0,
                    FocusDir::Down => dy > 0.0,
                    FocusDir::Up => dy < 0.0,
                };
                if !in_dir { continue; }
                let (main, cross) = match dir {
                    FocusDir::Right | FocusDir::Left => (dx.abs(), dy.abs()),
                    FocusDir::Down | FocusDir::Up => (dy.abs(), dx.abs()),
                };
                let score = main + cross * 2.0;
                if best.map_or(true, |(s, _)| score < s) {
                    best = Some((score, id));
                }
            }
            best
        };
        if let Some((_, target)) = best {
            let nodes = self.composer.arena_nodes_mut();
            crate::layout::node::clear_focus(nodes, r);
            if crate::layout::node::set_focus_by_id(nodes, r, target) {
                self.focused_id = Some(target);
                self.focused_slot_key =
                    crate::layout::node::find_node_by_id(nodes, r, target)
                        .map(|idx| nodes[idx].slot_key);
                // IME 按组件声明（方向键聚焦文本组件时开启输入法）
                self.apply_ime_for_focus(Some(target));
                return true;
            }
        }
        false
    }

    /// 增量重组 → 恢复焦点 → 布局 → 渲染（供 RedrawRequested 使用）
    /// 循环消费 notify 队列直到稳定，避免 tokio task 的并发通知丢失。
    fn recompose_layout_render(&mut self, after_draw: impl FnOnce(&[LayoutNode], usize, &mut skia_safe::Surface)) {
        // vsync 研究：渲染帧计数（每秒渲染次数——Fifo 下应 ~60）
        self.frame_counter += 1;
        debug_log!("[fps] render#{} compose#{} pending={}", self.frame_counter, self.composer.compose_count(), self.composer.pending_state_count());
        // 临时：窗口节点数（诊断主窗口塌缩）
        // 清除待关闭标志——只捕获本次重组的 on_remove，防止跨窗口污染
        crate::ui::window::reset_pending_remove();
        // 提供当前窗口 Density（从 scale_factor）——覆盖 compose + layout + draw 全程，
        // 保证 Dimension::Px / TextUnit::Px 在布局/渲染期使用窗口 sf 而非 standard(1.0)
        let density = crate::unit::Density::from_density(self.scale_factor as f32);
        crate::unit::with_density(density, || {
        // 循环 compose 直到没有新的 pending state——处理并发 task 在 compose 期间
        // 完成的 case（第二个 notify 的 state 在第一次 compose 之后才入队）
        // 循环 compose 直到没有新的 pending state
        let mut any_composed = false;
        loop {
            let did_compose = self.composer.recompose(|ctx| (self.content)(ctx));
            any_composed |= did_compose;
            if let Some(slot_key) = self.focused_slot_key {
                if let Some(r) = self.composer.layout_root_idx() {
                    let nodes = self.composer.arena_nodes_mut();
                    if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot_key) {
                        crate::layout::node::clear_focus(nodes, r);
                        crate::layout::node::set_focus_by_id(nodes, r, new_id);
                        self.focused_id = Some(new_id);
                            debug_log!("[focus] restored by slot_key id={}", new_id);
                    } else {
                        self.focused_id = None;
                        self.focused_slot_key = None;
                    }
                }
            }
            // 如果在 compose 期间又有新 notify 入队，需要再处理一次
            if !did_compose && !self.composer.has_pending_states() {
                break;
            }
        }
        self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
        // 焦点交互同步（Focus/Unfocus 发射——focus 标志已随重组刷新）
        self.sync_focus_interaction();

        // 顶层弹出层：同步（按 id 匹配保留 State）+ compose + layout + 定位
        // ⚠ recomposed 标志：recompose 跳过的帧（无 pending State 变化）不能执行
        // retain 删除——take_overlays 为空会误删仍开着的 overlay（dialog_open 未变），
        // 导致 overlay 消失但状态残留 → 下次点击 toggle 错乱（"点两次才开"）
        sync_overlays(self, any_composed);
        layout_overlays(self);

        let bg = self.theme.background;

        if let Some(root_idx) = self.composer.layout_root_idx() {
            let nodes = self.composer.arena_nodes();
            if let Some(ref mut sw) = self.skia_window {
                let sf = self.scale_factor as f32;
                if crate::debug::screenshot_requested() {
                    request_capture();
                }
                sw.draw(|surface| {
                    let canvas = surface.canvas();
                    canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save();
                    canvas.scale((sf, sf));
                    render::render(nodes, root_idx, canvas);
                    canvas.restore();
                    // overlay 渲染在主树之上（逻辑坐标——translate 已含 scale）
                    render_overlays(&self.overlays, canvas, sf, (self.width, self.height));
                    after_draw(nodes, root_idx, surface);
                });
                // 截图读回在 flush 之后（skiwin draw 内）——保证真实呈现帧
                if crate::debug::screenshot_requested() {
                    if let Some((w2, h2, pixels)) = take_capture() {
                        crate::debug::update_pixels(&pixels, w2, h2);
                    }
                    crate::debug::screenshot_done();
                }
            }
        }
        });
    }
}

// ── AppState ──

struct AppState {
    /// 已创建的窗口
    windows: HashMap<WindowId, PerWindow>,
    /// 待创建的窗口（由 Window composable 排队）
    pending_content: Vec<PendingWindow>,
    /// 父窗口 ID（用于 is_parent 判断，不依赖 HashMap 顺序）
    parent_window_id: Option<WindowId>,
    /// 窗口全局修饰键状态
    pub(crate) modifiers: winit::keyboard::ModifiersState,
    /// 初始化回调（仅首次调用，用于声明式创建主窗口）
    init: Option<Box<dyn FnOnce(&mut ComposeCtx)>>,
    /// 上轮动画是否活跃（停止时强制终帧渲染）
    was_animating: bool,
}

impl ApplicationHandler for AppState {
        fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        // Wait + request_redraw 自驱动动画（避免 Poll↔Wait 切换竞态丢帧）
        event_loop.set_control_flow(ControlFlow::Wait);
        // 每轮推进动画（与窗口解耦，多窗口/子窗口动画均正确推进）
        // 动画（水波纹/状态层过渡也注册在全局动画列表——自动驱动重绘）
        let animating = crate::animation::update_animations();
        if animating {
            // 动画活跃：WaitUntil 定时唤醒（对齐刷新率）保证每帧唤醒（不冻结），
            // request 节流（距上次渲染 >= 帧间隔）限制 WM_PAINT 生成频率——
            // 修复 request 每轮发送 → WM_PAINT 消息唤醒 Wait 的 55k/s 自驱动空转
            let interval = self.windows.values().map(|pw| pw.frame_interval).min()
                .unwrap_or(std::time::Duration::from_millis(16));
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + interval));
            let now = std::time::Instant::now();
            for pw in self.windows.values_mut() {
                if now.duration_since(pw.last_request_time) >= pw.frame_interval {
                    pw.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
        } else if self.was_animating {
            // 动画刚停止：强制终帧渲染（最后一次 set 的 request 可能被跳过）
            for pw in self.windows.values_mut() {
                if let Some(ref sw) = pw.skia_window {
                    pw.force_redraw = true; // 仅可渲染窗口设（避免 None 残留）
                    sw.request_redraw();
                }
            }
        }
        // 兜底：pending 存在（State 已变化待重组——点击/异步回调）或渲染欠账
        // （force_redraw——帧节流跳过的更新）时请求重绘。节流命中不丢弃：
        // WaitUntil 到下一个可用时刻重试，保证一次性更新不卡到外部事件
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            // 渲染欠账按 last_render 对齐（force_redraw 由帧节流跳过时设置）
            if pw.force_redraw {
                if now.duration_since(pw.last_render_time) >= pw.frame_interval {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.last_render_time + pw.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
            // pending state 按 last_request 节流（动画持续 pending 时每帧至多一次）
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.last_request_time) >= pw.frame_interval {
                    pw.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.last_request_time + pw.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // 延迟 tap 超时补发（Compose detectTapGestures：onDoubleTap 存在时
        // onTap 延迟触发）——超时立即补发；未超时用 WaitUntil 定时唤醒
        // （idle 时也能在双击窗口结束后补发，不依赖下一次输入事件）
        let now = std::time::Instant::now();
        let mut next_tap_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            let mut kept = Vec::new();
            for t in std::mem::take(&mut pw.pending_taps) {
                if t.deadline <= now {
                    pw.fire_pending_tap(t);
                } else {
                    let d = t.deadline;
                    kept.push(t);
                    next_tap_deadline = Some(match next_tap_deadline {
                        Some(e) => e.min(d),
                        None => d,
                    });
                }
            }
            pw.pending_taps = kept;
        }
        if let Some(d) = next_tap_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // DevTools 事件兜底消费（主窗口）——多窗口下主窗口在后台时
        // RedrawRequested 不来（window_event 不调用）→ 注入事件卡队列
        if let Some(wid) = self.parent_window_id {
            self.consume_debug_events(wid);
        }
        self.was_animating = animating;
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        AppState::process_pending_windows(self, event_loop);
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
                // 如果 debug server 请求关闭，退出事件循环
        if debug::is_shutdown() {
            event_loop.exit();
            return;
        }
        AppState::process_pending_windows(self, event_loop);
        // 消费 pending close（on_remove 推入，compose 末尾也消费一次）
        crate::ui::window::Window::process_detached(&mut self.windows, event_loop, &|| debug::force_shutdown());
        // 调试工具有 pending 请求时唤醒窗口（截图/模拟事件需要 RedrawRequested）
        if debug::has_pending() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // 异步 State 变更唤醒事件循环后需要 request_redraw（节流：距上次 request
        // 够帧间隔才发——动画每帧 notify 触发 proxy_wake_up，无节流会渲染风暴）
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.last_request_time) >= pw.frame_interval {
                    pw.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    // 节流命中不丢弃：到期重试（一次性更新不卡到外部事件）
                    let d = pw.last_request_time + pw.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_parent = self.parent_window_id.map(|p| p == window_id).unwrap_or(false);
        let closing_last = self.windows.len() == 1;
        let Some(pw) = self.windows.get_mut(&window_id) else { return };

        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if dy != 0.0 {
                    if let Some(root_idx) = pw.composer.layout_root_idx() {
                        apply_scroll_delta(pw.composer.arena_nodes_mut(), root_idx, dy, crate::unit::Density::from_density(pw.scale_factor as f32));
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            WindowEvent::CloseRequested => {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                self.windows.remove(&window_id);
                // 清理 debug 树条目（窗口关闭后不再渲染——残留会让 UI 测试误判）
                debug::remove_tree(window_id.into_raw() as u64);
                // 通知其他窗口重绘（状态可能已变化）
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if closing_last || is_parent {
                    debug::force_shutdown();
                    event_loop.exit();
                }
            }
            WindowEvent::Destroyed => {
                if let Some(cid) = pw.created_id { crate::ui::window::CREATED.lock().unwrap().remove(&cid); }
                self.windows.remove(&window_id);
                debug::remove_tree(window_id.into_raw() as u64);
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            // 跨屏移动：Moved 高频触发（拖动过程）——内部 300ms 去抖
            WindowEvent::Moved { .. } => {
                pw.refresh_frame_interval();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                pw.scale_factor = scale_factor;
                // 换显示器可能伴随缩放变化——顺带刷新帧间隔（去抖在方法内）
                pw.refresh_frame_interval();
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::PointerButton { position, state, button, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                let event_type = if state.is_pressed() {
                    crate::modifier::PointerEventType::Down
                } else {
                    crate::modifier::PointerEventType::Up
                };
                if state.is_pressed() {
                    // ── Down：指针按下核心（共享——真实/Debug 防分叉）──
                    handle_pointer_down(pw, scene_pos, crate::modifier::PointerKind::from_button_source(&button), &self.modifiers, true);
                }
                // ── Up：Compose 风格 click 检测（仅释放时——Down 保留
                // pointer_down_state 供拖动选择；无条件执行会 Down 后立即 take
                // 掉 state → 拖动无法选择）──
                // ⚠ 必须位于 `if state.is_pressed()`（Down 块）之外：Up 事件（Released）
                // 不进入 Down 块——嵌套在块内的 click 检测对 Up 事件永不执行
                // （曾导致真实点击永远无效，而 [sel-up-clean]（块外）正常打印）
                if !state.is_pressed() {
                    // 显式请求重绘：on_click 内的 State set 走 wake_up 链路（异步），
                    // 若无 pending 检查兜底会漏刷新（用户看到 count 不变）
                    if detect_click(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 点击执行（down 命中 overlay 时记录——up 触发；
                    // 主树 detect_click 因 down 短路未记录 pointer_down_state 而空转）
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 手势 up 判定（tap/double-tap/long-press/drag-end）
                    if gesture_up(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 释放按下交互（Compose Release 语义——clickable 按下态结束）
                    release_pressed_interaction(pw);
                }
                // ── 指针事件分发（Up 时先分发后清除 capture）──
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
                    let ptr_ev = crate::modifier::PointerEvent {
                        event_type,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::modifier::PointerKind::from_button_source(&button),
                        is_alt_pressed: self.modifiers.alt_key(),
                        is_ctrl_pressed: self.modifiers.control_key(),
                        is_shift_pressed: self.modifiers.shift_key(),
                        is_meta_pressed: self.modifiers.meta_key(),
                    };
                    pw.last_pointer_kind = ptr_ev.kind.clone();
                    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.pointer_down_slot);
                }
                // Up 后清除 capture + 通知选区变化
                if !state.is_pressed() {
                    // Compose 方式：从拖拽节点 slot_key 取 registrar 直接 fire
                    if let Some(slot) = pw.pointer_down_slot {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                                    if let Some(reg) = nodes[idx].registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.pointer_down_slot = None;
                    // 防御：take 可能未执行（click 检测分支外的边界）——强制清 state
                    pw.pointer_down_state = None;
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                event_loop.set_control_flow(ControlFlow::Poll);
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let fid = crate::layout::node::get_focus_id(nodes, r);
                    let slot = fid.and_then(|id| crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key));
                    pw.focused_id = fid;
                    pw.focused_slot_key = slot;
                }
                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                // 指针移动核心（共享——真实/Debug 防分叉；Debug 路径此前缺
                // x_off 对齐偏移——Center/Right 对齐文本选择错位，合并修复）
                let consumed = handle_pointer_move(pw, scene_pos, pw.last_pointer_kind.clone(), &self.modifiers);
                // 消费（on_pointer_event 可能更新 State）或按下拖动选区时请求重绘；
                // 未消费的悬停移动不唤醒事件循环（避免每帧白醒）
                if consumed || pw.pointer_down_state.is_some() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            // 指针离开窗口：清 hover（对最后一个 hoverable 补发 Exit）
            WindowEvent::PointerLeft { .. } => {
                if let Some(old) = pw.hovered_slot.take() {
                    exit_hover_at(pw, old);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event_type = if event.state.is_pressed() {
                    crate::modifier::KbEventType::KeyDown
                } else {
                    crate::modifier::KbEventType::KeyUp
                };
                let ke = crate::modifier::KbEvent {
                    key: event.logical_key.clone(),
                    event_type,
                    is_alt_pressed: self.modifiers.alt_key(),
                    is_ctrl_pressed: self.modifiers.control_key(),
                    is_shift_pressed: self.modifiers.shift_key(),
                    is_meta_pressed: self.modifiers.meta_key(),
                    repeat: event.repeat,
                };
                let mut consumed = false;
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                    if pw.focused_id.is_some() {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            crate::layout::node::clear_focus(pw.composer.arena_nodes_mut(), r);
                        }
                        pw.focused_id = None;
                        pw.focused_slot_key = None;
                        // Escape 清焦后关闭输入法（避免 IME 残留开启）
                        pw.apply_ime_for_focus(None);
                        consumed = true;
                    }
                }
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    let shift = self.modifiers.shift_key();
                    let (new_id, new_slot) = pw.composer.layout_root_idx().map(|r| {
                        let nodes = pw.composer.arena_nodes_mut();
                        if shift { focus_prev(nodes, r); } else { focus_next(nodes, r); }
                        let id = crate::layout::node::get_focus_id(nodes, r);
                        let slot = id.and_then(|fid| crate::layout::node::find_node_by_id(nodes, r, fid).map(|idx| nodes[idx].slot_key));
                        (id, slot)
                    }).unwrap_or((None, None));
                    pw.focused_id = new_id;
                    pw.focused_slot_key = new_slot;
                    // Tab 聚焦文本组件时同步开启输入法（与方向键/点击路径一致）
                    pw.apply_ime_for_focus(new_id);
                    consumed = true;
                }
                if !consumed {
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            // 收集焦点路径：root → ... → focused
                            let mut path: Vec<usize> = Vec::new();
                            if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                path.push(idx);
                                // 向上收集父链
                                let mut pid = nodes[idx].parent_id;
                                while let Some(id) = pid {
                                    if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                        path.push(anc);
                                        pid = nodes[anc].parent_id;
                                    } else { break; }
                                }
                                path.reverse(); // 现在 path[0] == root, path[last] == focused
                            }

                            // Preview: root → focused（对齐 onPreviewKeyEvent）
                            for &ni in &path {
                                for el in nodes[ni].modifier.elements() {
                                    if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                                        if handler(&ke) { consumed = true; break; }
                                    }
                                }
                                if consumed { break; }
                            }
                            if !consumed {
                                // Bubble: focused → root（对齐 onKeyEvent）
                                for &ni in path.iter().rev() {
                                    for el in nodes[ni].modifier.elements().iter().rev() {
                                        if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                            if handler(&ke) { consumed = true; break; }
                                        }
                                    }
                                    if consumed { break; }
                                }
                            }
                        }
                    }
                }
                // 聚焦组件的键盘激活（对标 Compose clickable：聚焦时按
                // Enter/Space 触发 onClick——仅聚焦节点自身的 clickable 响应，
                // 不向祖先冒泡：clickable 容器内的子组件聚焦时不应触发容器点击）
                let is_activate = matches!(&event.logical_key, Key::Named(NamedKey::Enter))
                    || matches!(&event.logical_key, Key::Character(c) if c == " ");
                if !consumed && event.state.is_pressed() && !event.repeat
                    && is_activate
                {
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                if let Some(on_click) = nodes[idx].modifier.on_click() {
                                    on_click();
                                    consumed = true;
                                }
                            }
                        }
                    }
                }
                // 方向键焦点导航（对标 Compose Desktop arrow-key navigation）——
                // 在目标方向半平面内选"方向距离 + 垂直偏离×2"最小的可聚焦节点
                if !consumed && event.state.is_pressed() && !event.repeat {
                    let dir = match &event.logical_key {
                        Key::Named(NamedKey::ArrowRight) => Some(FocusDir::Right),
                        Key::Named(NamedKey::ArrowLeft) => Some(FocusDir::Left),
                        Key::Named(NamedKey::ArrowDown) => Some(FocusDir::Down),
                        Key::Named(NamedKey::ArrowUp) => Some(FocusDir::Up),
                        _ => None,
                    };
                    if let Some(dir) = dir {
                        if pw.focus_move_directional(dir) {
                            consumed = true;
                        }
                    }
                }
                if consumed {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Preedit(text, cursor) => {
                        // 通过 focused node 的 ime_callback 通知 TextField
                        if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                    if let Some(cb) = nodes[idx].ime_callback.borrow_mut().as_mut() {
                                        cb(&text, cursor);
                                    }
                                }
                            }
                        }
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    Ime::Commit(text) => {
                        // IME 提交文本——派发给聚焦节点的 on_key_event 以 Character 形式
                        if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                // 逐字符发送
                                for ch in text.chars() {
                                    let s = ch.to_string();
                                    let ke = crate::modifier::KbEvent {
                                        key: winit::keyboard::Key::Character(s.clone().into()),
                                        event_type: crate::modifier::KbEventType::KeyDown,
                                        is_alt_pressed: false, is_ctrl_pressed: false,
                                        is_shift_pressed: false, is_meta_pressed: false,
                                        repeat: false,
                                    };
                                    let mut path: Vec<usize> = Vec::new();
                                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                        path.push(idx);
                                        let mut pid = nodes[idx].parent_id;
                                        while let Some(id) = pid {
                                            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                                path.push(anc); pid = nodes[anc].parent_id;
                                            } else { break; }
                                        }
                                        path.reverse();
                                    }
                                    for &ni in path.iter().rev() {
                                        let mut consumed = false;
                                        for el in nodes[ni].modifier.elements().iter().rev() {
                                            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                if handler(&ke) { consumed = true; break; }
                                            }
                                        }
                                        if consumed { break; }
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                            }
                        }
                    }
                    Ime::Enabled | Ime::Disabled => {}
                    _ => {}
                }
            }
            WindowEvent::SurfaceResized(s) => {
                let l = s.to_logical::<f32>(pw.scale_factor);
                if (l.width - pw.width).abs() > pw.width * 0.5
                    || (l.height - pw.height).abs() > pw.height * 0.5 { /* winit bug #2094 */ }
                else { pw.width = l.width; pw.height = l.height; }
                if let Some(ref mut sw) = pw.skia_window { sw.resize(); }
                // 确保下一帧以新尺寸重新布局（有些平台 resize 后不自动触发 RedrawRequested）
                pw.force_redraw = true; // resize 重绘不因帧率限制跳过而丢失
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // 帧时钟 tick（P3-12：with_frame_nanos 的驱动源——每帧广播时间戳）
                crate::effect::frame_tick();
                                // 动画推进已移到 new_events（每轮一次，与窗口解耦）
                // 消费焦点请求（在 compose 前处理，避免丢失）
                for id in crate::modifier::take_focus_requests() {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes_mut();
                        if crate::layout::node::focus_by_id(nodes, r, id) {
                            pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                        }
                    }
                    // 单独查 slot_key（避免与 root 的 borrow 冲突）
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(found) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                pw.focused_slot_key = Some(nodes[found].slot_key);
                                // IME 按组件声明开关：文本组件（TextField）在组合期
                                // set_current_node_ime_callback 主动声明 IME 需求；
                                // 框架只做机械转发，不判断组件类型。Button 等未声明
                                // ime_callback 的 focusable 聚焦时不开启输入法。
                                let wants_ime = nodes[found].ime_callback.borrow().is_some();
                                if let Some(ref sw) = pw.skia_window {
                                    sw.set_ime_allowed(wants_ime);
                                }
                            }
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                // 增量重组 → 布局 → 渲染
                // 帧率限制：Windows 上 acquire_next_image 不阻塞 vsync（实测 1-2ms），
                // 无节流会 ~1300fps 渲染风暴（present fence 只等 GPU 提交不等显示刷新）。
                // 距上次渲染 <16ms（~60fps）跳过——动画值下轮渲染时取最新（不丢帧）。
                let now = std::time::Instant::now();
                if pw.render_disabled {
                    // 崩溃边界（P3-3）：连续 panic 后停更——保留最后画面
                    return;
                }
                if !pw.force_redraw && now.duration_since(pw.last_render_time) < pw.frame_interval {
                    // 帧节流命中：不丢弃本次更新——记渲染欠账（force_redraw），
                    // 下一个可用帧由 new_events 补发 request_redraw。原实现直接
                    // 跳过：一次性状态变更（如双击延迟 tap）会卡到外部事件才刷新
                    pw.force_redraw = true;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(pw.last_render_time + pw.frame_interval));
                } else {
                pw.last_render_time = now;
                let w = pw.width;
                let h = pw.height;
                let sf = pw.scale_factor as f32;
                // 崩溃边界（P3-3）：compose/layout/draw 任一段 panic（用户 content 代码 /
                // skia 异常）不崩窗口——捕获后跳过本帧（保留上帧画面），下帧正常重试。
                // 连续 panic 计数防风暴：超过阈值打印错误并停更（不再自旋）。
                // 渲染路径无 unsafe（P2-3 后）——catch_unwind 后继续用 self 仅"逻辑不一致"
                // 非内存不安全；slot/arena 每帧从 root 重建结构，panic 中断的半状态下帧自愈。
                let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let wid = window_id.into_raw() as u64;
                    pw.recompose_layout_render(|nodes, root_idx, surface| {
                        debug::update_tree(wid, &debug::build_tree_json(nodes, root_idx));
                    });
                }));
                match panic_result {
                    Ok(()) => {
                        pw.force_redraw = false; // 渲染成功后才清除强制帧（中途异常保留）
                        pw.consecutive_panics = 0;
                        // 动画自驱动兜底：渲染中注册的动画依赖 wake_up 启动下一轮，
                        // 但 Windows 上从事件处理内调用 EventLoopProxy::wake_up 偶发丢失
                        // （winit 已知竞态）→ 动画冻结在起始值，直到下一个外部事件
                        // （鼠标移动）才用大 dt 一次收敛。显式 request_redraw 保证
                        // 动画帧持续推进，不依赖那次可能丢失的唤醒。
                        if crate::animation::is_animating() {
                            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        }
                    }
                    Err(e) => {
                        pw.consecutive_panics += 1;
                        let msg = if let Some(s) = e.downcast_ref::<&str>() { (*s).to_string() }
                                  else if let Some(s) = e.downcast_ref::<String>() { s.clone() }
                                  else { "unknown panic".to_string() };
                        eprintln!("[render-panic] 第 {} 次连续 panic（本帧已跳过，上帧画面保留）: {}", pw.consecutive_panics, msg);
                        if pw.consecutive_panics >= 30 {
                            eprintln!("[render-panic] 连续 30 次 panic——停止本窗口渲染（避免 panic 风暴）");
                            pw.render_disabled = true;
                        }
                        // force_redraw 保持 true——下帧继续尝试（若未停更）
                    }
                }
                // IME 光标区域更新（输入法候选框跟随光标位置）
                if let Some(ref sw) = pw.skia_window {
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(pidx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                if let Ok(borrow) = nodes[pidx].cached_paragraph.try_borrow() {
                                    if let Some(p) = borrow.as_ref() {
                                        let abs = node_abs_position(nodes, r, fid);
                                        // 内容区偏移（渲染侧 content_x = 节点原点 +
                                        // padding——IME 区域须与其对称，否则预选
                                        // 窗口整体偏 padding 偏移）
                                        let (pad_s, pad_t, pad_e, _) = nodes[pidx].modifier.get_padding_sides();
                                        let pad_x = if nodes[pidx].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                                        // 空文本：无 glyph 可定位——IME 区域放内容
                                        // 起点（行高近似；与渲染端空文本光标一致）
                                        let empty = nodes[pidx].modifier.content_len() == 0;
                                        // 光标索引是编辑偏移——经映射转显示偏移
                                        let caret_idx = nodes[pidx].modifier.elements().iter().find_map(|el| {
                                            if let crate::modifier::ModifierElement::TextFieldVisual { offset_mapping, .. } = el {
                                                Some(offset_mapping.as_ref().map(|m| m.original_to_transformed(nodes[pidx].cursor_index.get())).unwrap_or_else(|| nodes[pidx].cursor_index.get()))
                                            } else { None }
                                        }).unwrap_or_else(|| nodes[pidx].cursor_index.get());
                                        let (cx, cy, ch) = if empty {
                                            (0.0, 0.0, 20.0)
                                        } else {
                                            let tl = crate::text::TextLayout::new(
                                                p,
                                                p.paragraph_byte_to_real_indices.len(),
                                            );
                                            match tl.get_cursor_position(caret_idx) {
                                                Some(v) => v,
                                                None => (0.0, 0.0, 20.0),
                                            }
                                        };
                                        let align = nodes[pidx].modifier.align().unwrap_or(crate::ui::TextAlign::Left);
                                        let node_w = nodes[pidx].measured_size.width;
                                        let intrinsic_w = p.max_intrinsic_width();
                                        // ⚠ 空文本：渲染端光标画在内容起点（左对齐，
                                        // 不随 align 偏移）——IME 区域须一致（否则
                                        // Center/Right 对齐时空文本输入候选框在容器
                                        // 中央而非光标处）
                                        let x = if empty {
                                            (abs.0 + pad_x) as f64
                                        } else {
                                            let x_off = match align {
                                                crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => abs.0,
                                                crate::ui::TextAlign::Center => abs.0 + (node_w - intrinsic_w).max(0.0) / 2.0,
                                                crate::ui::TextAlign::Right => abs.0 + (node_w - intrinsic_w).max(0.0),
                                            };
                                            (x_off + pad_x + cx) as f64
                                        };
                                        let y = (abs.1 + pad_t + cy) as f64;
                                        let _ = sw.request_ime_update(
                                            winit::window::ImeRequest::Update(
                                                winit::window::ImeRequestData::default()
                                                    .with_cursor_area(
                                                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(x, y)),
                                                        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(2.0, ch as f64)),
                                                    )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                } // end 帧率限制 else（渲染 + IME 同步）
                // 检查 compose 后是否有待关闭窗口
                if crate::ui::window::Window::has_pending_close() {
                    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
                }
                // DevTools 事件消费（仅父窗口）——new_events 也兜底调用（见下）
                if is_parent {
                    self.consume_debug_events(window_id);
                }
            }
            _ => {}
        }
    }
}

impl AppState {
    /// 消费 DevTools 注入事件（点击/按键/滚动/拖拽等——UI 测试 + WS 调试）。
    /// 在 window_event（RedrawRequested）与 new_events（兜底）两处调用：
    /// 多窗口下主窗口可能在后台——RedrawRequested 不来时事件卡队列，
    /// new_events 每轮事件批次必然执行——保证注入事件不丢失。
    fn consume_debug_events(&mut self, window_id: WindowId) {
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        let mut handled = false;
        for evt in debug::take_queued_events() {
            match evt {
                debug::DebugEvent::Click { x, y } => {
                    // 只读阶段：hit_test + click 检测（arena 借用在块尾结束）
                    let (fid, sk, path_len, click_handled) = {
                        let arena = pw.composer.arena_nodes();
                        let root_idx = pw.composer.layout_root_idx();
                        if let Some(r) = root_idx {
                            let path = hit_test(arena, r, x, y);
                            let mut click_handled = false;
                            // debug 点击聚焦：path 中最深的 focusable 节点
                            // （text-field-v2 容器化后焦点/交互在容器——空字段
                            // 输入子节点 0 宽点不中；旧逻辑只看 ime_callback
                            // 已失效）；兼容声明 IME 的节点（旧 leaf 语义）
                            let (fid, sk) = path.iter().rev().find_map(|&i| {
                                let has_focusable = arena[i].modifier.elements().iter()
                                    .any(|el| matches!(el, crate::modifier::ModifierElement::Focusable { .. }));
                                let wants_ime = arena[i].ime_callback.borrow().is_some();
                                (has_focusable || wants_ime).then_some((arena[i].id, arena[i].slot_key))
                            }).unwrap_or((0, 0));
                            let path_len = path.len();
                            // Click 检测（消耗 path 前做）
                            for &i in path.iter().rev() {
                                if click_handled { break; }
                                if let Some(on_click) = arena[i].modifier.on_click() {
                                    on_click();
                                    handled = true;
                                    click_handled = true;
                                }
                            }
                            (fid, sk, path_len, click_handled)
                        } else { (0, 0, 0, false) }
                    };
                    if fid != 0 {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            crate::layout::node::clear_focus(nodes, r);
                            crate::layout::node::set_focus_by_id(nodes, r, fid);
                        }
                        pw.focused_id = Some(fid);
                        pw.focused_slot_key = Some(sk);
                        if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(true); }
                    }
                    debug_log!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, path_len, pw.scale_factor);
                    debug_log!("[debug-click] handled={} pos=({:.0},{:.0})", click_handled, x, y);
                }
                debug::DebugEvent::Key { key } => {
                    if key == "Tab" {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            focus_next(nodes, r);
                            pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                            pw.focused_slot_key = pw.focused_id.and_then(|id| crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key));
                            pw.apply_ime_for_focus(pw.focused_id);
                            handled = true;
                        }
                    }
                }
                debug::DebugEvent::FocusNext => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes_mut();
                        focus_next(nodes, r);
                        pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                        pw.focused_slot_key = pw.focused_id.and_then(|id| crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key));
                        pw.apply_ime_for_focus(pw.focused_id);
                        handled = true;
                    }
                }
                debug::DebugEvent::RequestFocus { id } => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes_mut();
                        if crate::layout::node::focus_by_id(nodes, r, id) {
                            pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                            pw.focused_slot_key = pw.focused_id.and_then(|fid| crate::layout::node::find_node_by_id(nodes, r, fid).map(|idx| nodes[idx].slot_key));
                            pw.apply_ime_for_focus(pw.focused_id);
                            handled = true;
                        }
                    }
                }
                debug::DebugEvent::PointerDown { x, y } => {
                    // 模拟指针按下：与真实 PointerButton Down 共用核心
                    // （with_focus=false——调试路径不做光标/聚焦）
                    handle_pointer_down(pw, (x, y), pw.last_pointer_kind.clone(), &self.modifiers, false);
                    handled = true;
                }
                debug::DebugEvent::PointerMove { x, y } => {
                    // 模拟拖动选择：与真实 PointerMoved 共用核心（含 x_off 对齐偏移）
                    handle_pointer_move(pw, (x, y), pw.last_pointer_kind.clone(), &self.modifiers);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                debug::DebugEvent::PointerUp { x, y } => {
                    // 模拟释放：先走真实 Up 的 click 检测（验证真实链路）
                    // 与真实路径（PointerButton Up 分支）一致：on_click 内
                    // State set 后必须 request_redraw——否则依赖 wake_up 异步
                    // 链（偶发丢失 → 用户看到 count 不刷新）
                    if detect_click(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 点击执行（与真实路径一致）
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 手势 up 判定（与真实路径一致）
                    if gesture_up(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 释放按下交互（与真实路径一致）
                    release_pressed_interaction(pw);
                    // 通知选区变化 + 清理
                    if let Some(slot) = pw.pointer_down_slot {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                                    if let Some(reg) = nodes[idx].registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.pointer_down_slot = None;
                    pw.pointer_down_state = None;
                    handled = true;
                }
                debug::DebugEvent::Scroll { dy, .. } => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        apply_scroll_delta(pw.composer.arena_nodes_mut(), r, dy, crate::unit::Density::from_density(pw.scale_factor as f32));
                        handled = true;
                    }
                }
                debug::DebugEvent::Resize { w, h } => { pw.width = w; pw.height = h; handled = true; }
                _ => {}
            }
        }
        if handled { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
        if debug::has_pending() { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
    }
}

impl AppState {
    /// 消费 `app::open_window` 排队的窗口请求 + 首次初始化
    fn process_pending_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        // 首次：运行 init 回调收集主窗口创建请求
        if self.windows.is_empty() {
            if let Some(init) = self.init.take() {
                let mut composer = Composer::new();
                composer.compose(|ctx| init(ctx));
                // 临时 composer：compose 内部已 take_deps（注册到其 slot_deps）——
                // 此处再 take 是防御性空操作（缓冲已空），确保 DEP_MODE 复位
                crate::core::state::take_deps();
                // 临时 composer 被 drop，其 on_remove 可能设置 PENDING_REMOVE_ID
                // 清除副作用，防止主窗口被错误关闭
                crate::ui::window::reset_lifecycle_flags();
            }
        }

        for item in take_pending_windows() {
            self.pending_content.push(item);
        }

        // 消费 pending：创建窗口
        while self.pending_content.len() > 0 {
            // drain-like: pop from front
            let pending = self.pending_content.remove(0);
            self.open_window(event_loop, pending);
        }
    }

    fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, pending: PendingWindow) {
        let mut a = winit::window::WindowAttributes::default();
        a.title = if pending.title.is_empty() { "Winia".into() } else { pending.title.clone() };
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(pending.width as f64, pending.height as f64)));
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        let window_id = w.id();
        let sf = w.scale_factor();
        // 帧间隔对齐屏幕刷新率（refresh_rate_millihertz：60000 = 60Hz）——须在 w 移入前获取
        let mhz_opt = w.current_monitor()
            .and_then(|m| m.current_video_mode())
            .and_then(|v| v.refresh_rate_millihertz())
            .map(|m| m.get());
        let frame_interval = mhz_opt
            .map(|mhz| std::time::Duration::from_nanos(1_000_000_000_000 / mhz as u64))
            .unwrap_or(std::time::Duration::from_millis(16));
        let skia_window = VulkanSkiaWindow::new(event_loop, w);
        let content = pending.content.unwrap_or_else(|| Box::new(|_| {}));
        let theme = pending.theme.unwrap_or_else(|| crate::ui::theme::ThemeColors::default_light());
        let mut pw = PerWindow::new(content, pending.width, pending.height, theme);
        pw.on_close = pending.on_close;
        pw.created_id = pending.created_id;
        pw.scale_factor = sf;
        pw.skia_window = Some(skia_window);
        pw.frame_interval = frame_interval;
        // 首次 compose+layout+draw 也提供 Density（Px 单位首帧即正确）
        let density = crate::unit::Density::from_density(sf as f32);
        crate::unit::with_density(density, || {
        pw.composer.compose(|ctx| (pw.content)(ctx));
        pw.composer.layout(Constraints::new(0.0, pending.width, 0.0, pending.height));
        let bg = pw.theme.background;
        if let Some(root_idx) = pw.composer.layout_root_idx() {
            let nodes = pw.composer.arena_nodes();
            if let Some(ref mut sw) = pw.skia_window {
                let sf2 = sf as f32;
                sw.draw(|surface| {
                    let canvas = surface.canvas(); canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save(); canvas.scale((sf2, sf2)); render::render(nodes, root_idx, canvas); canvas.restore();
                });
                sw.set_visible(true);
            }
        }
        });
        self.windows.insert(window_id, pw);
        if self.parent_window_id.is_none() {
            self.parent_window_id = Some(window_id);
        }
    }
}

// ── 公共 API ──

// ── 公共 API ──

static GLOBAL_PENDING: std::sync::Mutex<Vec<PendingWindow>> = std::sync::Mutex::new(Vec::new());
static APP_PROXY: Mutex<Option<winit::event_loop::EventLoopProxy>> = Mutex::new(None);

pub fn open_window_with_title(width: f32, height: f32, title: String, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, created_id: Option<u64>, theme: Option<crate::ui::theme::ThemeColors>) {
    GLOBAL_PENDING.lock().unwrap().push(PendingWindow { width, height, title, content, on_close, created_id, theme });
    wake_impl();
}

pub(crate) fn wake_impl() {
    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() {
        let _ = proxy.wake_up();
        return;
    }
    debug::wake();
}

pub(crate) fn take_pending_windows() -> Vec<PendingWindow> {
    std::mem::take(&mut *GLOBAL_PENDING.lock().unwrap())
}

fn apply_scroll_delta(nodes: &mut [LayoutNode], idx: usize, dy: f32, density: crate::unit::Density) -> bool {
    {
        let node = &nodes[idx];
        if let Some(state) = node.modifier.vertical_scroll_state() {
            let current = state.get();
            // 滚动极限 = 内容总高度 - 可视区域高度
            // viewport 高度优先用 scroll_viewport_height（fill_max_height 场景），
            // 降级到 fixed_size()（固定高度场景），再降级到 0（无限制）。
            let visible_h = if node.scroll_viewport_height > 0.0 {
                node.scroll_viewport_height
            } else {
                node.modifier.fixed_size()
                    .and_then(|(_, h)| {
                        use crate::modifier::Dimension;
                        match h {
                            Dimension::Fixed(h) | Dimension::Dp(crate::unit::Dp(h)) => Some(h),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            let max_offset = (node.measured_size.height - visible_h).max(0.0);
            let new = (current - dy).clamp(0.0, max_offset);
            state.set(new);
            return true;
        }
    }
    // 子节点（clone 索引后递归，避免与 nodes 的可变借用冲突）
    let children: Vec<usize> = nodes[idx].children.clone();
    for c in children {
        if apply_scroll_delta(nodes, c, dy, density) { return true; }
    }
    false
}

/// 手势动作 → 节点回调（坐标转组件本地——对标 Compose onTap 的本地 offset）。
/// `scene_pos` 是事件位置；tap/drag 系列动作自带 `down_pos`——坐标用动作
/// 携带的位置（slop 内移动后 up 位置与按下位置不同，用 up 会偏）。
/// 返回是否消费（有回调执行）。
fn fire_gesture_action(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    slot_key: u64,
    action: crate::input::gesture::GestureAction,
) -> bool {
    // ⚠ 用 slot_key 解析节点（跨重组稳定）——node_id 在重组后会变
    let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, slot_key) else {
        return false;
    };
    let Some(idx) = crate::layout::node::find_node_by_id(nodes, root, nid) else {
        return false;
    };
    let (ax, ay) = node_abs_position(nodes, root, nid);
    use crate::input::gesture::GestureAction as G;
    use crate::modifier::ModifierElement as E;
    let mut fired = false;
    for el in nodes[idx].modifier.elements() {
        match (el, action) {
            (E::TapOnPress { cb }, G::Press(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::TapOnTap { cb }, G::Tap(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::TapOnDoubleTap { cb }, G::DoubleTap(p)) => {
                (cb)((p.0 - ax, p.1 - ay));
                fired = true;
            }
            (E::TapOnLongPress { cb }, G::LongPress(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::DragOnStart { cb }, G::DragStart(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::DragOnMove { cb }, G::DragMove(p, delta)) => {
                (cb)((p.0 - ax, p.1 - ay), delta);
                fired = true;
            }
            (E::DragOnEnd { cb }, G::DragEnd) => {
                (cb)(); fired = true;
            }
            (E::DragOnCancel { cb }, G::DragCancel) => { (cb)(); fired = true; }
            _ => {}
        }
    }
    fired
}

/// 指针按下手势入口：hit test 找最内层手势节点 → 创建 tracker（capture 语义——
/// 后续 move/up 由 gesture_node 路由，指针移出组件仍接收）→ on_press 立即触发。
fn gesture_down(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // 先解析手势目标（借用结束即释放——后面要可变借用 pw 处理 pending tap）
    let hit = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
        let Some(gid) = path.iter().rev().find(|&&i| nodes[i].modifier.has_gesture()).copied() else {
            return;
        };
        let n = &nodes[gid];
        (n.id, n.slot_key, n.modifier.has_drag_gesture())
    };
    let (node_id, slot, has_drag) = hit;

    // 处理待补发的 tap（Compose 双击语义，规则见 pending_tap_on_down）：
    // - 超时 → 补发；同节点窗口内第二次按下 → 取消（等 up 判定双击）；
    //   其他节点按下 → 保留到 deadline 补发（不提前也不取消）
    let now = std::time::Instant::now();
    let mut kept = Vec::new();
    for t in std::mem::take(&mut pw.pending_taps) {
        use crate::input::gesture::PendingTapAction as A;
        match crate::input::gesture::pending_tap_on_down(&t, now, node_id) {
            A::Fire => pw.fire_pending_tap(t),
            A::Cancel => {}
            A::Keep => kept.push(t),
        }
    }
    pw.pending_taps = kept;

    // 双击上下文按节点隔离（Compose per-pointerInput 语义）——不同节点不共享
    let ctx = pw.gesture_tap_ctx.take()
        .filter(|(n, _, _)| *n == node_id)
        .map(|(_, t, p)| (t, p));
    pw.gesture = Some(crate::input::gesture::GestureTracker::new(node_id, scene_pos, has_drag, ctx));
    pw.gesture_node = Some(node_id);
    pw.gesture_slot = Some(slot);
    // on_press 立即触发（本地坐标）
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    fire_gesture_action(nodes, r, slot, crate::input::gesture::GestureAction::Press(scene_pos));
}

/// 指针移动手势入口：tracker 存在即路由（capture——不依赖 hit test）。
fn gesture_move(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.gesture_slot else { return false; };
    let action = {
        let Some(t) = pw.gesture.as_mut() else { return false; };
        t.on_move(scene_pos)
    };
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    fire_gesture_action(nodes, r, slot, action)
}

/// 指针释放手势入口：up 判定（tap/double-tap/long-press/drag-end）→ 销毁 tracker。
fn gesture_up(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.gesture_slot else { return false; };
    let Some(gid) = pw.gesture_node else { return false; };
    let action = {
        let Some(mut t) = pw.gesture.take() else { return false; };
        // ⚠ 必须先 on_up（Tap 分支记录 last_tap）再取 tap_context——
        // 顺序颠倒则双击上下文恒 None（ctx 在 up 判定前读取）
        let action = t.on_up();
        pw.gesture_tap_ctx = t.tap_context().map(|(t, p)| (gid, t, p));
        action
    };
    pw.gesture_node = None;
    pw.gesture_slot = None;
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    // ⚠ Compose detectTapGestures 语义：节点注册 onDoubleTap 时，onTap 延迟
    // 到双击窗口结束——窗口内第二次 up 命中双击 → 只发 DoubleTap（第一次 tap
    // 已在第二次 down 时取消）；超时/按下其他节点 → 补发 Tap（fire_pending_tap）
    if let crate::input::gesture::GestureAction::Tap(pos) = action {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return false; };
        let has_double_tap = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)
            .and_then(|nid| crate::layout::node::find_node_by_id(nodes, r, nid))
            .map(|idx| nodes[idx].modifier.has_double_tap())
            .unwrap_or(false);
        if has_double_tap {
            pw.pending_taps.push(crate::input::gesture::PendingTap::new(slot, gid, pos));
            return false;
        }
        return fire_gesture_action(nodes, r, slot, action);
    }
    if matches!(action, crate::input::gesture::GestureAction::DoubleTap(_)) {
        // 双击命中：第一次 tap 的 pending 应已在第二次 down 时取消——防御性清理同节点残留
        pw.pending_taps.retain(|t| t.node_id != gid);
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    fire_gesture_action(nodes, r, slot, action)
}

// ── 顶层弹出层（Popup/Dialog/DropdownMenu） ──

impl OverlayWindow {
    fn new(desc: crate::ui::overlay::OverlayDesc) -> Self {
        Self {
            id: desc.id,
            composer: Composer::new(),
            anchor_slot: desc.anchor_slot,
            position: desc.position,
            offset: desc.offset,
            modal: desc.modal,
            dismiss_on_outside: desc.dismiss_on_outside,
            on_dismiss: desc.on_dismiss,
            content: desc.content,
            screen_pos: (0.0, 0.0),
        }
    }

    fn update(&mut self, desc: crate::ui::overlay::OverlayDesc) {
        self.anchor_slot = desc.anchor_slot;
        self.position = desc.position;
        self.offset = desc.offset;
        self.modal = desc.modal;
        self.dismiss_on_outside = desc.dismiss_on_outside;
        self.on_dismiss = desc.on_dismiss;
        self.content = desc.content;
    }
}

/// 主树 compose 后同步 overlay：按 id 匹配（保留 State）——新增/更新/删除。
/// 删除依据 = 组合期显式记录的 active=false（Popup/Dialog/DropdownMenu build
/// 总执行时 record_overlay_active）——主动关闭（visible/expanded=false）→ 删；
/// 注册方 Skip（build 未执行 → 本帧无记录）→ 保留（闪烁根因：旧 retain 把
/// Skip 帧误判为主动关闭）。
fn sync_overlays(pw: &mut PerWindow, _recomposed: bool) {
    let descs = pw.composer.take_overlays();
    for desc in descs {
        if let Some(ov) = pw.overlays.iter_mut().find(|o| o.id == desc.id) {
            ov.update(desc);
        } else {
            pw.overlays.push(OverlayWindow::new(desc));
        }
    }
    // 组合期记录 active=false 的 overlay → 主动关闭 → 删除（先触发 on_dismiss
    // 再 retain——删除后 find 不到）
    let to_close: Vec<u64> = pw.composer.overlay_active.iter()
        .filter(|(_, active)| !**active)
        .map(|(&id, _)| id)
        .collect();
    if !to_close.is_empty() {
        for id in &to_close {
            if let Some(ov) = pw.overlays.iter_mut().find(|o| o.id == *id) {
                if let Some(cb) = ov.on_dismiss.take() {
                    (cb)();
                }
            }
        }
        pw.overlays.retain(|o| !to_close.contains(&o.id));
    }
}

/// overlay compose + layout（独立组合单元——约束为窗口尺寸），并计算屏幕定位
fn layout_overlays(pw: &mut PerWindow) {
    for ov in &mut pw.overlays {
        ov.composer.recompose(|ctx| (ov.content)(ctx));
        ov.composer.layout(crate::layout::Constraints::new(0.0, pw.width, 0.0, pw.height));
    }
    // 定位（需主树锚点位置——在 draw 前算）
    let nodes = pw.composer.arena_nodes();
    let root = pw.composer.layout_root_idx();
    let (w, h) = (pw.width, pw.height);
    for ov in &mut pw.overlays {
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // 锚点位置（主树）
        let anchor = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.map(|r| {
            node_abs_position(nodes, r, nid)
        }));
        let anchor_size = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.and_then(|r| {
            crate::layout::node::find_node_by_id(nodes, r, nid).map(|i| nodes[i].measured_size)
        }));
        let anchored = anchor.is_some() && anchor_size.is_some();
        let (ax, ay, aw, ah) = match (anchor, anchor_size) {
            (Some((x, y)), Some(s)) => (x, y, s.width, s.height),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        use crate::ui::overlay::PopupPosition as P;
        let pos = match ov.position {
            // 窗口对齐（无锚点）
            P::Center => ((w - size.0) / 2.0, (h - size.1) / 2.0),
            P::TopLeft => (0.0, 0.0),
            P::TopCenter => ((w - size.0) / 2.0, 0.0),
            P::TopRight => (w - size.0, 0.0),
            P::BottomLeft => (0.0, h - size.1),
            P::BottomCenter => ((w - size.0) / 2.0, h - size.1),
            P::BottomRight => (w - size.0, h - size.1),
        };
        // 有锚点（且在主树中找到）时：按位置相对锚点（Bottom* = 锚点下方，
        // Top* = 锚点上方）；锚点缺失/未物化（scope）回退窗口对齐——避免 (0,0)
        let pos = if anchored {
            match ov.position {
                P::BottomLeft => (ax, ay + ah),
                P::BottomCenter => (ax + (aw - size.0) / 2.0, ay + ah),
                P::BottomRight => (ax + aw - size.0, ay + ah),
                P::TopLeft => (ax, ay - size.1),
                P::TopCenter => (ax + (aw - size.0) / 2.0, ay - size.1),
                P::TopRight => (ax + aw - size.0, ay - size.1),
                P::Center => ((w - size.0) / 2.0, (h - size.1) / 2.0),
            }
        } else { pos };
        ov.screen_pos = (pos.0 + ov.offset.0, pos.1 + ov.offset.1);
    }
}

/// overlay 命中测试——返回 (overlay 索引, 本地坐标)——从最上层（最后一个）往下
fn hit_overlay(pw: &PerWindow, scene_pos: (f32, f32)) -> Option<(usize, (f32, f32))> {
    for i in (0..pw.overlays.len()).rev() {
        let ov = &pw.overlays[i];
        let local = (scene_pos.0 - ov.screen_pos.0, scene_pos.1 - ov.screen_pos.1);
        if let Some(r) = ov.composer.layout_root_idx() {
            let nodes = ov.composer.arena_nodes();
            if !hit_test(nodes, r, local.0, local.1).is_empty() {
                return Some((i, local));
            }
        }
    }
    None
}

/// overlay 渲染（主树之后——上层；模态先画遮罩）
fn render_overlays(overlays: &[OverlayWindow], canvas: &skia_safe::Canvas, scale: f32, window: (f32, f32)) {
    for ov in overlays {
        // 模态遮罩
        if ov.modal {
            let mut mask = skia_safe::Paint::default();
            mask.set_color(skia_safe::Color::from_argb(110, 0, 0, 0));
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, window.0 * scale, window.1 * scale),
                &mask,
            );
        }
        let Some(r) = ov.composer.layout_root_idx() else { continue; };
        let nodes = ov.composer.arena_nodes();
        canvas.save();
        canvas.translate((ov.screen_pos.0 * scale, ov.screen_pos.1 * scale));
        // overlay 内容与主树一致按 scale 绘制（坐标均为逻辑单位）——
        // 缺省会导致内容以 1x 绘制：可见位置/大小与命中测试（逻辑坐标）错位
        canvas.scale((scale, scale));
        render::render(nodes, r, canvas);
        canvas.restore();
    }
}

/// overlay 点击执行（up 时——v1 仅 clickable）
fn exec_overlay_click(pw: &mut PerWindow) -> bool {
    let Some((idx, local, _nid)) = pw.overlay_click.take() else { return false; };
    let Some(ov) = pw.overlays.get(idx) else { return false; };
    let Some(r) = ov.composer.layout_root_idx() else { return false; };
    let nodes = ov.composer.arena_nodes();
    let path = hit_test(nodes, r, local.0, local.1);
    // 沿路径找 clickable（最内层优先）
    let r = fire_click_along_path(nodes, &path);
    r
}

/// 沿命中路径从内到外触发第一个 on_click——返回是否触发。
/// 主树 click 与 overlay 点击共用（消除重复）
fn fire_click_along_path(nodes: &[crate::layout::node::LayoutNode], path: &[usize]) -> bool {
    for &i in path.iter().rev() {
        if let Some(cb) = nodes[i].modifier.on_click() {
            (cb)();
            return true;
        }
    }
    false
}

/// 指针按下：先测 overlay（最上层）——命中 → 记录点击目标；外部 → dismiss
fn overlay_down(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    if pw.overlays.is_empty() {
        return false;
    }
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        // 命中 overlay 内容——记录点击目标（v1：仅 clickable——up 时执行）
        let ov = &pw.overlays[i];
        let nid = ov.composer.layout_root_idx().and_then(|r| {
            let nodes = ov.composer.arena_nodes();
            hit_test(nodes, r, local.0, local.1).last().map(|&idx| nodes[idx].id)
        });
        pw.overlay_click = Some((i, local, nid.unwrap_or(0)));
        return true; // 事件消费——不进主树
    }
    // 外部点击：模态或可关闭 → dismiss（消费事件）
    // ⚠ 同步移除（不等 recompose）——否则残留 overlay 会吞掉关闭后
    // 紧接着的点击（用户"点两次才打开"）且多渲染一帧（视觉闪烁）
    for i in (0..pw.overlays.len()).rev() {
        if pw.overlays[i].modal || pw.overlays[i].dismiss_on_outside {
            let cb = pw.overlays[i].on_dismiss.take();
            pw.overlays.remove(i);
            if let Some(cb) = cb {
                (cb)();
            }
            return true;
        }
    }
    false
}

/// 按下：命中路径最内层 clickable 绑定的交互源 → 发射 Press
/// （对标 Compose clickable 的 PressInteraction.Press）
fn press_interaction_down(pw: &mut PerWindow, path: &[usize], scene_pos: (f32, f32)) {
    let (slot, idx, src) = {
        let nodes = pw.composer.arena_nodes();
        let Some(&idx) = path.iter().rev().find(|&&i| {
            nodes[i].modifier.clickable_interaction().is_some()
        }) else {
            return;
        };
        let src = match nodes[idx].modifier.clickable_interaction() {
            Some(s) => s.clone(),
            None => return,
        };
        (nodes[idx].slot_key, idx, src)
    };
    // 场景坐标 → 节点本地坐标（对标 Compose PressInteraction.Press.pressPosition）。
    // 波纹中心必须存本地坐标：绘制时加回布局原点，滚动/变换后波纹跟随节点；
    // 若存场景坐标，按下后滚动/动画会脱离按钮。
    let local = {
        let nodes = pw.composer.arena_nodes();
        let clickable_local =
            crate::layout::node::scene_to_node_local(nodes, path, idx, scene_pos.0, scene_pos.1);
        // Ripple 与 clickable 不同节点时（如 Switch 的 Handle 容器）：
        // 按压坐标要换算到 Ripple 节点本地空间，否则波纹锚点偏移。
        // 本地坐标差 = 两节点绝对位置差（无滚动/变换时等价于 scene 换算）。
        let ripple_adjust = pw.composer.layout_root_idx().and_then(|r| {
            // 优先在命中路径内定位（同一 source 复用时避免锚点错位）；Switch 的
            // ripple 挂在 Handle 子节点上，点击轨道空白处时不在 path 内——回退
            // 全树搜索。已知限制：同一 source 复用于多个组件时全树搜索可能取到
            // 错误节点（当前 Switch 均为每组件独立 source，不受影响）。
            let ridx = path
                .iter()
                .copied()
                .find(|&i| {
                    nodes[i]
                        .modifier
                        .ripple_interaction()
                        .map(|s| s == &src)
                        .unwrap_or(false)
                })
                .or_else(|| {
                    nodes.iter().position(|n| {
                        n.modifier
                            .ripple_interaction()
                            .map(|s| s == &src)
                            .unwrap_or(false)
                    })
                })?;
            if ridx == idx {
                return None;
            }
            let (ax, ay) = node_abs_position(nodes, r, nodes[idx].id);
            let (rx, ry) = node_abs_position(nodes, r, nodes[ridx].id);
            Some((ax - rx, ay - ry))
        });
        match ripple_adjust {
            Some((dx, dy)) => (clickable_local.0 + dx, clickable_local.1 + dy),
            None => clickable_local,
        }
    };
    src.emit_press_at(local);
    pw.pressed_interaction = Some((slot, src));
}

/// 释放/取消按下交互（Up 或越界 slop）——对标 PressInteraction.Release/Cancel
fn release_pressed_interaction(pw: &mut PerWindow) {
    if let Some((_, src)) = pw.pressed_interaction.take() {
        src.emit_release();
    }
}

/// 悬停更新：最内层 hoverable 节点进入/离开 → 发射 Hover Enter/Exit
/// （对标 Compose hoverable：Enter/Exit 成对；节点移除时自动补 Exit）
fn update_hover(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    let hit = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
        path.iter().rev()
            .find(|&&i| nodes[i].modifier.has_hoverable())
            .copied()
            .map(|i| (nodes[i].slot_key, nodes[i].modifier.hoverable_interaction().map(|s| s.clone())))
    };
    let Some((slot, Some(src))) = hit else {
        // 不在任何 hoverable 上：退出旧的
        if let Some(old) = pw.hovered_slot.take() {
            exit_hover_at(pw, old);
        }
        return;
    };
    if pw.hovered_slot == Some(slot) {
        return;
    }
    if let Some(old) = pw.hovered_slot.take() {
        exit_hover_at(pw, old);
    }
    src.emit_hover_enter();
    pw.hovered_slot = Some(slot);
}

/// 对指定 slot 的节点补发 Hover Exit（节点已移除则跳过——hover 状态自然清理）
fn exit_hover_at(pw: &mut PerWindow, slot: u64) {
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}

/// Compose 风格 click 检测：Down 记录（pointer_down_state）、Up 释放时触发 on_click。
/// 真实 PointerButton Up 与 debug 模拟（d/u）共用——消除平行实现并可模拟验证。
fn detect_click(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    const CLICK_SLOP: f32 = 18.0;
    const CLICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
    let Some(down) = pw.pointer_down_state.take() else {
        return false;
    };
    // Down 时所在节点的 slot_key（跨重组稳定——Down 与 Up 之间可能发生重组
    // （如聚焦触发），arena 节点重建 → node_id 变化 → 旧 id 匹配必然失败；
    // slot_key 按组合位置稳定，不受重组影响）
    let down_slot = pw.pointer_down_slot;
    let dx = scene_pos.0 - down.position.0;
    let dy = scene_pos.1 - down.position.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let in_time = down.time.elapsed() < CLICK_TIMEOUT;
    if dist <= CLICK_SLOP && in_time {
        let nodes = pw.composer.arena_nodes();
        if let Some(r) = pw.composer.layout_root_idx() {
            let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
            let hit = down_slot
                .map(|slot| path.iter().any(|&i| nodes[i].slot_key == slot))
                .unwrap_or_else(|| path.iter().any(|&i| nodes[i].id == down.node_id));
            if hit {
                // 只在相同节点触发 click（从内到外找第一个 on_click）
                return fire_click_along_path(nodes, &path);
            }
        }
    }
    false
}

/// 指针按下核心（真实 PointerButton 与 debug 模拟共用——防行为分叉）。
///
/// 统一：hit_test → 清除旧选区 → grapheme anchor 定位 → PtrDownState 记录 →
/// 光标设置/自动聚焦（仅真实路径 `with_focus`）→ on_pointer_event 分发。
/// 返回 true 表示命中节点并已处理。
fn handle_pointer_down(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::modifier::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
    with_focus: bool,
) -> bool {
    // 顶层弹出层优先：命中 overlay → 记录点击目标（事件不进主树）；
    // 外部点击 → dismiss（模态/可关闭）
    if overlay_down(pw, scene_pos) {
        return true;
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
    let Some(&innermost) = path.last() else { return false; };

    // 清除旧的选区（新点击开始）
    {
        let reg = nodes[innermost].registrar.borrow().as_ref().cloned()
            .unwrap_or_else(|| crate::ui::selection_container::active_registrar());
        reg.clear_selection();
    }
    // grapheme anchor 定位
    let anchor = if let Ok(borrow) = nodes[innermost].cached_paragraph.try_borrow() {
        borrow.as_ref().map(|para| {
            let (ax, ay) = node_abs_position(nodes, r, nodes[innermost].id);
            // 段落局部坐标须扣除节点 padding（渲染侧文本画在 content 区 =
            // 节点原点 + padding——不扣则点击位置整体偏 padding 偏移，
            // 光标跳错位置）
            let (pad_s, pad_t, pad_e, _) = nodes[innermost].modifier.get_padding_sides();
            let pad_x = if nodes[innermost].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
            let tl = crate::text::TextLayout::new(para, 0);
            let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - ax - pad_x, scene_pos.1 - ay - pad_t));
            // 定位结果是显示文本偏移（paragraph = 显示文本）——经 OffsetMapping
            // 转回编辑偏移（密码掩码/格式化输入）
            nodes[innermost].modifier.elements().iter().find_map(|el| {
                if let crate::modifier::ModifierElement::TextFieldVisual { offset_mapping, .. } = el {
                    Some(offset_mapping.as_ref().map(|m| m.transformed_to_original(hit)).unwrap_or(hit))
                } else { None }
            }).unwrap_or(hit)
        })
    } else {
        None
    };
    // reg 只用节点自己的 registrar（不 fallback active_registrar）——
    // 不可选节点（输出 Text 等未注册）的 node.registrar 为 None，
    // fallback 会取到全局残留（如 Container B 的）→ anchor_registrar
    // 错绑 B → 拖动到 B 时 same_reg=true → 混合偏移 → B 被选
    let own_reg = nodes[innermost].registrar.borrow().as_ref().cloned();
    let anchor_global = anchor.and_then(|a| {
        own_reg.as_ref()
            .and_then(|reg| reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off + a))
    });
    pw.pointer_down_state = Some(PtrDownState {
        node_id: nodes[innermost].id,
        position: scene_pos,
        time: std::time::Instant::now(),
        selection_anchor: anchor_global,
        anchor_registrar: own_reg, // None（不可选节点）→ 无 anchor 容器
    });
    pw.pointer_down_slot = Some(nodes[innermost].slot_key);

    if with_focus {
        // 设置 TextField 光标位置 + selection 更新回调（仅真实路径）。
        // ⚠ 点击聚焦由组件自己决定（TextField 内部 requestFocus）——
        // 框架层不自动聚焦（对标 Compose：clickable/focusable 点击不请求焦点，
        // 焦点由 Tab 导航或显式 requestFocus 获得）。
        if let Some(a) = anchor {
            nodes[innermost].cursor_index.set(a);
            if let Some(cb) = nodes[innermost].cursor_callback.borrow_mut().as_mut() {
                cb(a);
            }
        }
    }

    // 按下交互（clickable 绑定源——Compose Press 语义；置于 nodes 借用结束后）
    press_interaction_down(pw, &path, scene_pos);

    // 手势入口（on_press 立即触发；后续 move/up 由 gesture_node 路由）——
    // 置于 with_focus 块后（nodes 借用结束，避免与 pw mut 冲突）
    gesture_down(pw, scene_pos);

    // 分发 on_pointer_event（Down）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::modifier::PointerEvent {
        event_type: crate::modifier::PointerEventType::Down,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.last_pointer_kind = ptr_ev.kind.clone();
    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.pointer_down_slot);
    true
}

/// 指针移动核心（真实 PointerMoved 与 debug 模拟共用——防行为分叉）。
///
/// 统一：拖拽选区（18px slop + x_off 对齐偏移 + grapheme 定位 + compute_selection）
/// + on_pointer_event 分发。返回是否消费（选择更新或 handler 消费）。
fn handle_pointer_move(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::modifier::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
) -> bool {
    // 手势驱动（drag capture：tracker 存在即路由——指针移出组件仍接收）
    // ⚠ 必须在 nodes 借用之前（gesture_move 内部自取 nodes/pw mut）
    let mut handled = false;
    if pw.gesture_node.is_some() && gesture_move(pw, scene_pos) {
        handled = true;
    }
    // 悬停更新（自身 hit test——不依赖下方 nodes 借用）
    update_hover(pw, scene_pos);
    // 越界 slop：按下交互取消（Compose：press 超过 touch slop → Cancel）
    if pw.pointer_down_state.is_some() {
        let down = pw.pointer_down_state.as_ref().unwrap();
        let dx = scene_pos.0 - down.position.0;
        let dy = scene_pos.1 - down.position.1;
        if (dx * dx + dy * dy).sqrt() > 18.0 {
            release_pressed_interaction(pw);
        }
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);

    // 拖拽选中文本
    if pw.pointer_down_state.is_some() {
        if let Some(&innermost) = path.last() {
            let down = pw.pointer_down_state.as_ref().unwrap();
            let dx = scene_pos.0 - down.position.0;
            let dy = scene_pos.1 - down.position.1;
            const CLICK_SLOP: f32 = 18.0;
            // 文本选择：按下在可选中文本（anchor 有值）→ **任意移动即更新
            // 选区**（单字母 ~7px < CLICK_SLOP——slop 门槛导致无法选中单
            // 字母/小范围文本；对齐 Compose：按下即进入拖选）。其余场景
            // 保持 slop 判定（clickable/拖拽手势）
            let text_selection = down.selection_anchor.is_some();
            if text_selection || (dx * dx + dy * dy).sqrt() > CLICK_SLOP {
                let (abs_x, abs_y) = node_abs_position(nodes, r, nodes[innermost].id);
                if let Ok(borrow) = nodes[innermost].cached_paragraph.try_borrow() {
                    if let Some(para) = borrow.as_ref() {
                        // 对齐偏移（匹配渲染侧 x_off）
                        let node_w = nodes[innermost].measured_size.width;
                        let align = nodes[innermost].modifier.align().unwrap_or(crate::ui::TextAlign::Left);
                        let x_off = match align {
                            crate::ui::TextAlign::Center => abs_x + (node_w - para.max_intrinsic_width()).max(0.0) / 2.0,
                            crate::ui::TextAlign::Right => abs_x + (node_w - para.max_intrinsic_width()).max(0.0),
                            _ => abs_x,
                        };
                        // 段落局部坐标：扣节点 padding（与渲染侧 content 区一致——
                        // 否则拖动选区整体偏 padding 偏移）
                        let (pad_s, pad_t, pad_e, _) = nodes[innermost].modifier.get_padding_sides();
                        let pad_x = if nodes[innermost].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                        let tl = crate::text::TextLayout::new(para, 0);
                        {
                            let down = pw.pointer_down_state.as_ref().unwrap();
                            // 不可选节点（未注册到任何 SelectionContainer）→ 不更新选择
                            // （用 if let 包裹而非 else return——return 会跳过 dispatch_ptr_event/request_redraw）
                            if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
                                let current_index = tl.get_closest_grapheme_cluster_cluster_at(
                                    skia_safe::Point::new(scene_pos.0 - x_off - pad_x, scene_pos.1 - abs_y - pad_t));
                                // 显示偏移 → 编辑偏移（密码掩码/格式化输入）
                                let current_index = nodes[innermost].modifier.elements().iter().find_map(|el| {
                                    if let crate::modifier::ModifierElement::TextFieldVisual { offset_mapping, .. } = el {
                                        Some(offset_mapping.as_ref().map(|m| m.transformed_to_original(current_index)).unwrap_or(current_index))
                                    } else { None }
                                }).unwrap_or(current_index);
                                let cur_off = reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off);
                                if let Some((target, s, e)) = crate::ui::selection_container::compute_selection(
                                    down.anchor_registrar.as_ref(), down.selection_anchor,
                                    &reg, cur_off, current_index,
                                    scene_pos.1, down.position.1, abs_y,
                                ) {
                                    // 范围是编辑偏移（anchor/current 已转回）——
                                    // reg 空间 = 显示偏移，写入选区前转换
                                    let (ts, te) = nodes[innermost].modifier.elements().iter().find_map(|el| {
                                        if let crate::modifier::ModifierElement::TextFieldVisual { offset_mapping, .. } = el {
                                            Some(offset_mapping.as_ref().map(|m| {
                                                (m.original_to_transformed(s), m.original_to_transformed(e))
                                            }).unwrap_or((s, e)))
                                        } else { None }
                                    }).unwrap_or((s, e));
                                    target.set_selection(ts, te);
                                    handled = true;
                                }
                            } // end if let Some(reg)
                        }
                    }
                }
            }
        }
    }

    // 分发 on_pointer_event（悬停 Move 也可能更新 State）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::modifier::PointerEvent {
        event_type: crate::modifier::PointerEventType::Move,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.last_pointer_kind = ptr_ev.kind.clone();
    let consumed = dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.pointer_down_slot);
    handled || consumed
}

/// 分发指针事件到 hit_test 路径（pre: outer→inner, bubble: inner→outer）
/// 计算节点在布局树中的绝对位置（从根累加 position）。
///
/// ⚠ 与 `hit_test`/`scene_to_node_local` 同空间：累加时**减去祖先 scroll 偏移**
/// （渲染时滚动容器 `canvas.translate(-offset)` → 命中测试的 scene 坐标就是
/// 减过滚动量的"滚动画布坐标"）。此前纯累加 layout position，滚动容器内
/// 节点绝对 y 被滚动量污染 → 段落局部坐标（scene - abs）偏负 → skia 最近
/// glyph 恒为第一行——多行 TextField 点击/拖拽只能定位到第一行。
fn node_abs_position(nodes: &[LayoutNode], root: usize, id: u64) -> (f32, f32) {
    fn walk(nodes: &[LayoutNode], idx: usize, target: u64, abs_x: f32, abs_y: f32) -> Option<(f32, f32)> {
        let node = &nodes[idx];
        let nx = abs_x + node.position.x;
        let ny = abs_y + node.position.y;
        if node.id == target { return Some((nx, ny)); }
        // 命中测试同路径：子节点坐标 = 本节点坐标 - 本节点 scroll 偏移
        let (dx, dy) = crate::layout::node::scroll_offset_for_node(node);
        for &c in &node.children {
            if let Some(r) = walk(nodes, c, target, nx - dx, ny - dy) { return Some(r); }
        }
        None
    }
    walk(nodes, root, id, 0.0, 0.0).unwrap_or((0.0, 0.0))
}

fn dispatch_ptr_event(
    nodes: &[LayoutNode],
    root: usize,
    path: &[usize],
    event: &crate::modifier::PointerEvent,
    scene_pos: (f32, f32),
    captured_id: Option<u64>,
) -> bool {
    // 如果指针被一个节点捕获（Down 后未释放），用 root 查找节点并构建祖先链
    let captured_path: Vec<usize> = if let Some(cid) = captured_id {
        let mut ancestors: Vec<usize> = Vec::new();
        if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, cid).and_then(|nid| crate::layout::node::find_node_by_id(nodes, root, nid)) {
            ancestors.push(nid);
            let mut pid = nodes[nid].parent_id;
            while let Some(id) = pid {
                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, root, id) {
                    ancestors.push(anc);
                    pid = nodes[anc].parent_id;
                } else { break; }
            }
            ancestors.reverse(); // root → ... → captured
            ancestors
        } else {
            path.to_vec()
        }
    } else { path.to_vec() };
    let use_path = &captured_path;
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|&i| {
        abs_x += nodes[i].position.x;
        abs_y += nodes[i].position.y;
        (abs_x, abs_y)
    }).collect();

    // on_pre_ptr: outer → inner
    for (i, &ni) in use_path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
    }

    // on_ptr: inner → outer
    for (i, &ni) in use_path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
    }
    false
}

/// request 节流判断（纯函数——边界单测）：
/// 距上次 request >= 帧间隔才允许请求（WM_PAINT 生成频率受控为刷新率）。
/// 独立于渲染节流（last_render_time）——避免 WM_PAINT 晚于 request（ε>0）导致
/// 定时器唤醒时 now-last_render = I-ε < I 恒拦截 → 渲染频率减半（2I 间隔）。
pub(crate) fn should_request_redraw(last_request: std::time::Instant, now: std::time::Instant, interval: std::time::Duration) -> bool {
    now.duration_since(last_request) >= interval
}

#[cfg(test)]
mod frame_throttle_tests {
    use super::should_request_redraw;
    use std::time::{Duration, Instant};

    #[test]
    fn request_at_exact_interval_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval, interval));
    }

    #[test]
    fn request_below_interval_blocked() {
        // I-ε（ε=1ns）：减半回归的边界——晚于 request 的渲染不应把下次请求推迟到 2I
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(!should_request_redraw(t0, t0 + interval - Duration::from_nanos(1), interval));
    }

    #[test]
    fn request_after_two_intervals_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval * 2, interval));
    }

    #[test]
    fn request_at_zero_blocked() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(!should_request_redraw(t0, t0, interval));
    }

    #[test]
    fn request_over_interval_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval + Duration::from_millis(1), interval));
    }

    /// 滚动 delta 应用到 Column（scroll 节点查找 + offset 更新 + clamp）
    #[test]
    fn scroll_delta_applies_to_column() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = crate::core::composer::Composer::new();
        let scroll = crate::modifier::ScrollState::new();
        let scroll2 = scroll.clone();
        composer.compose(crate::compose!(|ctx| {
            crate::ui::Column::new()
                .modifier(crate::modifier::Modifier::new().fill_max_size().vertical_scroll(scroll2))
                .build(ctx, |ctx| {
                    // 内容总高 > 视口 600（每行 ~19px × 40 行 ≈ 760）
                    for _ in 0..40 {
                        crate::ui::Text::new("line content line content").build(ctx);
                    }
                });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(scroll.offset.get(), 0.0);
        let ok = super::apply_scroll_delta(
            composer.arena_nodes_mut(),
            root,
            -200.0, // 负 dy = 向下滚动（内容上移——与 winit 滚轮语义一致）
            crate::unit::Density::from_density(1.0),
        );
        assert!(ok, "应找到 scroll 节点");
        assert!(scroll.offset.get() > 0.0, "滚动后 offset 应 > 0（实际 {}）", scroll.offset.get());
    }
}

pub fn run_app(app: impl FnOnce(&mut ComposeCtx) + 'static) {
    let event_loop = EventLoop::new().expect("event loop");
    let proxy = event_loop.create_proxy();
    debug::set_event_loop_proxy(proxy.clone());
    let proxy2 = proxy.clone();
    *APP_PROXY.lock().unwrap() = Some(proxy);
    crate::core::state::set_wake_fn(move || { let _ = proxy2.wake_up(); });
    debug::start_stdin_channel();
    debug::start_ws_server();
    let state = AppState {
        init: Some(Box::new(app)),
        was_animating: false,
        windows: HashMap::new(),
        pending_content: Vec::new(),
        parent_window_id: None,
        modifiers: Default::default(),
    };
    event_loop.run_app(state).expect("run_app");
    debug::force_shutdown();
}
