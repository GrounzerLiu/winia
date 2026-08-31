# NavTransition 滑动过渡开发交接文档

> **交接人**：winia 架构审计会话（deepseek）
> **接手人**：GLM（zcode 环境）
> **日期**：2026-02（当前进行中）
> **分支**：`nav3-inspired`（基于 v2，领先 4 个审计提交 + afa6350）

---

## 1. 项目背景

winia 是 Rust GUI 框架（对标 Jetpack Compose，winit + skia-safe）。当前在做：

1. **架构审计 Phase 3/4**（已完成并提交，见 §7 提交历史）
2. **Navigation3 导航层研发**（当前工作，`winia/src/nav.rs`）

导航层对标 AndroidX Navigation3（NavBackStack / NavEntry / NavDisplay / SceneStrategy / SaveableStateHolder / transitionSpec）。

## 2. 当前任务

在 `winia/src/nav.rs` 的 NavDisplay 中实现 **滑动过渡动画（NavTransition）**，对标 Nav3 的 `transitionSpec / popTransitionSpec`（AnimatedContent 双页过渡）：

- **push**（forward）：新页从右滑入 (+100px)，旧页向左滑出 (-100px)
- **pop**（backward）：反向——旧页向右滑出 (+100px)，新页从左滑入 (-100px)
- 同时淡入淡出（旧页 alpha 1→0，新页保持 alpha=1）
- 300ms EaseInOutCubic
- 双页 Stack 层叠（旧页下层滑出，新页上层滑入），动画完成后移除旧页

> ⚠ 2026-08-29 规格更新（用户验收决定）：100px 小位移 + 新页恒不透明被判定为
> "直接出现然后位移"（视觉不合理），已改为 **Android 全幅滑入形态**——见 §11 第 6 条。

## 3. 已完成的工作（未提交）

`winia/src/nav.rs` 当前有大量未提交改动（git diff 532 行插入），包括：

### 3.1 Entry 状态池（对标 Nav3 SaveableStateHolder）
- `EntryStateScope` + `ENTRY_STATE_SCOPE`（CompositionLocal，`LazyLock<CompositionLocal<EntryStateScope>>`）
- `remember_entry_state<T>()`：槽 key = `(entry key hash, 调用序号 seq, TypeId)`，存 `Arc<Mutex<HashMap<(u64,u32,TypeId), Box<dyn Any>>>>`
- `clear_entry_state(key, pool)`：pop 时清理（对标 Nav3 removeState）
- `key_hash()`：**FNV-1a 手写 hasher**（⚠ 不能用 DefaultHasher——种子随机，槽 key 漂移）
- counter 每帧 `set_silent(0)` 重置（seq 按 entry 内调用顺序分配）

### 3.2 NavKey trait 扩展
- 从 `Clone + PartialEq + Eq + Debug + 'static` 扩展到 `+ Hash`（状态保持装饰器固定组合 key 需要）

### 3.3 NavTransition（滑动过渡状态机）

> ⚠ 本节公式/规格已过时——实现已改为 spec 驱动（`NavTransitionSpec` 原语），
> 见 §11.6 与 `winia/src/nav.rs` 当前文档。

```rust
struct NavTransition<K: NavKey> {
    current: State<Option<K>>,   // 当前显示的 entry key
    previous: State<Option<K>>,  // 过渡中的旧 entry key（动画完成后清空）
    forward: State<bool>,        // push=true / pop=false
    progress: State<f32>,        // 1→0：1=旧页全显，0=新页全显
}
```
- `init()`：4 个 State 全部 `ctx.remember`（跨帧稳定）
- `detect()`：每帧调用。`target != current.peek()` 时：
  1. `previous.set(current)`（旧页进入过渡）
  2. `forward.set(stack_len > prev_len)`（方向）
  3. `current.set(target)`
  4. **`progress.set_silent(1.0)`**（复位起点——上次动画结束停在 0，不复位则 push_animatable 见 peek==target 直接跳过）
  5. `push_animatable(progress, 0.0, 300ms EaseInOutCubic)`
  - 完成检测：`previous.is_some() && progress < 0.001 → previous.set(None)`
