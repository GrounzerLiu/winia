# Winia v2 — 接手文档（现状 / 问题 / 调试）

> 本文档面向**后来者**：读完即可上手开发、定位问题、跑调试。
> 与 `docs/architecture.md`（早期设计稿，部分过时）和 `docs/developer-guide.md`（入门概念，部分过时）互补。
> 最后更新：2026-08（`text-field` 分支 HEAD `4edd48d`）

---

## 0. 三分钟速览

- 这是一个 **Rust 声明式 GUI 框架**，对标 Jetpack Compose：`winit`（窗口/事件）+ `skia-safe`（绘制）+ 自研组合引擎。
- 核心文件就几个：`winia/src/core/composer.rs`（约 3700 行，组合引擎；物化已拆到 `core/materialize.rs`）、`src/app.rs`（约 1970 行，事件循环）、`src/layout/node.rs`（约 1500 行，布局）、`src/modifier.rs`（约 2200 行，修饰符）、`src/animation.rs`（约 2000 行，动画）。
- 构建：`cargo build -p winia`；测试：`cargo test -p winia --lib`（当前 273 个）；跑 demo：`cargo run -p winia --example counter`。
- 调试：`--features debug-server` 开启 WebSocket（9998 端口）调试通道，可模拟点击/滚动/截图/读树。
- 当前工作基座：`component-polish`（组件/Modifier 对齐 Compose、交互源与波纹、GraphicsLayer 3D）。各功能分支见 §1。

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
| `component-polish` | **当前工作基座** | 组件属性/Modifier 对齐 Compose：Overlay/Popup/Dialog 修复、InteractionSource/水波纹、GraphicsLayer 3D 等（清单见 `docs/component-gap-analysis.md`） |
| `text-field` | 历史主线 | TextField、选择、键盘事件等，已并入当前工作基座 |
| `composition-separation` | 已合并 | 组合/布局分离（compose 写 desc → materialize 建树） |
| `compose-core` | 已合并 | 死代码清理、两段式依赖、物化器拆分、Skip 结构签名、UI 测试框架 |
| `animation-system` / `scope-research` / `selection-api` / `vsync-research` / `v2` | 已合并 | 各自主题的研究线，均已合入 |
| `interaction-source` / `graphics-layer-3d` | 已合并（可删） | 交互源/波纹；GraphicsLayer 3D |
| `animation-improve` | **保留不删** | AnimatedVisibility 布局动画实验（shrink/expand）。**勿直接搬回主线**——`force_remeasure` 已被两段式依赖取代（见 §3.1），机制本身是历史旁路 |

**接手提示**：新功能工作从 `component-polish`（或合并后的主线）开分支；分支名用描述性短名称（如 `interaction-source`、`graphics-layer-3d`），**不要使用 `codex/` 前缀**（约定见 AGENTS.md）。

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
- `LayoutNode` 含 `dirty: bool` + `layout_dirty: bool` + `cached_constraints: Option<Constraints>`——**常量折叠**：`!dirty && !layout_dirty && cached_constraints == Some(constraints)` 直接复用；布局期动画值变化只标 `layout_dirty` 不触发重组（两段式依赖，见 §3.1）。
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

### 3.1 布局缓存与动画的旁路耦合 —— **已解决（2026-08，compose-core）**

**历史教训**：早期做 AnimatedVisibility 布局动画时被迫发明 `MeasurePolicy::force_remeasure()`（递归扫描祖先链每帧强制重测；`animation-improve` 分支的 `ShrinkPolicy` 是典型）。

**现方案（两段式依赖，取代旁路）**：
- 组合期 `State::get()` → `slot_deps` → 标 slot dirty → 重组；
- **布局期（measure 中）`State::get()` → `layout_deps`**（state.rs 记录分流 + composer.rs 收集，state_id → slot_key）→ notify 时只标 `layout_dirty`，不触发重组；
- 常量折叠条件为 `!dirty && !layout_dirty && cached_constraints == Some(constraints)`——动画推进自然重测；
- `AnimatedVisibility`/`AnimatedSize`/`AnimatedContent` 已全部走此正路；`force_remeasure` 已删除。
- 实现提交：`fbf451e`（核心）+ `bb564e7`（T1-T4 测试）+ `8ba77dd`（依赖注册收敛）；节点移除时清理 `layout_deps` 死 key。
- 已知优化点：`apply_layout_dirty` 命中后祖先全链标脏（保守超集——父必然依赖子尺寸；Compose 精确传播未做）。

### 3.2 Skip 恢复依赖路径 key，结构变化时脆弱 —— **已部分解决（2026-08，compose-core）**

**症状**：`prev_node_by_key: HashMap<Vec<usize>, CachedNode>` 以路径为 key。if 分支、面板移除/插入、列表项变化等**结构变化**后，同位置不同内容的 slot 会错误复用缓存——历史上反复出现：nest_demo 塌缩、面板双击消失、modifier 被清空、位置丢失、key 碰撞（三个同名 composable 共用函数 hash）。

