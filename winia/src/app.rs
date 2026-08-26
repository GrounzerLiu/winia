//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

use std::time::Instant;

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::debug_log;
use crate::layout::node::{hit_test, focus_next, focus_prev, LayoutNode};
use crate::render;

/// 拖拽/嵌套滚动调试 trace 开关（debug_assertions 下 + 环境变量 WINIA_DRAG_TRACE）。
/// 收敛 7 处重复判断（review E1）——release 构建零成本。
#[cfg(debug_assertions)]
pub(crate) fn drag_trace_enabled() -> bool {
    std::env::var("WINIA_DRAG_TRACE").is_ok()
}
#[cfg(not(debug_assertions))]
pub(crate) fn drag_trace_enabled() -> bool {
    false
}

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
    /// 窗口尺寸的响应式 State（首次组合时 ctx.remember 创建并挂载到
    /// ui::adaptive——resize set() → 依赖方（套件脚手架）slot dirty）
    window_size_state: std::cell::RefCell<Option<crate::core::state::State<(f32, f32)>>>,
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
    /// 最后一次 PointerMoved 的 scene 坐标（逻辑像素）——MouseWheel 命中
    /// 滚动目标用（§3.7：winit 0.31 MouseWheel 事件不带 cursor position，
    /// 需记录指针位置；PointerLeft 时由调用方清空）
    last_pointer_pos: Option<(f32, f32)>,
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
    /// 拖拽滚动会话（按下在滚动容器上：内容跟随指针，松手按速度 fling）
    drag_scroll: Option<DragScroll>,
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
    /// 当前窗口修饰键状态（ModifiersChanged 按 WindowId 维护）
    pub(crate) modifiers: winit::keyboard::ModifiersState,
    /// 当前悬停节点的 slot_key（指针移入/移出时发射 Hover Enter/Exit）
    /// 当前 hover 的 hoverable 节点 slot 集合（**支持嵌套**——Tooltip 锚点
    /// 容器与内部 Button 等可同时 hover；修复前只存最内层 → 嵌套 hoverable
    /// 外层收不到 Enter（Tooltip 锚点挂 hoverable 时内部 Button 抢走事件））
    hovered_slots: std::collections::HashSet<u64>,
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
    click_passthrough: bool,
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
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, window_size_state: std::cell::RefCell::new(None), on_close: None, created_id: None, theme, focused_slot_key: None, pointer_down_state: None, last_pointer_kind: crate::modifier::PointerKind::Mouse { button: crate::modifier::PointerButton::Primary }, last_pointer_pos: None, pointer_down_slot: None, gesture: None, gesture_node: None, gesture_tap_ctx: None, gesture_slot: None, drag_scroll: None, overlays: Vec::new(), overlay_click: None, pending_taps: Vec::new(), frame_counter: 0, last_render_time: std::time::Instant::now(), frame_interval: std::time::Duration::from_millis(16), force_redraw: false, consecutive_panics: 0, render_disabled: false, last_request_time: std::time::Instant::now(), last_refresh_check: std::time::Instant::now(), modifiers: Default::default(), hovered_slots: std::collections::HashSet::new(), pressed_interaction: None, focused_interaction_slot: None }
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
                    .map(|idx| node_or_descendant_wants_ime(nodes, idx))
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
    fn recompose_layout_render(&mut self, window_id: WindowId, after_draw: impl FnOnce(&[LayoutNode], usize, &mut skia_safe::Surface)) {
        let _focus_window = self.composer.focus_window(window_id.into_raw() as u64);
        // vsync 研究：渲染帧计数（每秒渲染次数——Fifo 下应 ~60）
        self.frame_counter += 1;
        debug_log!("[fps] render#{} compose#{} pending={}", self.frame_counter, self.composer.compose_count(), self.composer.pending_state_count());
        // 临时：窗口节点数（诊断主窗口塌缩）
        // 提供当前窗口 Density（从 scale_factor）——覆盖 compose + layout + draw 全程，
        // 保证 Dimension::Px / TextUnit::Px 在布局/渲染期使用窗口 sf 而非 standard(1.0)
        let density = crate::unit::Density::from_density(self.scale_factor as f32);
        crate::unit::with_density(density, || {
        // 更新当前 Composer 的 adaptive context；不再覆盖 thread-local singleton。
        self.composer.set_adaptive_window_size(self.width, self.height);
        // 循环 compose 直到没有新的 pending state——处理并发 task 在 compose 期间
        // 完成的 case（第二个 notify 的 state 在第一次 compose 之后才入队）
        // 循环 compose 直到没有新的 pending state
        let mut any_composed = false;
        loop {
            let did_compose = self.composer.recompose(|ctx| {
                // 窗口尺寸响应式 State：首帧 remember 创建（owner=本 Composer），
                // 每帧挂载到 adaptive + set_silent 同步值；resize 时由事件路径 set() 通知
                let slot = &self.window_size_state;
                let existing = slot.borrow().clone();
                let size_state = existing.unwrap_or_else(|| {
                    // remember 需稳定 key 上下文——裸重组闭包无语句注入，用 ctx.key 包裹
                    ctx.key("winia_window_size_state", |ctx| {
                        let s = ctx.remember(|| (self.width, self.height));
                        *slot.borrow_mut() = Some(s.clone());
                        s
                    })
                });
                size_state.set_silent((self.width, self.height));
                crate::ui::adaptive::set_window_size_state(size_state);
                (self.content)(ctx);
            });
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
                let debug_window_id = window_id.into_raw() as u64;
                if crate::debug::screenshot_requested(debug_window_id) {
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
                if crate::debug::screenshot_requested(debug_window_id) {
                    if let Some((w2, h2, pixels)) = take_capture() {
                        crate::debug::update_pixels(debug_window_id, &pixels, w2, h2);
                    }
                    crate::debug::screenshot_done(debug_window_id);
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
        // DevTools 事件兜底消费：只遍历有 queued events 的窗口，避免每轮
        // 事件批次空转全部窗口；legacy target 0 只会匹配 parent。
        let targets = debug::queued_event_targets();
        if !targets.is_empty() {
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for wid in windows {
                if targets.contains(&(wid.into_raw() as u64)) {
                    self.consume_debug_events(wid);
                }
            }
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
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                // Shift + 垂直滚轮 → 转为水平滚动（兼容 LazyRow 等横向容器；
                // 多数系统不会自动把 Shift+wheel 翻译成 dx，这里显式处理）
                let (dx, dy) = scroll_delta_with_shift(dx, dy, pw.modifiers.shift_key());
                if dx != 0.0 || dy != 0.0 {
                    if let Some(root_idx) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        // §3.7 修复：优先按鼠标位置 hit-test，在命中路径上找滚动
                        // 目标——两个并排滚动区域只滚鼠标悬停的那一个（旧实现
                        // find_scroll_target 按反向 child 顺序盲找，可能滚错）。
                        // 用最后 PointerMoved 位置（winit 0.31 MouseWheel 事件
                        // 不携带 cursor position）。鼠标不在任何滚动节点上时
                        // 回退旧逻辑（兜底）。
                        let target = pw.last_pointer_pos.and_then(|(px, py)| {
                            let path = crate::layout::node::hit_test(nodes, root_idx, px, py);
                            // 命中路径从根到叶——从内向外找第一个轴匹配的 scroll 节点
                            path.iter().rev().find(|&&idx| {
                                (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
                                    || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some())
                            }).copied()
                        }).or_else(|| find_scroll_target(nodes, root_idx, dx, dy));
                        if let Some(target) = target {
                            let _ = dispatch_nested_scroll_delta(pw.composer.arena_nodes_mut(), root_idx, target, crate::nested_scroll::ScrollDelta::new(dx, dy), crate::nested_scroll::NestedScrollSource::Wheel, crate::unit::Density::from_density(pw.scale_factor as f32));
                        }
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
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, scene_pos, crate::modifier::PointerKind::from_button_source(&button), &modifiers, true);
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
                    // 拖拽滚动结束：速度足够 → fling（惯性滚动）
                    drag_scroll_up(pw);
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
                        is_alt_pressed: pw.modifiers.alt_key(),
                        is_ctrl_pressed: pw.modifiers.control_key(),
                        is_shift_pressed: pw.modifiers.shift_key(),
                        is_meta_pressed: pw.modifiers.meta_key(),
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
                // 记录最后指针位置（MouseWheel 命中滚动目标用——§3.7；
                // winit 0.31 MouseWheel 事件不带 cursor position）
                pw.last_pointer_pos = Some(scene_pos);
                // 指针移动核心（共享——真实/Debug 防分叉；Debug 路径此前缺
                // x_off 对齐偏移——Center/Right 对齐文本选择错位，合并修复）
                let modifiers = pw.modifiers;
                let consumed = handle_pointer_move(pw, scene_pos, pw.last_pointer_kind.clone(), &modifiers);
                // 消费（on_pointer_event 可能更新 State）或按下拖动选区时请求重绘；
                // 未消费的悬停移动不唤醒事件循环（避免每帧白醒）
                if consumed || pw.pointer_down_state.is_some() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            // 指针离开窗口：清所有 hover（对每个 hoverable 补发 Exit）
            WindowEvent::PointerLeft { .. } => {
                let olds: Vec<u64> = pw.hovered_slots.drain().collect();
                for old in olds {
                    exit_hover_at(pw, old);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                pw.modifiers = m.state();
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
                    is_alt_pressed: pw.modifiers.alt_key(),
                    is_ctrl_pressed: pw.modifiers.control_key(),
                    is_shift_pressed: pw.modifiers.shift_key(),
                    is_meta_pressed: pw.modifiers.meta_key(),
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
                    let shift = pw.modifiers.shift_key();
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
                    consumed = dispatch_key_to_focus(pw, &ke);
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
                        // 通过 focused node 的 ime_callback 通知 TextField。
                        // ⚠ TextField 容器化：焦点在容器、ime_callback 在输入 leaf——
                        // 只查焦点节点自身则 Preedit 永远到不了（预输入不显示）。
                        // 从焦点节点向下找第一个 ime_callback。
                        if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                    if let Some(ime_idx) = find_descendant_ime_callback(nodes, idx) {
                                        if let Some(cb) = nodes[ime_idx].ime_callback.borrow_mut().as_mut() {
                                            cb(&text, cursor);
                                        }
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
                // 直接采纳事件尺寸。旧版对 >50% 变化做忽略（防 winit #2094 陈旧
                // 事件）——但最大化必然 >50%，整个最大化路径被吞（尺寸/布局全部
                // 卡旧值）。陈旧事件改由 RedrawRequested 的每帧 inner_size 校准自愈。
                let l = s.to_logical::<f32>(pw.scale_factor);
                pw.width = l.width; pw.height = l.height;
                if let Some(ref mut sw) = pw.skia_window { sw.resize(); }
                // 不设 force_redraw：拖拽时 SurfaceResized 以鼠标速率（~125Hz）到达，
                // 无条件渲染会绕过帧节流（实测 3.6ms/帧）。帧节流的"渲染欠账"机制
                // （跳过时自设 force_redraw）保证更新不丢；request_redraw 保持链路
                // （OS 拖拽期间持续 WM_PAINT + 自驱，帧距 ≥16ms）
                // 尺寸 State set() 通知——依赖方（NavigationSuiteScaffold 等
                // 读 window_size() 的 slot）标记 dirty，增量重组切换形态
                if let Some(s) = pw.window_size_state.borrow().as_ref() {
                    s.set((l.width, l.height));
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // 帧时钟 tick（P3-12：with_frame_nanos 的驱动源——每帧广播时间戳）
                crate::effect::frame_tick();
                // 尺寸自愈校准：以 winit 实际 surface 尺寸为准。winit #2094 类
                // 陈旧事件（最大化时旧尺寸事件可能后到）由此在下一帧纠正——
                // 偏差 >0.5 逻辑像素即更新 pw + 通知尺寸 State（套件形态随之刷新）
                if let Some(ref sw) = pw.skia_window {
                    let phys = sw.surface_size();
                    let l = phys.to_logical::<f32>(pw.scale_factor);
                    if (l.width - pw.width).abs() > 0.5 || (l.height - pw.height).abs() > 0.5 {
                        pw.width = l.width;
                        pw.height = l.height;
                        if let Some(s) = pw.window_size_state.borrow().as_ref() {
                            s.set((l.width, l.height));
                        }
                    }
                }
                // 动画推进（冗余于 new_events——Windows 拖拽 resize 是模态循环，
                // new_events/AboutToWait 被阻塞不触发；WM_PAINT 驱动的 RedrawRequested
                // 是拖拽中唯一持续到达的事件——在此 tick 才有"边拖边动"的动画。
                // 与 new_events 双 tick 无害：Animatable 按实际 dt 推进）
                crate::animation::update_animations();
                // 消费焦点请求（在 compose 前处理，避免丢失）
                let _focus_window = pw.composer.focus_window(window_id.into_raw() as u64);
                for id in pw.composer.take_focus_requests() {
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
                                // ⚠ TextField 容器化：焦点在容器、ime_callback 在输入
                                // leaf——须向下找子树（node_or_descendant_wants_ime）
                                let wants_ime = node_or_descendant_wants_ime(nodes, found);
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
                    pw.recompose_layout_render(window_id, |nodes, root_idx, surface| {
                        debug::update_tree(wid, &debug::build_tree_json(nodes, root_idx));
                    });
                    // overlay 独立 Composer 的 arena 同样进调试树（modal/popup 可观测；
                    // 每帧整体替换——overlay 关闭后条目自动消失）。z 序 = pw.overlays
                    // 栈序，与 render_overlays 绘制顺序一致。
                    let ov_trees: Vec<(u64, String)> = pw.overlays.iter().filter_map(|ov| {
                        ov.composer.layout_root_idx()
                            .map(|r| (ov.id, debug::build_tree_json(ov.composer.arena_nodes(), r)))
                    }).collect();
                    debug::set_overlay_trees(wid, ov_trees);
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
                            // ⚠ TextField 容器化：焦点在容器、paragraph/cursor 在输入
                            // leaf——须向下找 ime_callback 节点（find_descendant_ime_callback）；
                            // 只查焦点节点则容器无 paragraph → 候选框区域从不更新
                            let pidx = crate::layout::node::find_node_by_id(nodes, r, fid)
                                .and_then(|idx| find_descendant_ime_callback(nodes, idx));
                            if let Some(pidx) = pidx {
                                if let Ok(borrow) = nodes[pidx].cached_paragraph.try_borrow() {
                                    if let Some(p) = borrow.as_ref() {
                                        let abs = node_abs_position(nodes, r, nodes[pidx].id);
                                        // 内容区偏移（渲染侧 content_x = 节点原点 +
                                        // padding——IME 区域须与其对称，否则预选
                                        // 窗口整体偏 padding 偏移）
                                        let (pad_s, pad_t, pad_e, _) = nodes[pidx].modifier.get_padding_sides();
                                        let pad_x = if nodes[pidx].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                                        // 空文本：无 glyph 可定位——IME 区域放内容
                                        // 起点（行高近似；与渲染端空文本光标一致）
                                        let empty = nodes[pidx].modifier.content_len() == 0;
                                        // 光标索引是编辑偏移——经映射转显示偏移
                                        // （TextFieldVisual 挂在容器——向上找）
                                        let caret_idx = crate::ui::text_field::offset_mapping_for_node(nodes, r, pidx)
                                            .map(|m| m.original_to_transformed(nodes[pidx].cursor_index.get()))
                                            .unwrap_or_else(|| nodes[pidx].cursor_index.get());
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
                if pw.composer.pending_window_close_id().is_some() {
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
        for evt in debug::take_queued_events(window_id.into_raw() as u64) {
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
                        }
                    } else if let Some(k) = parse_debug_key(&key) {
                        // 任意按键：复用真实键盘派发路径（Preview/Bubble/激活）——
                        // WS 可模拟字符输入/删除/方向键。修饰键默认无（Ctrl 等
                        // 组合暂不支持——如需可扩展 KbEvent 修饰字段）
                        let ke = crate::modifier::KbEvent {
                            key: k,
                            event_type: crate::modifier::KbEventType::KeyDown,
                            is_alt_pressed: pw.modifiers.alt_key(),
                            is_ctrl_pressed: pw.modifiers.control_key(),
                            is_shift_pressed: pw.modifiers.shift_key(),
                            is_meta_pressed: pw.modifiers.meta_key(),
                            repeat: false,
                        };
                        dispatch_key_to_focus(pw, &ke);
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    handled = true;
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
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, (x, y), pw.last_pointer_kind.clone(), &modifiers, false);
                    handled = true;
                }
                debug::DebugEvent::PointerMove { x, y } => {
                    // 模拟拖动选择：与真实 PointerMoved 共用核心（含 x_off 对齐偏移）
                    let modifiers = pw.modifiers;
                    handle_pointer_move(pw, (x, y), pw.last_pointer_kind.clone(), &modifiers);
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
                    // 拖拽滚动结束：速度足够 → fling（与真实路径一致）
                    drag_scroll_up(pw);
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
                debug::DebugEvent::Scroll { dx, dy } => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(target) = find_scroll_target(nodes, r, dx, dy) {
                            let consumed = dispatch_nested_scroll_delta(pw.composer.arena_nodes_mut(), r, target, crate::nested_scroll::ScrollDelta::new(dx, dy), crate::nested_scroll::NestedScrollSource::Wheel, crate::unit::Density::from_density(pw.scale_factor as f32));
                            handled = consumed.x != 0.0 || consumed.y != 0.0;
                        }
                    }
                }
                debug::DebugEvent::Resize { w, h } => {
                    // 真实 resize：request_inner_size → WM_SIZE → SurfaceResized
                    // 事件自然回流——与拖拽/最大化完全同通路（pw 字段与尺寸 State
                    // 均由事件处理器统一更新，避免与 surface_size 自愈互相打架）
                    if let Some(ref sw) = pw.skia_window {
                        let _ = sw.request_surface_size(
                            winit::dpi::LogicalSize::new(w as f64, h as f64).into(),
                        );
                    }
                    handled = true;
                }
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
                // The temporary initialization Composer is not an active
                // window owner; its lifecycle state is dropped with it.
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
        // FocusRequester created by this window's initial composition is bound
        // to this WindowId, just like later redraw compositions.
        let _focus_window = pw.composer.focus_window(window_id.into_raw() as u64);
        // 首次 compose+layout+draw 也提供 Density（Px 单位首帧即正确）
        let density = crate::unit::Density::from_density(sf as f32);
        crate::unit::with_density(density, || {
        // 首帧同样注入窗口尺寸（自适应组件首帧即正确形态）+ 挂载响应式 State
        pw.composer.set_adaptive_window_size(pending.width, pending.height);
        let slot = &pw.window_size_state;
        let (w0, h0) = (pending.width, pending.height);
        pw.composer.compose(|ctx| {
            let existing = slot.borrow().clone();
            let size_state = existing.unwrap_or_else(|| {
                ctx.key("winia_window_size_state", |ctx| {
                    let s = ctx.remember(|| (w0, h0));
                    *slot.borrow_mut() = Some(s.clone());
                    s
                })
            });
            size_state.set_silent((w0, h0));
            crate::ui::adaptive::set_window_size_state(size_state);
            (pw.content)(ctx);
        });
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
            crate::debug::set_legacy_target(window_id.into_raw() as u64);
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

/// 拖拽滚动会话：slot key（跨重组稳定）+ 最近位置 + 速度样本（松手 fling 用）。
/// 双轴跟踪（LazyRow 横向拖拽/惯性用 x 轴）——样本 (t, x, y)。
struct DragScroll {
    slot: u64,
    last_x: f32,
    last_y: f32,
    samples: Vec<(std::time::Instant, f32, f32)>,
}

impl DragScroll {
    /// 手指速度（px/s）：最近 ~200ms 窗口的最小二乘斜率。
    /// ⚠ x 轴用"距离现在的时长"（越大越早）——回归斜率符号与真实时间相反，
    /// 取负修正（实测：向上拖 100px 得 +650 而非 -650，fling 方向反了）。
    fn regression(&self, sel: fn(&(std::time::Instant, f32, f32)) -> f32) -> f32 {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_millis(200);
        let pts: Vec<(f32, f32)> = self.samples
            .iter()
            .filter(|(t, _, _)| *t >= cutoff)
            .map(|s| (now.duration_since(s.0).as_secs_f32(), sel(s)))
            .collect();
        if pts.len() < 2 { return 0.0; }
        let n = pts.len() as f32;
        let sx: f32 = pts.iter().map(|p| p.0).sum();
        let sy: f32 = pts.iter().map(|p| p.1).sum();
        let sxy: f32 = pts.iter().map(|p| p.0 * p.1).sum();
        let sxx: f32 = pts.iter().map(|p| p.0 * p.0).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() < 1e-6 { return 0.0; }
        -(n * sxy - sx * sy) / denom
    }
    fn velocity_x(&self) -> f32 { self.regression(|s| s.1) }
    fn velocity_y(&self) -> f32 { self.regression(|s| s.2) }
}

/// Shift + 滚轮语义：把垂直滚轮 delta 转为水平滚动（保留方向，清除垂直分量）。
/// 用于兼容 LazyRow 等横向滚动容器——多数平台不会自动把 Shift+wheel 转成 dx。
fn scroll_delta_with_shift(dx: f32, dy: f32, shift: bool) -> (f32, f32) {
    if shift {
        (dy, 0.0)
    } else {
        (dx, dy)
    }
}

fn dispatch_nested_scroll_delta(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    delta: crate::nested_scroll::ScrollDelta,
    source: crate::nested_scroll::NestedScrollSource,
    density: crate::unit::Density,
) -> crate::nested_scroll::ScrollDelta {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return crate::nested_scroll::ScrollDelta::ZERO; }
    let mut remaining = delta;
    let mut total = crate::nested_scroll::ScrollDelta::ZERO;
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[pre-scroll] idx={} connection=有", idx);
            }
            let part = connection.on_pre_scroll(remaining, source).clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    let child_consumed = apply_scroll_delta_inner(nodes, target, remaining.x, remaining.y, density, false);
    total = total + child_consumed;
    remaining = remaining - child_consumed;
    // post-scroll：祖先从内到外（**含 target 自身**——TopAppBar 等 connection
    // 挂在 scroll 容器节点上，依赖 on_post_scroll 更新 content_offset 变色；
    // 排除 target 会破坏该行为，见 scaffold_demo/fixture_nested_scroll）
    for &idx in path.iter().rev() {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection.on_post_scroll(child_consumed, remaining, source).clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    total
}

fn dispatch_nested_scroll_fling(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    velocity: crate::nested_scroll::ScrollVelocity,
) -> crate::nested_scroll::ScrollVelocity {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return velocity; }

    let mut remaining = velocity;
    let mut consumed = crate::nested_scroll::ScrollVelocity::default();
    // pre-fling：祖先（含 target）先消费一部分速度——正序 path，target 自身
    // connection 也参与 pre（对标 Compose：目标自身的 connection 参与 pre）
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection.on_pre_fling(remaining).clamp_to(remaining);
            consumed.x += part.x;
            consumed.y += part.y;
            remaining.x -= part.x;
            remaining.y -= part.y;
        }
    }
    // child fling：剩余速度交给目标滚动节点；撞边界时把瞬时剩余速度交给
    // post-fling 链。post 链 = path 逆序（**含 target 自身**——TopAppBar 等
    // connection 挂在 scroll 容器节点上，依赖 on_post_fling 弹回/复位；
    // 排除 target 会破坏该行为，见 scaffold_demo/fixture_nested_scroll）。
    let child_velocity = remaining;
    // child 实际消费量 = 起始速度 − 边界剩余速度（在 boundary 回调内计算——
    // fling_with_boundary 回调传入的是撞边界时的瞬时剩余速度）。
    // ⚠ 不能传起始速度：on_post_fling 的 consumed_by_child 语义是"child 实际
    // 消费了多少"，TopAppBar 依赖它做回弹幅度（review C1）。
    let post_connections: Vec<std::sync::Arc<dyn crate::nested_scroll::NestedScrollConnection>> =
        path.iter().rev()
            .filter_map(|&idx| nodes[idx].modifier.nested_scroll_connection())
            .collect();
    let child_started = {
        let node = &nodes[target];
        if let Some(ss) = node.modifier.vertical_scroll_state() {
            if child_velocity.y.abs() >= 50.0 {
                let post_connections = post_connections.clone();
                ss.fling_with_boundary(child_velocity.y, move |remaining_velocity| {
                    // child 实际消费 = 起始 − 边界剩余（剩余为 0 时全消费）
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: 0.0,
                        y: child_velocity.y - remaining_velocity,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: 0.0, y: remaining_velocity };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.y -= crate::nested_scroll::ScrollVelocity { x: 0.0, y: part.y }.clamp_to(available).y;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else if let Some(ss) = node.modifier.horizontal_scroll_state() {
            if child_velocity.x.abs() >= 50.0 {
                let post_connections = post_connections.clone();
                ss.fling_with_boundary(child_velocity.x, move |remaining_velocity| {
                    // child 实际消费 = 起始 − 边界剩余
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: child_velocity.x - remaining_velocity,
                        y: 0.0,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: remaining_velocity, y: 0.0 };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.x -= crate::nested_scroll::ScrollVelocity { x: part.x, y: 0.0 }.clamp_to(available).x;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else { false }
    };
    if child_started {
        consumed.x += child_velocity.x;
        consumed.y += child_velocity.y;
    }
    consumed
}

fn find_scroll_target(nodes: &[LayoutNode], idx: usize, dx: f32, dy: f32) -> Option<usize> {
    for &child in nodes[idx].children.iter().rev() {
        if let Some(target) = find_scroll_target(nodes, child, dx, dy) { return Some(target); }
    }
    if (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
        || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some()) {
        Some(idx)
    } else { None }
}

fn apply_scroll_delta(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density) -> crate::nested_scroll::ScrollDelta {
    apply_scroll_delta_inner(nodes, idx, dx, dy, density, true)
}

/// `apply_scroll_delta` 实现。`recursive=true` 时，若节点自身未消费（已到
/// 边界），回退递归子节点（旧行为——wheel/拖拽的 fallback 语义，让内层
/// 子 scroll 消费）。`recursive=false` 时**只滚目标自身**——`dispatch_nested
/// _scroll_delta` 的显式 target 语义：target 滚不动应留给 post 链的祖先
/// connection 处理，**不能**偷偷滚子节点（否则"在外层顶部向下拖"会错误地
/// 滚动内层子列表——用户报告的 bug：外层已到顶，delta 递归到内层）。
fn apply_scroll_delta_inner(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density, recursive: bool) -> crate::nested_scroll::ScrollDelta {
    let mut consumed = crate::nested_scroll::ScrollDelta::ZERO;
    {
        let node = &nodes[idx];
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[apply-scroll] idx={} dy={} vp_h={} content_h={} has_vscroll={}",
                idx, dy, node.scroll_viewport_height, node.scroll_content_height,
                node.modifier.vertical_scroll_state().is_some());
        }
        if dy != 0.0 {
            if let Some(state) = node.modifier.vertical_scroll_state() {
            // 手动输入接管：取消进行中的 fling + 结束滚动中标记（拖拽路径随后置回）
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[apply-scroll] → state.offset 应用前 = {}", current);
            }
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
            // 内容总高：lazy 列表（scroll_content_height > 0）用其真实内容高；
            // 否则用节点自身高度（普通 scroll 容器）
            let content_h = if node.scroll_content_height > 0.0 {
                node.scroll_content_height
            } else {
                node.measured_size.height
            };
            let max_offset = (content_h - visible_h).max(0.0);
            let new = (current - dy).clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.y = current - new;
            }
        }
        // 水平滚动（LazyRow/横向 scroll 容器）——与垂直对称：dx 正 = 内容左移
        if dx != 0.0 {
        if let Some(state) = node.modifier.horizontal_scroll_state() {
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            let visible_w = if node.scroll_viewport_width > 0.0 {
                node.scroll_viewport_width
            } else {
                node.modifier.fixed_size()
                    .and_then(|(w, _)| {
                        use crate::modifier::Dimension;
                        match w {
                            Dimension::Fixed(w) | Dimension::Dp(crate::unit::Dp(w)) => Some(w),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            let content_w = if node.scroll_content_width > 0.0 {
                node.scroll_content_width
            } else {
                node.measured_size.width
            };
            let max_offset = (content_w - visible_w).max(0.0);
            let new = (current - dx).clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.x = current - new;
        }
        }
    }
    if consumed.x != 0.0 || consumed.y != 0.0 { return consumed; }
    // 自身未消费：recursive 模式回退递归子节点（旧 fallback 语义）；
    // 非 recursive（dispatch 路径）不递归——target 滚不动交给 post 链
    if !recursive { return consumed; }
    // 子节点（clone 索引后递归，避免与 nodes 的可变借用冲突）
    let children: Vec<usize> = nodes[idx].children.clone();
    for c in children {
        let child = apply_scroll_delta_inner(nodes, c, dx, dy, density, true);
        if child.x != 0.0 || child.y != 0.0 { return child; }
    }
    consumed
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

/// 拖拽滚动结束：速度足够 → 惯性 fling（内容速度 = -手指速度——手指向上甩
/// 内容继续向上 = offset 增大）。速度不足 → 仅结束滚动中标记。
fn drag_scroll_up(pw: &mut PerWindow) {
    let Some(ds) = pw.drag_scroll.take() else { eprintln!("[DBG-DS] up but no drag_scroll"); return };
    let (vx, vy) = (ds.velocity_x(), ds.velocity_y());
    let target: Option<usize> = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, ds.slot)?;
        crate::layout::node::find_node_by_id(nodes, r, id)
    })();
    #[cfg(debug_assertions)]
    if drag_trace_enabled() {
        eprintln!("[drag-up] slot={:?} target_idx={:?} v=({},{})",
            ds.slot, target, vx, vy);
    }
    let Some(idx) = target else { return };
    let Some(root) = pw.composer.layout_root_idx() else { return };
    // 手指速度 → 滚动速度（内容速度 = -手指速度），并走 nested scroll pre/post fling 链
    let velocity = crate::nested_scroll::ScrollVelocity { x: -vx, y: -vy };
    let _ = dispatch_nested_scroll_fling(pw.composer.arena_nodes_mut(), root, idx, velocity);
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
            click_passthrough: desc.click_passthrough,
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
        self.click_passthrough = desc.click_passthrough;
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
        // ⚠ click_passthrough（Tooltip）：命中浮层但**放行主树**——浮层盖住
        // 锚点（锚点上方 tooltip 与锚点本身重叠）时点击锚点仍生效（否则
        // tooltip 挡住锚点按钮 → 外部 visible 控制关不了）
        if ov.click_passthrough {
            return false;
        }
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
            let passthrough = pw.overlays[i].click_passthrough;
            let cb = pw.overlays[i].on_dismiss.take();
            pw.overlays.remove(i);
            if let Some(cb) = cb {
                (cb)();
            }
            // ⚠ Tooltip（passthrough）：dismiss 后**放行主树**——点击不消费
            // （否则点按钮第一次只关 tooltip、按钮收不到——需点两次）
            if passthrough {
                continue;
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

/// 悬停更新：**路径上所有 hoverable** 节点进入/离开 → 发射 Hover Enter/Exit
/// （对标 Compose hoverable：每个 hoverable 独立收到 Enter/Exit；修复前只
/// 发射最内层——嵌套 hoverable（如 Tooltip 锚点容器 + 内部 Button）外层
/// 收不到 Enter → Tooltip 不显示）。节点移除时自动补 Exit。
fn update_hover(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // 当前路径上所有 hoverable 的 (slot, interaction)
    let hit: Vec<(u64, crate::ui::interaction::MutableInteractionSource)> = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
        path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect()
    };
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    // 仍在 hover 的：跳过（已 enter）
    // 新进入：emit enter + 记录
    for (slot, src) in &hit {
        if pw.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    // 已退出：补 Exit + 移除
    let gone: Vec<u64> = pw.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        pw.hovered_slots.remove(&slot);
        exit_hover_at(pw, slot);
    }
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

/// 键盘事件派发到焦点路径（Preview: root→focused；Bubble: focused→root，
/// 对齐 onPreviewKeyEvent/onKeyEvent）+ 聚焦组件激活（Enter/Space 触发
/// onClick——仅聚焦节点自身的 clickable）。
///
/// 真实 KeyboardInput 与 debug 模拟共用（防行为分叉）：Escape/Tab 等
/// 框架级按键由调用方前置处理（聚焦导航/清焦），不进入本函数。
fn dispatch_key_to_focus(pw: &PerWindow, ke: &crate::modifier::KbEvent) -> bool {
    let Some(fid) = pw.focused_id else { return false };
    let Some(r) = pw.composer.layout_root_idx() else { return false };
    let nodes = pw.composer.arena_nodes();
    // 收集焦点路径：root → ... → focused
    let mut path: Vec<usize> = Vec::new();
    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
        path.push(idx);
        let mut pid = nodes[idx].parent_id;
        while let Some(id) = pid {
            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                path.push(anc);
                pid = nodes[anc].parent_id;
            } else { break; }
        }
        path.reverse(); // path[0] == root, path[last] == focused
    }
    // Preview: root → focused（对齐 onPreviewKeyEvent）
    for &ni in &path {
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
    }
    // Bubble: focused → root（对齐 onKeyEvent）
    for &ni in path.iter().rev() {
        for el in nodes[ni].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
    }
    // 聚焦组件的键盘激活（对标 Compose clickable：聚焦时按 Enter/Space
    // 触发 onClick——仅聚焦节点自身的 clickable 响应，不向祖先冒泡）
    let is_activate = matches!(&ke.key, winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter))
        || matches!(&ke.key, winit::keyboard::Key::Character(c) if c == " ");
    if ke.event_type == crate::modifier::KbEventType::KeyDown && !ke.repeat && is_activate {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
            if let Some(on_click) = nodes[idx].modifier.on_click() {
                on_click();
                return true;
            }
        }
    }
    false
}

/// 解析 debug `k` 命令的按键字符串为 winit Key（单字符 → Character；
/// 特殊名 → NamedKey）。无法解析返回 None（忽略）。
fn parse_debug_key(s: &str) -> Option<winit::keyboard::Key> {
    use winit::keyboard::{Key, NamedKey};
    match s {
        "Backspace" => Some(Key::Named(NamedKey::Backspace)),
        "Delete" => Some(Key::Named(NamedKey::Delete)),
        "Enter" => Some(Key::Named(NamedKey::Enter)),
        "Escape" => Some(Key::Named(NamedKey::Escape)),
        "ArrowLeft" => Some(Key::Named(NamedKey::ArrowLeft)),
        "ArrowRight" => Some(Key::Named(NamedKey::ArrowRight)),
        "ArrowUp" => Some(Key::Named(NamedKey::ArrowUp)),
        "ArrowDown" => Some(Key::Named(NamedKey::ArrowDown)),
        "Home" => Some(Key::Named(NamedKey::Home)),
        "End" => Some(Key::Named(NamedKey::End)),
        _ if s.chars().count() == 1 => Some(Key::Character(s.to_string().into())),
        _ => None,
    }
}

/// 查找 grapheme anchor 定位用的文本节点（有 cached_paragraph 的节点）。
///
/// 策略分两步：
/// 1. **向上**：沿命中路径从 innermost 起找第一个有 paragraph 的节点——
///    命中文本本身（输入 leaf / label / 输出 Text）时直接命中。
/// 2. **向下**（向上失败，且 innermost 是 TextField 容器）：点击容器空白
///    （文本右侧/padding/后缀区）时输入 leaf 不在命中路径——DFS 容器子树
///    找第一个「有 paragraph + 有 registrar」的后代（TextField 输入 leaf；
///    label/placeholder 有 para 无 reg，排除）。
///
/// ⚠ 向下分支必须**限定 TextField 容器**（modifier 含 TextFieldVisual；
/// 输入 leaf 无此标记）：非 TextField 空白（页面空隙/Column 空白/
/// SelectionContainer 空白）绝不向下找——否则 DFS 会误命中子树中任意
/// 「有 para+reg」的文本（如远处 TextField 的输入 leaf），从空白按下的
/// 拖动会错误选中/错绑 anchor。
///
/// 返回 None = 点击处无可定位文本（空区域点击 → 不设光标/不开始拖选）。
fn find_anchor_text_node(nodes: &[LayoutNode], path: &[usize], innermost: usize) -> Option<usize> {
    if let Some(&i) = path.iter().rev().find(|&&i| nodes[i].cached_paragraph.borrow().is_some()) {
        return Some(i);
    }
    let is_tf_container = nodes[innermost].modifier.elements().iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::TextFieldVisual { .. }));
    if !is_tf_container {
        return None;
    }
    // 向下找：TextField 容器子树中的输入 leaf（para + registrar 双条件）
    fn dfs(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
        if nodes[idx].cached_paragraph.borrow().is_some() && nodes[idx].registrar.borrow().is_some() {
            return Some(idx);
        }
        for &c in &nodes[idx].children {
            if let Some(f) = dfs(nodes, c) { return Some(f); }
        }
        None
    }
    dfs(nodes, innermost)
}

/// 节点或其子树中是否有节点声明 IME 需求（ime_callback 非空）。
///
/// ⚠ TextField 容器化：焦点/键盘在容器（focusable 挂在容器 modifier），而
/// ime_callback 在**输入 leaf**（set_current_node_ime_callback）——只查焦点
/// 节点自身则 TextField 聚焦时 IME 永不开启。DFS 向下找子树（与
/// find_anchor_text_node 同策略）；Button 等无 ime_callback 的 focusable
/// 子树不匹配 → 聚焦不开启输入法。
fn node_or_descendant_wants_ime(nodes: &[LayoutNode], idx: usize) -> bool {
    if nodes[idx].ime_callback.borrow().is_some() {
        return true;
    }
    for &c in &nodes[idx].children {
        if node_or_descendant_wants_ime(nodes, c) {
            return true;
        }
    }
    false
}

/// 从节点子树中找第一个声明 ime_callback 的节点（TextField 输入 leaf）。
///
/// ⚠ 与 node_or_descendant_wants_ime 同根因：焦点在容器、ime_callback 在
/// 输入 leaf——Preedit 事件须派发到实际持有 ime_callback 的节点。
fn find_descendant_ime_callback(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
    if nodes[idx].ime_callback.borrow().is_some() {
        return Some(idx);
    }
    for &c in &nodes[idx].children {
        if let Some(f) = find_descendant_ime_callback(nodes, c) {
            return Some(f);
        }
    }
    None
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
        if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
            reg.clear_selection();
        }
    }
    // grapheme anchor 定位。
    // ⚠ TextField 容器化后：paragraph 只缓存在**输入 leaf**，容器节点无
    // cached_paragraph——点击容器空白命中容器 → innermost 无 para。由
    // find_anchor_text_node 处理：向上找有 paragraph 的祖先（命中文本本身），
    // 失败且命中 TextField 容器时向下找子树中的输入 leaf。
    let anchor_node = find_anchor_text_node(nodes, &path, innermost);
    let anchor = anchor_node.and_then(|ai| {
        let borrow = nodes[ai].cached_paragraph.try_borrow().ok()?;
        borrow.as_ref().map(|para| {
            let (ax, ay) = node_abs_position(nodes, r, nodes[ai].id);
            // 段落局部坐标须扣除节点 padding（渲染侧文本画在 content 区 =
            // 节点原点 + padding——不扣则点击位置整体偏 padding 偏移，
            // 光标跳错位置）
            let (pad_s, pad_t, pad_e, _) = nodes[ai].modifier.get_padding_sides();
            let pad_x = if nodes[ai].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
            let tl = crate::text::TextLayout::new(para, 0);
            let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - ax - pad_x, scene_pos.1 - ay - pad_t));
            // 定位结果是显示文本偏移（paragraph = 显示文本）——经 OffsetMapping
            // 转回编辑偏移（密码掩码/格式化输入）。
            // ⚠ TextFieldVisual 挂在**容器**——须向上找（offset_mapping_for_node）
            crate::ui::text_field::offset_mapping_for_node(nodes, r, ai)
                .map(|m| m.transformed_to_original(hit))
                .unwrap_or(hit)
        })
    });
    // reg 只用 anchor 节点自己的 registrar（不 fallback active_registrar）——
    // 不可选节点（输出 Text 等未注册）的 node.registrar 为 None，
    // fallback 会取到全局残留（如 Container B 的）→ anchor_registrar
    // 错绑 B → 拖动到 B 时 same_reg=true → 混合偏移 → B 被选
    let own_reg = anchor_node.and_then(|ai| nodes[ai].registrar.borrow().as_ref().cloned());
    let anchor_global = anchor.and_then(|a| {
        anchor_node.and_then(|ai| {
            own_reg.as_ref()
                .and_then(|reg| reg.segment_info(nodes[ai].slot_key).map(|(off, _)| off + a))
        })
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
        // 光标设置在 anchor 节点（输入 leaf）上——光标绘制/回调都在 leaf
        if let (Some(a), Some(ai)) = (anchor, anchor_node) {
            nodes[ai].cursor_index.set(a);
            if let Some(cb) = nodes[ai].cursor_callback.borrow_mut().as_mut() {
                cb(a);
            }
        }
    }

    // 按下交互（clickable 绑定源——Compose Press 语义；置于 nodes 借用结束后）
    press_interaction_down(pw, &path, scene_pos);

    // 手势入口（on_press 立即触发；后续 move/up 由 gesture_node 路由）——
    // 置于 with_focus 块后（nodes 借用结束，避免与 pw mut 冲突）
    gesture_down(pw, scene_pos);

    // 拖拽滚动目标：按下点向上找最近滚动容器。文本选择/组件 drag 手势优先
    // （拖选文本/组件拖拽不滚动——对齐 Compose 最内层 pointerInput 消费）
    pw.drag_scroll = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let path = hit_test(nodes, r, scene_pos.0, scene_pos.1);
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[drag-down] scene=({},{}) path_len={} path={:?}",
                scene_pos.0, scene_pos.1, path.len(),
                path.iter().map(|&i| (i, nodes[i].slot_key, nodes[i].modifier.vertical_scroll_state().is_some())).collect::<Vec<_>>());
            for &i in &path {
                let n = &nodes[i];
                eprintln!("  [drag-node] idx={} pos=({},{}) size=({},{}) vp_h={} scroll={}",
                    i, n.position.x, n.position.y, n.measured_size.width, n.measured_size.height,
                    n.scroll_viewport_height,
                    n.modifier.vertical_scroll_state().is_some());
            }
        }
        let Some(&innermost) = path.last() else { return None };
        let selecting = pw.pointer_down_state.as_ref()
            .map(|s| s.selection_anchor.is_some())
            .unwrap_or(false);
        let child_drag = path.iter().rev()
            .find(|&&i| nodes[i].modifier.has_gesture())
            .map(|&i| nodes[i].modifier.has_drag_gesture())
            .unwrap_or(false);
        if selecting || child_drag {
            None
        } else {
            path.iter().rev()
                .find(|&&i| {
                    nodes[i].modifier.vertical_scroll_state().is_some()
                        || nodes[i].modifier.horizontal_scroll_state().is_some()
                })
                .map(|&i| DragScroll {
                    slot: nodes[i].slot_key,
                    last_x: scene_pos.0,
                    last_y: scene_pos.1,
                    samples: vec![(std::time::Instant::now(), scene_pos.0, scene_pos.1)],
                })
        }
    })();

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
    // 拖拽滚动：内容跟随指针 + 记录速度样本（松手 fling 用）。放在手势/文本
    // 选择之前——但按下时已排除组件 drag 手势与文本选择，此处无冲突
    if pw.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let nodes = pw.composer.arena_nodes();
            let Some(r) = pw.composer.layout_root_idx() else { return None };
            let slot = pw.drag_scroll.as_ref().unwrap().slot;
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                let nodes = pw.composer.arena_nodes();
                eprintln!("[drag-move] target_idx={} scroll_v={} scroll_h={} slot={:?}",
                    idx,
                    nodes[idx].modifier.vertical_scroll_state().is_some(),
                    nodes[idx].modifier.horizontal_scroll_state().is_some(),
                    pw.drag_scroll.as_ref().map(|d| d.slot));
            }
            // 轴感知：垂直容器吃 dy，水平容器吃 dx（apply_scroll_delta 按节点轴取）
            let (ax, ay) = {
                let nodes = pw.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() {
                    (0.0, dy)
                } else if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                    (dx, 0.0)
                } else {
                    (0.0, 0.0)
                }
            };
            if ax != 0.0 || ay != 0.0 {
                let root = pw.composer.layout_root_idx();
                let consumed = root.map(|root| dispatch_nested_scroll_delta(
                    pw.composer.arena_nodes_mut(),
                    root,
                    idx,
                    crate::nested_scroll::ScrollDelta::new(ax, ay),
                    crate::nested_scroll::NestedScrollSource::Drag,
                    crate::unit::Density::from_density(pw.scale_factor as f32),
                )).unwrap_or(crate::nested_scroll::ScrollDelta::ZERO);
                handled = consumed.x != 0.0 || consumed.y != 0.0;
            }
            // 拖拽中标记（apply_scroll_delta 内部取消 fling 时置 false——这里覆盖）
            let nodes = pw.composer.arena_nodes();
            if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            } else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            }
        }
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
                                // 显示偏移 → 编辑偏移（密码掩码/格式化输入；
                                // TextFieldVisual 在容器——向上找）
                                let current_index = crate::ui::text_field::offset_mapping_for_node(nodes, r, innermost)
                                    .map(|m| m.transformed_to_original(current_index))
                                    .unwrap_or(current_index);
                                let cur_off = reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off);
                                if let Some((target, s, e)) = crate::ui::selection_container::compute_selection(
                                    down.anchor_registrar.as_ref(), down.selection_anchor,
                                    &reg, cur_off, current_index,
                                    scene_pos.1, down.position.1, abs_y,
                                ) {
                                    // 范围是编辑偏移（anchor/current 已转回）——
                                    // reg 空间 = 显示偏移，写入选区前转换
                                    let (ts, te) = crate::ui::text_field::offset_mapping_for_node(nodes, r, innermost)
                                        .map(|m| (m.original_to_transformed(s), m.original_to_transformed(e)))
                                        .unwrap_or((s, e));
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
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）。
    // 与 hit_test_recursive 一致：节点 i 的视觉坐标 = Σposition[0..=i]
    // − Σscroll[0..i-1]（祖先的 scroll offset；节点自身 offset 只影响子节点，
    // 不影响自身左上角——见 node.rs hit_test_recursive 的 child_px/nx 逻辑）。
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let mut scroll_x = 0.0f32;
    let mut scroll_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|&i| {
        abs_x += nodes[i].position.x;
        abs_y += nodes[i].position.y;
        // 当前节点坐标 = 累积位置 − 祖先 scroll 累积（自身 offset 稍后累加）
        let pos = (abs_x - scroll_x, abs_y - scroll_y);
        let (sdx, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[i]);
        scroll_x += sdx;
        scroll_y += sdy;
        pos
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
    use super::{should_request_redraw, PerWindow};
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

    /// Shift + 滚轮：垂直 delta 转为水平，且清除垂直分量
    #[test]
    fn shift_wheel_delta_becomes_horizontal() {
        assert_eq!(super::scroll_delta_with_shift(0.0, -20.0, true), (-20.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(0.0, 20.0, true), (20.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(5.0, 0.0, false), (5.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(0.0, 10.0, false), (0.0, 10.0));
    }

    #[test]
    fn per_window_modifiers_are_independent() {
        let theme = crate::ui::theme::ThemeColors::default_light();
        let mut first = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, theme.clone());
        let mut second = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, theme);
        first.modifiers = winit::keyboard::ModifiersState::default();
        second.modifiers = winit::keyboard::ModifiersState::default();
        assert_eq!(first.modifiers, second.modifiers);
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
        let consumed = super::apply_scroll_delta(
            composer.arena_nodes_mut(),
            root,
            0.0,
            -200.0, // 负 dy = 向下滚动（内容上移——与 winit 滚轮语义一致）
            crate::unit::Density::from_density(1.0),
        );
        assert!(consumed.y != 0.0, "应消费 scroll delta");
        assert!(scroll.offset.get() > 0.0, "滚动后 offset 应 > 0（实际 {}）", scroll.offset.get());
    }
}

/// §3.6 坐标一致性回归：dispatch_ptr_event 传给 handler 的局部坐标必须与
/// hit_test/scene_to_node_local（同一坐标空间）一致——滚动容器内自定义
/// PointerEvent 的拖拽/capture 位置不偏移。
#[cfg(test)]
mod pointer_dispatch_coord_tests {
    use super::dispatch_ptr_event;
    use crate::layout::node::{hit_test, scene_to_node_local, LayoutNode};
    use crate::layout::{Point, Size};
    use crate::modifier::{Modifier, PointerButton, PointerEvent, PointerEventType, PointerKind, ScrollState};

    /// 构造 scroll 容器(0,0,100×200) + 子节点(0,100,100×60，带 PointerEvent handler)。
    /// scroll offset=50 → 子节点视觉顶边 y=50。
    fn build_scrolled_tree() -> (Vec<LayoutNode>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(50.0);
        // 子节点 handler 记录（验证子节点局部坐标扣除祖先 scroll）
        let received = std::sync::Arc::new(std::sync::Mutex::new(None::<(f32, f32)>));
        let recv2 = received.clone();
        // 容器自身 handler 记录（验证自身局部坐标不减自身 offset）
        let container_recv = std::sync::Arc::new(std::sync::Mutex::new(None::<(f32, f32)>));
        let crecv2 = container_recv.clone();
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).size(100.0, 200.0).on_pointer_event(move |ev| {
                *crecv2.lock().unwrap() = Some(ev.position);
                false
            })),
            LayoutNode::leaf(Modifier::new().size(100.0, 60.0).on_pointer_event(move |ev| {
                *recv2.lock().unwrap() = Some(ev.position);
                false
            })),
        ];
        nodes[0].measured_size = Size::new(100.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 60.0);
        nodes[1].position = Point::new(0.0, 100.0);
        nodes[0].children.push(1);
        (nodes, received, container_recv, scroll)
    }

    fn make_event() -> PointerEvent {
        PointerEvent {
            event_type: PointerEventType::Move,
            position: (0.0, 0.0), // dispatch 会覆盖
            scene_position: (0.0, 0.0),
            kind: PointerKind::Mouse { button: PointerButton::Primary },
            is_alt_pressed: false,
            is_ctrl_pressed: false,
            is_shift_pressed: false,
            is_meta_pressed: false,
        }
    }

    #[test]
    fn dispatch_local_coord_matches_scene_to_node_local_in_scroll() {
        let (nodes, received, _container_recv, _scroll) = build_scrolled_tree();
        let root = 0;
        // 场景点 (50,60)：滚动后应命中子节点（视觉顶边 y=50，范围 50..110）
        let path = hit_test(&nodes, root, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "滚动后子节点视觉范围应命中");

        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((expect_x, expect_y), (50.0, 10.0), "基线：scene_to_node_local 须扣除滚动");

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 60.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "dispatch 传给 handler 的局部坐标必须与 hit_test/scene_to_node_local 一致");
    }

    #[test]
    fn dispatch_coord_follows_scroll_offset_change() {
        let (nodes, received, _container_recv, scroll) = build_scrolled_tree();
        let root = 0;
        // offset 100：视觉顶边 y=0，场景 (50,50) → 本地 y=50
        scroll.offset.set(100.0);
        let path = hit_test(&nodes, root, 50.0, 50.0);
        assert_eq!(path, vec![0, 1], "offset=100 时场景 (50,50) 仍命中子节点");
        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 50.0);
        assert_eq!((expect_x, expect_y), (50.0, 50.0));

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 50.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "滚动偏移变化后 dispatch 坐标须同步");
    }

    #[test]
    fn dispatch_scroll_container_own_coord_uses_ancestor_not_self() {
        // scroll 容器自身的局部坐标 = 场景 - 容器位置（自身 offset 不影响自身左上角）
        let (nodes, _received, container_recv, _scroll) = build_scrolled_tree();
        let root = 0;
        let path = vec![0]; // 只命中容器自身（子节点外区域）
        // 场景 (20,30)：容器本地 (20,30)，不应减自身 offset=50
        dispatch_ptr_event(&nodes, root, &path, &make_event(), (20.0, 30.0), None);
        let got = *container_recv.lock().unwrap();
        assert_eq!(got, Some((20.0, 30.0)),
            "scroll 容器自身局部坐标不应减自身 offset（offset 只影响子节点）");
    }
}

