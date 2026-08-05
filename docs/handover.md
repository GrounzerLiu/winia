# Winia v2 — 接手文档（现状 / 问题 / 调试）

> 本文档面向**后来者**：读完即可上手开发、定位问题、跑调试。
> 与 `docs/architecture.md`（早期设计稿，部分过时）和 `docs/developer-guide.md`（入门概念，部分过时）互补。
> 最后更新：2026-08（`text-field` 分支 HEAD `4edd48d`）

---

## 0. 三分钟速览

- 这是一个 **Rust 声明式 GUI 框架**，对标 Jetpack Compose：`winit`（窗口/事件）+ `skia-safe`（绘制）+ 自研组合引擎。
- 核心文件就几个：`winia/src/core/composer.rs`（2952 行，组合引擎）、`src/app.rs`（1348 行，事件循环）、`src/layout/node.rs`（1169 行，布局）、`src/modifier.rs`（1307 行，修饰符）、`src/animation.rs`（1052 行，动画）。
- 构建：`cargo build -p winia`；测试：`cargo test --lib`（当前 162 个）；跑 demo：`cargo run -p winia --example counter`。
- 调试：`--features debug-server` 开启 WebSocket（9998 端口）调试通道，可模拟点击/滚动/截图/读树。
- 主线分支：`text-field`。各功能分支见 §1。

---

## 1. 仓库与分支

```
D:\Projects\winia\
├── winia/          UI 框架核心（crate winia）
├── winia-macros/   #[composable] 过程宏（函数级语句注入）
├── skiwin/         Skia 渲染后端（Vulkan/GL/D3D/CPU）
├── tools/          辅助脚本
└── docs/           文档
```

| 分支 | 状态 | 说明 |
|------|------|------|
| `text-field` | **主线** | TextField、选择、键盘事件等已合入，当前工作基座 |
| `composition-separation` | 已合并进 text-field 前的研究线 | 组合/布局分离（compose 写 desc → materialize 建树） |
| `animation-improve` | **已放弃** | AnimatedVisibility 布局动画实验（shrink/expand）。**保留不删**——里面有 `force_remeasure`/`ShrinkPolicy`/`roll_out` 的完整实现可参考，但机制本身是框架缺陷（见 §3.1），不要直接搬回主线 |
| `scope-research` / `selection-api` / `vsync-research` / `v2` | 历史 | 各自主题的研究线，均已合入或弃用 |

**接手提示**：新工作一律从 `text-field` 开新分支；`animation-improve` 的教训（布局动画为什么痛苦）见 §3。

---

## 2. 框架现状（真实架构）

### 2.1 一帧的完整流程

```
事件循环（app.rs ApplicationHandler）
  └─ new_events / window_event
       └─ recompose_layout_render(after_draw)   ← 每帧主入口（app.rs:106）
            ├─ compose()     用户 composable 执行 → SlotTable（组合树）
            ├─ materialize() SlotTable → arena 布局树（LayoutNode）
            ├─ layout()      递归 measure_node → placement
            └─ render()      render_pass1 遍历 LayoutNode 树 → Skia Canvas
```

状态变化触发链路：`State::set()` → `notify_state_changed` → 标记 slot dirty + `WAKE_FN`（wake 事件循环）→ 下一帧 `AboutToWait`/`new_events` 里 `request_redraw()` → 重跑上链。

### 2.2 组合引擎（core/composer.rs）

- **组合/物化分离**（composition-separation 的成果）：`compose()` 只把用户代码的执行结果写进 `SlotTable`（slot 树，含 key/dirty/依赖），`materialize()` 再把 slot 树转成 arena 布局树（`Vec<LayoutNode>` + `Vec<Box<dyn MeasurePolicy>>` + root index）。
- **key 系统**：`ctx.next_key()` / `ctx.next_group_key()` / `ctx.next_remember_key()`——key = 位置折叠（父 scope hash + 子索引），不是全局计数器（计数器曾导致兄弟节点 key 碰撞）。`#[composable]` 宏注入 `start_scope_keyed(source_hash)`（函数签名 hash），保证函数级 scope 稳定。
- **增量重组（Skip）**：`start_restartable_group` / `end_restartable_group` 返回 `GroupStatus`——slot clean 时跳过用户闭包执行，从 `prev_node_by_key: HashMap<Vec<usize>, CachedNode>` 恢复子树（`replay_clean_subtree`）。`CachedNode` 缓存 modifier/measured_size/cached_constraints/position/slot_key。
- **dirty 传播**：`SlotTable::mark_dirty(key)` 沿祖先链标 dirty（含 scope 子树整体失效）。
- **副作用注册**：`slot_deps: HashMap<u64, HashSet<u64>>`（state_id → slot_key），compose 末尾遍历 modifier 注册依赖；`set_active_slot_key` 桥接布局期 `State::get()` 的依赖注册。
- **State 依赖的自动化**：compose 末尾 `register_modifier_deps_recursive` 扫描 modifier 里的闭包（graphics_layer/size 动态闭包等）读取的 State 并注册依赖——所以**动画值变化会 dirty 对应节点**。

