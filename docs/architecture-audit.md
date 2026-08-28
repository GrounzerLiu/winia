# Winia v2 底层架构审计

> 状态：架构审计与增量修复跟踪。本轮只修改 ownerless State/frame 目标文件和本审计文档，不回滚既有 dirty worktree。
> 范围：D:/Projects/winia，v2 分支；当前工作树观测为 origin/v2 ahead 41。
> 结论：3.1 owner TLS 泄漏已由 ownerless StateSignal 基础切片修复；3.2/3.4 已有 RuntimeFrame、layout-only 和布局事务 retry 基础切片；3.9/6.1/6.2/6.3 已由多窗口上下文隔离修复；3.10/6.4/6.5 已部分修复；Phase 1（所有权与恢复边界）三项中第 2 项（guard 体系）与第 3 项（依赖收敛）已完成，第 1 项（per-Window context）仍有 debug 事件/动画表/事件循环三处全局单态残留——经评估 2 处为本质全局、1 处为代码美化，均已决策不继续清理（原因见 §11 checklist）；剩余 Phase 2（坐标/复用/LayoutNode）、Phase 3（LazyList/TextField——其中 height cache 按 key 缓存已实施，见 §Phase 3.1）、Phase 4（CompositionLocal/Theme/E2E）和其他风险仍开放。

## 1. 基线与验证状态

### 1.1 Workspace

Workspace Cargo.toml:1-17 当前包含 winia、winia-macros、skiwin、material-shapes。主要依赖包括 winit 0.31.0-beta.2、skia-safe/skia-bindings 0.99.0、parking_lot、log、thiserror。

README.md:1-93 将 Winia 定位为 Compose-inspired 的纯 Rust 声明式 GUI：winit 负责窗口和事件，Skia 负责绘制，Composer 负责增量组合，布局与渲染分离。

winia/src/lib.rs:11-36 声明 core、unit、font、icon、modifier、layout、ui、text、render、animation、nested_scroll、app、effect、input、debug；winia/src/ui.rs:6-45 声明 UI 模块并在 winia/src/ui.rs:47-146 重新导出公共组件。

### 1.2 已执行验证

- cargo check --workspace：通过，存在较多 warning。
- git diff --check：通过。
- cargo test -q -p winia --lib -- --test-threads=1：通过，612 passed；覆盖 ownerless State、public `State::state_id()`/u64-backed StateId 与 legacy u32 compatibility、StateSignal revision handshake、unsubscribe linearization、closed subscriber cleanup 与 wake acceptance、DependencyFrameGuard、RuntimeFrameGuard、nested Composer、mixed compose/layout pending、canonical reverse graph、compose dependency rollback、compose pending batch、layout-only、layout graph transaction retry 和既有 UI 回归。
- cargo check -p winia --lib：通过，仍有大量既有 warning。
- cargo fmt --all -- --check：此前会报告其他 dirty 文件格式差异；本轮未运行 formatter，避免触碰用户变更。git diff --check：通过。
- 既有 animated_size 全套并行/时间敏感失败（实际 148.4219、期望 200.0）仍记录为残余风险；本轮未修改动画实现。

### 1.3 已知迁移遗留

- winia/examples/navigation_bar_demo.rs:71 仍调用已移除的 .layout(...)，应核对当前 .icon_position(...) API。
- winia/tests/visual_matrix.rs:470 仍调用已移除的 .layout(...)，应核对当前 .icon_position(...) API。
- docs/navigation-bar.md:15 仍使用 Horizontal，而当前 enum 是 NavigationItemIconPosition::Start。

## 2. 总体架构与数据流

### 2.1 声明到帧

```text
用户闭包 / #[composable]
        |
        v
ComposeCtx + Composer
  SlotTable、remember、稳定 key、依赖记录
        |
        v
Slot / NodeDesc 描述树
        |
        v
materialize
  Skip 恢复、LayoutNode arena 复用、on_remove 生命周期
        |
        v
LayoutNode arena
  Constraints -> measure -> place -> cached_constraints
        |
        v
render::render
  Skia Canvas、Modifier 绘制、scroll clip/translate、children
        |
        v
winit RedrawRequested / Vulkan-GL-CPU backend
```

单个状态更新的主要路径：

```text
State::get()
  -> register_dependency()
  -> GROUP_STACK / ACTIVE_SLOT_KEY
  -> slot_deps 或 layout_deps

State::set/update()
  -> StateSignal 的 live ComposerSubscription 弱订阅队列
  -> pending StateId 去重 + wake event loop
  -> compose/layout 按依赖 channel 消费
  -> slot dirty / layout_dirty_keys
  -> materialize()
  -> layout()
  -> render()
```

### 2.2 宏与组合

winia-macros/src/lib.rs:383-455 的 #[composable] 会校验 ctx 参数、基于函数签名和源码位置生成稳定 scope hash（winia-macros/src/lib.rs:422-428），再注入 start_scope_callchain 和 enter_stmt RAII guard。ScopeGuard/StmtGuard 位于 winia/src/core/composer.rs:24-54。

app_root、run_app、compose 位于 winia-macros/src/lib.rs:458-495，根闭包变换位于 winia-macros/src/lib.rs:612-671；keyed_stmt/composable_keyed 位于 winia-macros/src/lib.rs:497-609。未经宏注入且没有 ctx.key 的调用点可能在运行时无法生成稳定 key。

ComposeCtx<'a> 位于 winia/src/core/composer.rs:123-125，严格持有 &mut Composer。remember、remember_at_key、next_key 位于 winia/src/core/composer.rs:134-182；scope API 位于 winia/src/core/composer.rs:184-237；enter_stmt、key、changed 位于 winia/src/core/composer.rs:239-329。

### 2.3 Stable key 与 SlotTable

稳定 key 逻辑位于 winia/src/core/composer.rs:1128-1227：优先使用 ctx.key，其次使用宏注入的 scope source、statement id、迭代位置，再通过 per-base counter 和 mix_key 混合。生产路径没有稳定来源时在 winia/src/core/composer.rs:1189-1197 panic，而不是静默退化。嵌套循环仍可能需要显式 ctx.key。

Slot、NodeDesc、SlotTable 位于 winia/src/core/composer.rs:568-699。Slot 持久化 remembered values、children、dirty、children_count、is_scope、params、desc、visited、skip_modifier、skip_policy 等信息。start_slot 位于 winia/src/core/composer.rs:866-915，reset/truncate 位于 winia/src/core/composer.rs:936-953，dirty 传播位于 winia/src/core/composer.rs:956-989。

start_restartable_group/end 位于 winia/src/core/composer.rs:1310-1427。Clean 只有在参数、modifier.param_eq 和 prev node cache 都匹配时才 Skip；Skip 不执行 content 但保留 slot 结构；Enter 清理未访问旧子 slot。核心不变量是每个 scope/group/node 的 push/pop 必须严格配对，即使遇到 Skip、early return 或 panic。

