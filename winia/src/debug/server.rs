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
    overlay_trees: std::collections::HashMap<u64, Vec<(u64, (f32, f32), String)>>,
    /// Compose+layout rounds per rendered frame, per window, oldest first — capped. `fp` reports it and
    /// `fpc` clears it, which is how a test tells "one frame that converged in itself" (a frame with 2)
    /// apart from "two frames, one round each" (the same visible result one frame later). That
    /// distinction is the whole point of the same-frame convergence in `PerWindow::recompose_layout_render`:
    /// no tree query can see it, because by the time a query is answered the frames have passed.
    frame_passes: std::collections::HashMap<u64, Vec<u8>>,
    /// Frames rendered per window since the last clear (pairs with `frame_passes`).
    frame_count: std::collections::HashMap<u64, u64>,
}

/// How many frames of per-frame pass counts are kept per window.
const FRAME_PASSES_KEPT: usize = 32;

/// Record one rendered frame's compose+layout rounds. Called by the frame handler, once per frame.
pub fn update_frame_passes(window_id: u64, passes: u8) {
    let mut data = DEBUG_STATE.lock().unwrap();
    let state = data.get_or_insert_with(DebugData::default);
    let ring = state.frame_passes.entry(window_id).or_default();
    if ring.len() == FRAME_PASSES_KEPT {
        ring.remove(0);
    }
    ring.push(passes);
    *state.frame_count.entry(window_id).or_insert(0) += 1;
}

/// Drop the recorded pass counts and frame count — a test clears them immediately before the input it
/// wants to observe, so an earlier frame (startup converges too) cannot be mistaken for it.
pub fn clear_frame_passes() {
    if let Some(state) = DEBUG_STATE.lock().unwrap().as_mut() {
        state.frame_passes.clear();
        state.frame_count.clear();
    }
}

/// `frames=<n> multi=<m> passes=<csv>` for the legacy target window. Frames are oldest-first, so the
/// first entry is the first frame rendered after the clear.
fn frame_passes_line() -> String {
    let data = DEBUG_STATE.lock().unwrap();
    let Some(state) = data.as_ref() else { return "frames=0 multi=0 passes=".to_string() };
    let id = legacy_target().unwrap_or(0);
    let empty = Vec::new();
    let ring = state.frame_passes.get(&id).unwrap_or(&empty);
    let multi = ring.iter().filter(|&&p| p > 1).count();
    let frames = state.frame_count.get(&id).copied().unwrap_or(0);
    let csv: Vec<String> = ring.iter().map(|p| p.to_string()).collect();
    format!("frames={frames} multi={multi} passes={}", csv.join(","))
}

impl Default for DebugData {
    fn default() -> Self {
        Self {
            pixel_frames: Default::default(),
            trees: Default::default(),
            overlay_trees: Default::default(),
            frame_passes: Default::default(),
            frame_count: Default::default(),
        }
    }
}

