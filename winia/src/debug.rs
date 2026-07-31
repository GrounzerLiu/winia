//! DevTools — stdin + WebSocket 双通道调试
//!
//! stdin:  echo 'c 190 130' | ./app   # 点击
//!         echo t | ./app              # 打印 UI 树
//!         echo r | ./app              # 截图请求
//! WebSocket: ws://127.0.0.1:9998
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
    pixels: Vec<u8>, width: u32, height: u32, tree_json: String,
}

pub fn update_pixels(pixels: &[u8], width: u32, height: u32) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if let Some(ref mut d) = *data { d.pixels = pixels.to_vec(); d.width = width; d.height = height; }
}

pub fn update_tree(json: &str) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        *data = Some(DebugData { pixels: Vec::new(), width: 0, height: 0, tree_json: json.to_string() });
    } else if let Some(ref mut d) = *data { d.tree_json = json.to_string(); }
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
}

pub fn queue_event(event: DebugEvent) { QUEUED_EVENTS.lock().unwrap().push(event); wake(); }
pub fn take_queued_events() -> Vec<DebugEvent> { std::mem::take(&mut *QUEUED_EVENTS.lock().unwrap()) }

pub fn simulate_native_click(x: f32, y: f32) {
    queue_event(DebugEvent::Click { x, y });
    wake();
}

// ── 组件树 JSON ──

pub fn build_tree_json(root: &LayoutNode) -> String {
    let mut out = String::from("[");
    build_node_json(root, &mut out, 0);
    out.push(']');
    out
}

fn build_node_json(node: &LayoutNode, out: &mut String, depth: usize) {
    let indent = "  ".repeat(depth + 1);
    let mod_desc = describe_modifier(&node.modifier);
    if depth > 0 { out.push_str(",\n"); }
    out.push_str(&format!(
        r#"{indent}{{"pos":[{:.0},{:.0}],"size":[{:.0},{:.0}],"mod":"{}","focused":{},"children":["#,
        node.position.x, node.position.y, node.measured_size.width, node.measured_size.height,
        mod_desc, node.focused,
    ));
    for (i, child) in node.children.iter().enumerate() {
        if i > 0 { out.push(','); }
        build_node_json(child, out, depth + 1);
    }
    out.push(']'); out.push('}');
}

fn describe_modifier(modifier: &crate::modifier::Modifier) -> String {
    modifier.elements().iter().filter_map(|el| match el {
        ModifierElement::Size { width, height } => Some(format!("size({:?},{:?})", width, height)),
        ModifierElement::Background { color_fn, .. } => Some("bg(<dynamic>)".into()),
        ModifierElement::Clickable { .. } => Some("click".into()),
        ModifierElement::Focusable => Some("focus".into()),
        ModifierElement::TextContent { content, .. } => Some(format!("text({})", content)),
        ModifierElement::Padding { all } => Some(format!("pad({})", all)),
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
                "k" if parts.len() >= 2 => queue_event(DebugEvent::Key { key: parts[1..].join(" ") }),
                "s" if parts.len() == 2 => {
                    let dy: f32 = parts[1].parse().unwrap_or(0.0);
                    queue_event(DebugEvent::Scroll { dx: 0.0, dy });
                }
                "r" => { request_screenshot(); wake(); }
                "t" => {
                    if let Some(ref d) = *DEBUG_STATE.lock().unwrap() { eprintln!("{}", d.tree_json); }
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
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:9998").await {
            Ok(l) => l,
            Err(e) => { eprintln!("[DevTools] ws bind failed: {e}"); return; }
        };
        eprintln!("[DevTools] WebSocket → ws://localhost:9998");
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
                let json = match DEBUG_STATE.lock() {
                    Ok(guard) => match guard.as_ref() {
                        Some(d) => d.tree_json.clone(),
                        None => r#"{"error":"no tree"}"#.into(),
                    },
                    Err(_) => r#"{"error":"lock error"}"#.into(),
                };
                let _ = write.send(Message::text(json)).await;
            }
            _ => { let _ = write.send(Message::Text("?".into())).await; }
        }
    }
}
