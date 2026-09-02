//! DevTools — stdin + WebSocket 双通道调试
//!
//! stdin:  echo 'c 190 130' | ./app   # 点击
//!         echo t | ./app              # 打印 UI 树
//!         echo r | ./app              # 截图请求
//! WebSocket: ws://127.0.0.1:9998（可用环境变量 WINIA_DEBUG_PORT 覆盖——UI 测试并行隔离）
//!         wscat -c ws://localhost:9998 → 输入 c 190 130

use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use std::net::TcpStream;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

// ── 全局状态 ──

static DEBUG_STATE: Mutex<Option<DebugData>> = Mutex::new(None);
/// WindowId used for legacy stdin/WebSocket requests with target 0.
static LEGACY_TARGET: Mutex<Option<u64>> = Mutex::new(None);
static SCREENSHOT_TARGET: Mutex<Option<u64>> = Mutex::new(None);

struct DebugRuntime {
    wake_callback: Option<Arc<dyn Fn() + Send + Sync>>,
    event_loop_proxy: Option<winit::event_loop::EventLoopProxy>,
    shutdown: bool,
}

static DEBUG_RUNTIME: LazyLock<Mutex<DebugRuntime>> = LazyLock::new(|| {
    Mutex::new(DebugRuntime {
        wake_callback: None,
        event_loop_proxy: None,
        shutdown: false,
    })
});

/// Start one application debug session and discard state left by an older run.
pub fn begin_session() {
    {
        let mut runtime = DEBUG_RUNTIME.lock().unwrap();
        runtime.wake_callback = None;
        runtime.event_loop_proxy = None;
        runtime.shutdown = false;
    }
    *LEGACY_TARGET.lock().unwrap() = None;
    *SCREENSHOT_TARGET.lock().unwrap() = None;
    QUEUED_EVENTS.lock().unwrap().clear();
    *DEBUG_STATE.lock().unwrap() = None;
}

/// Stop the current debug session and release its event-loop hooks.
pub fn end_session() {
    {
        DEBUG_RUNTIME.lock().unwrap().shutdown = true;
    }
    wake();
    let mut runtime = DEBUG_RUNTIME.lock().unwrap();
    runtime.wake_callback = None;
    runtime.event_loop_proxy = None;
}

pub fn force_shutdown() {
    DEBUG_RUNTIME.lock().unwrap().shutdown = true;
    let _ = TcpStream::connect("127.0.0.1:9998");
    wake();
}

pub fn is_shutdown() -> bool { DEBUG_RUNTIME.lock().unwrap().shutdown }

/// Bind legacy stdin/WebSocket requests to the parent window once it exists.
pub fn set_legacy_target(window_id: u64) {
    *LEGACY_TARGET.lock().unwrap() = Some(window_id);
    let mut screenshot = SCREENSHOT_TARGET.lock().unwrap();
    if *screenshot == Some(0) {
        *screenshot = Some(window_id);
    }
}

pub fn legacy_target() -> Option<u64> {
    *LEGACY_TARGET.lock().unwrap()
}

fn target_matches(target: u64, window_id: u64, legacy: Option<u64>) -> bool {
    if target == 0 {
        legacy == Some(window_id)
    } else {
        target == window_id
    }
}

pub fn request_screenshot(window_id: u64) {
    let target = if window_id == 0 { legacy_target().unwrap_or(0) } else { window_id };
    *SCREENSHOT_TARGET.lock().unwrap() = Some(target);
}
pub fn screenshot_requested(window_id: u64) -> bool {
    let legacy = legacy_target();
    let target = *SCREENSHOT_TARGET.lock().unwrap();
    target.is_some_and(|target| target_matches(target, window_id, legacy))
}
pub fn screenshot_done(window_id: u64) {
    let legacy = legacy_target();
    let target = *SCREENSHOT_TARGET.lock().unwrap();
    if target.is_some_and(|target| target_matches(target, window_id, legacy)) {
        *SCREENSHOT_TARGET.lock().unwrap() = None;
    }
}