### 2.4 State 与依赖分流

State 当前位于 winia/src/core/state.rs:14-445：Arc<StateInner<T>>、parking_lot::RwLock、StateSignal 和 ownerless State::new。StateSignal 使用 u64-backed StateId 和 revision handshake；State::state_id() 已暴露该 migration identity，State::id() 与 animation 调用方仍使用独立 public_id u32 兼容层。get 注册依赖，peek 不注册；set 使用 PartialEq 去重；update 总是通知；set_silent 只改值；set_no_wake 通知但不唤醒；set_visual 只改视觉值。

依赖缓冲、模式和 recorder queue 位于 winia/src/core/state.rs:448-641。State::get 通过 winia/src/core/composer.rs:146-165 选择最近 GROUP_STACK scope 或 ACTIVE_SLOT_KEY，并向当前 ComposerSubscription 建立 StateSignal 弱订阅；DependencyFrameGuard 在嵌套 compose/layout 或 panic 时恢复外层 recorder。StateSignal revision handshake 会在读/订阅与 set() 交错时补入 pending，避免 read-time subscription race 丢失一次失效；只有存在 live ComposerSubscription 时才触发 wake。

旧 STATE_OWNER_QUEUE、STATE_QUEUE_MAP、STATE_SUBSCRIBERS、COMPOSER_REGISTRY 已从生产路径删除。StateSignal 自持订阅列表，ComposerSubscription 维护去重 pending StateId、已读 signal 的弱引用和 closed 生命周期闸门；Composer drop、读集合收敛和 panic 后下一次成功帧会取消 stale 订阅，关闭后的 queue 不会被 in-flight read 重新订阅，且关闭时清空残留 pending；`enqueue()`/`restore_pending()` 也拒绝 closed queue 的后续写入，StateSignal 仅为实际接受通知的 queue 触发 wake。

compose-mode 依赖在 winia/src/core/composer.rs:1757-1799 形成 slot dirty，并通过 compose_slot_reads 替换 Entered slot 的读取集合；layout-mode 依赖在 winia/src/core/composer.rs:1987-2082 形成 layout_dirty_keys，layout() 可直接消费 layout-only pending。compose 消费 batch 由 winia/src/core/composer.rs:1290-1324 的 PendingBatchGuard 持有：drain 后帧内新通知留在队列，panic 恢复已消费 ID；`drain_non_compose_collect_layout` 保留同时属于 compose/layout 的 mixed ID；这不是 SlotTable/arena 事务。Composer RuntimeFrameGuard 已隔离 GROUP_STACK/STMT_STACK/ACTIVE_SLOT_KEY，nested Composer 基础路径已验证；layout() 现在有 LayoutTransaction snapshot，可恢复 pending、layout graph、prev maps、节点树状态和 scroll limits，并强制失败帧完整重测；compose 内部事务和完整 layout reverse graph 仍未完成。

### 2.5 物化、布局与坐标

DescNode 位于 winia/src/core/materialize.rs:11-42，承载 key、skip、modifier、policy、on_remove、focus、IME、selection、direction 和 children。materialize/materialize_node 位于 winia/src/core/materialize.rs:48-267：Enter 复用或新建节点，Skip 从 prev_node_by_key 恢复，当前结构校验主要是直接 children 数量，组合期捕获的 direction/focus/IME/selection 通过 desc 传递。

LayoutNode 位于 winia/src/layout/node.rs:128-202，NodeArena 位于 winia/src/layout/node.rs:365-485，MeasurePolicy 位于 winia/src/layout/node.rs:487-508。measure_node 位于 winia/src/layout/node.rs:1338-1649：缓存命中时跳过测量，否则解析 size、min、required、padding、fill、scroll、measure policy、place、RTL、文本/Image/RichText、aspect ratio，并更新缓存。

LayoutNode.position 是父相对坐标。render 在 winia/src/render.rs:597-603 累加 position，scroll 在 winia/src/render.rs:975-1028 通过 clip 和 translate(-offset) 处理。hit_test、scene_to_node_local 位于 winia/src/layout/node.rs:512-619，app.rs:2635-2648 的 node_abs_position 复用同一 scroll 空间。graphics_layer 只影响绘制、不做逆变换命中，这是当前明确策略。

## 3. 主要风险清单

### 3.1：State owner TLS 泄漏（基础切片已修复）

原问题位于旧版 winia/src/core/composer.rs:137-141、winia/src/core/composer.rs:171-176、winia/src/core/composer.rs:397-405 和 winia/src/core/state.rs:45-56：State::new 继承 creator TLS owner。当前生产路径已改为 ownerless State::new；StateSignal 按 State::get 的实际读取建立 ComposerSubscription，旧 creator-owner 路由已删除。

已覆盖 cross-Composer fan-out、explicit unsubscribe、stale read removal、Composer drop、layout-only pending、nested dependency frame 和 nested Composer 测试。StateId 现在是 public opaque u64-backed newtype，State::state_id() 已提供迁移入口；StateInner::public_id 独立保持 public State::id() 的 u32 动画兼容层。剩余风险是 Composer 内部 slot/依赖表尚未事务化，以及 animation API 尚未从 legacy u32 完成迁移。

### 3.2 HIGH：panic 后 GROUP_STACK 残留

位置：winia/src/core/composer.rs:62-123、winia/src/core/composer.rs:1838-1954、winia/src/core/state.rs:460-612。Composer::compose/layout 入口建立 RuntimeFrameGuard，保存并清空 GROUP_STACK、STMT_STACK、ACTIVE_SLOT_KEY；DependencyFrameGuard 同步隔离 DEP_BUFFER、DEP_MODE、RECORDER_QUEUE。panic Drop 会恢复外层 TLS/recorder，并取消失败 frame 新增的信号订阅。

残余风险：compose 路径现在有窄范围 ComposeDependencyTransaction，可在 late cleanup panic 后恢复 dependency maps、signal subscriptions 和 failed-frame pending；scope_source_stack、key_override_stack、SlotTable、arena、remember/on_remove 仍不是统一事务。layout 路径已有 LayoutTransaction rollback，但依赖 nested/user policy 的更深层副作用仍需继续验证。一次 broad ComposeTransaction/SlotTable checkpoint 实验因破坏 remember persistence 和 infinite-transition dispose 已撤回，不能把 compose 完整事务标记为完成。app.rs:951-995 的 catch_unwind 仍是更外层渲染保护。

### 3.3 HIGH：slot_deps 只追加、不收敛