pub fn update_pixels(window_id: u64, pixels: &[u8], width: u32, height: u32) {
    let mut data = DEBUG_STATE.lock().unwrap();
    let state = data.get_or_insert_with(DebugData::default);
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

/// Write the captured frame to a PNG. `Err` carries a message for the caller to print: a fixture has no
/// other way to report a failure here.
fn save_frame_png(path: &str) -> Result<(u32, u32), String> {
    let id = legacy_target().unwrap_or(0);
    let (w, h, pixels) = pixel_frame(id).ok_or_else(|| String::from("no captured frame (send `r` first)"))?;
    let info = skia_safe::ImageInfo::new(
        (w as i32, h as i32),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let image = skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(&pixels),
        (w as usize) * 4,
    )
    .ok_or_else(|| String::from("could not wrap the frame as a skia image"))?;
    let png = image
        .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
        .ok_or_else(|| String::from("PNG encoding failed"))?;
    std::fs::write(path, png.as_bytes()).map_err(|e| format!("writing {path}: {e}"))?;
    Ok((w, h))
}

/// One pixel of the last captured frame, as a line the stdin channel can carry: `WxH:x y r g b a`, or
/// `out`-of-frame / `none`.
///
/// Coordinates are FRAME pixels (physical: a 460-wide window at 1.5 scale captures 690 of them), stated in
/// the response's own `WxH` so a caller that only knows logical coordinates can scale. The four color
/// bytes are what the capture holds — RGBA, premultiplied by alpha (an opaque window makes the two
/// identical; a translucent one does not). `WxH:out-of-frame` answers both a point outside the frame and a
/// frame whose byte buffer is shorter than its dimensions claim.
fn pixel_line(x: u32, y: u32) -> String {
    let Some((w, h, pixels)) = legacy_target().and_then(pixel_frame) else {
        return "none".into();
    };
    if x >= w || y >= h {
        return format!("{w}x{h}:out-of-frame");
    }
    let i = ((y as usize) * w as usize + x as usize) * 4;
    match pixels.get(i..i + 4) {
        Some(p) => format!("{w}x{h}:{x} {y} {} {} {} {}", p[0], p[1], p[2], p[3]),
        None => format!("{w}x{h}:out-of-frame"),
    }
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

    /// These tests drive PROCESS-GLOBAL state (`LEGACY_TARGET`, `SCREENSHOT_TARGET`, the event queue, the
    /// frame store) and `reset_debug_requests` clears it for everyone: running them concurrently made them
    /// fail each other (measured: 5 mixed runs, 1-5 failures). They only compile with `--features
    /// debug-server`, which is not the default lib configuration, which is why this stayed unnoticed.
    static REQUEST_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Lock the module's serial section; a poisoned lock (an earlier test panicked) is still usable, the
    /// state is reset by every test anyway.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        REQUEST_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What `update_frame_passes` costs on the frame path, since it runs once per rendered frame in every
    /// `debug-server` build — including the builds the frame-cost probes use, which is why the cost had to
    /// be a number rather than an assumption. The bound is a smoke limit with ~two orders of magnitude of
    /// headroom over the measured cost (and no relation to a frame budget): this test is here to catch a
    /// future edit that makes recording allocate per frame or take a lock per state, not to police nanoseconds.
    #[test]
    fn recording_a_frame_s_passes_is_cheap_enough_for_the_frame_path() {
        let _serial = serial();
        let n = 20_000u32;
        let start = std::time::Instant::now();
        for _ in 0..n {
            update_frame_passes(1, 1);
        }
        let per_call = start.elapsed().as_nanos() as f64 / n as f64;
        eprintln!("update_frame_passes: {per_call:.0} ns/frame");
        clear_frame_passes();
        assert!(
            per_call < 5_000.0,
            "recording a frame's pass count must stay cheap on the frame path: {per_call:.0} ns"
        );
    }

    #[test]
    fn debug_session_resets_shutdown_and_requests() {
        let _serial = serial();
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
        let _serial = serial();
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
        let _serial = serial();
        reset_debug_requests();
        queue_event_for_window(22, DebugEvent::FocusNext);
        queue_event_for_window(11, DebugEvent::Click { x: 1.0, y: 2.0 });
        assert!(matches!(take_queued_events(11).as_slice(), [DebugEvent::Click { .. }]));
        assert!(matches!(take_queued_events(22).as_slice(), [DebugEvent::FocusNext]));
    }

    #[test]
    fn legacy_target_zero_resolves_to_parent_window() {
        let _serial = serial();
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
        let _serial = serial();
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
        let _serial = serial();
        reset_debug_requests();
        update_pixels(11, &[1, 2, 3, 4], 1, 1);
        update_pixels(22, &[5, 6, 7, 8], 2, 1);
        assert_eq!(pixel_frame(11), Some((1, 1, vec![1, 2, 3, 4])));
        assert_eq!(pixel_frame(22), Some((2, 1, vec![5, 6, 7, 8])));
    }

    /// The text pixel read (a UI test's only way to see what was drawn): the frame size comes back with
    /// the pixel so a caller holding logical coordinates can scale, and every miss says which kind it is.
    #[test]
    fn pixel_line_reports_the_frame_size_and_the_pixel() {
        let _serial = serial();
        reset_debug_requests();
        set_legacy_target(11);
        assert_eq!(pixel_line(0, 0), "none", "no frame captured yet");

        update_pixels(11, &[1, 2, 3, 4, 5, 6, 7, 8], 2, 1);
        // x=1 of a 2-px-wide frame is bytes 4..8 — this is the (x, y) → byte-offset arithmetic, not an
        // echo: the value has to be the SECOND pixel's four bytes.
        assert_eq!(pixel_line(1, 0), "2x1:1 0 5 6 7 8");
        assert_eq!(pixel_line(0, 0), "2x1:0 0 1 2 3 4");
        assert_eq!(pixel_line(2, 0), "2x1:out-of-frame");
        assert_eq!(pixel_line(0, 1), "2x1:out-of-frame");

        // A frame whose byte buffer is shorter than its dimensions claim is "out of frame", not a panic.
        update_pixels(11, &[1, 2, 3, 4], 4, 4);
        assert_eq!(pixel_line(3, 3), "4x4:out-of-frame");
    }
}

/// The semantics snapshot of one window as JSON, `[]`-shaped when nothing has been published yet.
///
/// Read from `crate::semantics`, which is the one store the frame loop publishes into — the
/// accessibility bridge reads the SAME snapshots (they are kept as a tree there, not as this string).
pub fn semantics_json(window_id: u64) -> String {
    crate::semantics::published(window_id)
        .map(|snapshot| snapshot.json())
        .unwrap_or_else(|| "{\"main\":[],\"overlays\":[]}".to_string())
}

/// 更新指定窗口的树 JSON（多窗口：各窗口独立存储——不再互相覆盖）
pub fn update_tree(window_id: u64, json: &str) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        *data = Some(DebugData::default());
    }
    if let Some(ref mut d) = *data {
        d.trees.insert(window_id, json.to_string());
    }
}

