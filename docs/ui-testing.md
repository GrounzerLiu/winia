# UI 测试（界面级集成测试）

通过真实运行 demo（Vulkan 窗口 + 渲染循环），向 demo 进程注入操作（点击/滚动/拖拽），
从树 JSON 断言界面状态——验证**重组/布局/多窗口**等真实行为，而非单元级行为。

## 架构

```
cargo test --test ui_test --features debug-server
        │
        ├─ tests/ui/mod.rs   UiTest 封装（进程管理 + 管道协议 + 断言辅助）
        └─ tests/ui_test.rs  场景测试（counter / nest_demo / …）
                │
                │ stdin 管道（命令）        stdout 管道（响应）
                ▼                             ▲
        ┌───────────────────────────────────────────┐
        │  demo 进程（target/debug/examples/*.exe）   │
        │  --features debug-server 编译（管道协议）    │
        └───────────────────────────────────────────┘
```

### 管道协议（debug-server feature，`winia/src/debug.rs`）

| 命令 | 作用 | 响应 |
|------|------|------|
| `c x y` | 点击（命中测试 + on_click） | `ok click` |
| `d x y` / `m x y` / `u` | 按下 / 移动 / 释放（拖拽选择） | `ok …` |
| `k <key>` | 键盘事件 | `ok key` |
| `s <dy>` | 滚动 | `ok scroll` |
| `t` | 树 JSON（**单行**，stdout 前缀 `TREE:`） | `TREE:[{window,root},…]` |
| `r` | 截图（stderr 打印路径） | — |
| `q` | 优雅退出（force_shutdown → 事件循环退出） | — |

- 命令走 **stdin 管道**（行分隔）；树响应走 **stdout 管道**（`TREE:` 前缀——测试按前缀过滤）。
- 树 JSON 为**多窗口格式**：`[{"window":<id>,"root":[...]}, …]`——每个窗口独立存储
  （`update_tree(window_id, json)`），互不覆盖；窗口关闭时 `remove_tree` 清理条目。
- 树 JSON 必须是**紧凑单行**：`build_node_json` 内部不输出换行（多行会被 println 拆散，
  UI 测试按行读无法拼回完整 JSON——历史教训）。
- 树 JSON 中的 NaN/Inf 格式化为 0（serde_json 拒绝 NaN——历史教训）。
- DevTools 注入事件（c/d/m/u/k/s）在 `new_events` 兜底消费——多窗口下主窗口在后台时
  RedrawRequested 不来（window_event 不调用）→ 事件卡队列（历史教训：子窗口打开后
  第二次点击失效）。

### UiTest 封装（`tests/ui/mod.rs`）

- `launch("counter")`：spawn exe（**先 taskkill 同名残留**——测试中断的孤儿进程防累积）、
  等待首帧树（20s 超时，带诊断）。
- **串行锁**：所有 UI 测试共用 `TEST_SERIAL` Mutex——taskkill 清理会误杀并行测试刚启动的
  进程（历史教训：第一版并行 + taskkill = 连环互杀全挂）。
- `Drop`：发 `q` 优雅关闭 → 3s 限时等待 → 超时 kill（测试失败也保证无残留窗口）。
- 查询：`tree()`（t 命令）、`find(label)`（找节点绝对坐标）、`all_texts()`（全部 mod 文本）。
- 断言：`expect_text` / `expect_text_timeout`（轮询 5s——异步重组）、`expect_no_text`。
- **`click_until(x, y, timeout, cond)`**：点击 + 轮询树直到条件满足；点击丢失自动重试
  （最多 3 次）——**debug 点击链路的已知可靠性问题**（见下）。

## 已知限制（debug 模拟链路）

1. **点击偶发丢失 / 树刷新延迟**：debug 注入走 `queue_event`，在 winit `Wait` 模式下
   `RedrawRequested` 偶发不来（渲染断）→ 状态已变但树未刷新。**真实鼠标正常**（仅模拟链路）。
   → 测试一律用 `click_until`（自动重试）；断言用轮询版（expect_text_timeout）。
   已缓解：DebugEvent 在 `new_events` 兜底消费（多窗口后台主窗口也不卡队列）。
2. **多窗口树**：一次 `t` 查询返回**所有窗口**（按 window id 排序）——测试可同时断言
   主/子窗口内容（`window_count()` 辅助）。`find` 返回第一个匹配窗口的坐标（点击只注入
   父窗口——子窗口按钮不可点击，关闭走主窗口按钮）。
3. **端口隔离**：各测试用独立 debug 端口（9100 起递增），不占用默认 9998。
4. stderr 管道**必须被读线程消费**（不读会填满 64KB 阻塞 demo——历史教训）。

## 运行

```bash
cd winia
# 需要先构建带 debug-server 的 demo exe（ui_test 直接 spawn exe，不触发 cargo 构建）
cargo build -p winia --examples --features debug-server
# 跑 UI 测试（串行 ~25s）
cargo test --test ui_test --features debug-server
# 单测全量（含 UI 测试）
cargo test --features debug-server
```

环境变量：`UI_TEST_STDERR=1` 转发 demo stderr 到测试输出（诊断用）。

## 新增场景测试

1. 在 `winia/examples/` 写 demo（含可点击按钮 + 可断言文本）。
2. `cargo build -p winia --example <name> --features debug-server` 确认构建。
3. 在 `tests/ui_test.rs` 加 `#[test] fn …`：
   - `launch("<name>")` → `expect_text` 等首帧 → `find` 定位按钮 → `click_until` 交互 →
     `tree()` / `all_texts()` 断言。
   - 按钮坐标用 `find` 返回的 rect 中心（`x + w/2, y + h/2`）。
4. 跑测试 + 检查残留进程（`Get-Process <name>` 应为 0）。

## 测试清单（当前 5 个场景）

| 测试 | demo | 覆盖 |
|------|------|------|
| counter_click_increments_count | counter | 点击 → State 更新 → 重组渲染 |
| counter_toggle_alt_shows_hides | counter | 条件分支结构切换（Show Alt ↔ Add 10） |
| counter_sub_window_open_close | counter | 多窗口开/关 + 主窗口完整 |
| counter_scroll_moves_content | counter | 滚动容器 |
| nest_demo_structure_switch_stable | nest_demo | 多级 if/else 结构切换 3 态循环——节点数稳定 + 按钮不漂移 |