基础问题已修复：当前 winia/src/core/composer.rs:1757-1799 维护 compose_slot_reads，Entered slot 会用本帧读取集合替换，Skip 子树保留旧集合，removed/stale keys 通过 ComposerSubscription::retain_signals 取消订阅。新增 test_compose_read_removal_unsubscribes_stale_state 覆盖条件读取移除。

剩余边界：nested compose 的 recorder 和基础 TLS 已隔离；`rebuild_compose_reverse_deps()`/`rebuild_layout_reverse_deps()` 现在从 canonical forward graphs 重建 reverse maps，并由 debug invariant 检查 drift；但完整双向增量更新、事务 rollback 和精确递归 attribution 仍未落地。

### 3.4 HIGH：layout-only State 不能独立驱动 layout

基础问题已修复：winia/src/core/composer.rs:1987-2082 的 layout() 会先消费 layout-only pending，按 layout_deps 标记 layout_dirty_keys，再执行 measure；recompose() 也会在决定是否 compose 前处理 layout-only ID。新增 test_layout_only_pending_consumed_without_recompose 覆盖直接 layout 路径。

剩余边界：混合 compose/layout 依赖的 State 会保留 pending 供 compose 消费；layout-only 消费已在 ComposerSubscription 锁内原子分类，失败 layout 会恢复已消费 pending 并在 retry 时完整重测；compose 已增加已消费 batch 的 panic 恢复和帧内新通知留存，但 compose/layout 的完整批次语义、并发边界和统一事务仍待形式化。

### 3.5 HIGH：LayoutNode 复用留下旧语义字段

位置：winia/src/core/materialize.rs:167-219、winia/src/core/materialize.rs:226-257、winia/src/layout/node.rs:145-202。复用节点会更新 modifier、direction、policy、on_remove、slot_key、dirty，但 cursor_callback、ime_callback、composing_range、selection_range、display_focused、registrar 等字段不是每次都清空。

has_text_content、has_richtext_content、has_image_content 只在 LayoutNode::new 和 restore_from() 中初始化；materialize 当前直接赋值 n.modifier，却没有同步这些标记。measure_node 在 winia/src/layout/node.rs:1554-1605 依赖它们选择文本、图片或普通叶子路径。parent_id、scroll metadata 也应在统一更新路径中显式重置。

建议建立统一的 LayoutNode::reset/update_from_desc，处理所有内容类型、IME、cursor、selection、interaction、scroll metadata 和父子关系。

### 3.6 HIGH：PointerEvent.position 忽略 ancestor scroll

位置：winia/src/layout/node.rs:555-619、winia/src/app.rs:2635-2648、winia/src/app.rs:2678-2708。hit_test、scene_to_node_local 和 node_abs_position 会扣除祖先 scroll offset，但 dispatch_ptr_event 只累加 node.position。滚动容器内的自定义 PointerEvent、拖拽和 capture 坐标因此错误。

### 3.7 HIGH：MouseWheel/Debug Scroll 不按鼠标位置选目标

位置：winia/src/app.rs:571-587、winia/src/app.rs:1233-1240、winia/src/app.rs:1540-1547。MouseWheel 丢弃 cursor position，find_scroll_target 只按树的反向 child 顺序寻找轴匹配 scroll node。两个并排滚动区域可能滚错目标，debug Scroll 也无法表达命中位置。

建议记录每个窗口最后 PointerMoved 的逻辑坐标，wheel 时先 hit-test，再在命中路径上执行 nested scroll，并测试子节点到边界后的 ancestor handoff。

### 3.8 HIGH：Nested fling 消费量和回调顺序不正确

位置：winia/src/app.rs:1464-1537。当前实现将目标自身 NestedScrollConnection 放进 post 链，把原始 child_velocity 当作 consumed_by_child，并在 child 撞边界时仍将完整 child velocity 计入 consumed。可能造成目标回调重复、父级收到错误速度和双重消费。

### 3.9 HIGH：FocusRequester 请求没有 WindowId ✅ 已修复（未提交改动）

位置：winia/src/modifier.rs:2193-2220、winia/src/app.rs:902-929。FOCUS_REQUESTS 是进程级 Vec<u64>；任意窗口的 RedrawRequested 都会消费全部请求。B 窗口请求可能先被 A 消费并丢弃。

当前状态：`FOCUS_REQUESTS` 已改为 `Vec<FocusRequest{ window_id, id }>`（modifier.rs:2200-2205）；`FocusRequester::new()` 通过 `CURRENT_FOCUS_WINDOW` thread-local 记录当前窗口（modifier.rs:2195-2220）；`take_focus_requests()` 只消费匹配当前窗口的请求、保留其他窗口请求；app.rs 在 recompose_layout_render 内通过 `composer.focus_window(window_id)` 绑定窗口上下文。新增测试 `test_focus_requests_are_window_scoped`、`test_focus_requester_restores_window_context`（modifier.rs:2281-2312）。

### 3.10 HIGH：SelectionRegistrar fallback 是进程全局 last-writer-wins ✅ 已修复（未提交改动）

位置：winia/src/ui/selection_container.rs:206-212、winia/src/ui/selection_container.rs:244-269、winia/src/app.rs:2341-2345、winia/src/render.rs:836-845。ACTIVE_REGISTRAR 在 SelectionContainer build 时覆盖，但退出不恢复/清空；未注册节点的点击和渲染 fallback 可能访问另一窗口或另一嵌套容器的 registrar。

当前状态：全局 `ACTIVE_REGISTRAR` 已删除，render.rs:836/841/926 与 app.rs:2357 的 fallback 改为仅用 node 本地 `registrar`（Option），不再回退全局；SelectionRegistrar 通过 `LOCAL_SELECTION_REGISTRAR` CompositionLocal provides 作用域化（新增测试 `test_local_selection_registrar_scopes_to_provides`）。

### 3.11 HIGH：TextField 光标闪烁任务绕过生命周期

位置：winia/src/ui/text_field.rs:1010-1049；对应的生命周期安全 API 位于 winia/src/effect.rs:22-201。TextField 直接 tokio::spawn 无限循环，没有 remember_coroutine_scope、LaunchedEffect 或 on_remove cleanup。TextField 移除后任务仍可能每 100ms 轮询并持有 State/interaction source；没有 Tokio runtime 的环境还可能在 build 时失败。

## 4. LazyList、滚动和布局缓存风险

### 4.1 同数量数据变化会复用错误高度

位置：winia/src/ui/lazy_column.rs:494-516、winia/src/ui/lazy_column.rs:603-617。ItemHeightCache 按 index 存储高度，last_known_first_key 只有 total_changed 时才校正。同 total 的替换、重排或 key 内容变化会保留旧高度，导致 prefix_height、visible_range 和 first-visible 错误。

### 4.2 measure 校正可能没有触发下一轮组合