/// 整体替换指定窗口的弹出层树（每帧调用；空 vec = 无弹出层——清除残留）。
/// `trees` elements are `(overlay_id, logical screen origin, tree JSON)`, in render z order.
///
/// The origin matters because a popup's tree is in ITS OWN coordinates (it is rendered
/// translated to `screen_pos`), so a consumer that wants to address a popup node — a test
/// clicking it — has to add it. Emitted as `"screen":[x,y]`.
pub fn set_overlay_trees(window_id: u64, trees: Vec<(u64, (f32, f32), String)>) {
    let mut data = DEBUG_STATE.lock().unwrap();
    if data.is_none() {
        if trees.is_empty() { return; }
        *data = Some(DebugData::default());
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
        crate::semantics::forget(window_id);
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
/// - Popup entries carry `overlay` = z index (0 is bottom-most), `id` = the OverlayDesc id and
///   `screen` = that layer's logical screen origin (a layer's node coordinates are layer-local).
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
            for (i, (oid, (ox, oy), oj)) in ovs.iter().enumerate() {
                out.push(',');
                out.push_str(&format!(
                    r#"{{"window":{id},"overlay":{i},"id":{oid},"screen":[{ox:.0},{oy:.0}],"root":{oj}}}"#
                ));
            }
        }
    }
    out.push(']');
    out
}

// ── 事件队列 ──

static QUEUED_EVENTS: Mutex<Vec<(u64, DebugEvent)>> = Mutex::new(Vec::new());