/// §3.8 nested scroll 链级集成测试：dispatch_nested_scroll_delta 的 pre/post
/// 顺序与 target 参与。树结构 root(connection R) → mid(connection M) →
/// target(scroll + connection T)。验证：
/// 1. pre 按 R→M→T 正序
/// 2. child 消费后 post 按 M→T→R 逆序（**含 target T**——TopAppBar 等
///    connection 挂在 scroll 容器节点上，排除 target 会破坏 post 回调）
#[cfg(test)]
mod nested_scroll_chain_tests {
    use super::dispatch_nested_scroll_delta;
    use crate::layout::node::LayoutNode;
    use crate::layout::{Point, Size};
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta, ScrollVelocity};

    /// 记录器 connection：记录 on_pre_scroll/on_post_scroll 的调用顺序。
    /// pre 消费一半，post 消费全部 available——便于验证顺序与消费量。
    /// `global_log`（可选）记录**跨节点**顺序（如 "pre-R" "post-T"）——
    /// 独立 log 只能验证单节点内部顺序，无法捕获 pre-R→M→T→post-T→M→R。
    struct Recorder {
        name: String,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        global_log: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
        order: std::sync::atomic::AtomicUsize,
    }
    impl Recorder {
        fn new(name: &str, log: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self { name: name.to_string(), log, global_log: None, order: std::sync::atomic::AtomicUsize::new(0) }
        }
        fn with_global(name: &str, log: std::sync::Arc<std::sync::Mutex<Vec<String>>>, global: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self { name: name.to_string(), log, global_log: Some(global), order: std::sync::atomic::AtomicUsize::new(0) }
        }
    }
    impl NestedScrollConnection for Recorder {
        fn on_pre_scroll(&self, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("pre-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("pre-{}", self.name));
            }
            ScrollDelta::new(available.x / 2.0, available.y / 2.0)
        }
        fn on_post_scroll(&self, _: ScrollDelta, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("post-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("post-{}", self.name));
            }
            ScrollDelta::new(available.x, available.y)
        }
        fn on_pre_fling(&self, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
        fn on_post_fling(&self, _: ScrollVelocity, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
    }

    #[test]
    fn delta_chain_pre_post_order_includes_target() {
        // 构造 root(connection R) → mid(connection M) → target(scroll + connection T)
        let r_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let m_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let t_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        // 共享全局 log：验证跨节点顺序 pre-R→M→T→post-T→M→R
        let global = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

        let conn_r = Recorder::with_global("R", r_log.clone(), global.clone());
        let conn_m = Recorder::with_global("M", m_log.clone(), global.clone());
        let conn_t = Recorder::with_global("T", t_log.clone(), global.clone());

        let scroll = ScrollState::new();
        scroll.offset.set(0.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_r).size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_m).size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).nested_scroll(conn_t).size(100.0, 100.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[1].position = Point::new(0.0, 0.0);
        nodes[2].measured_size = Size::new(100.0, 100.0);
        nodes[2].position = Point::new(0.0, 100.0);
        nodes[2].scroll_viewport_height = 100.0;
        nodes[2].scroll_content_height = 200.0; // 可滚动 100
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let density = crate::unit::Density::from_density(1.0);
        // 消费推演（dy=-20，负 delta = 内容上移 = offset 增加；每个 pre 吃一半）：
        //   R pre -10 → M pre -5 → T pre -2.5 → child 剩余 -2.5 → child 消费 2.5
        //   post 链（含 T）：M post 全吃 available → T post 全吃 → R post 全吃剩余
        let consumed = dispatch_nested_scroll_delta(
            &mut nodes, 0, 2,
            ScrollDelta::new(0.0, -20.0),
            NestedScrollSource::Wheel,
            density,
        );

        // R：pre 一次（吃 10）+ post 一次（child 消费后，吃剩余）——R 是最后 post
        assert_eq!(*r_log.lock().unwrap(), vec!["pre-R-0", "post-R-1"], "R pre 后 post");
        // M：pre 一次 + post 一次（在 T/R 之前——逆序 M→T→R）
        assert_eq!(*m_log.lock().unwrap(), vec!["pre-M-0", "post-M-1"], "M pre 后 post");
        // T：pre + post（TopAppBar 类 connection 挂在 target 上，post 必须被调用——
        // 修复前的关键回归点：排除 target 会破坏 content_offset 变色/回弹）
        assert_eq!(*t_log.lock().unwrap(), vec!["pre-T-0", "post-T-1"], "T 也参与 post（TopAppBar 依赖）");

        // consumed 应为负（负 delta 方向消费），且 child 已实际滚动 offset>0
        assert!(consumed.y < 0.0, "应沿 delta 方向消费，实际 {:?}", consumed);
        assert!(scroll.offset.get() > 0.0, "child 应实际滚动（pre 只吃一半），实际 {}", scroll.offset.get());

        // 跨节点全局顺序：pre 正序 R→M→T，post 逆序 T→M→R（含 target T）
        let g = global.lock().unwrap().clone();
        assert_eq!(g, vec!["pre-R", "pre-M", "pre-T", "post-T", "post-M", "post-R"],
            "全局顺序应为 pre-R→M→T→post-T→M→R（含 target T 参与 post）——实际 {g:?}");
    }
}

