# State 响应式架构重构进度

> 目标：彻底移除 State creator-owner TLS，改为 ownerless State + 按读取建立 Composer 订阅。
> 范围：D:/Projects/winia 当前 v2。此文档跟踪实现进度、验证结果和剩余风险。
> 约束：保留用户既有 dirty worktree 变更；每阶段独立验证；不把临时 RAII owner 修复冒充最终架构。

## 总体状态

- 当前阶段：Phase 3 - RuntimeFrame、pending 消费与布局事务（基础事务切片已落地；ComposeRuntimeTransaction 新增）
- 当前状态：进行中；DependencyFrameGuard、Composer RuntimeFrameGuard、ComposeRuntimeTransaction、layout forward graph 和布局 panic retry 已完成基础切片，多窗口上下文隔离（6 个单例→per-Window/Composer）已完成，完整 compose 事务与精确递归 measure attribution 仍开放
- 最后更新：ComposeRuntimeTransaction、ComposeRuntimeSnapshot、多窗口上下文隔离（Window lifecycle/SelectionRegistrar/FocusRequester/debug/adaptive/modifiers per-Composer/per-Window）、animation registry 生命周期、debug runtime session 生命周期
- 已完成代码：ownerless StateSignal、ComposerSubscription、compose_slot_reads/layout_slot_reads、StateSignal 弱订阅清理、读/写 revision handshake、unread State wake 抑制、Composer drop 取消订阅与 closed 生命周期闸门、layout-only pending 直接消费、compose PendingBatchGuard、DependencyFrameGuard panic 回滚、Composer RuntimeFrameGuard TLS 恢复、布局 pending/graph/cache retry snapshot、ComposeRuntimeTransaction（key/scope/child_counters/dirty_keys/overlay_active 恢复）、ComposeRuntimeSnapshot 七字段、多窗口上下文隔离（AdaptiveContext per-Composer、LifecycleState per-Composer、FocusRequest window_id、PerWindow modifiers、DebugRuntime per-window session、SelectionRegistrar CompositionLocal 隔离、animation_state_ids 生命周期清理）、debug runtime begin_session/end_session
- 当前验证：cargo check -p winia --lib 通过；cargo test -q -p winia --lib -- --test-threads=1 通过（620 项）；cargo check --workspace 和 git diff --check 通过；layout graph empty-read/untouched/removed/empty-root、panic retry、nested State frame、RuntimeFrame、nested Composer、read-removal、layout-only、mixed compose/layout pending、compose pending batch、canonical reverse graph、compose dependency rollback、StateId、revision handshake、unsubscribe linearization、closed subscription、Composer drop、compose runtime transaction、多窗口隔离（FocusRequester window-scoped、Window lifecycle per-Composer、AdaptiveContext isolation、SelectionRegistrar scoped、animation lifecycle per-Composer）回归通过。

## 架构目标

### 最终模型

```text
State::new(value)
  -> 不绑定 Composer，不读取 creator TLS

State::get() in compose/layout
  -> 当前 RuntimeFrame 记录 signal read
  -> 当前 ComposerSubscription 订阅 StateSignal

State::set/update()
  -> StateSignal 通知 live ComposerSubscription
  -> pending StateId 去重入队
  -> compose 或 layout 按 channel 消费
```

### 明确不采用

- 不把 owner TLS 简单清成 None 作为最终方案；嵌套 initializer 仍会被隐式 owner 语义污染。
- 不强制所有公共 State 构造函数接收 StateFactory；这会污染 MutableInteractionSource、ScrollState、LazyListState、TextField 和导航组件 API。
- 不从 RECORDER_QUEUE 推断 State creator owner；reader 和 creator 是不同概念。

## 阶段计划

### Phase 0 - 基础设施准备

- [x] 创建本进度文档。
- [x] 记录当前源码 API、dirty worktree 和测试基线。
- [x] 确定 StateId、StateSignal、ComposerSubscription 的内部接口；StateId 已成为 u64-backed 内部 newtype，public State::id() 暂保留 u32 兼容。

### Phase 1 - Ownerless StateSignal

