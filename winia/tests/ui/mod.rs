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

/// UI 测试串行锁——launch 时 taskkill 清理残留会误杀并行测试刚启动的进程，
/// 且多个 demo 窗口同开干扰（焦点/输入）。串行执行（5 个测试 ~20s 可接受）。
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
    /// 启动测试 fixture（`tests/ui_fixtures/` 下，`[[bin]]` 构建的独立 exe——
    /// 不依赖 examples；cargo test 不执行 bin，仅构建）。exe 路径固定
    /// `target/debug/fixture_<name>.exe`。
    /// 注意：持有全局串行锁（防 taskkill 互杀 + 窗口干扰）——UiTest drop 释放。
    pub fn launch(fixture: &str) -> Self {
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 定位 fixture exe（workspace 共享 target——在 crate 目录上一级）
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crate 不在 workspace 根下一级");
        let exe = workspace_root.join("target/debug").join(format!("fixture_{fixture}.exe"));
        if !exe.exists() {
            panic!(
                "fixture 未构建：请先 `cargo build --bins --features debug-server`（exe: {}）",
                exe.display()
            );
        }
        let exe_name = format!("fixture_{fixture}.exe");

        // 清理同名残留进程（测试中断/上次失败可能留下孤儿 fixture——防窗口累积）。
        // 注意：仅在持有串行锁时执行（并行会互杀）；若残留进程占着 exe 文件锁，
        // 等待其退出后再 spawn。
        let _ = Command::new("taskkill")
            .args(["/f", "/im", &exe_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_millis(500));

        let mut child = Command::new(&exe)
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

        // 等待首帧树（demo 渲染第一帧后 t 才有内容）
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut tree = Value::Null;
        let mut tree_responses = 0u32;
        loop {
            if child.try_wait().ok().flatten().is_some() {
                panic!("demo 进程提前退出（检查 exe 构建/窗口环境）");
            }
            let (t, received) = query_tree(&mut child_stdin, &rx, Duration::from_millis(300));
            if received {
                tree_responses += 1;
            }
            if let Some(t) = t {
                // 就绪 = 至少一个窗口的树（空数组 `[]` 是 DEBUG_STATE 未渲染——继续等）
                let has_window = t
                    .as_array()
                    .map(|arr| arr.iter().any(|w| w.get("root").is_some()))
                    .unwrap_or(false);
                if has_window {
                    tree = t;
                    break;
                }
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let mut buf = String::new();
                while let Ok(line) = rx.try_recv() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
                let alive = child.try_wait().ok().flatten().is_none();
                panic!(
                    "等待 demo 首帧树超时。进程存活={alive}。TREE 响应数={tree_responses}。stdout 已收: {}",
                    if buf.is_empty() { "(空)" } else { &buf[..buf.len().min(300)] }
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        // 解析窗口尺寸（根节点 size）
        let (width, height) = tree_size(&tree);
        Self { child, child_stdin, stdout_rx: rx, _serial, tree, width, height }
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

    /// 深度遍历树，收集所有 mod 描述文本
    pub fn all_texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_texts(&self.tree, &mut out);
        out
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
        if let Err(e) = stdin.write_all(b"t\n") {
            panic!("stdin 写入失败（demo 读端已关闭？）: {e}");
        }
        if let Err(e) = stdin.flush() {
            panic!("stdin flush 失败: {e}");
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
    if let Some(arr) = tree.as_array() {
        for w in arr {
            // 跳过弹出层条目（带 "overlay" 字段）——主树断言不应被浮层内容干扰
            // （expect_no_text 等语义针对主窗口；弹层断言用 overlay_count）
            if w.get("overlay").is_some() {
                continue;
            }
            if let Some(root) = w.get("root") {
                let id = w.get("window").and_then(|v| v.as_u64()).unwrap_or(0);
                f(id, root);
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
    for_each_window(tree, |_, root| walk(root, out));
}


/// 查找 tag 精确匹配的节点，并返回绝对位置和尺寸。
fn find_node_tag(tree: &Value, tag: &str) -> Option<(f32, f32, f32, f32)> {
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
    for_each_window(tree, |_, root| {
        if found.is_none() {
            found = walk(root, 0.0, 0.0, tag);
        }
    });
    found
}

fn node_tag_is_focused(tree: &Value, tag: &str) -> bool {
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
    for_each_window(tree, |_, root| {
        if found.is_none() {
            found = walk(root, tag);
        }
    });
    found.unwrap_or(false)
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