- `render()`：Stack 层叠两页。每页 `Modifier::new().fill_max_size().graphics_layer(move || ...)` 动画闭包 **peek progress 零重组**：
  - 旧页（下层）：`translation_x = if fwd { -(1.0-p) } else { 1.0-p } * 100.0`，`alpha = p`
  - 新页（上层）：`translation_x = if fwd { p } else { -p } * 100.0`，`alpha = 1.0`

### 3.4 NavEntryDecorator（对标 Nav3 NavEntryDecorator）
- `NavEntryDecorator<K>` trait：`on_pop()` + `wrap()`（默认原样渲染）
- `RememberStateDecorator`：`ctx.key(key.clone(), |ctx| entry.build(ctx))` 固定组合 key 包裹
- `BackStackAwareDecorator`：占位（on_pop 打 debug_log）
- NavDisplay 默认带 `vec![Box::new(RememberStateDecorator)]`，`add_decorator()` 追加

### 3.5 NavDisplay::build 重构
- entry 状态池：`ctx.remember(|| Arc::new(Mutex::new(HashMap::new())))`
- on_pop 检测：`prev_stack` State 对比上一帧，被移除的 entry → `clear_entry_state` + 装饰器 `on_pop`
- SinglePane 分支：`NavTransition::init` + `detect` + `render`（替换原 Crossfade）
- ListDetail 分支：双栏也提供 EntryStateScope（counter + provides）
- 每个 entry 渲染前：`counter.set_silent(0)` + `ENTRY_STATE_SCOPE.provides(scope, ...)`

### 3.6 测试
- `remember_state_decorator_preserves_entry_state`：Detail(7) 计数 → push Settings → pop → counter 从旧值继续
- `TestRoute` 加了 `Settings` 变体

## 4. 遇到的问题（关键！）

### 4.1 已修复：progress 不复位 → 第二次动画不启动
**症状**：第一次导航有动画，第二次起没有过渡。
**根因**：动画结束后 progress 停在 0.0。第二次 `push_animatable(progress, 0.0)` 检测 `peek()==target(0.0)` 直接 return。
**修复**：detect 里 `self.progress.set_silent(1.0)` 复位。

### 4.2 已修复：位移公式方向反了
**症状**：动画期间新页滑向右侧而非从右侧滑入，结束后 snap 回原位。
**根因**：原公式 `new: (1-p)*100`（p=1 起点 0，p=0 终点 +100——滑向右侧）；`old: -p*100`（p=1 时 -100，p=0 时 0——从左侧跳入）。
**修复**：改为
- 新页：`fwd ? p : -p` * 100（push 从 +100 滑到 0，pop 从 -100 滑到 0）
- 旧页：`fwd ? -(1-p) : (1-p)` * 100（push 从 0 滑到 -100，pop 从 0 滑到 +100）

### 4.3 待验证：多次动画后位置偏移（用户报告"多几次动画后就不对"）
**当前状态**：公式和 progress 复位已修，但**尚未充分验证**。用户用 mimo-vision 分析截图 pos_check.png 显示"内容整体偏左，右侧大片空白"——但这可能是动画中截帧，也可能是真 bug。

**怀疑点**：
1. `progress` 动画被打断（快速连续导航）时 set_silent(1.0) 与进行中动画竞争——push_animatable 会取消旧动画（同 state 不同目标 → retain 移除），但 set_silent 在 push 之前执行，时序需确认
2. **动画完成后 progress 停在 0.0**，但 `render()` 里新页 `translation_x = fwd ? p : -p`——p=0 时新页回到原位 ✅。但**旧页在动画完成后已被移除**（previous.set(None)），所以不应残留
3. **ListDetail 模式无过渡**（双栏直接切换）——模式切换按钮在 demo 里
4. 可能还有 `init` 的 `ctx.remember` 与 `render` 内 `ctx.remember(|| State::new(0u32))`（counter）在闭包内调用位置漂移问题