- [x] 引入内部 StateId newtype，依赖图、订阅队列和 Composer reverse maps 不再直接暴露原始 u32。
- [x] 将 StateId backing 从 u32 升级为 u64；内部路由使用 u64 StateId。
- [~] 暴露 public `State::state_id() -> StateId` 作为 u64-backed migration API；legacy `State::id() -> u32` 和 animation API 仍保持兼容，完整调用方迁移未完成。
- [x] 将 StateInner 改为持有 StateSignal，不再持有 owner_queue。
- [x] 增加每个 StateSignal 的 ComposerSubscription 弱订阅列表。
- [x] 增加 pending StateId 去重队列。
- [x] 让 State::new 完全不读取 STATE_OWNER_QUEUE。

### Phase 2 - 读取订阅与依赖图

- [x] State::get 在当前 compose/layout DependencyFrameGuard 中建立读取订阅。
- [x] 删除 STATE_QUEUE_MAP 和 STATE_SUBSCRIBERS。
- [x] compose_slot_reads 按 Enter/Skip 收敛 compose 读取集合；layout 继续按 touched key 收敛。
- [x] Enter/measure 替换当前读取集合；Skip/常量折叠保留旧集合。
- [x] removed/stale reads 和 Composer drop 取消订阅。
- [~] 以 `compose_slot_reads`/`layout_slot_reads` 为 canonical forward graphs 重建 `slot_deps`/`layout_deps` reverse maps，并加入 debug invariant；完整增量双向更新与事务回滚仍未完成。

### Phase 3 - RuntimeFrame 与 pending 消费

- [x] 基础切片：DependencyFrameGuard 隔离 DEP_BUFFER、DEP_MODE、RECORDER_QUEUE，并在嵌套/panic 时恢复外层 recorder。
- [x] 基础切片：Composer RuntimeFrameGuard 隔离 GROUP_STACK、STMT_STACK、ACTIVE_SLOT_KEY，并在嵌套/panic 时恢复外层 TLS。
- [x] panic 时丢弃失败 frame 新增的 StateSignal 订阅；已提交 frame 保留读取订阅。
- [x] 允许 layout-only State 直接驱动 layout()。
- [x] compose 消费的 pending batch 使用 PendingBatchGuard；帧内新通知留在下一批，compose panic 恢复已消费 batch。
- [x] 布局 slice：snapshot pending、layout graph、prev maps、arena node state 和 scroll limits；panic 后保留 pending 并强制完整 remeasure retry。
- [~] 增加窄范围 `ComposeDependencyTransaction`：late compose panic 可恢复 compose/layout dependency maps 和 signal subscriptions；SlotTable/arena/remember/on_remove 仍未纳入同一事务。
- [x] 增加 `ComposeRuntimeTransaction`/`ComposeRuntimeSnapshot`：panic 时恢复 path/child_counters/dirty_keys/scope_source_stack/key_override_stack/entered_compose_keys/reused_nodes 等小型非持有型运行时上下文；SlotTable/arena/remember/on_remove 仍是明确保留的边界（持有 `Box<dyn Any>` 与用户回调，见 composer.rs:815-817）。
- [x] 多窗口上下文隔离：adaptive size 改为 per-Composer `AdaptiveContext`（不再 thread-local singleton）。
- [x] 多窗口上下文隔离：Window lifecycle（`WINDOW_REBUILT`/`PENDING_REMOVE_ID`）改为 per-Composer `LifecycleState`。
- [x] 多窗口上下文隔离：FocusRequester 带 `window_id`（`FocusRequest` + `CURRENT_FOCUS_WINDOW` TLS），`take_focus_requests` 按窗口过滤。
- [x] 多窗口上下文隔离：App modifiers 改为 per-Window（`PerWindow::modifiers`）。
- [x] 多窗口上下文隔离：SelectionRegistrar 删除全局 `ACTIVE_REGISTRAR` fallback，改 CompositionLocal 作用域化。
- [x] 多窗口上下文隔离：debug event/screenshot/pixel 按窗口路由（`DebugRuntime` + per-window target/queue）。
- [x] 多窗口上下文隔离：animation registry 生命周期——`clear_animations_for_states` + per-Composer `animation_state_ids`，Composer drop 时清理自有动画。
- [x] debug runtime session 生命周期：`begin_session`/`end_session`。
- [ ] 形式化 compose/layout 批次边界，证明 frame 期间到达的新通知只进入下一批。
- [ ] 修复递归 measure 中 parent post-child State::get 的精确 slot attribution；当前 layout tracking 仍由 measure_node 的 active key 驱动。

