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
use std::sync::Mutex;
use std::thread;

// ── 全局状态 ──

static DEBUG_STATE: Mutex<Option<DebugData>> = Mutex::new(None);
static SCREENSHOT_FLAG: Mutex<bool> = Mutex::new(false);
static WAKE_CALLBACK: Mutex<Option<Box<dyn Fn() + Send + Sync>>> = Mutex::new(None);
static EVENT_LOOP_PROXY: Mutex<Option<winit::event_loop::EventLoopProxy>> = Mutex::new(None);
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn force_shutdown() {
    SHUTDOWN.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect("127.0.0.1:9998");
    wake();
}

pub fn is_shutdown() -> bool { SHUTDOWN.load(Ordering::SeqCst) }

pub fn request_screenshot() { *SCREENSHOT_FLAG.lock().unwrap() = true; }
pub fn screenshot_requested() -> bool { *SCREENSHOT_FLAG.lock().unwrap() }
pub fn screenshot_done() { *SCREENSHOT_FLAG.lock().unwrap() = false; }

pub fn has_pending() -> bool {
    *SCREENSHOT_FLAG.lock().unwrap() || !QUEUED_EVENTS.lock().unwrap().is_empty()
}

pub fn set_wake_callback(cb: impl Fn() + Send + Sync + 'static) {
    *WAKE_CALLBACK.lock().unwrap() = Some(Box::new(cb));
}

pub fn set_event_loop_proxy(proxy: winit::event_loop::EventLoopProxy) {
    *EVENT_LOOP_PROXY.lock().unwrap() = Some(proxy);
}

pub fn wake() {
    if let Some(ref proxy) = *EVENT_LOOP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); return; }
    if let Some(ref cb) = *WAKE_CALLBACK.lock().unwrap() { cb(); }
}

struct DebugData {
    pixels: Vec<u8>, width: u32, height: u32,
    /// 每个窗口的树 JSON（单行、合法 JSON）——window_id → 树根数组
    trees: std::collections::HashMap<u64, String>,
}

pub fn update_pixels(pixels: &[u8], width: u32, height: u32) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if let Some(ref mut d) = *data { d.pixels = pixels.to_vec(); d.width = width; d.height = height; }
}

/// 更新指定窗口的树 JSON（多窗口：各窗口独立存储——不再互相覆盖）
pub fn update_tree(window_id: u64, json: &str) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        *data = Some(DebugData { pixels: Vec::new(), width: 0, height: 0, trees: Default::default() });
    }
    if let Some(ref mut d) = *data {
        d.trees.insert(window_id, json.to_string());
    }
}

/// 移除指定窗口的树 JSON（窗口关闭时清理——避免残留）
pub fn remove_tree(window_id: u64) {
    if let Some(ref mut d) = *DEBUG_STATE.lock().unwrap() {
        d.trees.remove(&window_id);
    }
}

/// 全部窗口树 → 多窗口 JSON：`[{"window":0,"root":[...]},{"window":1,"root":[...]}]`
/// （按 window id 排序——顺序稳定；空树列表输出 `[]`）
fn all_trees_json() -> String {
    let data = DEBUG_STATE.lock().unwrap();
    let Some(d) = data.as_ref() else { return "[]".to_string() };
    let mut entries: Vec<(u64, &String)> = d.trees.iter().map(|(id, j)| (*id, j)).collect();
    entries.sort_by_key(|(id, _)| *id);
    let mut out = String::from("[");
    for (i, (id, json)) in entries.iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push_str(&format!(r#"{{"window":{id},"root":{json}}}"#));
    }
    out.push(']');
    out
}

// ── 事件队列 ──

static QUEUED_EVENTS: Mutex<Vec<DebugEvent>> = Mutex::new(Vec::new());

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

pub fn queue_event(event: DebugEvent) { QUEUED_EVENTS.lock().unwrap().push(event); wake(); }
pub fn take_queued_events() -> Vec<DebugEvent> { std::mem::take(&mut *QUEUED_EVENTS.lock().unwrap()) }

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
        ModifierElement::Focusable => Some("focus".into()),
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
            if SHUTDOWN.load(Ordering::SeqCst) { break; }
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
                "s" if parts.len() == 2 => {
                    let dy: f32 = parts[1].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::Scroll { dx: 0.0, dy });
                }
                "r" => { request_screenshot(); wake(); }
                "t" => {
                    // 树响应走 stdout（前缀 TREE:——UI 测试读管道；其他 demo
                    // 输出可能污染 stdout——测试按前缀过滤）。无条件响应
                    // （DEBUG_STATE 未填充时输出空——测试可区分 stdin 链路 vs 渲染时序）
                    println!("TREE:{}", all_trees_json());
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
            if SHUTDOWN.load(Ordering::SeqCst) { break; }
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
        if SHUTDOWN.load(Ordering::SeqCst) { break; }
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
            "s" if parts.len() == 2 => {
                let dy: f32 = parts[1].parse().unwrap_or(0.0);
                queue_event(DebugEvent::Scroll { dx: 0.0, dy });
                let _ = write.send(Message::Text("ok scroll".into())).await;
            }
            "r" => {
                request_screenshot(); wake();
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                let s = match DEBUG_STATE.lock() {
                    Ok(guard) => match guard.as_ref() {
                        Some(d) => format!("screenshot {}x{}", d.width, d.height),
                        None => "no frame".into(),
                    },
                    Err(_) => "lock error".into(),
                };
                let _ = write.send(Message::text(s)).await;
            }
            "t" => {
                let _ = write.send(Message::text(all_trees_json())).await;
            }
            "p" => {
                // 像素转储（调试截图分析）：二进制帧 = 8 字节 header(WxH u32 LE) + RGBA
                let shot: Option<(u32, u32, Vec<u8>)> = DEBUG_STATE.lock().ok()
                    .and_then(|g| g.as_ref().map(|d| (d.width, d.height, d.pixels.clone())));
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
            _ => { let _ = write.send(Message::Text("?".into())).await; }
        }
    }
}
