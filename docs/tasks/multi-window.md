# 多窗口支持 — 对齐 Compose Desktop Window

> 任务 ID：`2c10c0bc4c4e`  
> 对标：Compose Multiplatform `application { Window() }`

---

## 一、Compose Desktop 模型

```kotlin
fun main() = application {
    Window(onCloseRequest = ::exitApplication, title = "主窗口") {
        Text("Hello")
    }
    if (showSettings) {
        Window(onCloseRequest = { showSettings = false }, title = "设置") {
            Button(onClick = { theme = "dark" }) { Text("暗色模式") }
        }
    }
}
```

**核心设计**：
- `application { }` — 入口作用域，持有事件循环
- `Window()` — composable 函数，声明式创建窗口
- 多窗口自然通过状态变量控制（`if (show) { Window(...) }`）
- 关闭窗口 = 改变状态（`showSettings = false`），而非调用命令式 API
- 每个窗口独立的 composable 树

---

## 二、Winia v2 当前状态

```rust
fn main() { app::run_app(counter_ui, 400.0, 300.0); }
```

- 单窗口硬绑定
- 无多窗口能力
- `AppState` 只持有单个 `VulkanSkiaWindow`

---

## 三、目标 API

```rust
use winia::app::{application, Window};

fn counter_ui(ctx: &mut ComposeCtx) {
    let show_about = ctx.remember(|| State::new(false));

    Column::new().build(ctx, |ctx| {
        Text::new("Counter").build(ctx);

        Button::new()
            .on_click({ let s = show_about.clone(); move || s.set(true) })
            .build(ctx, |ctx| { Text::new("About...").build(ctx); });
    });

    // 多窗口——声明式
    if show_about.get() {
        Window::new()
            .title("About")
            .size(300.0, 200.0)
            .on_close({ let s = show_about.clone(); move || s.set(false) })
            .build(|ctx| {
                Text::new("Winia v2").build(ctx);
            });
    }
}

fn main() {
    application(|| { Window::new().title("Counter").size(400, 300).build(counter_ui); });
}
```

---

## 四、架构改动

### 4.1 WindowManager

替代当前 `AppState` 的单一窗口管理：

```rust
struct WindowManager {
    windows: HashMap<WindowId, WindowEntry>,
    pending_events: Vec<WindowEvent>,
}

struct WindowEntry {
    composer: Composer,
    skia_window: VulkanSkiaWindow,
    title: String,
    width: f32, height: f32,
    scale_factor: f64,
    focused_id: Option<u64>,
}
```

### 4.2 Window composable

```rust
pub struct Window {
    title: String,
    width: f32,
    height: f32,
    on_close: Option<Arc<dyn Fn()>>,
}

impl Window {
    pub fn new() -> Self;
    pub fn title(self, t: impl Into<String>) -> Self;
    pub fn size(self, w: f32, h: f32) -> Self;
    pub fn on_close(self, f: impl Fn() + 'static) -> Self;

    pub fn build(self, content: impl Fn(&mut ComposeCtx) + 'static);
}
```

### 4.3 application 入口

```rust
pub fn application(f: impl FnOnce(&mut WindowManager) + 'static) {
    let mut wm = WindowManager::new();
    f(&mut wm); // 第一帧：创建初始窗口
    run_event_loop(wm);
}
```

### 4.4 新窗口创建时机

在 compose 期间，`Window::build()` 不立即创建 OS 窗口——只是**注册到 WindowManager 的新建队列**。下一帧 `RedrawRequested` 中创建实际窗口。

---

## 五、实施步骤

| 步 | 内容 |
|----|------|
| 1 | 设计文档（本文档） |
| 2 | 抽取 WindowEntry（composer + skia_window + 状态） |
| 3 | 实现 WindowManager 多窗口事件循环 |
| 4 | 实现 Window composable |
| 5 | 示例 + 编译测试 |
