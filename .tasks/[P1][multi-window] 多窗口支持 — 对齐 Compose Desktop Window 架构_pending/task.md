# [P1] [multi-window] 多窗口支持 — 对齐 Compose Desktop Window 架构

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度


| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. 写多窗口设计文档 | ⏳ 待开始 | 对齐 Compose Window composable，设计 WindowManager + 多 Composer |
| 2. 重构 run_app 为 application 入口 | ⏳ 待开始 | 类似 Compose application {} 作用域，管理多窗口生命周期 |
| 3. 实现 Window composable | ⏳ 待开始 | 声明式创建窗口，窗口内独立的 composable 树 |
| 4. 实现窗口间通信 | ⏳ 待开始 | 跨窗口共享 State 或事件通道 |
| 5. 编译测试 + Counter 多窗口示例 | ⏳ 待开始 |  |

- **进度类型**：sequential（顺序）
- **完成进度**：0/5（当前：1/5）

## 设计文档

详见 `docs/tasks/multi-window.md`