位置：winia/src/ui/lazy_column.rs:693-769、winia/src/ui/lazy_column.rs:847-908。组合期按预估高度注册窗口，测量期写回真实高度。如果 first_visible_index/offset 数值未变但 end 窗口发生变化，可能不会 Enter 组合新 item。

### 4.3 reverse LazyList 首帧 content height 过期

位置：winia/src/layout/node.rs:1436-1487、winia/src/render.rs:697-712。measure 前读取上一帧或 0 的 content height，measure 后才写真实值，可能影响首帧 reverse translate、max offset 和首次输入 clamp。

### 4.4 Scroll metadata 不在 CachedNode

位置：winia/src/layout/node.rs:204-265、winia/src/layout/node.rs:1436-1473。viewport、content height/width、reverse 不在 CachedNode；clean/stub 节点在 modifier 或 constraint 语义改变后，下一次真实 measure 前可能继续被 render、hit-test 或输入 clamp 使用。

### 4.5 Flex spacing 对零尺寸 child 不一致

位置：winia/src/layout/flex.rs:159-178、winia/src/layout/flex.rs:206-245。phase 1 只统计之前尺寸非零的 child，最终 total_spacing 却按全部 child 计算。零尺寸节点会造成剩余约束和最终放置的 spacing 语义不一致。

## 5. TextField、IME、Selection 风险

### 5.1 IME candidate rectangle 可能偏移

位置：winia/src/app.rs:996-1062、winia/src/render.rs:826-930。app.rs 的 center/right x_off 使用 input leaf 全宽和 intrinsic width；render.rs 使用扣除 padding 的 content_x/content_w。非左对齐且带 padding 的 TextField，候选框可能与绘制光标不一致。

### 5.2 Undo 快照缺少 composing_range

位置：winia/src/ui/text_field.rs:91-132、winia/src/ui/text_field.rs:134-146、winia/src/ui/text_field.rs:1779-1828。UndoManager 只保存 String 和 selection，preedit/commit/undo 交错时无法恢复组合范围。

### 5.3 Preedit cursor 单位未明确

位置：winia/src/ui/text_field.rs:1803-1815。文本和 composing_range 使用 UTF-8 byte offset，但 platform cursor 值直接加到 byte position。CJK、emoji 或 UTF-16/code-unit 偏移可能形成非字符边界。

### 5.4 Deleted range 没有防御 clamp

位置：winia/src/ui/text_field.rs:37-75。Inserted 路径有 clamp，Deleted 直接 text.drain(range)，stale selection/IME/registrar range 会导致字符串边界 panic。

### 5.5 选区存在两个状态源

位置：winia/src/render.rs:836-859、winia/src/ui/text_field.rs:1832-1869。registrar selection 在拖动中先更新，value/selection_range 通常 pointer-up 才同步；render 同时绘制两套高亮，重组或 blink tick 期间可能重复或回退。

### 5.6 Focus restore 可能保留 stale focused_id

位置：winia/src/app.rs:240-257、winia/src/app.rs:347-358、winia/src/app.rs:801-857。树中暂时没有 focused node 时保留旧 focused_id，之后 KeyboardInput/IME 仍使用旧 id；slot key 重新绑定后还可能把焦点/IME 送到语义不同的输入节点。

## 6. 多窗口与上下文边界

### 6.1 adaptive 是 thread-local singleton ✅ 已修复（未提交改动）

位置：winia/src/ui/adaptive.rs:12-39、winia/src/app.rs:320-343、winia/src/app.rs:1315-1331。WINDOW_SIZE 和 WINDOW_SIZE_STATE 每线程只有一份，每个窗口 compose 都覆盖上一窗口的值。当前同步单线程路径看似可用，但异步读取、未来并行 compose 或 deferred callback 会看到错误窗口尺寸。

当前状态：引入 per-Composer `AdaptiveContext`（adaptive.rs:17-77）+ `AdaptiveContextGuard`/`enter_context`；composer 持有 `adaptive` 字段（composer.rs:1576），compose/layout 入口注入。旧 `WINDOW_SIZE`/`WINDOW_SIZE_STATE` thread-local 降级为 `FALLBACK_*`（仅纯测试兜底，无响应式）。新增测试 `nested_composers_keep_adaptive_context_isolated`。

### 6.2 App modifier 状态全局共享 ✅ 已修复（未提交改动）

位置：winia/src/app.rs:412-425、winia/src/app.rs:571-584、winia/src/app.rs:662-674、winia/src/app.rs:728-744。AppState 只有一个 modifiers，A 的 ModifiersChanged 会改变 B 的 wheel、pointer 和 keyboard 语义。

当前状态：`AppState::modifiers` 删除，改为 `PerWindow::modifiers`（app.rs:108）；`ModifiersChanged` 写入当前窗口 `pw.modifiers`（app.rs:735）；wheel/pointer/keyboard 全改用 `pw.modifiers`（app.rs:582/640/676/719/746）。

### 6.3 Window lifecycle flags 全局共享 ✅ 已修复（未提交改动）

位置：winia/src/ui/window.rs:12-16、winia/src/ui/window.rs:84-110、winia/src/core/composer.rs:1444-1445、winia/src/app.rs:315-316。WINDOW_REBUILT 和 PENDING_REMOVE_ID 是线程级 singleton，交错窗口 compose/回收时可能抑制或误消费另一个窗口的关闭事件。

当前状态：`WINDOW_REBUILT`/`PENDING_REMOVE_ID` thread-local 删除，改为 per-Composer `LifecycleState`（window.rs:27-52，Arc<AtomicBool>/Arc<AtomicU64>）；composer 持有 `lifecycle` 字段（composer.rs:1574），compose 入口 `reset_for_compose`；`process_detached` 按每个 composer 的 `pending_window_close_id()` 逐窗口处理。新增测试 `test_window_lifecycle_isolation_per_composer`（composer.rs:2508）。

### 6.4 Global animation registry 不是窗口级 🔶 部分修复（未提交改动）

位置：winia/src/animation.rs:35-39、winia/src/animation.rs:129-186、winia/src/app.rs:431-446、winia/src/app.rs:897-901。任一窗口动画都会参与全局 update_animations 和调度；关闭窗口期间动画也依赖显式 dispose/on_remove 清理。全局测试表会造成时间敏感和相互影响。

当前状态：registry 仍是全局 `ACTIVE_ANIMATIONS`/`ACTIVE_COLOR_ANIMATIONS`（animation.rs:38-39），`AnimationInstance::state_id() -> u32` 仍 u32；未提交改动新增 `clear_animations_for_states(&[u32])`（animation.rs:43-50）+ per-Composer `animation_state_ids`（composer.rs:1578），Composer drop 时清理自有动画（新增测试 `composer_drop_cleans_owned_animation_without_touching_other_composer`），缓解跨 Composer 泄漏；全局 registry + 时间敏感测试表问题仍存在。