### 2.3 State（core/state.rs）

- `State<T>`：Arc 内部值 + id + notify 版本。**必须通过 `ctx.remember()` 创建**（注册到全局队列映射）；直接 `State::new` 的 notify 不会推送到 composer（测试里踩过坑）。
- 写方法家族（区分重要）：
  - `set()` —— 值变化才 notify + wake（标准写法）
  - `set_no_wake()` —— notify 但不 wake 事件循环（同帧内继续用新值，避免二次 compose）
  - `set_silent()` —— 改值**不 notify**（内部标记用，如窗口 created_id）
  - `set_visual()` —— 只更新视觉不触发重组
  - `peek()` —— **读但不注册依赖**（动画值布局期读取的关键手段）
- `WAKE_FN`（`set_wake_fn`，app.rs:1335 注入 EventLoopProxy）+ `wake_loop()`：异步 State 变更后唤醒事件循环。

### 2.4 布局（layout/node.rs + layout/*.rs）

- 三阶段：`Constraints → measure() → place()`，`MeasurePolicy` trait（measure 返回 `(Size, Vec<Placement>)`）。
- `LayoutNode` 含 `dirty: bool` + `cached_constraints: Option<Constraints>`——**常量折叠**：`!dirty && cached_constraints == Some(constraints)` 直接复用（node.rs:749）。
- 布局策略：`ColumnLayout`/`RowLayout`（flex.rs 泛型 `measure_flex<A: FlexAxis>`）/`BoxLayout`。
- `hit_test(root, x, y)` 深度优先命中；focus 遍历用 slot_key（跨重组稳定）。
- 尺寸单位：`unit.rs` 的 `Dp`/`Sp`/`Px`/`Offset`/`Size`，`current_density()` 全局密度。

### 2.5 渲染（render.rs + skiwin）

- `render_pass1(nodes, root, canvas, ...)` 递归绘制：modifier 链（background/border/clip/text/graphics_layer/backdrop blur）。
- `graphics_layer`：alpha/scale/rotation/translation 变换——**只重绘不重排**（动画的主要渲染通道）。
- 文本：`text/paragraph.rs`（Paragraph 构建+布局）、`text/index_bimap.rs`（UTF-16↔UTF-8 双向映射）、`text/inline_drawable.rs`（占位符/图片）、`text/text_layout.rs`。`LayoutNode.cached_paragraph: RefCell<Option<Paragraph>>` 缓存测量结果。
- skiwin：Vulkan（默认）/GL/CPU，`vulkan/capture.rs` flush 后读回像素（debug 截图通道）。

### 2.6 动画（animation.rs）

- `AnimationSpec { Tween, Spring, Snap, Keyframes, Repeatable }` + `interpolator.rs` 21 种插值器（查表实现）。
- `Animatable<T>`：`animate_to` + `update()` 推进；`push_animation` 全局列表按帧驱动；**同 state 去重**（新意图替换旧动画，否则 exit 动画 on_done 丢失）。
- `Transition<T>` / `InfiniteTransition`（remember_infinite_transition）/ `animate_float_as_state` 等派生动画 API。
- `AnimatedVisibility`（ui/animated_visibility.rs）：visible 切换 → alpha State 动画 + graphics_layer 渲染 + 退出后延迟移除。**⚠ 布局动画版（shrink/expand/force_remeasure）在 animation-improve 分支，已知框架缺陷，勿直接合入**。

### 2.7 输入与焦点（app.rs + modifier.rs）

- 指针：`on_pointer_event`（bubble 外→内）/ `on_pre_pointer_event`（preview 内→外），PointerDown 捕获（capture_id，Move/Up 发给捕获者），slop 判定拖动。
- 键盘：`on_key_event`（bubble）+ `on_pre_key_event`，焦点 `focused_slot_key`（跨重组稳定），Tab/Shift+Tab 遍历，Esc 取消焦点，ModifiersState 修饰键。
- IME：`set_current_node_ime_callback` + `sync_composing_range`（预输入下划线）。