pub fn has_pending() -> bool {
    SCREENSHOT_TARGET.lock().unwrap().is_some() || !QUEUED_EVENTS.lock().unwrap().is_empty()
}

pub fn set_wake_callback(cb: impl Fn() + Send + Sync + 'static) {
    DEBUG_RUNTIME.lock().unwrap().wake_callback = Some(Arc::new(cb));
}

pub fn set_event_loop_proxy(proxy: winit::event_loop::EventLoopProxy) {
    DEBUG_RUNTIME.lock().unwrap().event_loop_proxy = Some(proxy);
}

pub fn wake() {
    let (proxy, callback) = {
        let runtime = DEBUG_RUNTIME.lock().unwrap();
        (runtime.event_loop_proxy.clone(), runtime.wake_callback.clone())
    };
    if let Some(proxy) = proxy {
        let _ = proxy.wake_up();
    } else if let Some(callback) = callback {
        callback();
    }
}

struct PixelFrame {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

struct DebugData {
    pixel_frames: std::collections::HashMap<u64, PixelFrame>,
    /// 每个窗口的树 JSON（单行、合法 JSON）——window_id → 树根数组
    trees: std::collections::HashMap<u64, String>,
    /// 每个窗口的顶层弹出层树（overlay 独立 Composer 的 arena）——
    /// window_id → [(overlay_id, JSON)]，按 z 序（栈序）排列；
    /// 每帧整体替换（overlay 关闭后条目自动消失，无残留）
    overlay_trees: std::collections::HashMap<u64, Vec<(u64, String)>>,
}

pub fn update_pixels(window_id: u64, pixels: &[u8], width: u32, height: u32) {
    let mut data = DEBUG_STATE.lock().unwrap();
    let state = data.get_or_insert_with(|| DebugData {
        pixel_frames: Default::default(),
        trees: Default::default(),
        overlay_trees: Default::default(),
    });
    state.pixel_frames.insert(window_id, PixelFrame {
        pixels: pixels.to_vec(),
        width,
        height,
    });
}

fn pixel_frame(window_id: u64) -> Option<(u32, u32, Vec<u8>)> {
    let data = DEBUG_STATE.lock().ok()?;
    let frame = data.as_ref()?.pixel_frames.get(&window_id)?;
    Some((frame.width, frame.height, frame.pixels.clone()))
}

#[cfg(test)]
fn reset_debug_requests() {
    *LEGACY_TARGET.lock().unwrap() = None;
    *SCREENSHOT_TARGET.lock().unwrap() = None;
    QUEUED_EVENTS.lock().unwrap().clear();
    *DEBUG_STATE.lock().unwrap() = None;
    reset_debug_runtime();
}

#[cfg(test)]
fn reset_debug_runtime() {
    let mut runtime = DEBUG_RUNTIME.lock().unwrap();
    runtime.wake_callback = None;
    runtime.event_loop_proxy = None;
    runtime.shutdown = false;
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn debug_session_resets_shutdown_and_requests() {
        reset_debug_requests();
        force_shutdown();
        assert!(is_shutdown());
        request_screenshot(22);
        begin_session();
        assert!(!is_shutdown());
        assert!(!screenshot_requested(22));
        assert!(take_queued_events(22).is_empty());
    }

    #[test]
    fn screenshot_target_is_consumed_only_by_matching_window() {
        reset_debug_requests();
        request_screenshot(22);
        assert!(!screenshot_requested(11));
        assert!(screenshot_requested(22));
        screenshot_done(11);
        assert!(screenshot_requested(22));
        screenshot_done(22);
        assert!(!screenshot_requested(22));
    }

    #[test]
    fn debug_events_remain_queued_for_their_target_window() {
        reset_debug_requests();
        queue_event_for_window(22, DebugEvent::FocusNext);
        queue_event_for_window(11, DebugEvent::Click { x: 1.0, y: 2.0 });
        assert!(matches!(take_queued_events(11).as_slice(), [DebugEvent::Click { .. }]));
        assert!(matches!(take_queued_events(22).as_slice(), [DebugEvent::FocusNext]));
    }

    #[test]
    fn legacy_target_zero_resolves_to_parent_window() {
        reset_debug_requests();
        set_legacy_target(22);
        queue_event(DebugEvent::FocusNext);
        assert!(take_queued_events(11).is_empty());
        assert!(matches!(take_queued_events(22).as_slice(), [DebugEvent::FocusNext]));
        request_screenshot(0);
        assert!(!screenshot_requested(11));
        assert!(screenshot_requested(22));
    }

    #[test]
    fn queued_event_targets_reports_matching_windows() {
        reset_debug_requests();
        assert!(queued_event_targets().is_empty());
        set_legacy_target(22);
        queue_event(DebugEvent::FocusNext);
        assert_eq!(queued_event_targets(), std::collections::HashSet::from([22]));
        queue_event_for_window(33, DebugEvent::Click { x: 0.0, y: 0.0 });
        assert_eq!(queued_event_targets(), std::collections::HashSet::from([22, 33]));
    }

    #[test]
    fn pixel_frames_are_owned_by_window() {
        reset_debug_requests();
        update_pixels(11, &[1, 2, 3, 4], 1, 1);
        update_pixels(22, &[5, 6, 7, 8], 2, 1);
        assert_eq!(pixel_frame(11), Some((1, 1, vec![1, 2, 3, 4])));
        assert_eq!(pixel_frame(22), Some((2, 1, vec![5, 6, 7, 8])));
    }
}

/// 更新指定窗口的树 JSON（多窗口：各窗口独立存储——不再互相覆盖）
pub fn update_tree(window_id: u64, json: &str) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        *data = Some(DebugData { pixel_frames: Default::default(), trees: Default::default(), overlay_trees: Default::default() });
    }
    if let Some(ref mut d) = *data {
        d.trees.insert(window_id, json.to_string());
    }
}