### 6.5 Focus request、debug queue、screenshot 都是 singleton 🔶 部分修复（未提交改动）

FocusRequester、debug event queue、screenshot flag/pixel buffer、event loop proxy 和 shutdown flag 分别位于 winia/src/modifier.rs:2193-2220、winia/src/debug.rs:17-52、winia/src/debug.rs:137-159、winia/src/app.rs:1358-1372。debug event 只由 parent window 消费，见 winia/src/app.rs:1088-1255；WebSocket 的 ok click/ok key 在实际命中、重组和绘制前返回，见 winia/src/debug.rs:334-376。

当前状态：FocusRequester 已带 window_id（见 3.9）。debug.rs 重构为 `DebugRuntime` 统一结构体收拢 wake_callback/event_loop_proxy/shutdown（debug.rs:22-67）+ `begin_session`/`end_session`；截图改为 per-window `SCREENSHOT_TARGET` + per-window `pixel_frames` HashMap；事件队列改为 `(u64, DebugEvent)` 带目标窗口，`take_queued_events(window_id)`/`queued_event_targets()` 按窗口路由；lib.rs no-debug 桩同步更新。剩余：WebSocket screenshot 仍固定等待 100ms 无 frame sequence（见 §8）。

## 7. CompositionLocal、Theme、Density

CompositionLocal 的核心实现位于 winia/src/core/composition_local.rs:13-93：线程局部 SLOTS、同步 provides、rposition 查找和 PopGuard panic 清理。同步嵌套本身正确，但 provider 退出后异步任务和渲染阶段只能看到默认值，因此 direction、theme、density 等必须在组合期捕获。

Theme provider 位于 winia/src/ui/theme.rs:254-367，Density provider 位于 winia/src/unit.rs:346-357。

### 7.1 ScaleFactorChanged 不会自动触发组合

位置：winia/src/app.rs:618-623、winia/src/ui/text.rs:225-279、winia/src/ui/rich_text.rs:81-97。ScaleFactorChanged 更新 scale_factor 并 request_redraw，但没有标记 composer dirty；TextUnit::Px 等值在组合期转换后写入 modifier，scale-only 变化可能保留旧 logical size。

### 7.2 render 阶段读取默认 Theme

位置：winia/src/render.rs:854-855、winia/src/render.rs:919-920。render 发生在 with_theme provider 退出后，这两处 WiniaTheme::colors() 会返回默认主题；selection highlight 和 composing underline 可能忽略窗口/子树自定义主题。

## 8. Render、Backend 与 Debug

render::render 入口位于 winia/src/render.rs:20-22，主遍历位于 winia/src/render.rs:595-1042，顺序是 backdrop blur、graphics layer、shadow、modifier 绘制、scroll clip/translate、children、ripple/focus。backdrop blur 位于 winia/src/render.rs:1174-1274，使用 device-space snapshot、3*sigma margin、surface clamp、CropRect blur 和逆矩阵 draw-back；旋转 ancestor 或自身 graphics transform 仍是近似限制。

skiwin 统一 trait 位于 skiwin/src/lib.rs:21-101，但 app 直接持有 VulkanSkiaWindow。Vulkan acquire/device-loss 在 skiwin/src/vulkan/renderer.rs:269-295 的 debug 构建会 panic；CPU resize/present 错误在 skiwin/src/cpu.rs:69-123 也存在 debug panic 路径。

RedrawRequested 的 catch_unwind 位于 winia/src/app.rs:951-995；连续 30 次 panic 后 render_disabled，见 winia/src/app.rs:983-994。此策略能防止 panic 风暴，但把可恢复的用户 content panic 与 backend 故障合并为永久停更。

UI tree 由 winia/src/debug.rs:168-228 手工拼接。TextContent 做了转义，但 tag 在 winia/src/debug.rs:185-190 直接插入，带引号、反斜杠或控制字符的 testTag 会生成非法 JSON。

截图状态：未提交改动已从单 bool 改为 per-window `SCREENSHOT_TARGET` + per-window `pixel_frames` HashMap（见 6.5），多窗口/连续请求合并问题已修复；但 WebSocket screenshot 仍固定等待 100ms（debug.rs:378-388），没有 frame sequence，可能返回上一帧。

## 9. 关键不变量

1. State、FocusRequester、SelectionRegistrar、adaptive size、debug event、Window lifecycle 和 animation 必须明确绑定 Composer 或 WindowId。
2. panic 后必须同时恢复 GROUP_STACK、STMT_STACK、scope_source_stack、key override、DEP_MODE、DEP_BUFFER 和 recorder queue。
3. slot_deps 与 layout_deps 必须按实际读取生命周期收敛；节点移除必须注销依赖。
4. LayoutNode 复用必须清除 IME、cursor、selection、interaction、scroll metadata、parent_id 和 content-kind flags。
5. Skip 恢复必须验证 key、直接子结构和语义身份，不能只依赖 child count。
6. render、hit-test、PointerEvent.position、scene_to_node_local、node_abs_position、scroll clip/translate 必须共享同一坐标转换。
7. LazyList 的“组合期估算 -> measure 期校正”必须保证校正后窗口最终重组并包含所有可见 item。
8. Overlay click capture 应保存稳定 overlay id，而不是 vector index；当前字段位于 winia/src/app.rs:98-101。
9. set_visual 和动态绘制闭包只保证下一次可靠 redraw 使用新值；动画 scheduler 必须保证 redraw 不被节流或跨窗口状态吞掉。
10. CompositionLocal 只在 provider 同步闭包内有效；异步任务和 render 不能依赖 provider 当前值，必须捕获快照或显式传 context。

## 10. 最小验证矩阵

### 10.1 Composer/State

- owner TLS：remember 后创建普通 State，确认不通知旧 Composer。
- panic recovery：scope/group/node 中 panic 后重新 compose，确认依赖目标和 GROUP_STACK 恢复。
- dependency unsubscribe：A 依赖移除后 set(A) 不再重组旧 slot。
- nested compose：外层依赖在调用内层 Composer 后仍保留。
- layout-only：只改 measure State，直接 layout 也必须重新 measure。
- same-key replacement：相同 child count 但语义节点替换时不复用旧状态。
- duplicate key：在缓存写入前 fail-fast，不允许 HashMap 静默覆盖。

### 10.2 Layout/Scroll/Input

