# UI 测试（界面级集成测试）

通过真实运行**测试专用 fixture**（Vulkan 窗口 + 渲染循环），向 fixture 进程注入操作
（点击/滚动/拖拽），从树 JSON 断言界面状态——验证**重组/布局/多窗口**等真实行为。

**Fixtures are separate from examples**: examples only demonstrate; the UI suite uses the scenarios
under `tests/ui_fixtures/`, one responsibility per case (Given/When/Then). Every scenario compiles
into the SAME `fixture_all` binary (selected by `argv[1]`) and the harness spawns one process per
case — isolation is unchanged, but the whole suite links skia once (see "adding a scenario").

## 架构

```
cargo test --features debug-server
        │
        ├─ tests/ui/mod.rs       UiTest 封装（进程管理 + 管道协议 + 断言辅助）
        ├─ tests/ui_test.rs      scenario assertions (25 cases)
        ├─ tests/ui_fixtures/    fixture sources + fixture_all.rs (the single dispatcher)
        └─ tests/{event_flow,layout_snapshot,render_snapshot}.rs  库行为快照测试
                │
                │ stdin 管道（命令）        stdout 管道（响应）
                ▼                             ▲
        ┌───────────────────────────────────────────┐
        │  fixture process (target/debug/fixture_all.exe)│
        │  built with --features debug-server (pipe)   │
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
- **Popup content** has its own three entry points, because the main-tree lookups deliberately
  skip popup entries (`expect_no_text` and friends are about the main window):
  `find_tag_in_overlay(tag)` — in WINDOW coordinates, since a popup's tree is in its own
  coordinates and the debug tree therefore emits each popup entry's `screen` origin —
  `click_overlay_tag(tag)` and `overlay_tag_is_focused(tag)`.
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
cargo build --bin fixture_all --features debug-server
# 跑全部测试（含 UI 测试——串行 ~60s，见下方 load sensitivity）
cargo test --features debug-server
# 只跑 UI 测试
cargo test --features debug-server --test ui_test
```

环境变量：`UI_TEST_STDERR=1` 转发 fixture stderr 到测试输出（诊断用）。

⚠ Always pass `--features debug-server`. Without it the fixture binary is relinked *without* the
debug server, and every UI test then fails in the worst possible way: the child process stays alive
but is silent (no `TREE:` on stdout, no stderr), so `launch` burns all its attempts and panics with
`no first frame within 20s. alive=true, TREE responses=0, stdout so far: (empty)` — it looks like an
environment/window problem, not a build-flag problem.

⚠ The synthetic `c x y` click (`DebugEvent::Click`) is NOT the gesture path. It fires `on_click` and
focus, plus — in a popup — the full overlay sequence including the gesture up, but on the MAIN tree
it never enters `gesture_down`, so `on_tap` / `on_double_tap` / `on_long_press` cannot be driven with
it. Use explicit `d x y` / `u x y` for anything that lives on the gesture path (that is why every
fixture taps clickable buttons with `click_tag` but drives sliders and tap zones with down/up).

⚠ An overlay that dismisses on an outside press (`dismiss_on_outside` — every modal overlay, and a
`Popup` by default) CONSUMES the press that closes it: it does not reach the main tree. So a ui test
cannot drag main-tree content while such an overlay is open — the gesture's first press disappears
and the drag looks like it "did not arrive" (measured while writing `fixture_popup_drag`). Either
keep the gesture inside the overlay, or keep the overlay open with
`Popup::dismiss_on_outside(false)` / `Dialog::dismiss_on_outside(false)`; `click_passthrough`
tooltips are the one overlay that deliberately lets the press through.

## 新增场景测试（按测试用例设计原则）

1. Add a fixture file under `tests/ui_fixtures/` (**one scenario**, readable as Given/When/Then). It is
   NOT its own binary: it is a module of `fixture_all`, and its `pub fn main()` is what the dispatcher
   calls (it starts the event loop and never returns).
   ```rust
   //! UI-test fixture: <what the scenario is>
   use winia::prelude::*;
   #[composable]
   fn ui(ctx: &mut ComposeCtx) { /* the minimal scenario UI */ }
   pub fn main() { /* tokio rt + winia::run_app!(Window::new()...) */ }
   ```
2. Register it in `tests/ui_fixtures/fixture_all.rs`: a `#[path = "fixture_<name>.rs"] mod <name>;` line
   and one row in the `SCENARIOS` table (`("<name>", <name>::main)`). That table is the registry — it
   drives the dispatch and the usage message, so those two edits are all that is needed.
   ⚠ Each scenario still runs in its OWN PROCESS (the harness spawns `fixture_all.exe <name>` per test),
   so isolation is unchanged. What changed is the build: a scenario used to be its own `[[bin]]`, i.e.
   its own full link against skia plus its own ~30 MB executable and ~125 MB PDB (measured at 18
   fixtures: 538 MB of exes + 2.26 GB of PDBs, and every lib change relinked all of them — cargo links
   in parallel, which is where the memory spike came from). Now one scenario costs a module and a table
   row, and the whole suite links skia once.
3. Add the case to `tests/ui_test.rs`: `UiTest::launch("<name>")` → `expect_text` for the first frame →
   `find`/`find_tag` → `click_until` (retries a lost click) → assertions.