/// 整体替换指定窗口的弹出层树（每帧调用；空 vec = 无弹出层——清除残留）。
/// trees 元素 = (overlay_id, 树 JSON)，按渲染 z 序排列。
pub fn set_overlay_trees(window_id: u64, trees: Vec<(u64, String)>) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        if trees.is_empty() { return; }
        *data = Some(DebugData { pixel_frames: Default::default(), trees: Default::default(), overlay_trees: Default::default() });
    }
    if let Some(ref mut d) = *data {
        if trees.is_empty() {
            d.overlay_trees.remove(&window_id);
        } else {
            d.overlay_trees.insert(window_id, trees);
        }
    }
}

/// 移除指定窗口的树 JSON（窗口关闭时清理——避免残留）
pub fn remove_tree(window_id: u64) {
    if let Some(ref mut d) = *DEBUG_STATE.lock().unwrap() {
        d.trees.remove(&window_id);
        d.overlay_trees.remove(&window_id);
        d.pixel_frames.remove(&window_id);
    }
}

/// 全部窗口树 → 多窗口 JSON：主窗口条目 + 紧随其后的弹出层条目——
///
/// ```json
/// [{"window":0,"root":[...]},
///  {"window":0,"overlay":0,"id":7,"root":[...]}]
/// ```
///
/// - 主条目无 "overlay" 字段（既有解析兼容——零弹窗时输出与旧版完全一致）
/// - 弹出层条目：`overlay` = z 序索引（0 最底）、`id` = OverlayDesc 稳定 id
/// - 按 window id 排序；弹层跟随其宿主窗口
fn all_trees_json() -> String {
    let data = DEBUG_STATE.lock().unwrap();
    let Some(d) = data.as_ref() else { return "[]".to_string() };
    let mut entries: Vec<(u64, &String)> = d.trees.iter().map(|(id, j)| (*id, j)).collect();
    entries.sort_by_key(|(id, _)| *id);
    let mut out = String::from("[");
    let mut first = true;
    for (id, json) in entries {
        if !first { out.push(','); }
        first = false;
        out.push_str(&format!(r#"{{"window":{id},"root":{json}}}"#));
        if let Some(ovs) = d.overlay_trees.get(&id) {
            for (i, (oid, oj)) in ovs.iter().enumerate() {
                out.push(',');
                out.push_str(&format!(r#"{{"window":{id},"overlay":{i},"id":{oid},"root":{oj}}}"#));
            }
        }
    }
    out.push(']');
    out
}

// ── 事件队列 ──

static QUEUED_EVENTS: Mutex<Vec<(u64, DebugEvent)>> = Mutex::new(Vec::new());

#[derive(Debug, Clone)]
pub enum DebugEvent {
    Click { x: f32, y: f32 },
    Key { key: String },
    Text { value: String },
    Scroll { dx: f32, dy: f32 },
    Resize { w: f32, h: f32 },
    FocusNext,
    RequestFocus { id: u64 },
    /// 模拟指针按下（选择拖动的起点）
    PointerDown { x: f32, y: f32 },
    /// 模拟指针移动（拖动选择）
    PointerMove { x: f32, y: f32 },
    /// 模拟指针释放
    PointerUp { x: f32, y: f32 },
}

pub fn queue_event(event: DebugEvent) { queue_event_for_window(0, event); }
pub fn queue_event_for_window(window_id: u64, event: DebugEvent) {
    QUEUED_EVENTS.lock().unwrap().push((window_id, event));
    wake();
}
pub fn take_queued_events(window_id: u64) -> Vec<DebugEvent> {
    let legacy = legacy_target();
    let mut queue = QUEUED_EVENTS.lock().unwrap();
    let mut drained = Vec::new();
    let mut kept = Vec::new();
    for (target, event) in queue.drain(..) {
        let matches = if target == 0 {
            legacy == Some(window_id)
        } else {
            target == window_id
        };
        if matches {
            drained.push(event);
        } else {
            kept.push((target, event));
        }
    }
    *queue = kept;
    drained
}

/// Set of WindowIds that currently have queued debug events. An empty set
/// means no window needs to consume debug events this frame.
pub fn queued_event_targets() -> std::collections::HashSet<u64> {
    let legacy = legacy_target();
    let queue = QUEUED_EVENTS.lock().unwrap();
    let mut targets = std::collections::HashSet::new();
    for (target, _) in queue.iter() {
        if *target == 0 {
            if let Some(parent) = legacy {
                targets.insert(parent);
            }
        } else {
            targets.insert(*target);
        }
    }
    targets
}

pub fn simulate_native_click(x: f32, y: f32) {
    queue_event(DebugEvent::Click { x, y });
    wake();
}

// ── 组件树 JSON ──

pub fn build_tree_json(nodes: &[LayoutNode], root_idx: usize) -> String {
    let mut out = String::from("[");
    build_node_json(nodes, root_idx, &mut out, 0);
    out.push(']');
    out
}

fn build_node_json(nodes: &[LayoutNode], idx: usize, out: &mut String, depth: usize) {
    let node = &nodes[idx];
    let indent = "  ".repeat(depth + 1);
    let mod_desc = describe_modifier(&node.modifier);
    // 非有限数（NaN/Inf——未初始化的测量值）格式化为 0——保证 JSON 合法
    // （serde_json 拒绝 NaN——UI 测试解析树会失败）
    let pos_x = if node.position.x.is_finite() { node.position.x } else { 0.0 };
    let pos_y = if node.position.y.is_finite() { node.position.y } else { 0.0 };
    let size_w = if node.measured_size.width.is_finite() { node.measured_size.width } else { 0.0 };
    let size_h = if node.measured_size.height.is_finite() { node.measured_size.height } else { 0.0 };
    out.push_str(&format!(
        r#"{indent}{{"pos":[{pos_x:.0},{pos_y:.0}],"size":[{size_w:.0},{size_h:.0}],"mod":"{}","tag":{},"focused":{},"children":["#,
        mod_desc,
        node.modifier.get_test_tag().map(|t| format!("\"{}\"", t)).unwrap_or_else(|| "null".into()),
        node.focused,
    ));
    for (i, &child) in node.children.iter().enumerate() {
        if i > 0 { out.push_str(","); } // 紧凑单行（无换行——println 走 stdout 管道不拆行）
        build_node_json(nodes, child, out, depth + 1);
    }
    out.push(']'); out.push('}');
}

fn describe_modifier(modifier: &crate::modifier::Modifier) -> String {
    modifier.elements().iter().filter_map(|el| match el {
        ModifierElement::Size { width, height } => Some(format!("size({:?},{:?})", width, height)),
        ModifierElement::Background { color_fn, .. } => Some("bg(<dynamic>)".into()),
        ModifierElement::Clickable { .. } => Some("click".into()),
        ModifierElement::Focusable { .. } => Some("focus".into()),
        ModifierElement::Hoverable { .. } => Some("hover".into()),
        ModifierElement::Ripple { .. } => Some("ripple".into()),
        ModifierElement::TextContent { content, .. } => Some(format!("text({})",
            // 完整转义（JSON 字符串——\t/\r/\b/\f 等控制字符不转义会生成非法 JSON，
            // serde_json 解析失败 → 测试表现为超时难排查）
            content.replace('\\', "\\\\").replace('"', "\\\"")
                .replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t")
                .replace('\u{8}', "\\b").replace('\u{c}', "\\f"))),
        ModifierElement::PaddingSides { start, top, end, bottom } => {
            use crate::modifier::{Dimension, SizeValue};
            // 四边求值（Debug 场景：显示累积/动态标记）
            let sv = |v: &SizeValue| match v {
                SizeValue::Static(Dimension::Fixed(x)) | SizeValue::Static(Dimension::Dp(crate::unit::Dp(x))) => format!("{x}"),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => "0".to_string(),
                SizeValue::Static(Dimension::Px(p)) => format!("{:.0}", p.to_logical(crate::unit::current_density())),
                SizeValue::Dynamic(_) => "<dyn>".to_string(),
            };
            Some(format!(
                "pad({},{},{},{})",
                sv(start), sv(top), sv(end), sv(bottom)
            ))
        }
        _ => None,
    }).collect::<Vec<_>>().join("|")
}

// ═══════════════════════════════════════════════════════════
// stdin 通道
// ═══════════════════════════════════════════════════════════

pub fn start_stdin_channel() {
    use std::io::{self, BufRead};
    thread::spawn(|| {
        let stdin = io::stdin();
        eprintln!("[DevTools] stdin ready — try: echo 'c 190 130'");
        for line in stdin.lock().lines() {
            if is_shutdown() { break; }
            let line = match line { Ok(l) => l, Err(_) => break };
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() { continue; }
            match parts[0] {
                "c" if parts.len() >= 3 => {
                    let x: f32 = parts[1].parse().unwrap_or(0.0);
                    let y: f32 = parts[2].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::Click { x, y });
                }
                "d" if parts.len() >= 3 => {
                    let x: f32 = parts[1].parse().unwrap_or(0.0);
                    let y: f32 = parts[2].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::PointerDown { x, y });
                }
                "m" if parts.len() >= 3 => {
                    let x: f32 = parts[1].parse().unwrap_or(0.0);
                    let y: f32 = parts[2].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::PointerMove { x, y });
                }
                "u" if parts.len() >= 3 => {
                    let x: f32 = parts[1].parse().unwrap_or(0.0);
                    let y: f32 = parts[2].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::PointerUp { x, y });
                }
                "k" if parts.len() >= 2 => queue_event(DebugEvent::Key { key: parts[1..].join(" ") }),
                "s" if parts.len() >= 2 => {
                    // s dy（单参）或 s dx dy（双参——横向滚轮/测试）
                    let (dx, dy) = if parts.len() >= 3 {
                        (parts[1].parse().unwrap_or(0.0), parts[2].parse().unwrap_or(0.0))
                    } else {
                        (0.0, parts[1].parse().unwrap_or(0.0))
                    };
                    queue_event(DebugEvent::Scroll { dx, dy });
                }
                // w <width> <height>：模拟窗口 resize（逻辑像素——驱动自适应组件）
                "w" if parts.len() >= 3 => {
                    let w: f32 = parts[1].parse().unwrap_or(0.0);
                    let h: f32 = parts[2].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::Resize { w, h });
                }
                // Screenshot target is selected by the app's parent window.
                "r" => { request_screenshot(0); wake(); }
                "t" => {
                    // 树响应走 stdout（前缀 TREE:——UI 测试读管道；其他 demo
                    // 输出可能污染 stdout——测试按前缀过滤）。无条件响应
                    // （DEBUG_STATE 未填充时输出空——测试可区分 stdin 链路 vs 渲染时序）
                    println!("TREE:{}", all_trees_json());
                }
                "swipe" if parts.len() >= 5 => {
                    // swipe x1 y1 x2 y2 [steps] [delay_ms] — stdin 同步版（无延迟，全部入队）
                    let x1: f32 = parts[1].parse().unwrap_or(0.0);
                    let y1: f32 = parts[2].parse().unwrap_or(0.0);
                    let x2: f32 = parts[3].parse().unwrap_or(0.0);
                    let y2: f32 = parts[4].parse().unwrap_or(0.0);
                    let steps: usize = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(10);
                    queue_event(DebugEvent::PointerDown { x: x1, y: y1 });
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let x = x1 + (x2 - x1) * t;
                        let y = y1 + (y2 - y1) * t;
                        queue_event(DebugEvent::PointerMove { x, y });
                    }
                    queue_event(DebugEvent::PointerUp { x: x2, y: y2 });
                    wake();
                }
                "q" => { force_shutdown(); break; }
                _ => {}
            }
        }
    });
}