### 2.8 选择（ui/selection_container.rs）

- `SelectionContainer` 注册 `SelectionRegistrar`（挂在 Composer 上，避免全局 Mutex 死锁）。
- Text 通过 `ctx.set_current_node_registrar` 注册；选择状态（anchor/cursor）在 container 内部，跨 Text 合并选择。
- `on_selection_change` 回调携带 `Selection`（文本 + range）。

---

## 3. 已知问题（接手必读）

### 3.1 布局缓存与动画的旁路耦合（**最痛**）

**症状**：做 AnimatedVisibility 布局动画（高度收缩/展开）时，被迫发明 `MeasurePolicy::force_remeasure()`——递归扫描整条祖先链，每帧强制重测。`animation-improve` 分支的 `ShrinkPolicy` 就是典型。

**根因**：动画值用 `peek()` 读（零依赖、不重组），布局缓存只认 `(dirty, constraints)`——**缓存不知道动画值变了**。于是"动画驱动布局"只能走旁路（force_remeasure），每加一种布局动画都要新造一个旁路机制，且必须小心翼翼保证祖先链全重测，否则下方组件不跟随。

**正确方向**：给 LayoutNode（或 Composer）加**动画版本号**（每帧 bump），measure 缓存检查 `(constraints, anim_version)` 二元组——动画期间自然失效，`force_remeasure` 旁路可整体删除。改动小、机制统一。

### 3.2 Skip 恢复依赖路径 key，结构变化时脆弱

**症状**：`prev_node_by_key: HashMap<Vec<usize>, CachedNode>` 以路径为 key。if 分支、面板移除/插入、列表项变化等**结构变化**后，同位置不同内容的 slot 会错误复用缓存——历史上反复出现：nest_demo 塌缩、面板双击消失、modifier 被清空、位置丢失、key 碰撞（三个同名 composable 共用函数 hash）。

**根因**：路径 key 隐含"结构没变"假设；结构一变，缓存恢复就是错的。`#[composable]` 宏的 scope hash 只到函数级，同名函数多实例靠位置区分——补丁是"位置折叠进 key"。

**正确方向**：物化恢复改为"树内 diff + 局部重建"，或 desc 树比较；至少把 CachedNode 恢复的命中条件从"路径相等"升级为"路径相等 + 结构签名相等"。

### 3.3 副作用与 Skip 的协调脆弱

**症状**：动画注册、`on_done` 回调、`remember` 状态初始化这些副作用，与"子树被 Skip 跳过"反复冲突：快速切换时 exit 动画 on_done 丢失（面板卡住）、Skip 分支清空后代 modifier、动画注册时机导致双 compose 消耗 prev 缓存。

**根因**：Skip 是"从缓存重建"，但副作用（动画对象、回调、注册表项）**不是缓存的一部分**——Skip 与副作用两个系统没有统一的生命周期契约。

**建议**：副作用（尤其是动画）尽量挂在 State 上而非 slot 上（State 是跨重组稳定的），或给 Skip 恢复路径显式提供副作用重放钩子。

### 3.4 布局期 modifier 扫描重复

`measure_node` 里对 modifier 的多遍扫描（resolved_size / fixed_size / padding / graphics_layer……每遍遍历元素列表），加上渲染期又扫一遍——O(n) 遍历次数多。小问题，性能优化时顺手做。

### 3.5 其他历史遗留

- `FocusManager` 曾有双焦点系统（死代码已清，但历史提交里有教训）。
- debug-server 的 WS 模拟点击存在渲染时序"断"（wm_paint 偶发不来）——真实鼠标正常，调试时注意区分。
- `skiwin` vulkan `acquire_next_image` 不阻塞 + present 不等待，动画首帧偶发闪烁（已用 timeout acquire 缓解，未根治 GPU 同步）。
- 多窗口：`PerWindow` 状态在 `app.rs`，窗口间 State 队列独立（`STATE_QUEUE_MAP` per-Composer）——子窗口操作主窗口内容的历史 bug 都源于此，改动时注意。

---

## 4. 如何调试

### 4.1 构建 & 运行（后台模式）

```bash
# 编译（带调试通道）
cargo build -p winia --example counter --features debug-server

# 后台运行（reasonix 原生后台 shell——窗口保持存活，可边跑边查输出）
cd /d/Projects/winia/winia && /d/Projects/winia/target/debug/examples/counter.exe > /tmp/run1.txt 2>&1 &
# 用 bash_output 读输出；改动代码后先 taskkill 旧实例再重建再跑
powershell -NoProfile -Command "Get-Process counter -ErrorAction SilentlyContinue | Stop-Process -Force"
```