- 两个并排 scroll container，鼠标分别位于左右区域，wheel 只改变命中区域。
- nested scroll child 到边界后，将剩余 delta/velocity 按明确顺序交给 ancestor。
- 三层 nested fling recorder 检查 pre/post identity 和 consumed_by_child。
- scroll 内 PointerEvent.position 与渲染局部坐标一致。
- reverse LazyList 首帧、wheel、drag、fling 的方向和 clamp 一致。
- 同 total 的 LazyList reorder/replace，异构 item 高度和 stable key 都保持正确 anchor。
- zero-size child + spacing 的 Row/Column/LazyList 几何稳定。
- scroll/non-scroll、Text/Image/RichText 切换后 node metadata 全部刷新。

### 10.3 TextField/IME

- centered/right aligned + padding 的 TextField，比较绘制光标和 IME candidate rect。
- CJK/emoji preedit，验证 cursor 单位、composing range 和 replace_range 边界。
- preedit -> undo/redo -> preedit update，验证 composing_range。
- stale selection/IME range 不得让 TextChange::Deleted panic。
- 拖动选择中触发 blink/recompose，不能双重或回退高亮。
- focused TextField 移除/替换后，键盘和 IME 事件不能发送到旧 id。
- TextField 移除后 blink task 停止；无 Tokio runtime 时契约明确。

### 10.4 Multi-window/Debug

- A/B 两窗口交错 compose，adaptive size、SelectionRegistrar、modifier keys、FocusRequester 互不污染。
- B.request_focus 后先触发 A redraw，B 仍能在之后聚焦。
- 每个窗口独立 debug event queue、目标窗口和 screenshot sequence。
- 自定义 testTag 含引号、反斜杠、换行时 tree JSON 仍可解析。
- resize/DPI-only 事件能更新 Px 文本和布局。
- overlay open/close/host close 后 tree overlay 条目无残留。
- backend OutOfDate/device-loss 后窗口能重建或给出明确停更状态。

## 11. 推荐处理顺序

### Phase 1：所有权与恢复边界（当前状态：🔶 部分完成）

1. [x] 引入 per-Composer/per-Window context，收拢 State owner、adaptive、focus、selection、debug event 和 Window lifecycle。

   **完成项**：
   - State owner：`ComposerSubscription` 每 Composer 独立持有（state.rs:37），`compose_slot_reads`/`layout_slot_reads`/`slot_deps`/`layout_deps`（composer.rs:1563-1575）均为 Composer 字段。
   - adaptive：`Composer::adaptive`（composer.rs:1597）+ `AdaptiveContext`（adaptive.rs:17），compose/layout 期经 `enter_context` 注入/弹出。
   - focus：`FocusRequester` 携带 `window_id`（modifier.rs:2248），`CURRENT_FOCUS_WINDOW` TLS 隔离，`take_focus_requests` 按窗口过滤。
   - selection：`LOCAL_SELECTION_REGISTRAR`（selection_container.rs:206）为 CompositionLocal，由 `ctx.remember_at_key` 创建，等效 per-composer。
   - Window lifecycle：`Composer::lifecycle`（composer.rs:1595）+ `LifecycleState`（window.rs:27），测试 `test_window_lifecycle_isolation_per_composer`。

   **残留缺口**（未收拢的全局单态）：
   - [~] debug event：`DEBUG_RUNTIME`（debug.rs:28）、`DEBUG_STATE`（debug.rs:17）、`LEGACY_TARGET`（debug.rs:19）、`SCREENSHOT_TARGET`（debug.rs:20）仍为全局 `LazyLock<Mutex>`/`static Mutex`，`DebugRuntime` 无 per-window 字段。
       **不继续清理的原因**：`DEBUG_RUNTIME`（wake_callback/event_loop_proxy）为单进程单事件循环，本质全局。`DEBUG_STATE` 已按 `window_id` 在 HashMap 内部分区（debug.rs:138-144）。`LEGACY_TARGET`/`SCREENSHOT_TARGET` 可合入 `DebugRuntime` 但仅是代码美化，不影响正确性。
   - [~] 动画表：`ACTIVE_ANIMATIONS`（animation.rs:35）、`ACTIVE_COLOR_ANIMATIONS`（animation.rs:38）为全局 `LazyLock<Mutex<Vec<..>>>`，并行测试互相干扰。
       **不继续清理的原因**：全局唯一 `StateId`（u64）使动画表本质上可跨窗口安全查找；`Composer.animation_state_ids`（composer.rs:1599）已实现 Composer drop 时只清理自有动画（`clear_animations_for_states`），达成语义隔离。per-window 物理收拢需改动画注册/调度/全局表查询接口，影响面大、收益低。并行测试干扰是全局测试表固有现象，不属生产问题。
   - [~] 事件循环/窗口路由：`GLOBAL_PENDING`（app.rs:1375）、`APP_PROXY`（app.rs:1376）、`CREATED`（window.rs:57）全局 HashSet、`NEXT_ID`（window.rs:58）全局 AtomicU64。
       **不继续清理的原因**：`APP_PROXY` 是 winit 单事件循环代理，天然全局。`GLOBAL_PENDING` 是窗口创建/关闭的进程级队列，需要全局可见。`CREATED` 是窗口注册表、`NEXT_ID` 是 ID 分配器——两者改为 per-window 反而有害（窗口 ID 必须在全局范围内唯一）。这些是设计上正确的全局，不属架构缺陷。
   - [x] TLS 框架隔离：`ACTIVE_SLOT_KEY`/`GROUP_STACK`/`STMT_STACK`（composer.rs:47-49）、`DEP_BUFFER`/`DEP_MODE`/`RECORDER_QUEUE`（state.rs:533-535）有 `RuntimeFrameGuard`/`DependencyFrameGuard` 兜底，已隔离。

2. [x] 用 guard 或统一 compose transaction 清理所有 TLS/依赖模式，覆盖 panic 与 nested/reentrant compose 契约。

   **完成项**（6 个 guard 分层串联，非单一大事务但已覆盖所有 panic/nested 路径）：
   - `RuntimeFrameGuard`（composer.rs:62）：隔离 compose/layout 的 TLS，`restore_runtime_frame` 正常与 panic 都恢复。
   - `ComposeRuntimeTransaction`（composer.rs:818）+ `rollback_compose_runtime`（composer.rs:2125）：回滚 SlotTable 运行时上下文（key 栈/计数器等）。
   - `ComposeDependencyTransaction`（composer.rs:1393）+ `rollback`（composer.rs:1423）：回滚完整依赖图 + 清失败帧订阅 + 恢复 pending 批。
   - `PendingBatchGuard`（composer.rs:1491）+ Drop（composer.rs:1516）：panic 时 `restore_pending` 把未提交批放回队列。
   - `LayoutTransaction`（composer.rs:1242）+ rollback（composer.rs:1314）：回滚 arena 节点/依赖图/prev 节点/pending。
   - `DependencyFrameGuard`（state.rs:521）+ Drop（state.rs:602）：嵌套 compose/layout 隔离 recorder，panic 清除失败帧订阅。

   **测试覆盖**：`test_runtime_frame_restores_tls_after_panic_and_nested_drop`、`test_nested_composer_compose_preserves_outer_dependencies`、`test_compose_late_cleanup_panic_restores_dependency_graph`、`test_compose_runtime_snapshot_restores_key_context_after_panic`、`test_compose_panic_recovers_next_frame`、`test_compose_pending_batch_restored_after_panic`、`test_panic_restore_keeps_both_consumed_batch_and_in_frame_notification`、`test_layout_dependency_panic_rolls_back_new_subscription`、`test_mixed_compose_layout_pending_reaches_recompose` 等 14 个 panic/nested/reentrant 测试。

   **残留缺口**：
   - `Slot.remembered`（`HashMap<u64, Box<dyn Any>>`）未纳入任何事务——不属于任何 guard 的 snapshot/restore 范围；但现有测试验证 panic 后下帧自愈（`test_compose_panic_recovers_next_frame`）。此缺口是设计取舍（`Box<dyn Any>` 不可 Clone，见 §3.2 comments）。