// ═══════════════════════════════════════════════════════════
// WebSocket 通道
// ═══════════════════════════════════════════════════════════

pub fn start_ws_server() {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        eprintln!("[DevTools] No tokio runtime — WebSocket disabled, stdin still works");
        return;
    };
    handle.spawn(async move {
        // 端口参数化（环境变量 WINIA_DEBUG_PORT）——UI 测试并行隔离；默认 9998
        let port: u16 = std::env::var("WINIA_DEBUG_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9998);
        let addr = format!("127.0.0.1:{port}");
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => { eprintln!("[DevTools] ws bind failed: {e}"); return; }
        };
        eprintln!("[DevTools] WebSocket → ws://localhost:{port}");
        while let Ok((stream, _)) = listener.accept().await {
            if is_shutdown() { break; }
            tokio::spawn(handle_ws(stream));
        }
    });
}

async fn handle_ws(stream: tokio::net::TcpStream) {
    use tokio_tungstenite::tungstenite::protocol::Message;
    use futures_util::{SinkExt, StreamExt};
    let ws = match tokio_tungstenite::accept_async(stream).await { Ok(w) => w, Err(_) => return };
    let (mut write, mut read) = ws.split();
    while let Some(msg) = read.next().await {
        if is_shutdown() { break; }
        let text = match msg { Ok(Message::Text(t)) => t.to_string(), _ => continue };
        let parts: Vec<&str> = text.split_whitespace().collect();
        if parts.is_empty() { continue; }
        match parts[0] {
            "c" if parts.len() >= 3 => {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                queue_event(DebugEvent::Click { x, y });
                let _ = write.send(Message::Text("ok click".into())).await;
            }
            "d" if parts.len() >= 3 => {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                queue_event(DebugEvent::PointerDown { x, y });
                let _ = write.send(Message::Text("ok down".into())).await;
            }
            "m" if parts.len() >= 3 => {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                queue_event(DebugEvent::PointerMove { x, y });
                let _ = write.send(Message::Text("ok move".into())).await;
            }
            "u" if parts.len() >= 3 => {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                queue_event(DebugEvent::PointerUp { x, y });
                let _ = write.send(Message::Text("ok up".into())).await;
            }
            "k" if parts.len() >= 2 => {
                queue_event(DebugEvent::Key { key: parts[1..].join(" ") });
                let _ = write.send(Message::Text("ok key".into())).await;
            }
            "s" if parts.len() >= 2 => {
                let (dx, dy) = if parts.len() >= 3 {
                    (parts[1].parse().unwrap_or(0.0), parts[2].parse().unwrap_or(0.0))
                } else {
                    (0.0, parts[1].parse().unwrap_or(0.0))
                };
                queue_event(DebugEvent::Scroll { dx, dy });
                let _ = write.send(Message::Text("ok scroll".into())).await;
            }
            // w <width> <height>：模拟窗口 resize（逻辑像素——驱动自适应组件）
            "w" if parts.len() >= 3 => {
                let w: f32 = parts[1].parse().unwrap_or(0.0);
                let h: f32 = parts[2].parse().unwrap_or(0.0);
                queue_event(DebugEvent::Resize { w, h });
                let _ = write.send(Message::Text("ok resize".into())).await;
            }
            "r" => {
                request_screenshot(0); wake();
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                let s = legacy_target()
                    .and_then(pixel_frame)
                    .map(|(w, h, _)| format!("screenshot {w}x{h}"))
                    .unwrap_or_else(|| "no frame".into());
                let _ = write.send(Message::text(s)).await;
            }
            "t" => {
                let _ = write.send(Message::text(all_trees_json())).await;
            }
            "p" => {
                // 像素转储（调试截图分析）：二进制帧 = 8 字节 header(WxH u32 LE) + RGBA
                let shot = legacy_target().and_then(pixel_frame);
                match shot {
                    Some((w, h, pixels)) => {
                        let mut raw = Vec::with_capacity(8 + pixels.len());
                        raw.extend_from_slice(&w.to_le_bytes());
                        raw.extend_from_slice(&h.to_le_bytes());
                        raw.extend_from_slice(&pixels);
                        let _ = write.send(Message::Binary(raw.into())).await;
                    }
                    None => { let _ = write.send(Message::text("no frame".to_string())).await; }
                }
            }
            "swipe" if parts.len() >= 5 => {
                // swipe x1 y1 x2 y2 [steps] [delay_ms] — 模拟拖拽（down → moves → up）
                // 例：swipe 240 480 240 560 10 16
                let x1: f32 = parts[1].parse().unwrap_or(0.0);
                let y1: f32 = parts[2].parse().unwrap_or(0.0);
                let x2: f32 = parts[3].parse().unwrap_or(0.0);
                let y2: f32 = parts[4].parse().unwrap_or(0.0);
                let steps: usize = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(10);
                let delay: u64 = parts.get(6).and_then(|s| s.parse().ok()).unwrap_or(16);
                queue_event(DebugEvent::PointerDown { x: x1, y: y1 });
                let _ = write.send(Message::Text("ok swipe down".into())).await;
                wake();
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let x = x1 + (x2 - x1) * t;
                    let y = y1 + (y2 - y1) * t;
                    queue_event(DebugEvent::PointerMove { x, y });
                    wake();
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
                queue_event(DebugEvent::PointerUp { x: x2, y: y2 });
                let _ = write.send(Message::Text("ok swipe up".into())).await;
                wake();
            }
            _ => { let _ = write.send(Message::Text("?".into())).await; }
        }
    }
}
