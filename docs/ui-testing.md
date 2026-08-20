# UI 测试（界面级集成测试）

通过真实运行**测试专用 fixture**（Vulkan 窗口 + 渲染循环），向 fixture 进程注入操作
（点击/滚动/拖拽），从树 JSON 断言界面状态——验证**重组/布局/多窗口**等真实行为。

**fixture 与 examples 分离**：examples 只做展示；UI 测试使用 `tests/ui_fixtures/` 下的
独立场景（`[[test]] harness = false`——cargo 编译为独立 exe），每个用例单一职责
（Given/When/Then 设计原则）。

## 架构

```
cargo test --features debug-server
        │
        ├─ tests/ui/mod.rs       UiTest 封装（进程管理 + 管道协议 + 断言辅助）
        ├─ tests/ui_test.rs      场景断言（click/toggle/subwindow/scroll/nest 5 用例）
        ├─ tests/ui_fixtures/    fixture 源码（harness=false 的 [[test]] → 独立 exe）
        └─ tests/{event_flow,layout_snapshot,render_snapshot}.rs  库行为快照测试
                │
                │ stdin 管道（命令）        stdout 管道（响应）
                ▼                             ▲
        ┌───────────────────────────────────────────┐
        │  fixture 进程（target/debug/deps/fixture_*）│
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
4. stderr 管道**必须被读线程消费**（不读会填满 64KB 阻塞 fixture——历史教训）。

## 运行

```bash
cd winia
# 需要先构建带 debug-server 的 fixture exe（ui_test 直接 spawn exe，不触发 cargo 构建）
cargo test --features debug-server --no-run
# 跑全部测试（含 UI 测试——串行 ~25s）
cargo test --features debug-server
# 只跑 UI 测试
cargo test --features debug-server --test ui_test
```

环境变量：`UI_TEST_STDERR=1` 转发 fixture stderr 到测试输出（诊断用）。

## 新增场景测试（按测试用例设计原则）

1. 在 `tests/ui_fixtures/` 写 fixture（**单一场景**——Given/When/Then 可读）：
   ```rust
   //! UI 测试 fixture：<场景描述>
   use winia::prelude::*;
   #[composable]
   fn ui(ctx: &mut ComposeCtx) { /* 最小场景 UI */ }
   fn main() { /* tokio rt + winia::run_app!(Window::new()...) */ }
   ```
2. `Cargo.toml` 注册为独立 fixture 二进制：
   ```toml
   [[bin]]
   name = "fixture_<name>"
   path = "tests/ui_fixtures/fixture_<name>.rs"
   ```
   `UiTest::launch("<name>")` 会启动 `target/debug/fixture_<name>`；fixture 不是由
   `cargo test` 直接执行的测试 target。
3. `tests/ui_test.rs` 加用例：`launch("<name>")` → `expect_text` 等首帧 → `find` 定位 →
   `click_until` 交互（点击丢失自动重试）→ 断言。
4. 跑测试 + 检查残留进程（`Get-Process fixture_*` 应为 0）。

## 测试清单

### UI 集成（fixture 驱动）

| 用例 | fixture | 覆盖（Given/When/Then） |
|------|---------|------------------------|
| click_updates_state_and_keeps_structure | fixture_click | 点击 +1 三次 → Count 更新；静态行不塌缩 |
| toggle_switches_conditional_branch_exclusively | fixture_toggle | 切换两次 → 分支 A ↔ B 互斥 |
| subwindow_open_close_preserves_main_and_trees | fixture_subwindow | 多窗口开/关 → 两窗口树共存（window_count 1→2→1） |
| scroll_container_keeps_content | fixture_scroll | 滚动 ± → 内容保持不崩溃 |
| nest_structure_switch_cycles_stably | fixture_nest | 3 态循环 6 次 → 状态重复进入节点数一致 + 按钮不漂移 |
| text_field_focus_input_and_backspace_update_state | fixture_text_field | 点击聚焦、逐字符输入、Backspace 与状态文本重组 |
| text_field_password_and_multiline_states_update | fixture_text_field | 密码掩码状态、长度阈值、Enter 多行与 minLines 几何 |
| text_field_error_readonly_and_disabled_states_are_enforced | fixture_text_field | error 解除、read-only/disabled 输入约束与焦点语义 |
| top_app_bar_variants_collapse_and_restore_with_scroll | fixture_top_app_bar | Medium/Large 高度、共享滚动 offset 折叠与回滚恢复 |
| scaffold_fab_clicks_and_rtl_mirrors_without_changing_content_inset | fixture_scaffold | FAB 点击、content inset 与 RTL BottomEnd 镜像 |

### 库行为快照（无窗口，直接驱动 Composer）

| 文件 | 覆盖 |
|------|------|
| tests/event_flow.rs | State 变化 → 增量重组/布局重算/remember 持久/跳过 |
| tests/layout_snapshot.rs | 布局断言（leaf/column/row/padding/fill） |
| tests/render_snapshot.rs | 渲染快照（文本/圆角矩形/嵌套不崩溃） |