**根因**：路径 key 隐含"结构没变"假设；结构一变，缓存恢复就是错的。`#[composable]` 宏的 scope hash 只到函数级，同名函数多实例靠位置区分——补丁是"位置折叠进 key"。

**已落地**：
- P3-1 `CachedNode.children_count` 结构签名校验——if 分支/列表项增删后放弃恢复走 Enter 重建（25d9a40，T2-T4）；
- for 循环 key 稳定性：`seq = max(自身执行计数, 栈顶外层语句 seq)` + `STMT_SEQ` 按 `(scope_src, id)` 计数（跨函数不泄漏）；**嵌套循环内层仍可能混淆——正解 `ctx.key(i, ...)`**（已泛型化为 `impl Hash`）。
- 完整"树内 diff + 局部重建"未做（YAGNI 决策——数量相同内容不同的恢复保留，Compose 语义）。

### 3.3 副作用与 Skip 的协调脆弱 —— **已部分解决（2026-08，compose-core）**

**症状**：动画注册、`on_done` 回调、`remember` 状态初始化这些副作用，与"子树被 Skip 跳过"反复冲突：快速切换时 exit 动画 on_done 丢失（面板卡住）、Skip 分支清空后代 modifier、动画注册时机导致双 compose 消耗 prev 缓存。

**根因**：Skip 是"从缓存重建"，但副作用（动画对象、回调、注册表项）**不是缓存的一部分**——Skip 与副作用两个系统没有统一的生命周期契约。

**已落地**：P3-2 无限动画自动 dispose（`InfiniteTransition` 生命周期与组合点对齐，11bb3e2，T5/T6）。

**未做**：`on_done` 回调等其余副作用的统一生命周期契约；设计原则仍是副作用（尤其是动画）尽量挂在 State 上（State 跨重组稳定），或给 Skip 恢复路径显式提供副作用重放钩子。

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
- `WINIA_STMT_TRACE=1`：语句级 id/seq/self/outer 跟踪（for 循环 key 问题调试）
- `WINIA_MAT_PROBE=1`：物化探针（[mat]/[mat-fb]/[collect]）

```bash
WINIA_SLOT_TRACE=1 /d/Projects/winia/target/debug/examples/animated_visibility_demo.exe 2>&1 | grep slot
```

### 4.4 探针日志

- `debug_log!("[tag] {}", x)` 宏：仅 `debug-server` feature 下编译进代码，用户构建零开销。**加日志排错后记得删**（历史教训：残留探针多次被 review 抓出）。
- 排查渲染断帧：`p` 像素分析（亮度/标准差），确认是真实断帧还是 debug 读回时机问题（读回必须在 draw 之后，`skiwin::vulkan::capture` 已修）。

### 4.5 测试

```bash
cargo test -p winia --lib          # 273 个，改动后必须全绿
cargo test --test ui_test --features debug-server   # UI 集成测试（真实窗口）
cargo build -p winia               # 库编译干净（0 error）
```

### 4.6 UI 集成测试的坑（compose-core 固化的教训）

- **stderr 管道必须被读线程消费**——不读 64KB 填满会阻塞 demo 进程（表现为超时）；
- launch 前 **taskkill 同名残留 + 全局串行锁**——并行测试的 taskkill 会互杀刚启动的进程；
- 树 JSON 必须**单行紧凑 + 完整转义**（`\r`/`\t`/`\b`/`\f`）——控制字符生成非法 JSON 解析失败；
- fixture 用 `[[bin]]` 而非 `[[test]] harness=false`——后者会被 cargo test 当测试执行（跑窗口循环）卡死全量测试；
- debug 注入事件只作用于主窗口——消费需双路兜底（window_event + new_events）；
- 测试创建 State 必须走 `ctx.remember`（直接 `State::new` 无 owner → notify 不推送 → 增量重组不触发）。

**写测试的纪律**：
- State 必须 `remember` 创建（直接 `State::new` 不注册队列，notify 不推送）。
- 测试失败先按逻辑分析是测试错还是实现错，不要为了绿改断言。
- 边界情况优先：空文本、emoji（多 byte）、越界 range、结构插入/删除、快速切换。

---

## 5. 接手路线建议

1. **功能面缺口（按需）**：`docs/component-gap-analysis.md` 剩余项——TextField label（浮动 Composable）、TextField 内置容器视觉（Outlined/Filled）、GraphicsLayer shape 裁剪、draggable/pointer_input（远期）。
2. **性能**：§3.4 布局期 modifier 多遍扫描（每帧多遍遍历元素列表，顺手优化）。
3. **长远架构**：组合树/布局树完整分离（当前为"desc 树 → 物化"中间形态）、副作用契约完善（State 级动画挂载）、LazyColumn、基于两段式依赖重做布局动画实验。
4. 历史分支 `animation-improve` 的实验勿直接搬回主线（机制已被两段式依赖取代）。

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