**不要**在后台 shell 里直接跑 demo 等它退出——窗口程序会一直运行，要用 `run_in_background` + `preserve_background_processes`。

### 4.2 DevTools WebSocket（端口 9998）

demo 带 `--features debug-server` 启动后：

```python
import asyncio, websockets
async def t():
    w = await websockets.connect('ws://localhost:9998', ping_interval=None)
    await w.send('t')                       # 树 JSON（节点 pos/mod/text/children）
    r = await asyncio.wait_for(w.recv(), timeout=5)
    print(r.decode() if isinstance(r, bytes) else r)
asyncio.run(t())
```

| 命令 | 作用 |
|------|------|
| `c x y` | 模拟点击（down+up） |
| `d x y` / `m x y` / `u x y` | 指针 down/move/up（拖动选择用） |
| `k <key>` | 键盘事件 |
| `s dy` | 滚动 |
| `r` | 请求截图（等 100ms 返回尺寸信息） |
| `t` | 返回布局树 JSON（**最常用**——验证节点位置/存在性） |
| `p` | 二进制像素帧（8 字节 header WxH + RGBA，可 PIL 分析） |
| `q` | 退出 |

**stdin 通道**：同一组命令也可通过 stdin 发（`echo 'c 190 130'`），但 WS 支持响应，优先用 WS。

### 4.3 环境变量跟踪

- `WINIA_SLOT_TRACE=1`：slot 分配/复用跟踪（composer.rs）
- `WINIA_SKIP_TRACE=1`：Skip 恢复路径跟踪

```bash
WINIA_SLOT_TRACE=1 /d/Projects/winia/target/debug/examples/animated_visibility_demo.exe 2>&1 | grep slot
```

### 4.4 探针日志

- `debug_log!("[tag] {}", x)` 宏：仅 `debug-server` feature 下编译进代码，用户构建零开销。**加日志排错后记得删**（历史教训：残留探针多次被 review 抓出）。
- 排查渲染断帧：`p` 像素分析（亮度/标准差），确认是真实断帧还是 debug 读回时机问题（读回必须在 draw 之后，`skiwin::vulkan::capture` 已修）。

### 4.5 测试

```bash
cargo test --lib          # 162 个，改动后必须全绿
cargo build -p winia      # 库编译干净（0 error）
```

**写测试的纪律**：
- State 必须 `remember` 创建（直接 `State::new` 不注册队列，notify 不推送）。
- 测试失败先按逻辑分析是测试错还是实现错，不要为了绿改断言。
- 边界情况优先：空文本、emoji（多 byte）、越界 range、结构插入/删除、快速切换。

---

## 5. 接手路线建议

1. **先修 §3.1（动画版本号）**——小改动、立刻消除布局动画的最大摩擦，`animation-improve` 分支的布局动画实验可以基于它重做并合回。
2. 再攻 §3.2（物化恢复健壮性）——这是历史 bug 的最大来源，但工程量大，建议先补结构变化的回归测试再动。
3. §3.3 副作用契约——设计 State 级动画挂载，逐步减少 slot 级副作用。
4. 功能面缺口（按需）：动画 API 补全（AnimatedContent、Crossfade、keyframes 完善）、LazyColumn、布局动画（§3.1 修复后）、TextField 文本选择。

---

## 附：常用符号索引

| 符号 | 位置 |
|------|------|
| `ComposeCtx::remember/next_key/start_scope/start_container/end_node` | composer.rs:117-441 |
| `Composer::compose/materialize/start_node/end_node` | composer.rs:1101-1246 |
| `SlotTable::mark_dirty/start_slot/end_slot` | composer.rs:562-824 |
| `measure_node` 常量折叠 | node.rs:739-749 |
| `State::{set,set_no_wake,set_silent,peek}` | state.rs:93-157 |
| `notify_state_changed/wake_loop/set_wake_fn` | state.rs:296-338 |
| `recompose_layout_render` | app.rs:106 |
| `render_pass1` | render.rs:70 |
| `push_animation/update_animations` | animation.rs:165-294 |
| `Animatable::animate_to` | animation.rs:327 |
| `debug_log!` | lib.rs:16 |
| `start_ws_server`（WS 协议） | debug.rs:192-294 |
| `#[composable]` 宏 | winia-macros/src/lib.rs:301 |