3. [x] 让 slot_deps 按实际读取收敛，并把节点移除与依赖注销统一起来。

   **完成项**：
   - 按实际读取收敛：`reconcile_compose_deps`（composer.rs:2031-2069）——`retain(live_keys)` 清死 key（2047），Enter 用本帧实际读取替换（2048-2058），Skip 保留旧读取，无读取的 Enter slot 移除。
   - 节点移除与依赖注销统一：compose 末尾死 key 写入 `removed_slot_keys`（composer.rs:2250-2253）；layout 末尾对每个 removed key 从 `layout_slot_reads`/`layout_dirty_keys` 删除（composer.rs:2374-2377）；订阅注销统一走 `cleanup_signal_subscriptions`（composer.rs:2071-2105）→ `signal.unsubscribe` + `retain_signals`。
   - 反向图权威：`rebuild_compose_reverse_deps`（composer.rs:1989-1996）与 `rebuild_layout_reverse_deps`（composer.rs:1999-2006）从正向图 clear+rebuild（单一权威源防 stale 边），`debug_assert_dependency_graphs`（composer.rs:2009-2029）校验正反向一致性。
   - 测试：`test_dependency_reverse_graph_rebuilds_from_forward_reads`、`test_layout_slot_reads_remove_empty_remeasure`、`test_layout_slot_reads_remove_removed_slot`、`test_compose_read_removal_unsubscribes_stale_state`、`test_composer_drop_unsubscribes_state_signals`、`test_removed_read_notification_during_compose_is_dropped`、`test_layout_dep_remesures_without_recompose` 等。

   **残留缺口**：反向图是 clear+rebuild 而非逐边增量 patch——此为设计取舍（注释 composer.rs:1988 明示），`debug_assert` 担保一致性，不作为缺口。

### Phase 2：节点复用与坐标（当前状态：🔶 进行中）

1. [~] 建立 LayoutNode::update_from_desc/reset_from_desc（对应 §3.5）

   **目标**：复用节点时统一重置所有语义字段，防止残留旧值污染测量/渲染路径。

   **子项**：
   - [x] content-kind 标记同步：materialize 复用路径按新 modifier 调用 `modifier_has_text`/`modifier_has_richtext`/`modifier_has_image` 更新 `has_*_content`，内容类型切换时清空 `cached_paragraph`（materialize.rs:187-202）。`edd4827` 已落地。
   - [x] IME/cursor/selection/registrar 的 reset：复用路径在**内容类型切换**时调用 `clear_textfield_state` 清理（cursor_callback/ime_callback/composing_range/selection_range/registrar/display_focused/focus_color/focused）；**Enter 路径且 desc 不提供 IME/cursor 回调**（语义角色切换 TextField→普通 Text）时也清理，覆盖 content-kind 不变的场景。新测试 `test_materialize_reuse_clears_stale_textfield_state_on_role_switch`。
   - [x] scroll metadata 的 reset：`scroll_viewport_height/width`、`scroll_content_height/width`、`scroll_reverse` 在复用路径若新 modifier 无 scroll state 则重置为 0（measure_node 重新计算）。
   - [x] parent_id 的 reset：复用路径防御性重置为 `None`（`add_child` 在末尾重新设置正确的值）。

   **验证**：`test_text_content_change_remeasures` 通过；新增 `test_layout_node_reuse_resets_ime_selection_registrar` 覆盖 IME/selection 残留场景。

   **涉及文件**：`core/materialize.rs`（复用路径）、`layout/node.rs`（LayoutNode 字段定义）

2. [~] 统一 scroll-aware path transform（对应 §3.6 PointerEvent + §3.7 MouseWheel）

   **目标**：修正滚动容器内指针事件、命中测试、capture 和滚轮操作的坐标系统，确保所有坐标变换路径一致。

   **子项**：
   - [x] `dispatch_ptr_event` 坐标修正：`app.rs:2668-2708` 当前只累加 `node.position`，未扣除祖先 scroll offset。改为使用 `scroll_offset_for_node`（`layout/node.rs:586-593`）的坐标、与 `hit_test`/`node_abs_position` 保持一致。
   - [x] MouseWheel 按命中位置选目标：`app.rs:571-587`/`app.rs:1540-1547` 的 `find_scroll_target` 当前丢弃 cursor position。改为记录每个窗口最后 `PointerMoved` 的逻辑坐标，wheel 时先 hit-test，再在命中路径上执行 nested scroll。
   - [x] 坐标一致性测试：新增 `pointer_dispatch_coord_tests`（app.rs）3 个测试——`dispatch_local_coord_matches_scene_to_node_local_in_scroll`（子节点坐标扣除祖先 scroll）、`dispatch_coord_follows_scroll_offset_change`（滚动偏移变化后同步）、`dispatch_scroll_container_own_coord_uses_ancestor_not_self`（容器自身不减自身 offset）。

   **验证**：`hit_test`、`scene_to_node_local`、`node_abs_position`、`dispatch_ptr_event` 坐标一致；并排滚动容器各自正确响应滚轮。

   **涉及文件**：`layout/node.rs`（`scroll_offset_for_node`、`hit_test`、`scene_to_node_local`）、`app.rs`（`dispatch_ptr_event`、`find_scroll_target`、`apply_scroll_delta`）、`render.rs`（scroll clip/translate）

