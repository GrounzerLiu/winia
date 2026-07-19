# [P1] [multi-window] 每个窗口独立内容 + Counter 演示

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度


| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. PerWindow 持有独立 content | ⏳ 待开始 | Box dyn Fn(&mut ComposeCtx) 替代全局 content，open_window 接收 content |
| 2. Window composable 传递内容 | ⏳ 待开始 | Window::build(ctx, content) 打包 content 排队 |
| 3. Counter 示例：按钮打开第二个窗口 | ⏳ 待开始 | 按钮1：计数器 按钮2：打开新窗口（独立 UI） |

- **进度类型**：sequential（顺序）
- **完成进度**：0/3（当前：1/3）

