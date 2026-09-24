//! UI 集成测试（真实窗口 + stdin/stdout 管道驱动）。
//!
//! 前提：fixture 已用 debug-server feature 构建（测试会检查 exe 存在）：
//! ```sh
//! cargo test --features debug-server --no-run
//! cargo test --test ui_test --features debug-server
//! ```
//!
//! 原理：测试 spawn fixture 进程并持有其 stdin/stdout 管道——写命令
//! （c 点击 / s 滚动 / k 键盘 / t 树查询）→ 读 `TREE:` 前缀的树 JSON
//! 响应 → 断言（带重试——等待异步重组）。
//!
//! 相比 WS：零网络/零握手/零帧协议/无端口冲突——请求-响应天然配对，
//! 每进程独立管道（测试天然并行隔离）。
//!
//! 注意：UI 测试需要图形环境（真实窗口）。

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde_json::Value;

/// First-frame deadline per launch attempt, and how many attempts a launch gets: a loaded machine can
/// need far longer than a fresh one for its first frame, and a retry is much cheaper than a red suite.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(20);
const LAUNCH_ATTEMPTS: u32 = 3;

/// Serial lock for the whole suite: `launch` clears leftover fixtures with `taskkill`, which would
/// kill a fixture another test had just started, and several fixture windows at once interfere
/// (focus, input). Serial execution is fine — 25 cases run in ~60 s.
static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 与 demo 进程的管道封装
pub struct UiTest {
    child: Child,
    child_stdin: std::process::ChildStdin,
    /// stdout 读线程（mpsc——树响应/日志行）
    stdout_rx: Receiver<String>,
    /// 串行锁 guard（整个测试生命周期持有——防互杀/窗口干扰）
    _serial: std::sync::MutexGuard<'static, ()>,
    tree: Value,
    /// 窗口尺寸（根节点 size）
    pub width: f32,
    pub height: f32,
}

impl Drop for UiTest {
    fn drop(&mut self) {
        // 优雅关闭：发 q（force_shutdown → 事件循环退出）→ 限时等待 → 超时 kill
        let _ = self.child_stdin.write_all(b"q\n");
        let _ = self.child_stdin.flush();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl UiTest {
    /// 启动测试 fixture（`tests/ui_fixtures/`，单一 `[[bin]]` 构建的 exe——不依赖
    /// examples；cargo test 不执行 bin，仅构建）。exe 路径固定
    /// `target/debug/fixture_all.exe`，场景名作为 argv[1]（`fixture_all.rs` 分发到
    /// 对应模块——每个场景仍是独立进程，隔离性不变；但整个套件只链接一次 skia，
    /// 不再是每场景一个 exe + 一份 PDB）。
    /// 注意：持有全局串行锁（防 taskkill 互杀 + 窗口干扰）——UiTest drop 释放。
    pub fn launch(fixture: &str) -> Self {
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 定位 fixture exe（workspace 共享 target——在 crate 目录上一级）
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crate 不在 workspace 根下一级");
        let exe = workspace_root.join("target/debug").join("fixture_all.exe");
        if !exe.exists() {
            panic!(
                "fixture 未构建：请先 `cargo build --bin fixture_all --features debug-server`（exe: {}）",
                exe.display()
            );
        }
        let exe_name = "fixture_all.exe".to_string();

        // Launch, and RETRY the whole thing when the first frame does not arrive in time. A loaded
        // machine can take far longer than a fresh one to produce the first frame (measured: the app
        // ran 2.4-6x slower under 40 CPU-burning processes, and the first-frame deadline was the first
        // thing to fail in a loaded suite run), and a retry costs one process spawn instead of a red
        // suite.
        let mut last_failure = String::new();
        for attempt in 1..=LAUNCH_ATTEMPTS {
            // Clear leftovers (an interrupted or failed run can leave an orphan fixture behind, and
            // windows would pile up). Only safe while holding the serial lock — in parallel this would
            // kill each other's fixtures — and a leftover holding the exe's file lock needs a moment
            // to exit before the spawn. With one binary the kill is by image name: inside the suite the
            // serial lock guarantees a single fixture process, at the cost that a HAND-STARTED
            // `fixture_all` (or one from a second, concurrent `cargo test`) is killed too.
            let _ = Command::new("taskkill")
                .args(["/f", "/im", &exe_name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            std::thread::sleep(Duration::from_millis(500));

            let mut child = Command::new(&exe)
                .arg(fixture)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()) // 日志走 stderr——读线程消费（防阻塞）
                .spawn()
                .expect("spawn demo 失败");

            // stdout 读线程（行 → mpsc）
            let mut stdout = child.stdout.take().expect("stdout piped");
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let mut reader = std::io::BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if tx.send(line.trim_end().to_string()).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

        // stderr 读线程（消费防阻塞——日志可选转发到测试 stdout）
        if let Some(mut stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 2048];
                loop {
                    match stderr.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let s = String::from_utf8_lossy(&buf[..n]);
                            if std::env::var("UI_TEST_STDERR").is_ok() {
                                eprintln!("[demo] {s}");
                            }
                        }
                    }
                }
            });
        }