4. Build and run:
   ```bash
   cargo build --bin fixture_all --features debug-server   # the only fixture target while iterating
   cargo test -p winia --features debug-server --test ui_test <case name>
   ```
5. Run the suite and check for leftovers (`Get-Process fixture_all` should be empty).

## 测试清单

### UI 集成（fixture 驱动）

| Case | launch name (`UiTest::launch`) | Coverage (Given/When/Then) |
|------|---------|------------------------|
| click_updates_state_and_keeps_structure | click | 点击 +1 三次 → Count 更新；静态行不塌缩 |
| toggle_switches_conditional_branch_exclusively | toggle | 切换两次 → 分支 A ↔ B 互斥 |
| subwindow_open_close_preserves_main_and_trees | subwindow | 多窗口开/关 → 两窗口树共存（window_count 1→2→1） |
| scroll_container_keeps_content | scroll | 滚动 ± → 内容保持不崩溃 |
| nest_structure_switch_cycles_stably | nest | 3 态循环 6 次 → 状态重复进入节点数一致 + 按钮不漂移 |
| text_field_focus_input_and_backspace_update_state | text_field | 点击聚焦、逐字符输入、Backspace 与状态文本重组 |
| text_field_password_and_multiline_states_update | text_field | 密码掩码状态、长度阈值、Enter 多行与 minLines 几何 |
| text_field_error_readonly_and_disabled_states_are_enforced | text_field | error 解除、read-only/disabled 输入约束与焦点语义 |
| top_app_bar_variants_collapse_and_restore_with_scroll | top_app_bar | Medium/Large 高度、共享滚动 offset 折叠与回滚恢复 |
| scaffold_fab_clicks_and_rtl_mirrors_without_changing_content_inset | scaffold | FAB 点击、content inset 与 RTL BottomEnd 镜像 |
| a_popup_tap_zone_fires_the_tap_family_like_the_main_tree | popup_tap | tap + long-press in both arenas, driven with down/up |
| a_tap_survives_its_own_popup_moving | popup_slide_tap | the popup jumps 300 px on press; the tap must still fire (frozen arena origin) |
| a_popup_drag_target_also_taps_once | popup_tap | a tap+drag card in a popup: tap fires once, drag fires one start/one end (no double dispatch) |
| a_popup_double_tap_zone_fires_and_defers_its_single_tap | popup_tap | a popup's deferred single tap and double tap (`PendingTap` arena routing) |
| a_drag_inside_a_popup_reaches_the_same_value_as_in_the_main_tree | popup_drag | popup drag callbacks receive layer-local coordinates |
| a_modal_dialog_with_dismiss_on_outside_false_stays_open | dialog_dismiss | `dismiss_on_outside` honoured for a modal overlay, and the press still consumed |
| an_overlay_press_zone_receives_the_press_gesture | overlay_focus | popup pointer-down dispatches the press gesture |
| clicking_an_overlay_button_does_not_steal_focus | overlay_focus | a popup clickable does not take focus from the field beside it |
| range_slider_drags_the_thumb_the_press_resolved | range_slider | a real drag moves the nearer thumb only, and the value lands where the inset axis says |
| range_slider_keyboard_moves_the_focused_thumb | range_slider | a press hands focus to the resolved thumb, `k Arrow*` moves it, `k Tab` switches to the other |

### Load sensitivity (what the suite tolerates)

Every case drives a real window, so the suite inherits the machine's timing. Measured:

| Machine state | Result |
|---|---|
| idle (32 cores) | 25/25, ~63 s |
| 16 CPU burners (half the cores) | 25/25, ~78 s |
| 40 CPU burners (app 2.4-6x slower) | 23-24/25 — individual timing-sensitive cases fail |

(The load runs behind these numbers are 40 *processes* on 32 cores; Python threads would not do —
they share one GIL and barely load the machine.)

What the harness does about it:

- `launch` retries the whole spawn (`LAUNCH_ATTEMPTS = 3`, `FIRST_FRAME_TIMEOUT = 20 s` each) and warns
  when a first frame took longer than 3 s, because a saturated machine is the usual explanation for a
  red run. Every interaction retry in the suite uses the same count of 3 (it is a per-helper literal,
  not a shared constant).
- Interactions that the debug path can DROP are retried by the case that needs them: `click_until`,
  `click_tag_and_type_until`, and the burst-driven gesture helpers (a fixed sleep between `d` and `u`
  turns a tap into a long press under load — see the gesture tests).
- The drag/fling cases retry the gesture and require both halves (the scroll moved AND the fling
  advanced) in one attempt.
- What is NOT protected: assertions that depend on a *rate* (fling velocity, animation progress) or on
  a single interaction landing. Under a 40-burner load those can fail even with retries; each failure
  names what did not happen, so a red run there means "the machine was saturated", not "a regression".

### 库行为快照（无窗口，直接驱动 Composer）

| 文件 | 覆盖 |
|------|------|
| tests/event_flow.rs | State 变化 → 增量重组/布局重算/remember 持久/跳过 |
| tests/layout_snapshot.rs | 布局断言（leaf/column/row/padding/fill） |
| tests/render_snapshot.rs | 渲染快照（文本/圆角矩形/嵌套不崩溃） |