## 5. 如何调试验证

### 5.1 构建与运行
```bash
# 构建（注意 nav.rs 是 CRLF——git 会转 LF，无碍）
cargo build -p winia
cargo build -p winia --example nav_demo --features debug-server

# 运行（必须带 --features debug-server 才有 ws 调试端口）
./target/debug/examples/nav_demo.exe
# DevTools WebSocket → ws://localhost:9998
```

### 5.2 调试工具（tools/ 目录）
| 工具 | 用途 |
|---|---|
| `tools/ws_nav_click.py <port> <x> <y>` | 点击 + 打印树（d/u 模拟按下抬起，树含绝对坐标） |
| `tools/ws_nav3_state.py <port>` | 打印状态（back-stack/页面/计数） |
| `tools/ws_conv_shot.py <port> <out.png>` | 截图（max_size 30MB） |

**注意**：demo 端口是 **9998**（不是工具默认的 10160/10162）——必须显式传参。

### 5.3 验证流程（多次导航）

> ⚠ 本节坐标/±100px 断言基于旧布局与旧公式——按钮布局已变（demo 新增"过渡"
> 按钮，整体下移）、过渡已改 spec 驱动。以 §11.6 与实际树 dump 为准。

```bash
# demo 起来后：
# Home 页按钮：模式切换 (92,95)、打开 Detail (73,196)
# Detail 页按钮：计数+1 (73,196)、打开 Settings (73,244)、返回 (73,292)
# Settings 页按钮：返回 Detail (63,172)  ← 只有这一个按钮！

# 完整验证链：
python tools/ws_nav_click.py 9998 73 196   # Home → Detail
python tools/ws_nav_click.py 9998 73 244   # Detail → Settings
python tools/ws_nav_click.py 9998 63 172   # Settings → Detail（返回）
python tools/ws_nav_click.py 9998 73 244   # Detail → Settings（再进）
python tools/ws_nav_click.py 9998 63 172   # Settings → Detail（再返回）
# 每步检查 back-stack 和页面标题的 abs 坐标是否正常（应为 (16,51) 和 (16,123)）
```

**动画期间验证**：在点击后立即（0ms 延迟）截图或读树，看过渡中间帧：
- push 时新页应从右 (+100px 外) 滑入——截图应看到内容偏右
- pop 时新页应从左 (-100px 外) 滑入——截图应看到内容偏左
- 动画完成后内容应回到原位 (16,123) 等正常坐标

### 5.4 截图分析
```bash
python tools/ws_conv_shot.py 9998 /tmp/frame.png
# 用 mimo-vision 分析（API key 已更新）：
# mcp__mimo-vision__analyze_image { image_path, prompt: "描述内容位置" }
```

## 6. 需要阅读的代码

| 文件 | 要点 |
|---|---|
| `winia/src/nav.rs` | **主战场**。NavTransition（~265-373 行）、NavDisplay::build（~505-660 行）、EntryStateScope/remember_entry_state（~30-110 行） |
| `winia/src/animation.rs` | `push_animatable`（151 行起）——同目标 dedup、不同目标取消旧动画、继承速度；`Animatable` 内部 |
| `winia/src/modifier.rs` | `GraphicsLayerParams`（1955 行）——`translation_x/y` 单位是 **逻辑 px**；graphics_layer 应用（2634 行 `current += next`） |
| `winia/src/render.rs` | 62 行 `canvas.translate((gl.translation_x, gl.translation_y))`——translation 应用方式 |
| `winia/src/ui/animated_visibility.rs` | 170-216 行——**正确的滑动实现参考**（`off = (1-p)*48`，`SlideDirection::Left => translation_x = -off`）——注意它是 **visible 0→1 语义**（p=0 隐藏，p=1 显示），与 NavTransition 的 1→0 相反 |
| `winia/src/core/composer.rs` | `remember`（227 行）、`remember_at_key`（259 行）、`key()`、slot_table 机制 |
| `winia/src/core/state.rs` | `State::get/set/set_silent/peek` 语义——get 注册依赖触发重组，set_silent 不通知，peek 渲染期读零重组 |
| `winia/examples/nav_demo.rs` | demo 页面（Home/Detail/Settings）与按钮布局 |
| `docs/navigation3.md` | Nav3 对标设计文档 |
| `docs/render_modifier_analysis.md` | render/modifier 分析（未提交，参考用） |

