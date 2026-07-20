# [P1] 实现声明式 Window composable（if Window { ... }）

## 设计文档

### 架构

```rust
// 用户预期写法
if show_window.get() {  // ← show_window: State<bool>, 声明式控制生命周期
    Window::new()
        .title("独立窗口")
        .size(300.0, 200.0)
        .on_close(move || show_window.update(|v| *v = false))
        .build(ctx, |ctx| {
            Text::new("Hello").build(ctx);
            Button::new().text("Click").build(ctx);
        });
}
```

### 组件

| 组件 | 位置 | 职责 |
|------|------|------|
| `WindowState` | `modifier.rs` | 声明式窗口参数（title, width, height, visible 等） |
| `WindowHandle` | `modifier.rs` | `Rc<RefCell<Option<WindowId>>>`——记录窗口是否已创建，Drop 时发关闭请求 |
| `Window` Builder | `ui/window.rs` | builder 链式 API：`.title()` `.size()` `.on_close()` `.build(ctx, \|ctx\| {})` |
| `app::open_window` | `app.rs` | 已存在——push (width, height, content) 到 GLOBAL_PENDING |
| `app::close_window` | `app.rs` | 新增——通过 WindowId 关闭（设置 `visible = false` / 移除） |

### 生命周期

```
组合存在 ───→ build() 调 app::open_window ──→ PerWindow 创建
     │                                              │
     │ show_window = false                           │ 用户点 ×
     │                                              │
     ├── if 移出组合 ─→ WindowHandle::drop() ──→ close_window()
     └── on_close 回调 ─→ set show_window=false ─→ 同上
```

### 关键设计决策

1. **Window content 捕获父组合 State**：`State<T>` 使用 Arc，跨 PerWindow 共享安全
2. **Slot Table 隔离**：每个 PerWindow 有独立 Composer，不影响主窗口
3. **WindowHandle::drop**：使用 `Rc<RefCell<Option<WindowId>>>`，Drop 时通过 GLOBAL_PENDING 发关闭事件
4. **与现有 open_window 兼容**：`Window::build` 底层复用 `app::open_window`

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度




| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. 创建 WindowState + WindowHandle 基础结构 | ✅ 完成 |  |
| 2. 实现 Window composable Builder（ui/window.rs） | ✅ 完成 |  |
| 3. Window::build 调用 app::open_window 传递 content | ✅ 完成 |  |
| 4. 注册 on_close_request 回调：状态切换 → 移出组合 | ✅ 完成 |  |
| 5. Counter 示例展示 if-else 声明式多窗口 | ✅ 完成 |  |
| 6. 编译测试 31/31 通过 | ✅ 完成 |  |

- **进度类型**：sequential（顺序）
- **完成进度**：6/6（当前：6/6）