/// 拖拽滚动目标复现（用户报告：鼠标在外层内容上按下拖拽，内层却滚动了）。
/// 构造与 nested_scroll_demo 同几何的树：外层 scroll 视口 536（y=184..720），
/// 内层 scroll 视口 180（内容流 y=296，视觉 480..660，scroll_viewport 已设）。
/// 验证：鼠标在外层内容 0-7 区域（如 y=250）按下时，hit_test 命中的是外层
/// 内容节点而非内层——drag_scroll 目标选择（path 逆序找 scroll）应选外层。
#[cfg(test)]
mod drag_target_selection_tests {
    use crate::layout::node::{hit_test, LayoutNode};
    use crate::layout::{Point, Size};
    use crate::modifier::{Modifier, ScrollState};

    /// 构造：root(Column) → outer_scroll(0,184,420×536) → [内容0..7, 内层scroll, 内容8..]
    fn build_demo_tree() -> (Vec<LayoutNode>, usize) {
        let outer = ScrollState::new();
        let inner = ScrollState::new();
        let mut nodes = Vec::new();
        // idx 0: root Column（全窗口 420×720）
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 720.0)));
        nodes[0].measured_size = Size::new(420.0, 720.0);
        // idx 1: 外层 scroll（视口 536，顶部 y=184）
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(outer).size(420.0, 536.0)));
        nodes[1].position = Point::new(0.0, 184.0);
        nodes[1].measured_size = Size::new(420.0, 536.0);
        nodes[1].scroll_viewport_height = 536.0;
        nodes[0].children.push(1); // 外层 scroll 挂到 root
        // 内容 0..7：每行 37 高
        for i in 0..8 {
            let idx = nodes.len();
            nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 37.0)));
            nodes[idx].position = Point::new(0.0, i as f32 * 37.0);
            nodes[idx].measured_size = Size::new(420.0, 37.0);
            nodes[1].children.push(idx);
        }
        // 内层 scroll：内容流 y = 8*37 = 296，视口 180
        let inner_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(inner).size(420.0, 180.0)));
        nodes[inner_idx].position = Point::new(0.0, 296.0);
        nodes[inner_idx].measured_size = Size::new(420.0, 180.0);
        nodes[inner_idx].scroll_viewport_height = 180.0;
        // 内层子项：一列（可滚动内容）
        let item_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 32.0)));
        nodes[item_idx].position = Point::new(0.0, 0.0);
        nodes[item_idx].measured_size = Size::new(420.0, 32.0);
        nodes[inner_idx].children.push(item_idx);
        nodes[1].children.push(inner_idx);
        (nodes, 1) // 返回 outer_scroll idx
    }

    #[test]
    fn drag_target_on_outer_content_is_outer_not_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // 鼠标在外层内容区域：屏幕 y=250（外层顶部 184 + 内容 y=66 → 内容 1）
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        assert!(!path.is_empty(), "应命中某节点");
        // 命中路径应包含外层 scroll，且**不含内层 scroll**
        assert!(path.contains(&outer_idx), "外层 scroll 应在命中路径");
        // 内层 scroll 视觉位置 y=480..660——y=250 不应命中
        let inner_idx = nodes[outer_idx].children[8]; // 内容 0..7 之后是内层
        assert!(!path.contains(&inner_idx), "鼠标在外层内容区域不应命中内层 scroll");
        // drag_scroll 目标选择：path 逆序找第一个 scroll → 应为外层
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&outer_idx), "drag 目标应是外层 scroll");
    }

    #[test]
    fn drag_target_on_inner_list_is_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // 鼠标在内层列表视觉区域：屏幕 y=500（内层 480..660）
        let path = hit_test(&nodes, 0, 210.0, 500.0);
        let inner_idx = nodes[outer_idx].children[8];
        assert!(path.contains(&inner_idx), "内层 scroll 应在命中路径");
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "drag 目标应是内层 scroll");
    }

    /// 用户复现场景：外层已滚动（offset>0），内层列表视觉上移到外层内容
    /// 0-7 区域。此时鼠标在外层顶部内容区域按下——期望滚外层（鼠标视觉
    /// 在"外层内容"上），但若命中内层则 bug。
    #[test]
    fn drag_target_after_outer_scrolled_uses_visual_position() {
        let (mut nodes, outer_idx) = build_demo_tree();
        // 外层滚 offset=300：内层视觉 y = 184 + 296 - 300 = 180（上移到顶部区域）
        let outer = nodes[outer_idx].modifier.vertical_scroll_state().unwrap().clone();
        outer.offset.set(300.0);
        let inner_idx = nodes[outer_idx].children[8];
        // 内层视觉位置（渲染）应在 180..360——覆盖"外层内容 0-7"区域
        let (_, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[outer_idx]);
        let inner_visual_y = 184.0 + nodes[inner_idx].position.y - sdy;
        assert!(inner_visual_y < 400.0, "内层应上移到顶部区域（视觉 y={}）", inner_visual_y);

        // 情形 1：鼠标在 y=250（内层视觉覆盖区）——应命中内层（z-order）
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        let contains_inner = path.contains(&inner_idx);
        assert!(contains_inner,
            "内层视觉覆盖 y=250（视觉 y={inner_visual_y}..{:.0}）应命中内层——z-order 语义",
            inner_visual_y + 180.0);
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "drag 目标应是内层（视觉覆盖区）");

        // 情形 2：鼠标在 y=440（内层视觉区下方，仍在外层视口内）——应命中
        // 外层内容（内层不覆盖该点）
        let path2 = hit_test(&nodes, 0, 210.0, 440.0);
        assert!(!path2.contains(&inner_idx),
            "y=440 在内层视觉区（{inner_visual_y}..{:.0}）下方，不应命中内层",
            inner_visual_y + 180.0);
        let target2 = path2.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target2, Some(&outer_idx), "drag 目标应是外层（内层不覆盖处）");
    }

    /// 验证：真实 Composer 两次 compose（状态行文本变化触发重组）后，
    /// 外层/内层 scroll 容器的 slot_key 保持稳定（drag_scroll 依赖 slot_key
    /// 在 move 阶段定位节点——key 漂移会滚错目标）。
    #[test]
    fn scroll_slot_keys_stable_across_recompose() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::{Column, Text};
        let mut composer = Composer::new();
        let outer = ScrollState::new();
        let inner = ScrollState::new();

        let build = |composer: &mut Composer, outer: &ScrollState, inner: &ScrollState| {
            composer.compose(|ctx| {
                crate::ui::Column::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        // 状态行（文本随 offset 变化 → 触发重组）
                        Text::new(format!("outer: {:.0} inner: {:.0}", outer.offset.get(), inner.offset.get()))
                            .font_size(12.0).build(ctx);
                        // 外层滚动区
                        Column::new()
                            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(outer.clone()))
                            .build(ctx, |ctx| {
                                for i in 0..8 {
                                    Text::new(format!("页面内容 {i}"))
                                        .modifier(Modifier::new().padding(10.0).fill_max_width())
                                        .build(ctx);
                                }
                                // 内层列表
                                Column::new()
                                    .modifier(Modifier::new().fill_max_width().height(180.0).vertical_scroll(inner.clone()))
                                    .build(ctx, |ctx| {
                                        for i in 0..30 {
                                            Text::new(format!("内层列表项 {i}"))
                                                .modifier(Modifier::new().padding(8.0).fill_max_width())
                                                .build(ctx);
                                        }
                                    });
                            });
                    });
            });
        };

        // 帧 1
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let collect_keys = |composer: &Composer| -> Vec<u64> {
            composer.arena_nodes().iter()
                .filter(|n| n.modifier.vertical_scroll_state().is_some())
                .map(|n| n.slot_key)
                .collect()
        };
        let keys1 = collect_keys(&composer);
        assert_eq!(keys1.len(), 2, "应有外层+内层两个 scroll 节点");
        // ⚠ 关键：两个 scroll 节点的 slot_key 必须**互不相同**——若碰撞，
        // drag_scroll move 阶段 find_node_id_by_slot_key 会返回第一个匹配
        // （可能滚错目标：按下外层、move 滚内层——用户报告的 bug）
        assert_ne!(keys1[0], keys1[1],
            "外层与内层 scroll 的 slot_key 碰撞！{keys1:?}——drag_scroll 会定位到错误节点");
        eprintln!("[keys] 帧1 scroll keys = {keys1:?}（互不相同 ✅）");

        // 帧 2：内层滚动（状态行文本变化 → 重组）
        inner.offset.set(29.0);
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let keys2 = collect_keys(&composer);
        assert_eq!(keys1, keys2,
            "scroll 容器 slot_key 跨重组必须稳定（drag_scroll move 阶段依赖）——\n帧1={keys1:?} 帧2={keys2:?}");
    }

    /// scroll 容器高度 clamp 回归（review B1）：`.height(180)` + padding 的
    /// 滚动容器，measured_size 必须精确为 180（内容再多也不撑开）。
    #[test]
    fn scroll_container_height_clamps_to_fixed_height() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::{Column, Text};
        let mut composer = Composer::new();
        let inner = ScrollState::new();
        composer.compose(|ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().height(180.0).padding(4.0).vertical_scroll(inner.clone()))
                .build(ctx, |ctx| {
                    // 30 项内容——总高远超 180
                    for i in 0..30 {
                        Text::new(format!("列表项 {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert_eq!(n.measured_size.height, 180.0,
            "scroll 容器高度必须 clamp 回 .height(180)，实际 {}", n.measured_size.height);
        assert!(n.scroll_viewport_height > 0.0, "viewport 应已回写（供滚动 clamp）");
    }

    /// scroll 容器无固定高度 + 无限父约束：随内容撑开（clamp 是 no-op）。
    #[test]
    fn scroll_container_grows_with_unbounded_parent() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::{Column, Text};
        let mut composer = Composer::new();
        let inner = ScrollState::new();
        composer.compose(|ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().vertical_scroll(inner.clone()))
                .build(ctx, |ctx| {
                    for i in 0..5 {
                        Text::new(format!("列表项 {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        // 父约束高度无限（f32::MAX）——容器应随内容撑开
        composer.layout(Constraints::new(0.0, 420.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert!(n.measured_size.height > 0.0,
            "无固定高度 + 无限约束应随内容撑开（>0），实际 {}", n.measured_size.height);
    }
}

pub fn run_app(app: impl FnOnce(&mut ComposeCtx) + 'static) {
    let event_loop = EventLoop::new().expect("event loop");
    debug::begin_session();
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
    };
    event_loop.run_app(state).expect("run_app");
    debug::end_session();
}
