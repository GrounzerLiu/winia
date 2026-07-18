//! DevTools — 内建调试服务器
//!
//! 启动后开放 http://localhost:9999，提供:
//! - 组件树 JSON
//! - 截图
//! - 模拟点击
//!
//! 用浏览器打开 http://localhost:9999 即可使用。

use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;

// ── 全局状态 ──

static DEBUG_STATE: Mutex<Option<DebugData>> = Mutex::new(None);
static SCREENSHOT_FLAG: Mutex<bool> = Mutex::new(false);

/// 请求一帧截图（由 HTTP handler 调用）
pub fn request_screenshot() {
    *SCREENSHOT_FLAG.lock().unwrap() = true;
}

/// 渲染循环检查是否需要截图
pub fn screenshot_requested() -> bool {
    *SCREENSHOT_FLAG.lock().unwrap()
}

/// 截图完成后清除标志
pub fn screenshot_done() {
    *SCREENSHOT_FLAG.lock().unwrap() = false;
}

/// 是否有 pending 的截图请求
pub fn has_pending() -> bool {
    *SCREENSHOT_FLAG.lock().unwrap() || !QUEUED_EVENTS.lock().unwrap().is_empty()
}

/// 唤醒回调（由 app.rs 注册）
static WAKE_CALLBACK: Mutex<Option<Box<dyn Fn() + Send + Sync>>> = Mutex::new(None);
use crate::core::state::State;
use std::sync::atomic::{AtomicBool, Ordering};

/// 直接模拟原生点击（绕过 debug 队列）
static NATIVE_CLICK: std::sync::Mutex<Option<(f32, f32)>> = std::sync::Mutex::new(None);

pub fn simulate_native_click(x: f32, y: f32) {
    *NATIVE_CLICK.lock().unwrap() = Some((x, y));
}

/// app.rs 在 RedrawRequested 中消费
pub fn take_native_click() -> Option<(f32, f32)> {
    NATIVE_CLICK.lock().unwrap().take()
}

pub fn set_wake_callback(cb: impl Fn() + Send + Sync + 'static) {
    *WAKE_CALLBACK.lock().unwrap() = Some(Box::new(cb));
}

fn wake() {
    if let Some(ref cb) = *WAKE_CALLBACK.lock().unwrap() {
        cb();
    }
}

struct DebugData {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    tree_json: String,
}

pub fn update_pixels(pixels: &[u8], width: u32, height: u32) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if let Some(ref mut d) = *data {
        d.pixels = pixels.to_vec();
        d.width = width;
        d.height = height;
    }
}

/// 更新组件树 JSON（每次渲染调用）
pub fn update_tree(json: &str) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        *data = Some(DebugData {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            tree_json: json.to_string(),
        });
    } else if let Some(ref mut d) = *data {
        d.tree_json = json.to_string();
    }
}

/// 启动 debug HTTP 服务器（非阻塞，在独立线程运行）
pub fn start_server() {
    thread::spawn(|| {
        let listener = TcpListener::bind("127.0.0.1:9999").expect("DevTools: failed to bind port 9999");
        println!("[DevTools] 服务器已启动 → http://localhost:9999");

        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                thread::spawn(|| handle_connection(stream));
            }
        }
    });
}

fn handle_connection(mut stream: TcpStream) {
    let reader = BufReader::new(&mut stream);
    let request_line = reader.lines().next().and_then(|l| l.ok()).unwrap_or_default();

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    match (method, path) {
        ("GET", "/") => serve_index(&mut stream),
        ("GET" | "POST", "/screenshot") => serve_screenshot(&mut stream),
        ("GET", "/tree") => serve_tree(&mut stream),
        ("GET" | "POST", p) if p.starts_with("/click") => {
            let q = parse_query(p);
            let x: f32 = q.get("x").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let y: f32 = q.get("y").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            simulate_native_click(x, y); wake();
            respond(&mut stream, 200, "text/plain", "click queued");
        }
        ("GET" | "POST", "/shutdown") => {
            respond(&mut stream, 200, "text/plain", "shutting down");
            let _ = stream.flush();
            std::process::exit(0);
        }
        ("GET" | "POST", p) if p.starts_with("/event") => serve_event(&mut stream, p),
        _ => serve_404(&mut stream),
    }
}