        let mut child_stdin = child.stdin.take().expect("stdin piped");

            match Self::wait_for_first_frame(&mut child, &mut child_stdin, &rx, FIRST_FRAME_TIMEOUT) {
                Ok(tree) => {
                    // 解析窗口尺寸（根节点 size）
                    let (width, height) = tree_size(&tree);
                    return Self { child, child_stdin, stdout_rx: rx, _serial, tree, width, height };
                }
                Err(why) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    last_failure = why;
                    if attempt < LAUNCH_ATTEMPTS {
                        eprintln!(
                            "[ui-test] fixture `{fixture}` attempt {attempt}/{LAUNCH_ATTEMPTS} failed: {last_failure} — retrying"
                        );
                    }
                }
            }
        }
        panic!(
            "fixture `{fixture}` produced no first frame in {LAUNCH_ATTEMPTS} attempts: {last_failure}"
        );
    }

    /// Wait for the first frame tree (`t` only answers once the fixture has rendered a frame).
    ///
    /// The failure reason comes back as `Err` so `launch` can retry: a loaded machine needs far longer
    /// than a fresh one for its first frame (measured 2.4-6x slower under load), and one extra process
    /// spawn is much cheaper than a red suite.
    fn wait_for_first_frame(
        child: &mut Child,
        child_stdin: &mut std::process::ChildStdin,
        rx: &Receiver<String>,
        timeout: Duration,
    ) -> Result<Value, String> {
        let started = Instant::now();
        let deadline = started + timeout;
        let mut tree_responses = 0u32;
        loop {
            if let Some(status) = child.try_wait().ok().flatten() {
                return Err(format!("process exited early ({status}) — check the fixture build/window env"));
            }
            let (t, received) = query_tree(child_stdin, rx, Duration::from_millis(300));
            if received {
                tree_responses += 1;
            }
            if let Some(t) = t {
                // Ready = a tree with at least one window (an empty array `[]` means DEBUG_STATE has
                // not rendered yet — keep waiting).
                let has_window = t
                    .as_array()
                    .map(|arr| arr.iter().any(|w| w.get("root").is_some()))
                    .unwrap_or(false);
                if has_window {
                    // A first frame this slow means the machine is saturated; the interaction-level
                    // expectations in the suite (drags, typing, animations) may then fail on their own
                    // timing even though `launch` retried. Say so, so a red suite is readable.
                    let took = started.elapsed();
                    if took > Duration::from_secs(3) {
                        eprintln!(
                            "[ui-test] warning: the first frame took {took:?} — this machine looks loaded, timing-sensitive cases may fail"
                        );
                    }
                    return Ok(t);
                }
            }
            if Instant::now() > deadline {
                let mut buf = String::new();
                while let Ok(line) = rx.try_recv() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
                let alive = child.try_wait().ok().flatten().is_none();
                return Err(format!(
                    "no first frame within {timeout:?}. alive={alive}, TREE responses={tree_responses}, stdout so far: {}",
                    if buf.is_empty() { "(empty)" } else { &buf[..buf.len().min(300)] }
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// 发送一条命令（stdin 写——无响应；命令异步处理，断言靠 expect_* 轮询）
    pub fn send(&mut self, cmd: &str) {
        let _ = self.child_stdin.write_all(cmd.as_bytes());
        let _ = self.child_stdin.write_all(b"\n");
        let _ = self.child_stdin.flush();
    }

    /// 点击坐标（scene 坐标）
    pub fn click(&mut self, x: f32, y: f32) {
        self.send(&format!("c {} {}", x as i32, y as i32));
        std::thread::sleep(Duration::from_millis(150));
    }

    /// 按下/移动/释放（拖拽选择）
    pub fn drag(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.send(&format!("d {} {}", x1 as i32, y1 as i32));
        let steps = 8;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let x = x1 + (x2 - x1) * t;
            let y = y1 + (y2 - y1) * t;
            self.send(&format!("m {} {}", x as i32, y as i32));
            std::thread::sleep(Duration::from_millis(20));
        }
        self.send(&format!("u {} {}", x2 as i32, y2 as i32));
        std::thread::sleep(Duration::from_millis(100));
    }

    /// 键盘输入（Key 名称——winit 命名）
    pub fn key(&mut self, key: &str) {
        self.send(&format!("k {key}"));
        std::thread::sleep(Duration::from_millis(80));
    }

    /// 滚动（dy；横向用 scroll_delta）
    pub fn scroll(&mut self, dy: f32) {
        self.send(&format!("s {}", dy as i32));
        std::thread::sleep(Duration::from_millis(100));
    }

    /// 滚动（双轴：dx dy）
    pub fn scroll_delta(&mut self, dx: f32, dy: f32) {
        self.send(&format!("s {} {}", dx as i32, dy as i32));
        std::thread::sleep(Duration::from_millis(100));
    }

    /// 模拟窗口 resize（w <width> <height> 命令——逻辑像素，走真实
    /// request_surface_size → SurfaceResized 事件通路）
    pub fn resize(&mut self, w: f32, h: f32) {
        self.send(&format!("w {} {}", w as i32, h as i32));
        std::thread::sleep(Duration::from_millis(250));
    }

    /// 查询最新树 JSON（写 t → 读 TREE: 响应；超时/无窗口返回 None）
    pub fn tree(&mut self) -> Option<Value> {
        let (t, _) = query_tree(&mut self.child_stdin, &self.stdout_rx, Duration::from_secs(2));
        if let Some(t) = t {
            // 至少一个窗口的树才视为有效（空数组 `[]` = 未渲染）
            let has_window = t
                .as_array()
                .map(|arr| arr.iter().any(|w| w.get("root").is_some()))
                .unwrap_or(false);
            if has_window {
                self.tree = t.clone();
                return Some(t);
            }
        }
        None
    }

    /// 刷新树（记录到 self.tree）
    pub fn refresh(&mut self) {
        if let Some(t) = self.tree() {
            self.tree = t;
        }
    }

    /// One pixel of the CURRENT frame — `(frame_w, frame_h, r, g, b, a)` — in FRAME (physical)
    /// coordinates; `None` when no frame could be captured or the point is outside it.
    ///
    /// A frame is captured only on request (`r`), and the capture lands on the next rendered frame, so
    /// this asks and waits rather than reading whatever the last capture happened to be. The binary `p`
    /// frame only travels over the WebSocket; `px` is its text form, which is what this line-based
    /// channel can carry — and the only way a UI test can see something the layout tree cannot name
    /// (a theme-derived color is resolved at build time, and every `bg(...)` in the tree is `<dynamic>`).
    pub fn pixel(&mut self, x: u32, y: u32) -> Option<(u32, u32, u8, u8, u8, u8)> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.send("r");
            std::thread::sleep(Duration::from_millis(120)); // let the next frame render and be captured
            self.send(&format!("px {x} {y}"));
            if let Some(line) = self.read_prefixed_line("PIXEL:", Duration::from_millis(800)) {
                if let Some((w, h, rx, ry, rgba)) = line.strip_prefix("PIXEL:").and_then(parse_pixel_line) {
                    // The reply names the point it answered; a read that timed out leaves its line in the
                    // channel, and the next call would otherwise take that answer for its own.
                    if (rx, ry) == (x, y) {
                        return Some((w, h, rgba.0, rgba.1, rgba.2, rgba.3));
                    }
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
        }
    }

    /// The pixel at the CENTRE of the current frame — `(r, g, b, a)`. The one point whose frame
    /// coordinates need no scale-factor arithmetic: a frame pixel is physical, and a test's other
    /// coordinates are logical.
    pub fn centre_pixel(&mut self) -> Option<(u8, u8, u8, u8)> {
        let (w, h) = self.frame_size()?;
        self.pixel(w / 2, h / 2).map(|(_, _, r, g, b, a)| (r, g, b, a))
    }

    /// Frame (physical) size of the current frame. Asked for as an OUT-OF-FRAME point, whose reply carries
    /// the size without a second request answering a different question than `centre_pixel` asked.
    pub fn frame_size(&mut self) -> Option<(u32, u32)> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.send("r");
            std::thread::sleep(Duration::from_millis(120));
            self.send(&format!("px {OUT_OF_FRAME} {OUT_OF_FRAME}"));
            if let Some(line) = self.read_prefixed_line("PIXEL:", Duration::from_millis(800)) {
                if let Some(size) = line.strip_prefix("PIXEL:").and_then(parse_frame_size) {
                    return Some(size);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
        }
    }

    /// The centre pixel's luma (0 = black, 255 = white), re-read until it satisfies `pred` or `timeout`
    /// expires — a theme change lands on a later frame than the click that caused it.
    ///
    /// `None` means NO pixel could be read at all (an unreachable or broken capture path): a caller must
    /// not read that as "the pixel is black", which is what a sentinel like `-1.0` invited. `Some` carries
    /// the luma that satisfied `pred`, or the last one seen when it never did.
    pub fn wait_centre_luma(&mut self, timeout: Duration, pred: impl Fn(f32) -> bool) -> Option<f32> {
        let deadline = Instant::now() + timeout;
        let mut last: Option<f32> = None;
        loop {
            if let Some((r, g, b, _)) = self.centre_pixel() {
                let l = luma(r, g, b);
                last = Some(l);
                if pred(l) {
                    return last;
                }
            }
            if Instant::now() >= deadline {
                return last;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The pixel at a LOGICAL point — the coordinates the tree and the click helpers use. A frame pixel is
    /// physical, so the point is scaled by the frame-to-window ratio (a popup's rect, for instance, only
    /// exists in window coordinates).
    pub fn pixel_at_logical(&mut self, x: f32, y: f32) -> Option<(u8, u8, u8, u8)> {
        let (fw, fh) = self.frame_size()?;
        if self.width <= 0.0 || self.height <= 0.0 {
            return None;
        }
        let sx = fw as f32 / self.width;
        let sy = fh as f32 / self.height;
        self.pixel((x * sx) as u32, (y * sy) as u32).map(|(_, _, r, g, b, a)| (r, g, b, a))
    }

    /// Like [`UiTest::wait_centre_luma`], at a logical point: `None` means no pixel could be read at all,
    /// `Some` carries the luma that satisfied `pred` or the last one seen.
    pub fn wait_pixel_luma(
        &mut self,
        x: f32,
        y: f32,
        timeout: Duration,
        pred: impl Fn(f32) -> bool,
    ) -> Option<f32> {
        let deadline = Instant::now() + timeout;
        let mut last: Option<f32> = None;
        loop {
            if let Some((r, g, b, _)) = self.pixel_at_logical(x, y) {
                let l = luma(r, g, b);
                last = Some(l);
                if pred(l) {
                    return last;
                }
            }
            if Instant::now() >= deadline {
                return last;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Read one line from the fixture's stdout, skipping lines that do not carry `prefix` (device logs
    /// share the pipe) until `timeout`. A line that arrives after the timeout is left for the next call,
    /// which is why [`UiTest::pixel`] checks the point a reply names.
    fn read_prefixed_line(&mut self, prefix: &str, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match self.stdout_rx.recv_timeout(remaining) {
                Ok(line) => {
                    if line.starts_with(prefix) {
                        return Some(line);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    /// 深度遍历树，收集所有 mod 描述文本
    pub fn all_texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_texts(&self.tree, &mut out);
        out
    }

    /// The `mod` strings of the POPUP entries only. `all_texts` covers the main tree, so popup content
    /// (a dialog's message, an expanded panel's rows) needs its own accessor to be asserted on.
    pub fn overlay_texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for_each_window_scoped(&self.tree, true, |_id, _ox, _oy, root| {
            collect_texts_of(root, &mut out)
        });
        out
    }

    /// 断言 popup 条目中出现 `text`（带重试）
    pub fn expect_overlay_text(&mut self, text: &str) {
        self.expect_overlay_text_timeout(text, Duration::from_secs(5));
    }

    /// 断言 popup 条目中出现 `text`（带超时）——与 `expect_text_timeout` 同语义，但只看弹层。
    pub fn expect_overlay_text_timeout(&mut self, text: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.refresh();
            let all = self.overlay_texts();
            if all.iter().any(|t| t.contains(text)) {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "popup entries never showed `{text}` ({timeout:?}). Popup texts: {}",
                    all.join(" | ")
                );
            }
            std::thread::sleep(Duration::from_millis(120));
        }
    }

    /// 提取树中所有 mod 文本（供 click_until 闭包使用）
    pub fn tree_texts(tree: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        collect_texts(tree, &mut out);
        out
    }

    /// 当前树中的窗口数量（多窗口树；仅主窗口——overlay 条目不计入，
    /// 断言弹层用 overlay_count）
    pub fn window_count(&self) -> usize {
        self.tree
            .as_array()
            .map(|arr| arr.iter().filter(|w| w.get("root").is_some() && w.get("overlay").is_none()).count())
            .unwrap_or(0)
    }

    /// 当前打开的顶层弹出层数量（全部窗口累计；0 = 无 Popup/Dialog 残留）
    pub fn overlay_count(&self) -> usize {
        self.tree
            .as_array()
            .map(|arr| arr.iter().filter(|w| w.get("root").is_some() && w.get("overlay").is_some()).count())
            .unwrap_or(0)
    }

    /// 通过稳定的 `Modifier::test_tag` 查找首个窗口节点，返回 (abs_x, abs_y, width, height)。
    pub fn find_tag(&self, tag: &str) -> Option<(f32, f32, f32, f32)> {
        find_node_tag(&self.tree, tag)
    }

    /// 点击稳定 tag 节点。焦点和输入结果由调用方的场景断言验证。
    pub fn click_tag(&mut self, tag: &str) {
        let (x, y, w, h) = self.find_tag(tag).unwrap_or_else(|| panic!("找不到 tag `{tag}`"));
        self.click(x + w / 2.0, y + h / 2.0);
        std::thread::sleep(Duration::from_millis(120));
    }

    /// 查询主窗口中 tag 节点的焦点状态。
    pub fn tag_is_focused(&mut self, tag: &str) -> bool {
        self.refresh();
        node_tag_is_focused(&self.tree, tag)
    }

    /// Tags of every node the tree shows as focused, across the main tree and every overlay entry.
    ///
    /// The framework keeps ONE visible focus: a layer that is not the keyboard owner remembers where
    /// its focus was in a slot key, not in the tree, so a test can assert this list has exactly one
    /// entry whenever something is focused — two entries mean a layer is drawing a ring it should
    /// not be showing (or the single-owner rule broke).
    pub fn focused_tags(&mut self) -> Vec<String> {
        self.refresh();
        let mut out = Vec::new();
        // Both sides of the split: `for_each_window_scoped` visits either the main-tree entries or
        // the popup entries, and this assertion is about every layer at once.
        for overlay_only in [false, true] {
            for_each_window_scoped(&self.tree, overlay_only, |_, _, _, root| {
                collect_focused_tags(root, &mut out);
            });
        }
        out
    }

    /// Find a tag inside the POPUP entries, in WINDOW coordinates. Main-tree lookups skip
    /// popups on purpose, so popup content needs its own entry point; a popup node's tree
    /// coordinates are layer-local, and this adds that layer's screen origin.
    pub fn find_tag_in_overlay(&self, tag: &str) -> Option<(f32, f32, f32, f32)> {
        find_node_tag_in_overlay(&self.tree, tag)
    }

    /// Click a tag inside the popup entries.
    pub fn click_overlay_tag(&mut self, tag: &str) {
        let (x, y, w, h) = self
            .find_tag_in_overlay(tag)
            .unwrap_or_else(|| panic!("no popup entry carries the tag `{tag}`"));
        self.click(x + w / 2.0, y + h / 2.0);
        std::thread::sleep(Duration::from_millis(120));
    }

    /// Whether the tag inside the popup entries is focused.
    pub fn overlay_tag_is_focused(&mut self, tag: &str) -> bool {
        self.refresh();
        node_tag_is_focused_in_overlay(&self.tree, tag)
    }

    /// 在树中查找第一个 mod 包含 `label` 的节点，返回 (abs_x, abs_y, width, height)。
    /// ⚠ 仅对主窗口可靠（debug 注入事件只作用于主窗口）——多窗口请用 `find_in_window`。
    pub fn find(&self, label: &str) -> Option<(f32, f32, f32, f32)> {
        find_node(&self.tree, label)
    }

    /// 在指定窗口内查找节点（多窗口精确定位）
    pub fn find_in_window(&self, window_id: u64, label: &str) -> Option<(f32, f32, f32, f32)> {
        find_node_in_window(&self.tree, window_id, label)
    }

    /// 断言树中出现 `text`（带重试——等待异步重组/动画）
    pub fn expect_text(&mut self, text: &str) {
        self.expect_text_timeout(text, Duration::from_secs(5));
    }

    pub fn expect_text_timeout(&mut self, text: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.refresh();
            if self.all_texts().iter().any(|t| t.contains(text)) {
                return;
            }
            if Instant::now() > deadline {
                let all = self.all_texts();
                panic!(
                    "超时未见文本 `{text}`（{timeout:?}）。当前文本: {}",
                    all.join(" | ")
                );
            }
            std::thread::sleep(Duration::from_millis(120));
        }
    }

    /// 点击并在 timeout 内轮询 `until`（树最新状态）；点击丢失（debug 渲染断——
    /// RedrawRequested 偶发不来，树不刷新）时自动重试点击，最多 3 次。
    pub fn click_until(
        &mut self,
        x: f32,
        y: f32,
        timeout: Duration,
        mut until: impl FnMut(&serde_json::Value) -> bool,
    ) {
        for attempt in 0..3 {
            self.click(x, y);
            let deadline = Instant::now() + timeout;
            loop {
                let tree = self.tree().expect("树查询失败（进程退出或树为空）");
                if until(&tree) {
                    return;
                }
                if Instant::now() > deadline {
                    break; // 本轮未生效——重试点击
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            if attempt == 2 {
                panic!("点击 3 次均未满足条件（渲染断持续）");
            }
            eprintln!("[ui-test] 点击未生效（第 {} 次）——重试", attempt + 1);
        }
    }

    /// Send a named key and wait for `expect` to show up, retrying the key itself.
    ///
    /// A loaded machine drops key events the way it drops clicks (see `click_until`), and a focus
    /// hand-off lands on a LATER frame than the value change that triggered it — so a test that sends
    /// an arrow right after a press can be talking to a window that has nothing focused yet.
    pub fn key_until(&mut self, key: &str, expect: &str, timeout: Duration) {
        for attempt in 0..3 {
            self.send(&format!("k {key}"));
            let deadline = Instant::now() + timeout;
            loop {
                self.refresh();
                if self.all_texts().iter().any(|t| t.contains(expect)) {
                    return;
                }
                if Instant::now() > deadline {
                    break; // this attempt did not take — send the key again
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if attempt == 2 {
                panic!("`k {key}` did not produce `{expect}` in 3 attempts (rendering stalled)");
            }
            eprintln!("[ui-test] `k {key}` did not take (attempt {}) — retrying", attempt + 1);
        }
    }

    /// 断言树中不出现 `text`
    pub fn expect_no_text(&mut self, text: &str) {
        self.refresh();
        let all = self.all_texts();
        assert!(
            !all.iter().any(|t| t.contains(text)),
            "不应出现文本 `{text}`。当前文本: {}",
            all.join(" | ")
        );
    }

    /// 断言树节点总数（精确）
    pub fn expect_node_count(&mut self, count: usize) {
        self.refresh();
        let n = count_nodes(&self.tree);
        assert_eq!(n, count, "节点数不符");
    }
}

    /// 写 `t` 命令并读 TREE: 响应（跳过污染行——demo 可能向 stdout 打印日志）。
    /// 返回 (树 JSON, 是否收到过 TREE 前缀响应)——空 JSON 表示 DEBUG_STATE 未就绪。
    fn query_tree(
        stdin: &mut std::process::ChildStdin,
        rx: &Receiver<String>,
        timeout: Duration,
    ) -> (Option<Value>, bool) {
        if stdin.write_all(b"t\n").is_err() || stdin.flush().is_err() {
            // A write fails when the child died between the caller's liveness probe and this write.
            // Report "no response" instead of panicking: `wait_for_first_frame` then sees the exit on
            // its next probe and hands the reason back to `launch`, which retries.
            return (None, false);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return (None, false);
            }
            match rx.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some(json) = line.strip_prefix("TREE:") {
                        return (
                            serde_json::from_str(json).ok(),
                            true,
                        );
                    }
                    // 其他行（demo 日志）忽略
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return (None, false),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return (None, false),
            }
        }
    }

// ── 树 JSON 辅助 ──

/// 多窗口树格式：`[{"window":<id>,"root":{...}}, ...]`（debug.rs 按 window id 排序）。
/// 遍历所有窗口的 root，对每个调用 `f(window_id, root)`。
fn for_each_window(tree: &Value, mut f: impl FnMut(u64, &Value)) {
    for_each_window_scoped(tree, false, |id, _, _, root| f(id, root));
}

/// Same iteration, but `overlay_only` selects the POPUP entries instead of skipping them:
/// main-tree assertions deliberately ignore popup content (`expect_no_text` is about the main
/// window), while a test OF popup content needs the other side of that split.
fn for_each_window_scoped(
    tree: &Value,
    overlay_only: bool,
    mut f: impl FnMut(u64, f32, f32, &Value),
) {
    if let Some(arr) = tree.as_array() {
        for w in arr {
            // Popup entries carry an "overlay" field; main-tree entries do not.
            if w.get("overlay").is_some() != overlay_only {
                continue;
            }
            if let Some(root) = w.get("root") {
                let id = w.get("window").and_then(|v| v.as_u64()).unwrap_or(0);
                // A popup's node coordinates are layer-local (the layer is translated to
                // `screen`); main-tree entries have no such field.
                let (ox, oy) = match w.get("screen").and_then(|v| v.as_array()) {
                    Some(v) if v.len() >= 2 => (
                        v[0].as_f64().unwrap_or(0.0) as f32,
                        v[1].as_f64().unwrap_or(0.0) as f32,
                    ),
                    _ => (0.0, 0.0),
                };
                f(id, ox, oy, root);
            }
        }
    }
}

/// 第一个窗口的根节点（root 可能是数组——取第一个元素；单窗口退化兼容）
fn root_of(tree: &Value) -> &Value {
    if let Some(arr) = tree.as_array() {
        if let Some(first) = arr.first() {
            if let Some(root) = first.get("root") {
                // root 值可能是根节点数组——取第一个节点
                if let Some(rarr) = root.as_array() {
                    if let Some(node) = rarr.first() {
                        return node;
                    }
                }
                return root;
            }
        }
    }
    tree
}

fn tree_size(tree: &Value) -> (f32, f32) {
    let root = root_of(tree);
    let size = root.get("size").and_then(|s| s.as_array());
    match size {
        Some(v) if v.len() >= 2 => (
            v[0].as_f64().unwrap_or(0.0) as f32,
            v[1].as_f64().unwrap_or(0.0) as f32,
        ),
        _ => (0.0, 0.0),
    }
}

fn collect_texts(tree: &Value, out: &mut Vec<String>) {
    for_each_window(tree, |_, root| collect_texts_of(root, out));
}

/// Collect the `mod` strings of one root node (a window's tree, or a popup entry's).
fn collect_texts_of(root: &Value, out: &mut Vec<String>) {
    fn walk(n: &Value, out: &mut Vec<String>) {
        // root 可能是数组（根节点列表）
        if let Some(arr) = n.as_array() {
            for el in arr {
                walk(el, out);
            }
            return;
        }
        if let Some(m) = n.get("mod").and_then(|m| m.as_str()) {
            out.push(m.to_string());
        }
        if let Some(children) = n.get("children").and_then(|c| c.as_array()) {
            for c in children {
                walk(c, out);
            }
        }
    }
    walk(root, out);
}


/// 查找 tag 精确匹配的节点，并返回绝对位置和尺寸。
fn find_node_tag(tree: &Value, tag: &str) -> Option<(f32, f32, f32, f32)> {
    find_node_tag_scoped(tree, tag, false)
}

/// As above, but only within the POPUP entries (the entry point for popup content).
fn find_node_tag_in_overlay(tree: &Value, tag: &str) -> Option<(f32, f32, f32, f32)> {
    find_node_tag_scoped(tree, tag, true)
}

fn find_node_tag_scoped(tree: &Value, tag: &str, overlay_only: bool) -> Option<(f32, f32, f32, f32)> {
    fn walk(n: &Value, ax: f32, ay: f32, tag: &str) -> Option<(f32, f32, f32, f32)> {
        if let Some(arr) = n.as_array() {
            return arr.iter().find_map(|child| walk(child, ax, ay, tag));
        }
        let pos = n.get("pos").and_then(|value| value.as_array());
        let (x, y) = match pos {
            Some(value) if value.len() >= 2 => (
                ax + value[0].as_f64().unwrap_or(0.0) as f32,
                ay + value[1].as_f64().unwrap_or(0.0) as f32,
            ),
            _ => (ax, ay),
        };
        if n.get("tag").and_then(|value| value.as_str()) == Some(tag) {
            let size = n.get("size").and_then(|value| value.as_array());
            let (width, height) = match size {
                Some(value) if value.len() >= 2 => (
                    value[0].as_f64().unwrap_or(0.0) as f32,
                    value[1].as_f64().unwrap_or(0.0) as f32,
                ),
                _ => (0.0, 0.0),
            };
            return Some((x, y, width, height));
        }
        n.get("children")
            .and_then(|value| value.as_array())
            .and_then(|children| children.iter().find_map(|child| walk(child, x, y, tag)))
    }

    let mut found = None;
    for_each_window_scoped(tree, overlay_only, |_, ox, oy, root| {
        if found.is_none() {
            found = walk(root, ox, oy, tag);
        }
    });
    found
}

fn collect_focused_tags(n: &Value, out: &mut Vec<String>) {
    if let Some(arr) = n.as_array() {
        for child in arr {
            collect_focused_tags(child, out);
        }
        return;
    }
    if n.get("focused").and_then(|value| value.as_bool()).unwrap_or(false) {
        out.push(
            n.get("tag")
                .and_then(|value| value.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("<untagged>")
                .to_string(),
        );
    }
    for key in ["children", "content", "root"] {
        if let Some(child) = n.get(key) {
            collect_focused_tags(child, out);
        }
    }
}

fn node_tag_is_focused(tree: &Value, tag: &str) -> bool {
    node_tag_is_focused_scoped(tree, tag, false)
}

/// As above, but only within the POPUP entries.
fn node_tag_is_focused_in_overlay(tree: &Value, tag: &str) -> bool {
    node_tag_is_focused_scoped(tree, tag, true)
}

fn node_tag_is_focused_scoped(tree: &Value, tag: &str, overlay_only: bool) -> bool {
    fn subtree_is_focused(n: &Value) -> bool {
        if let Some(arr) = n.as_array() {
            return arr.iter().any(subtree_is_focused);
        }
        n.get("focused").and_then(|value| value.as_bool()).unwrap_or(false)
            || n.get("children")
                .and_then(|value| value.as_array())
                .is_some_and(|children| children.iter().any(subtree_is_focused))
    }

    fn walk(n: &Value, tag: &str) -> Option<bool> {
        if let Some(arr) = n.as_array() {
            return arr.iter().find_map(|child| walk(child, tag));
        }
        if n.get("tag").and_then(|value| value.as_str()) == Some(tag) {
            return Some(subtree_is_focused(n));
        }
        n.get("children")
            .and_then(|value| value.as_array())
            .and_then(|children| children.iter().find_map(|child| walk(child, tag)))
    }

    let mut found = None;
    for_each_window_scoped(tree, overlay_only, |_, _, _, root| {
        if found.is_none() {
            found = walk(root, tag);
        }
    });
    found.unwrap_or(false)
}

/// `OUT_OF_FRAME` — an x/y no frame can contain, so `px` answers with the frame size and no color.
const OUT_OF_FRAME: u32 = u32::MAX;

/// Perceived brightness of a frame pixel (Rec. 709) — enough for a test to tell a light surface from a
/// dark one without knowing either palette.
fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

/// `WxH:…` (the debug server's `px` reply, prefix stripped) → `(W, H)`. Works for every reply shape,
/// including `WxH:out-of-frame`.
fn parse_frame_size(body: &str) -> Option<(u32, u32)> {
    let (size, _) = body.split_once(':')?;
    let (w, h) = size.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// `WxH:<x> <y> <r> <g> <b> <a>` (the `px` reply, prefix stripped) → the size, the point it answered, and
/// the color. `None` for the two miss forms and for a malformed line.
///
/// The point is FRAME (physical) coordinates even though the r/g/b/a are bytes — parsing all six as `u8`
/// silently rejected every frame whose point sat past 255, i.e. every window whose physical centre is
/// beyond 255 on either axis (a 175% or 200% scaled display, a large window), which is exactly the case a
/// pixel helper has to survive.
fn parse_pixel_line(body: &str) -> Option<(u32, u32, u32, u32, (u8, u8, u8, u8))> {
    let (w, h) = parse_frame_size(body)?;
    let (_, rest) = body.split_once(':')?;
    let v: Vec<&str> = rest.split_whitespace().collect();
    match v.as_slice() {
        [x, y, r, g, b, a] => Some((
            w,
            h,
            x.parse().ok()?,
            y.parse().ok()?,
            (r.parse().ok()?, g.parse().ok()?, b.parse().ok()?, a.parse().ok()?),
        )),
        _ => None,
    }
}

fn count_nodes(tree: &Value) -> usize {
    fn walk(n: &Value) -> usize {
        if let Some(arr) = n.as_array() {
            return arr.iter().map(walk).sum();
        }
        1 + n.get("children")
            .and_then(|c| c.as_array())
            .map(|cs| cs.iter().map(walk).sum())
            .unwrap_or(0)
    }
    let mut total = 0;
    for_each_window(tree, |_, root| total += walk(root));
    total
}

/// 查找 mod 包含 label 的节点 → (abs_x, abs_y, w, h)。
/// ⚠ 仅对主窗口有效：遍历所有窗口（按 window id 顺序）返回第一个匹配，坐标是该
/// 窗口内的绝对坐标——但 debug 注入事件只作用于主窗口。多窗口场景请用
/// `find_in_window` 精确定位。
fn find_node(tree: &Value, label: &str) -> Option<(f32, f32, f32, f32)> {
    fn walk(n: &Value, ax: f32, ay: f32, label: &str) -> Option<(f32, f32, f32, f32)> {
        // root 可能是数组（根节点列表）
        if let Some(arr) = n.as_array() {
            for el in arr {
                if let Some(r) = walk(el, ax, ay, label) {
                    return Some(r);
                }
            }
            return None;
        }
        let pos = n.get("pos").and_then(|p| p.as_array());
        let (x, y) = match pos {
            Some(v) if v.len() >= 2 => (
                ax + v[0].as_f64().unwrap_or(0.0) as f32,
                ay + v[1].as_f64().unwrap_or(0.0) as f32,
            ),
            _ => (ax, ay),
        };
        let m = n.get("mod").and_then(|m| m.as_str()).unwrap_or("");
        if m.contains(label) {
            let size = n.get("size").and_then(|s| s.as_array());
            let (w, h) = match size {
                Some(v) if v.len() >= 2 => (
                    v[0].as_f64().unwrap_or(0.0) as f32,
                    v[1].as_f64().unwrap_or(0.0) as f32,
                ),
                _ => (0.0, 0.0),
            };
            return Some((x, y, w, h));
        }
        if let Some(children) = n.get("children").and_then(|c| c.as_array()) {
            for c in children {
                if let Some(r) = walk(c, x, y, label) {
                    return Some(r);
                }
            }
        }
        None
    }
    let mut found = None;
    for_each_window(tree, |_, root| {
        if found.is_none() {
            found = walk(root, 0.0, 0.0, label);
        }
    });
    found
}

/// 在指定窗口内查找节点（多窗口精确定位——坐标是该窗口内的绝对坐标；
/// 注意 debug 注入事件仍只作用于主窗口，坐标用于断言/记录）
fn find_node_in_window(tree: &Value, window_id: u64, label: &str) -> Option<(f32, f32, f32, f32)> {
    fn walk(n: &Value, ax: f32, ay: f32, label: &str) -> Option<(f32, f32, f32, f32)> {
        if let Some(arr) = n.as_array() {
            for el in arr {
                if let Some(r) = walk(el, ax, ay, label) {
                    return Some(r);
                }
            }
            return None;
        }
        let pos = n.get("pos").and_then(|p| p.as_array());
        let (x, y) = match pos {
            Some(v) if v.len() >= 2 => (
                ax + v[0].as_f64().unwrap_or(0.0) as f32,
                ay + v[1].as_f64().unwrap_or(0.0) as f32,
            ),
            _ => (ax, ay),
        };
        let m = n.get("mod").and_then(|m| m.as_str()).unwrap_or("");
        if m.contains(label) {
            let size = n.get("size").and_then(|s| s.as_array());
            let (w, h) = match size {
                Some(v) if v.len() >= 2 => (
                    v[0].as_f64().unwrap_or(0.0) as f32,
                    v[1].as_f64().unwrap_or(0.0) as f32,
                ),
                _ => (0.0, 0.0),
            };
            return Some((x, y, w, h));
        }
        if let Some(children) = n.get("children").and_then(|c| c.as_array()) {
            for c in children {
                if let Some(r) = walk(c, x, y, label) {
                    return Some(r);
                }
            }
        }
        None
    }
    let mut found = None;
    for_each_window(tree, |id, root| {
        if found.is_none() && id == window_id {
            found = walk(root, 0.0, 0.0, label);
        }
    });
    found
}

#[cfg(test)]
mod pixel_line_parsing {
    use super::*;

    /// The reply's point is in FRAME (physical) pixels, so it can be far past 255 — parsing it as a byte
    /// rejected every frame whose centre sat beyond 255 on either axis (a 175%/200% scaled display, a large
    /// window), which the centre-pixel helper then reported as "no pixel at all".
    #[test]
    fn frame_coordinates_are_not_bytes() {
        assert_eq!(
            parse_pixel_line("480x330:240 165 20 18 24 255"),
            Some((480, 330, 240, 165, (20, 18, 24, 255)))
        );
        assert_eq!(
            parse_pixel_line("1920x1080:960 540 253 247 255 255"),
            Some((1920, 1080, 960, 540, (253, 247, 255, 255)))
        );
    }

    /// Both miss forms are "no color"; the size is readable from either of them.
    #[test]
    fn misses_are_not_pixels_but_still_name_the_frame() {
        assert_eq!(parse_pixel_line("none"), None);
        assert_eq!(parse_pixel_line("480x330:out-of-frame"), None);
        assert_eq!(parse_pixel_line("480x330:42"), None);
        assert_eq!(parse_frame_size("480x330:out-of-frame"), Some((480, 330)));
        assert_eq!(parse_frame_size("480x330:240 165 20 18 24 255"), Some((480, 330)));
        assert_eq!(parse_frame_size("none"), None);
    }
}
