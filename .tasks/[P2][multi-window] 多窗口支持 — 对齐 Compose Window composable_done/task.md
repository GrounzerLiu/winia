# [P2] [multi-window] 多窗口支持 — 对齐 Compose Window composable

## 设计文档

### Compose 对照

```kotlin
fun main() = application {
    var showSecond by remember { mutableStateOf(false) }
    
    Window(onCloseRequest = ::exitApplication, title = "Main") {
        Button(onClick = { showSecond = true }) { Text("Open") }
    }
    
    if (showSecond) {
        Window(onCloseRequest = { showSecond = false }, title = "Second") {
            Text("Hello")
        }
    }
}
```

**关键设计**：
- `Window()` 是 composable 函数——窗口创建/销毁由 compose 状态驱动
- 每个 `Window` 有独立事件循环（同一个 `application` 下共享）
- `if (condition) { Window(...) }` 声明式开关窗口
- 关闭窗口 = 状态变化 → recomposition → `if` 分支不进入 → 窗口销毁

### Winia v2 方案

```
AppState {
    windows: HashMap<WindowId, PerWindow>
}

PerWindow {
    composer: Composer,
    skia_window: VulkanSkiaWindow,
    width: f32, height: f32,
    scale_factor: f64,
}

Window composable {
    build(ctx) → 在 AppState 注册新 PerWindow
    close → 从 windows map 移除
}
```

**流程**：

```
run_app(main_ui) → 创建主窗口
  │
  ├── PointerButton → on_click { show_second = true } → request_redraw
  │
  └── RedrawRequested:
        compose → 主 UI → if show_second { Window.build(ctx) }
        → AppState.windows 新增 second window_id
        → 下帧 event loop 创建 VulkanSkiaWindow + 首次渲染
```

**事件循环**：

```
event_loop.run_app {
  for (window_id, per_window) in state.windows {
      match event {
          WindowId匹配 → 该窗口 RedrawRequested → compose + layout + render
      }
  }
  // 新窗口延迟创建（在 can_create_surfaces 中处理）
}
```

### 新旧窗口管理对比

| | Compose | Winia v2 |
|---|---|---|
| 声明方式 | `Window { }` composable | `Window::new().build(ctx)` |
| 关闭 | `onCloseRequest` 回调 + 状态 | `close()` 方法 |
| 事件循环 | `application { }` 自动分派 | `WindowId` 匹配手动分派 |
| 多窗口状态 | `mutableStateListOf` | `HashMap<WindowId, PerWindow>` |

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度



| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. 设计文档 + Compose 对照 | ✅ 完成 | task.md 开头写完整设计文档，对照 Compose Window() API
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |
| 2. AppState 支持多窗口 | ✅ 完成 | windows: HashMap WindowId - (Composer, SkiaWindow)
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |
| 3. Window composable | ✅ 完成 | Window 组件：声明式创建-销毁窗口
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |
| 4. run_app 多窗口事件循环 | ✅ 完成 | 每个窗口独立 RedrawRequested
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |
| 5. Counter 多窗口示例 | ✅ 完成 | 按钮打开新窗口
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |
| 6. 编译测试 | ✅ 完成 | cargo test 全绿
app.rs 重构为多窗口架构 HashMap WindowId → PerWindow，Window composable + open_window 全局排队，can_create_surfaces 消费创建 |

- **进度类型**：sequential（顺序）
- **完成进度**：6/6（当前：6/6）