## 7. 提交历史（已完成部分）

```
afa6350 feat(nav): Navigation3 风格导航层——SceneStrategy 多栏 + Crossfade 过渡
4455416 feat(platform): Phase 4——ScaleFactor 重组 + composing 色捕获 + E2E 测试 + 旧 API 清理（审计 Phase 4）
486830f refactor(text-field): 第 4 点方向 B——清理 dual selection source 死代码（审计 Phase 3.4）
c963f82 fix(text-field): 第 3 点——blink 纳入 effect 生命周期 + Undo 快照含 composing_range + Preedit char 边界 + Deleted clamp（审计 Phase 3.3）
4ec97e1 fix(lazy): 方向一——build 感知真实视口，measure/build convergence 跨帧收敛（审计 Phase 3.2）
e1a685f fix(lazy): 高度缓存按 item key 迁移（方案 A）——数据增删/重排后高度与滚动位置按 key 跟随
```

**当前未提交**（git status）：
```
 M docs/navigation3.md          （NavTransition 文档，41 行新增）
 M winia/examples/nav_demo.rs   （35 行新增）
 M winia/src/nav.rs             （478 行新增——主改动）
?? docs/render_modifier_analysis.md
?? nav_view.png                 （截图，可删）
?? pos_check.png                （截图，可删）
```

## 8. 测试

```bash
cargo test -p winia --lib   # 644 项通过（含 5 个 nav 测试）
```

nav 模块测试在 `winia/src/nav.rs` 底部 `#[cfg(test)] mod tests`（10 个）：
- `back_stack_push_pop_works` / `back_stack_remove_last_and_clear`（栈操作）
- `nav_display_renders_top_entry`（push/pop 端态渲染 + 动画推进）
- `list_detail_scene_renders_both_entries`（双栏）
- `remember_state_decorator_preserves_entry_state`（状态池：覆盖返回保持 + pop 清理）
- `transition_midframe_renders_both_pages`（过渡中间帧双页同树）
- `pop_slideout_does_not_repollute_entry_state_pool`（滑出 draining 防池重污染）

## 9. 环境备忘

- 工作目录：`D:\Projects\winia`
- demo 进程清理：`taskkill //F //IM nav_demo.exe`
- 分支：`nav3-inspired`（领先 v2）
- mimo-vision API key 已更新为 `sk-lH24WVmg6JTpVS2akMzI17PdgT5VQfXedsK9K2uosGKI8hJ8kBdoZWB6zOM1INLM`（`D:/mcp/mimo_vision_mcp.py:24`，重启 MCP 生效）
- nav.rs 是 CRLF 行尾（git 会转 LF，正常警告）

## 10. 下一步建议

1. **充分验证多次动画**：跑 §5.3 完整验证链，确认多次 push/pop 后位置无偏移
2. **动画中间帧截图**：点击后立即截图，确认 push/pop 方向正确（新页从右/左滑入）
3. **检查动画中断**：快速连续导航（动画中途再点）是否正常——push_animatable 的取消逻辑
4. **ListDetail 模式**：双栏模式切换是否也需要过渡（目前无）
5. 验证通过后提交（建议拆两个 commit：状态池/decorator 一个，NavTransition 一个）