### Phase 4 - Modifier 与公共状态迁移

- [ ] 移除 vertical_scroll/horizontal_scroll builder 的 State::get 副作用。
- [ ] 将 modifier State 依赖绑定到实际 node slot。
- [ ] 更新 remember、动画、overlay、effect 和 UI 状态注释。
- [ ] 完成 animation API 从 legacy `u32` 迁移到 public `StateId`；当前已提供 `State::state_id()`，内部 u64 与 legacy public u32 已分离。

### Phase 5 - 验证与文档

- [x] 增加 ownerless read-subscribe、unsubscribe、cross-Composer、panic、nested compose、RuntimeFrame、layout-only、StateId 和 layout graph transaction 测试。
- [ ] 完成 cargo fmt --check、cargo test --workspace；cargo check --workspace 和 git diff --check 已通过，formatter 仍不能触碰其他 dirty 文件。
- [x] 更新 docs/architecture-audit.md 中 3.1/3.2 的基础切片状态。
- [x] 记录未解决的多窗口、动画和 debug 全局状态风险；完整 workspace/E2E 仍待验证。

## 当前设计接口草案

### State signal

```rust
struct StateSignal {
    id: StateId,
    revision: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
}
```

```rust
pub struct StateId(u64); // opaque public migration identity; legacy State::id() remains u32
```

### Composer subscription

```rust
struct ComposerSubscription {
    id: u64,
    pending: Mutex<Vec<StateId>>,
    signals: Mutex<Vec<Weak<StateSignal>>>,
    closed: AtomicBool,
}
```

### Frame guards

```rust
struct DependencyFrameGuard { /* restores outer buffer/mode/queue */ }
struct RuntimeFrameGuard { /* restores outer slot/group/statement TLS */ }
```

### 依赖通道

- Compose：StateId -> slot dirty -> recomposition。
- Layout：StateId -> layout_dirty_keys -> remeasure。
- Visual：peek/set_visual -> redraw only。

## 验收不变量

1. State 创建位置不决定响应式通知归属。
2. State 只有被当前 Composer 读取后才订阅该 Composer。
3. 依赖从 slot 移除后，State 更新不再唤醒旧 slot。
4. Composer drop 后，StateSignal 不再投递已销毁队列。
5. panic/nested compose 不丢失外层 RuntimeFrame。
6. layout-only 更新不会被 compose 消费逻辑吞掉。
7. 多次 set 在一个消费周期只产生一个 pending StateId，但读到最新值。
8. StateSignal 通知不在 signal 锁内执行 wake callback；队列去重入队在 signal lock 内线性化，且不执行用户代码。

## 变更记录