use super::DebugEvent;

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
        r#"{indent}{{"id":{},"pos":[{pos_x:.0},{pos_y:.0}],"size":[{size_w:.0},{size_h:.0}],"mod":"{}","tag":{},"focused":{},"children":["#,
        node.id,
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
    let mut parts: Vec<String> = modifier.elements().iter().filter_map(|el| match el {
        ModifierElement::Size { width, height } => Some(format!("size({:?},{:?})", width, height)),
        ModifierElement::Background { .. } => Some("bg(<dynamic>)".into()),
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
            use crate::layout::{Dimension, SizeValue};
            // 四边求值（Debug 场景：显示累积/动态标记）
            let sv = |v: &SizeValue| match v {
                SizeValue::Static(Dimension::Fixed(x)) | SizeValue::Static(Dimension::Dp(crate::unit::Dp(x))) => format!("{x}"),
                SizeValue::Static(Dimension::Auto) | SizeValue::Static(Dimension::Fill) => "0".to_string(),
                SizeValue::Static(Dimension::Px(p)) => format!("{:.0}", p.to_logical(crate::runtime::density::current_density())),
                SizeValue::Dynamic(_) => "<dyn>".to_string(),
                SizeValue::Intrinsic(s) => format!("intrinsic({:?})", s),
            };
            Some(format!(
                "pad({},{},{},{})",
                sv(start), sv(top), sv(end), sv(bottom)
            ))
        }
        // The offset is what a test can assert on a scroll container: scrolling moves the RENDER
        // translation, not the children's layout positions, so a node's reported rect never changes.
        ModifierElement::VerticalScroll { state } => Some(format!("vscroll({:.0})", state.offset.get())),
        ModifierElement::HorizontalScroll { state, .. } => {
            Some(format!("hscroll({:.0})", state.offset.get()))
        }
        _ => None,
    }).collect();
    // 开放节点（exp/modifier-node）：node_key 进树，调试时可见第三方行为。
    // P1-5：key 同文本做 JSON 转义（第三方 key 含引号/反斜杠/换行即非法 JSON）。
    for n in modifier.modifier_nodes() {
        let raw = crate::modifier::node_key_of(n);
        let esc = raw
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
            .replace('\u{8}', "\\b")
            .replace('\u{c}', "\\f");
        parts.push(format!("node({})", esc));
    }
    parts.join("|")
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
                // sem: the SEMANTICS tree of the last rendered frame — role/name/state per element,
                // what a screen reader would be told. Same prefix convention as TREE:.
                "sem" => {
                    let id = legacy_target().unwrap_or(0);
                    println!("SEMANTICS:{}", semantics_json(id));
                }
                // px <x> <y>: one pixel of the last captured frame, as TEXT. The binary `p` frame only
                // travels over the WebSocket; this line form is what the UI-test harness (stdin/stdout)
                // can read, so a test can assert on what was actually drawn — the only way to see a
                // theme change, which no node in the layout tree names.
                "px" if parts.len() >= 3 => {
                    let x: u32 = parts[1].parse().unwrap_or(0);
                    let y: u32 = parts[2].parse().unwrap_or(0);
                    println!("PIXEL:{}", pixel_line(x, y));
                }
                // tr [n]: the last n animation-trace records (NDJSON lines). Empty without the
                // `anim-trace` feature.
                "tr" => {
                    let n: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
                    println!("TRACE:{}", crate::anim_trace::recent_lines(n).join("\n"));
                }
                // save <path>: write the last CAPTURED frame (see `r`) to a PNG. The other pixel routes
                // hand colour values to a client (`px` one at a time, `p` over the WebSocket); this is the
                // in-process way to get an image file out of a fixture or demo run — what looking at a
                // component's own rendering needs.
                "save" if parts.len() >= 2 => match save_frame_png(parts[1]) {
                    Ok((w, h)) => println!("SAVED:{} {}x{}", parts[1], w, h),
                    Err(e) => println!("SAVED:error {e}"),
                },
                // fp: compose+layout rounds for each rendered frame since the last clear, oldest first.
                // This is the only way to see the same-frame convergence from a test: a frame that ran
                // 2 rounds is one frame that caught up with its own measurement, while 2 frames of 1
                // round each is the one-frame lag it replaced — a difference no tree query can show.
                "fp" => println!("FRAME_PASSES:{}", frame_passes_line()),
                // fpc: forget the recorded frames, so the next reading starts at the input under test.
                "fpc" => clear_frame_passes(),
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
            "sem" => {
                let id = legacy_target().unwrap_or(0);
                let _ = write.send(Message::text(semantics_json(id))).await;
            }
            // tr [n]: the last n animation-trace records, one JSON object per line — a live view of
            // what an animation is doing, no WINIA_ANIM_TRACE file needed. Empty without `anim-trace`.
            "tr" => {
                let n: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
                let lines = crate::anim_trace::recent_lines(n);
                let _ = write
                    .send(Message::text(format!(
                        "{}",
                        lines.join("\n")
                    )))
                    .await;
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
            // px <x> <y>: the text form of one pixel (`pixel_line`) — same response as the stdin channel.
            "px" if parts.len() >= 3 => {
                let x: u32 = parts[1].parse().unwrap_or(0);
                let y: u32 = parts[2].parse().unwrap_or(0);
                let _ = write.send(Message::text(pixel_line(x, y))).await;
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