## 11. 接手结果（GLM，2026-08-29，未提交待验收）

§10 各项已全部处理：

1. **多次动画验证通过**（树坐标 + 临时埋点日志双重验证：17+ 次导航含 60ms 间隔打断，
   动画全部启动并完成、静置归零）。
2. **发现并修复启动/静置偏移 bug**（§4.3 两个怀疑点的真正根因）：`progress` 初值 1.0
   且无过渡时无人归零 → 启动时 Home 停在 +100px（nav_view.png / pos_check.png 即此 bug
   的旧现场）。修复：初值 0.0 + 渲染层 `active = previous.is_some()` 门条件（无过渡必归位）。
   §4.3 怀疑的"中断竞争"在 set_silent(1.0) 复位修复后未复现（13+ 次打断全部正确）。
3. **子代理双路评审后追加修复**：
   - P0：`add_decorator` 曾只应用最后一个装饰器的 wrap——改为全列表链式包裹
     （`NavEntryDecorator::wrap` 签名改为接收 `inner` 闭包，首个装饰器最外层，对标 Nav3）；
   - P1：滑出层 draining 只读作用域（旧页滑出 300ms 内内容每帧重跑曾把已 removeState
     清理的池槽重新插回）；
   - P1：过渡中途再次导航的一帧闪跳（有进行中动画时跳过 `set_silent(1.0)` 复位，
     旧动画同目标去重保留、原时间轴平滑续走）；
   - 文档全面同步（Crossfade → NavTransition 滑动过渡；`remember_entry_state` 去掉
     未用的 ctx 参数；plain remember 不保证跨 pop/push 已在文档注明）。
4. **测试 7 个全过**（新增 `transition_midframe_renders_both_pages` 与
   `pop_slideout_does_not_repollute_entry_state_pool` 两个回归）。
5. 未做（后续项）：ListDetail 双栏切换无过渡动画；过渡共享 tween 的自定义时长/曲线、
   per-entry metadata 覆盖、predictivePop（docs/navigation3.md §五.4）；plain remember
   的跨位置保持需框架级槽身份支持。
6. **过渡规格可配置（对标 Compose，2026-08-29）**：过渡 API 对标 Nav3
   transitionSpec / popTransitionSpec——`NavDisplay::transition_spec` /
   `pop_transition_spec`，各自 = `NavTransitionSpec { enter, exit }`（对标
   ContentTransform；`NavEnter`/`NavExit` 原语：None/Fade/Slide/Slide+Fade，
   位移 `SlideOffset::Fraction/Px` 对标 Compose `{ it }` 全宽闭包）。**默认 fade
   （对标 Nav3 本体默认，androidx 为 tween(700)——winia 共享 300ms）**；
   Android 全幅滑动 = `horizontal_slide()` 便捷对（demo 默认选中）。关键语义：
   - spec 在过渡启动时**快照固化**（对标 Nav3 求值时机——中途改配置不影响进行中过渡）；
   - `NavExit::None` = 旧页原样保留到过渡结束（对标 ExitTransition.None，非瞬时消失；
     enter/exit 均 None 才是瞬时切换，且会清理进行中的过渡防幽灵页）；
   - 层序按方向定 z（push 新页上 / pop 旧页上——对标 Nav3 targetContentZIndex 方向
     规则；用户自定义 zIndex androidx 当前亦忽略）；
   - 过渡期全尺寸点击屏蔽层（winia 命中测试不计 graphics_layer 位移——否则滑动页
     按钮在原命中盒可被误触，如连点返回清空栈）。
   demo 顶部"过渡"按钮循环 4 种规格可实时体验；位移基准经新增
   `Modifier.on_size_changed`（对标 Compose onSizeChanged）捕获；
   埋点确认 push cur tx 568→0、prev 0→-170 淡出，pop 反向，静置归零。
