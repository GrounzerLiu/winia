# [P2] [multi-window] 多窗口支持 — Window composable + 每窗口独立 Composer

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度

| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. 设计 Window composable 函数 | ⏳ 待开始 | 对齐 Compose Window() API，研究 run_app 如何管理多窗口 |
| 2. App 多窗口状态管理 | ⏳ 待开始 | AppState 改为 window列表，每个窗口独立 Composer+skia_window |
| 3. Window composable 实现 | ⏳ 待开始 | 声明式创建窗口：if showSecond { Window { ... } } |
| 4. Counter 多窗口示例 | ⏳ 待开始 | 两个窗口：主窗口+弹出对话窗口 |
| 5. 编译测试 | ⏳ 待开始 | cargo test 全绿 + 两窗口同时显示 |

- **进度类型**：sequential（顺序）
- **完成进度**：0/5（当前：1/5）

<!-- followups-start -->

<!-- 后续事项表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 后续事项

| 事项 | 状态 | 备注 |
|------|------|------|
| Dialog 模态窗口 | 🔓 待定 | 类似 Compose DialogWindow，需要模态锁住父窗口 |
| 窗口间通信 | 🔓 待定 | 两个窗口共享 State？通过全局 EventLoopProxy 还是 State 订阅 |
| SystemTray 支持 | 🔓 待定 | 类似 Compose Tray composable |

<!-- followups-end -->