fn parse_query(path: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(q) = path.split('?').nth(1) {
        for pair in q.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

// ── 路由处理 ──

fn serve_index(stream: &mut TcpStream) {
    let html = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Winia DevTools</title>
<style>
body{font:14px monospace;background:#1e1e1e;color:#ccc;margin:0;display:flex;height:100vh}
#left{width:320px;padding:12px;overflow:auto;border-right:1px solid #444}
#right{flex:1;display:flex;align-items:center;justify-content:center;background:#222}
#tree{white-space:pre;font-size:12px}
#img{max-width:100%;cursor:crosshair}
#info{margin-top:8px;color:#888}
button{margin:4px 2px;padding:4px 8px;background:#333;color:#ccc;border:1px solid #555;cursor:pointer}
</style></head><body>
<div id="left">
  <button onclick="refresh()">⟳ Refresh</button>
  <button onclick="fetch('/tree').then(r=>r.text()).then(t=>document.getElementById('tree').innerHTML=t)">Tree</button>
  <button onclick="send('key','key=Tab')">Tab</button>
  <button onclick="send('key','key=Enter')">Enter</button>
  <button onclick="send('scroll','dy=-50')">Scroll ↑</button>
  <button onclick="send('scroll','dy=50')">Scroll ↓</button>
  <button onclick="send('focus_next')">Focus→</button>
  <div id="info"></div>
  <pre id="tree"></pre>
</div>
<div id="right">
  <img id="img" src="/screenshot" onclick="onClick(event)">
</div>
<script>
function refresh(){document.getElementById('img').src='/screenshot?'+Date.now()}
function send(type,params=''){
  fetch('/event?type='+type+(params?'&'+params:'')).then(r=>r.text()).then(t=>{
    document.getElementById('info').innerHTML='event '+type+': '+t;
    setTimeout(refresh,100);
  });
}
function onClick(e){
  const img=e.target.getBoundingClientRect();
  const x=e.clientX-img.left, y=e.clientY-img.top;
  fetch('/click?x='+x+'&y='+y).then(r=>r.text()).then(t=>{
    document.getElementById('info').innerHTML='click ('+x+','+y+'): '+t;
    setTimeout(refresh,100);
  });
}
refresh();
</script>
</body></html>"#;

    respond(stream, 200, "text/html", html);
}

fn serve_screenshot(stream: &mut TcpStream) {
    request_screenshot();
    wake();
    // 等待截图完成（最多等 1 秒）
    for _ in 0..100 {
        if !screenshot_requested() {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }

    let guard = DEBUG_STATE.lock().unwrap();
    let data = match guard.as_ref() {
        Some(d) => d,
        None => {
            respond(stream, 503, "text/plain", "No frame yet");
            return;
        }
    };

    // 编码为 BMP
    let row_size = (data.width * 3 + 3) & !3; // 4-byte aligned
    let pixel_size = row_size * data.height;
    let file_size = 54 + pixel_size as u32;
    let mut body = Vec::with_capacity(file_size as usize);

    // BMP header (14 bytes)
    body.extend_from_slice(b"BM");
    body.extend_from_slice(&file_size.to_le_bytes());
    body.extend_from_slice(&[0, 0, 0, 0]); // reserved
    body.extend_from_slice(&54u32.to_le_bytes());

    // DIB header (40 bytes)
    body.extend_from_slice(&40u32.to_le_bytes());
    body.extend_from_slice(&data.width.to_le_bytes());
    body.extend_from_slice(&(data.height as i32).to_le_bytes()); // negative = top-down
    body.extend_from_slice(&1u16.to_le_bytes()); // planes
    body.extend_from_slice(&24u16.to_le_bytes()); // bpp
    body.extend_from_slice(&[0u8; 4]); // compression
    body.extend_from_slice(&(pixel_size as u32).to_le_bytes());
    body.extend_from_slice(&2835u32.to_le_bytes()); // h res
    body.extend_from_slice(&2835u32.to_le_bytes()); // v res
    body.extend_from_slice(&[0u8; 8]);

    // Pixel data (bottom-up, BGR)
    for y in (0..data.height).rev() {
        let row_start = (y * data.width * 4) as usize;
        for x in 0..data.width as usize {
            let i = row_start + x * 4;
            body.push(data.pixels[i + 2]); // R → B
            body.push(data.pixels[i + 1]); // G
            body.push(data.pixels[i]);     // B → R
        }
        // Padding
        for _ in 0..(row_size - data.width * 3) {
            body.push(0);
        }
    }

    respond(stream, 200, "image/bmp", &body);
}

fn serve_tree(stream: &mut TcpStream) {
    let guard = DEBUG_STATE.lock().unwrap();
    let tree = match guard.as_ref() {
        Some(d) => d.tree_json.clone(),
        None => r#"{"error":"no tree"}"#.to_string(),
    };
    respond(stream, 200, "application/json", &tree);
}

fn serve_event(stream: &mut TcpStream, path: &str) {
    let query = parse_query(path);
    let evt_type = query.get("type").map(|s| s.as_str()).unwrap_or("click");
    let x: f32 = query.get("x").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let y: f32 = query.get("y").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let key = query.get("key").cloned().unwrap_or_default();
    let text = query.get("text").cloned().unwrap_or_default();
    let dx: f32 = query.get("dx").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let dy: f32 = query.get("dy").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let w: f32 = query.get("w").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let h: f32 = query.get("h").and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let event = match evt_type {
        "click" => DebugEvent::Click { x, y },
        "key" => DebugEvent::Key { key },
        "text" => DebugEvent::Text { value: text },
        "scroll" => DebugEvent::Scroll { dx, dy },
        "resize" => DebugEvent::Resize { w, h },
        "focus_next" => DebugEvent::FocusNext,
        _ => {
            respond(stream, 400, "text/plain", "Unknown event type");
            return;
        }
    };
    queue_event(event);
    wake();
    respond(stream, 200, "text/plain", &format!("OK: {evt_type}"));
}

fn serve_404(stream: &mut TcpStream) {
    respond(stream, 404, "text/plain", "Not Found");
}

fn respond(stream: &mut TcpStream, code: u16, content_type: &str, body: &(impl AsRef<[u8]> + ?Sized)) {
    let body = body.as_ref();
    let header = format!(
        "HTTP/1.0 {code} OK\r\nContent-Type: {ct}\r\nContent-Length: {len}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        code = code,
        ct = content_type,
        len = body.len(),
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
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

/// 消费排队的模拟事件（app.rs 每帧调用）
pub fn take_queued_events() -> Vec<DebugEvent> {
    let mut events = QUEUED_EVENTS.lock().unwrap();
    std::mem::take(&mut *events)
}

pub(crate) fn queue_event(event: DebugEvent) {
    QUEUED_EVENTS.lock().unwrap().push(event);
}

// ── 组件树 JSON ──

/// 构建组件树 JSON（由 app.rs 在 render 闭包中调用）
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

    out.push(']');
    out.push('}');
}

fn describe_modifier(modifier: &crate::modifier::Modifier) -> String {
    let parts: Vec<String> = modifier.elements().iter().filter_map(|el| match el {
        ModifierElement::Size { width, height } => Some(format!("size({:?},{:?})", width, height)),
        ModifierElement::Background { color, .. } => Some(format!("bg({:?})", color)),
        ModifierElement::Clickable { .. } => Some("click".into()),
        ModifierElement::Focusable => Some("focus".into()),
        ModifierElement::TextContent { content, .. } => Some(format!("text({})", content)),
        ModifierElement::Padding { all } => Some(format!("pad({})", all)),
        _ => None,
    }).collect();
    parts.join("|")
}