3. [ ] 锁定 nested scroll delta/fling 的 target/ancestor 顺序和消费语义（对应 §3.8）

   **目标**：修正 nested fling 的回调链和消费量计算，确保 pre/post 顺序、ancestor handoff、consumed_by_child 语义正确。

   **子项**：
   - [ ] 修正 `dispatch_nested_scroll_fling`（`app.rs:1464-1537`）：目标自身 NestedScrollConnection 不应放入 post 链；`child_velocity` 应为实际消费后的余量而非原始值；child 撞边界后的 handoff 应将剩余 velocity 传给 post 链而非 pre。
   - [ ] 新增测试：`test_nested_fling_consumption_order` 验证三层嵌套 fling 的 pre/post identity 和 consumed_by_child 正确。

   **验证**：三层嵌套 scroll 容器 + TopAppBar 的 fling 消费量、回调顺序、边界 handoff 符合预期。

   **涉及文件**：`app.rs`（`dispatch_nested_scroll_fling`、`apply_scroll_delta`）、`nested_scroll.rs`（`NestedScrollConnection` trait 定义）

### Phase 3：LazyList 与 TextField

1. [x] 用 stable item key 管理 LazyList height cache，保证同 total 数据变化也能失效（方案 A——按 item key 缓存高度，已实施）

   **目标**：`ItemHeightCache` 此前按 global index 缓存高度（`Vec<f32>`），数据前部增删/重排导致 index 平移时旧高度错位（同 key 的项高度被错误继承到别的位置）；"同 total 但内容变化"无任何失效机制。改为让高度缓存跟随**项身份（item key）**而非位置。

   **子项**：
   - [x] `ItemHeightCache` 增加 key 视图：新增私有 `keyed: HashMap<u64, f32>`（item key → 高度），公开方法 `record_keyed(key, h)` / `height_keyed(key)` / `rebase(&[u64])`；保留 `heights: Vec<f32>` index 视图与 `record`/`height` 接口（向后兼容，公开 API 签名不变，`PartialEq`/`Clone`/`Default` 仍可用）。
   - [x] `LazyListPolicy` 增加 `keys: Vec<u64>` 字段（当前数据 global index → item key），measure 阶段在写 index 视图的同时 `record_keyed(keys[global], h)` 写 key 视图。
   - [x] `LazyList::build` 每次从 `intervals` 提取 `keys`（`(0..total).map(|g| key_of(g))`，无 key 段用索引自身）；用 FNV-1a 折叠 key 序列作数据签名（`data_sig` State），签名变化（total 变或同 total 重排/替换）时调 `cache.rebase(&keys)`——把 key 视图里仍存在的项高度迁移到正确 index，清空越界高度与消失的 key 记录。
   - [x] **rebase 必须位于 `total_changed` 的 offset 校正之前**（debug-server 验证发现）：`total_changed` 用 `last_known_first_key` + `prefix_height` 重算 offset，若此刻 cache 仍是旧 index 视图（未 rebase），prefix 按旧数据 index 算错 → offset 错位（实测插入 10 项后 firstVisible=62 offset=3840，应为 firstVisible=60 offset=3696，差 144）。先 rebase 再校正则精确。

   **效果**：数据前部增删/重排后，同一 item key 的项高度精确跟随（不因 index 平移丢失或错位）；新插入项走预估后由测量填充；数据变短时越界高度清除。滚动位置与已测高度同时稳定。

   **局限**（方案 A 固有，非本实现缺陷）：
   - **同 key 内容变化**（item id 没变但内容高度变了，如某行文本更新且 key 仍是数据 id）：key 相同 → 高度缓存复用旧值（一帧陈旧）。根因是信息论局限——框架无法自动区分"项没变"与"项内容变了但身份没变"。Compose 语义同样如此，靠约定"内容变化应伴随 key 变化"（key = 内容哈希 或数据版本）。
   - **无 key 段（`items_plain`，index 即 key）**：无法追踪项身份，数据前部增删仍会错位（这是放弃 key 的固有代价，Compose 同理）。建议使用 `items_from`/`item_keyed` 提供稳定 key。
   - **数据签名用 key 序列 FNV 折叠**：不同 key 序列可能碰撞（64 位 FNV 碰撞概率极低，理论存在）；碰撞会导致漏触发 rebase（连带 `total_changed` 的 offset 校正读到未 rebase 的旧 cache，同样受影响），实际可忽略。

   **提升方案**（如需彻底解决"同 key 内容变化"）：
   - 方向 B：增加显式 `.data_version(n)`（内容 generation）参数，用户数据内容变化但 key 不变时递增，build 检测到 generation 变化即强制重测受影响项（或清空缓存）。与方案 A 可叠加：key 决定身份迁移，generation 决定内容失效。
   - 更细：rebase 只对 `keyed` 中缺失的项（新 key 或无 key 段）强制重测，而非整表清空——已由 `record` 逐项覆盖达成。
   - 性能：当前 `rebase` 每次数据变化 O(n) 重建 index 视图；若列表极大且变化频繁，可改为按 key 直接读写（彻底弃用 index 视图），但 `prefix_height`/`anchor_from_offset`/`visible_range` 依赖 index 连续扫描与越界预估，改动面大，未做。

   **验证**：新增单元测试 `height_cache_keyed_rebase_follows_identity`（前插 1 项后 key=2→index 3、key=5→index 6 高度迁移正确；新项走预估；旧 index 不残留；数据变短清越界 + 清消失 key）；既有 lazy 测试 27 项全通过；完整 `cargo test -p winia --lib` 634 项通过。另用 debug-server 交互验证 demo `winia/examples/hc_verify_demo.rs`：100 项（48/96 混合高，key=id）滚动到项 50 → 连续前部插入 10 项三次，`firstVisible` 精确 50→60→70→80、`offset` 3216→3696→4176→4656（每次 +480 = 10×48），可见项始终保持 Item 50 在视口——高度缓存与滚动位置按 key 精确跟随。

   **涉及文件**：`winia/src/ui/lazy_column.rs`（`ItemHeightCache`、`LazyListPolicy`、`LazyList::build`）

2. 为 measure/build convergence 增加显式的窗口变化重组通道。
3. 将 TextField blink 纳入 effect 生命周期，统一 UTF-8 byte offset、IME cursor 单位和 composing undo 快照。
4. 清理 dual selection source，明确 registrar 与 TextFieldValue 的单一事实来源。

### Phase 4：窗口平台与 E2E

1. ScaleFactorChanged 标记需要重新组合/重新解析 density-sensitive modifier。
2. 将主题颜色捕获到 node/descriptor，禁止 render 阶段读取已退出的 CompositionLocal。
3. 增加真实窗口 E2E：focus、IME、pointer drag、wheel、resize、DPI、overlay、debug tree、screenshot、panic recovery 和多窗口。
4. 最后处理 navigation suite 的旧 API 调用点和组件集成覆盖。

本审计是架构风险和验证顺序记录，不代表所有问题都必须立即修复。建议先用最小验证矩阵锁定语义，再进行实现调整。