- 初始：完成长期 ownerless StateSignal 架构设计。
- 本轮：完成 ownerless StateSignal、读取订阅、cross-Composer fan-out、pending 去重、compose_slot_reads 收敛、stale/removed read 取消订阅、Composer drop 清理、layout-only pending 直接消费。
- 继续：增加 DependencyFrameGuard、Composer RuntimeFrameGuard、panic 时新增订阅回滚、nested Composer 读取隔离与 RuntimeFrame 回归。
- 验证：cargo check -p winia --lib 通过；cargo test -p winia --lib -- --test-threads=1 通过，606 passed；git diff --check 通过。cargo fmt --all -- --check 仍报告 winia-macros/src/lib.rs 等既有/其他 dirty 文件格式差异，未运行格式化以避免触碰用户 dirty changes。
- 继续：增加 LayoutTransaction snapshot，失败 layout 恢复 pending/graph/maps，清理 panic-only layout signal handles，并强制节点完整 remeasure retry；新增 layout_slot_reads 生命周期回归。
- 本轮恢复：撤回未完成的 ComposeTransaction/SlotTable checkpoint 实验，恢复 `Slot.remember` 的 `HashMap<u64, Box<dyn Any>>` 持久化语义，并移除临时 WINIA_FREE_TRACE/WINIA_STATE_TRACE 诊断；remember persistence、infinite transition dispose、layout transaction、RuntimeFrame 和 full library suite 均恢复通过。Compose 内部事务仍保持开放，不把失败实验计入完成项。
- 继续：compose 增加 PendingBatchGuard，只提交已消费 batch；帧内新通知不会被当前批次吞掉，panic 会恢复已消费 ID；新增 compose batch 与 stale-read pending 回归测试。此切片不覆盖 SlotTable/arena/依赖图事务。
- 继续：StateSignal/ComposerSubscription/compose/layout reverse maps 改用 u64-backed 内部 StateId；StateInner::public_id 独立保留 u32，动画 API 行为兼容。StateId 最大值边界和 focused/full regression 已验证。
- 上轮：让 ComposerSubscription 的 signal retention、Composer drop cleanup 和 closed lifecycle 与 StateSignal subscription 在 tracking lock 下协调，禁止关闭后的 queue 被 in-flight read 重新订阅，并在关闭时清空 stale pending；State focused tests 11 passed。
- 本轮：增加 `drain_non_compose_collect_layout` 的 mixed compose/layout pending 回归，证明 layout 分类会标 dirty 但保留 mixed ID 给 compose；新增 Composer integration test 验证同一 State 同时驱动 recompose 与 remeasure；补充 closed queue 清空 stale pending 的 State 回归。State focused tests 12 passed，610 项 full library suite、cargo check --workspace、cargo check -p winia --lib 和 git diff --check 通过。
- 上轮：增加 public `StateId` opaque type、`State::state_id()` 和 raw u64 accessor；legacy `State::id() -> u32` 保持 animation compatibility，animation caller migration 仍开放。StateId focused test 1 passed；随后 full library suite 610 passed。
- 本轮：关闭后的 ComposerSubscription 在 `enqueue()` 和 `restore_pending()` 入口拒绝新通知/失败批次恢复，避免 signal snapshot 或 panic unwind 在 close 后重新制造 pending；State focused tests 12 passed。
- 上轮：closed queue guard 之后重新通过 610 项 full library suite、cargo check -p winia --lib、cargo check --workspace 和 git diff --check。
- 上轮：增加 `rebuild_compose_reverse_deps()`、`rebuild_layout_reverse_deps()` 和 debug-only `debug_assert_dependency_graphs()`，并添加 forward-to-reverse graph regression；canonical reverse map slice 已验证。
- 上轮：`ComposerSubscription::enqueue()` 返回 acceptance，StateSignal 只为实际接受通知的 live queue 计算 wake，并在 notify/handshake 中清理 rejected closed subscribers；新增 closed subscriber cleanup regression。
- 本轮：增加 `ComposeDependencyTransaction` 和 `test_compose_late_cleanup_panic_restores_dependency_graph`；late cleanup panic 恢复上一成功帧的 dependency maps/subscriptions，并清理 failed-frame pending。612 项 full library suite、cargo check -p winia --lib、cargo check --workspace 和 git diff --check 通过。
- 继续：增加 `ComposeRuntimeTransaction`/`ComposeRuntimeSnapshot`，panic 时恢复 path/child_counters/dirty_keys/scope_source_stack/key_override_stack/entered_compose_keys/reused_nodes 等非持有型运行时上下文（SlotTable/arena/remember/on_remove 仍是明确保留边界，持有 `Box<dyn Any>` 与用户回调，见 composer.rs:815-817）。同期完成多窗口上下文隔离：AdaptiveContext per-Composer（adaptive size）、LifecycleState per-Composer（window lifecycle flags）、FocusRequest window_id + CURRENT_FOCUS_WINDOW TLS（FocusRequester 按窗口）、PerWindow::modifiers（App modifier 键）、SelectionRegistrar 删全局 ACTIVE_REGISTRAR 改 CompositionLocal 作用域、DebugRuntime + per-window screenshot/事件队列、animation_state_ids per-Composer + clear_animations_for_states（animation registry 生命周期）、debug begin_session/end_session。620 项 full library suite、cargo check -p winia --lib 通过（cargo test -p winia --lib -- --test-threads=1 实测 620 passed，0 failed）。
- 未完成：Composer 完整 SlotTable/arena/remember/on_remove 事务 rollback、严格 frame 批次证明、精确递归 measure parent attribution、animation API 从 legacy u32 迁移、modifier builder 副作用迁移。
