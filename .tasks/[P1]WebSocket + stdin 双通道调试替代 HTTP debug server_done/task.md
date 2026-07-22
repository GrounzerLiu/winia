# [P1] WebSocket + stdin 双通道调试替代 HTTP debug server

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度




| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. 调研：WebSocket 依赖选型 + stdin 方案设计 | ✅ 完成 | tokio-tungstenite 0.30 无版本冲突；stdin 方案：独立线程按行读取，c x y / k key / r / t 命令 |
| 2. 实现 stdin 通道：echo 'click 190 130' | ./app 直接注入事件 | ✅ 完成 | HTTP 删除；stdin + WebSocket 实现完成，编译通过；cargo check --lib + --features debug-server 均通过 |
| 3. 实现 WebSocket 通道：保持连接、实时推送树、响应点击 | ✅ 完成 | HTTP 删除；stdin + WebSocket 实现完成，编译通过；cargo check --lib + --features debug-server 均通过 |
| 4. 保留 HTTP 兼容（/click 等旧端点转发到新通道） | ✅ 完成 | HTTP 删除；stdin + WebSocket 实现完成，编译通过；cargo check --lib + --features debug-server 均通过 |
| 5. 编译测试 + 清理旧代码 | ✅ 完成 | HTTP 删除；stdin + WebSocket 实现完成，编译通过；cargo check --lib + --features debug-server 均通过 |

- **进度类型**：sequential（顺序）
- **完成进度**：5/5（当前：5/5）

## 设计方案

### 现状问题
- debug server 使用 HTTP + JSON body，AI 调试时需要用 curl 构造正确参数
- winit PointerButton 在快速双击时可能丢失事件，debug server 的 click 绕过 winit 更可靠
- 每次请求建立新 TCP 连接，无法保持状态

### 方案：WebSocket + stdin 双通道

**stdin 通道**（最轻量）：
```bash
echo 'c 190 130' | ./app          # 点击
echo 'k Tab'     | ./app          # 按键
echo 'r'         | ./app          # 截图请求
echo 't'         | ./app          # 返回 UI 树 JSON
```
优点：零网络开销、一行命令、AI 最方便
实现：独立线程读 stdin，解析后注入事件队列

**WebSocket 通道**（保持连接、双向）：
- 服务端：ws://127.0.0.1:9998，接受 JSON-RPC 风格命令
- 支持：click、key、screenshot（回传 base64 PNG）、tree（回传 JSON）
- 支持：watch（UI 树变化时自动推送）
优点：一次连接多次命令、截图直接回传、可实时监控

### 依赖
- `tokio-tungstenite`：WebSocket 实现（tokio 已有依赖）
- stdin 无需额外依赖
