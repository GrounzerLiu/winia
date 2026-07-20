# [P1] 实现 Window.on_close 回调

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度


| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. PerWindow 加 on_close: Option<Box<dyn FnMut() + Send>> | ✅ 完成 |  |
| 2. GLOBAL_PENDING 传递 on_close 回调 | ✅ 完成 |  |
| 3. Window::on_close(f) 存储回调 | ✅ 完成 |  |
| 4. CloseRequested 事件调用 on_close | ✅ 完成 |  |
| 5. Counter 示例验证：关闭子窗口→按钮恢复 | ✅ 完成 |  |

- **进度类型**：sequential（顺序）
- **完成进度**：5/5（当前：5/5）

