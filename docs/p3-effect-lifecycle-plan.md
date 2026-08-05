# P3-2 副作用生命周期契约

> 分支：`compose-core` ｜ 状态：设计确认中 ｜ 关联：`docs/cleanup-plan.md` §P3-2

## 一、目标

审计并修复副作用（Effect / 动画 / 协程）与组合生命周期（Enter / Skip / 移除）的
协调问题——历史 bug（快速切换卡住、exit 动画 on_done 丢失、无限动画泄漏）的
系统性排查与收口。

## 二、现状审计（代码证据）

调查结论：**副作用生命周期机制大部分已完整**，仅剩一个真实缺陷。

| 机制 | 实现 | 完整性 |
|------|------|--------|
| DisposableEffect 清理 | remember 状态（prev_key/cleanup）+ `start_leaf_with_remove` 挂 on_remove；key 变化先旧清理再新 setup | ✅ |
| LaunchedEffect 取消 | key 变化 abort 旧协程 + 启动新协程；on_remove abort | ✅ |
| 协程作用域 | `CoroutineScope` = `Arc<ScopeState>`；**Arc 归零（组合点移除→remember 状态 drop）时 drop abort 全部协程**（effect.rs `impl Drop for ScopeState`） | ✅ |
| remember 状态释放 | `Slot.remembered: HashMap`——Slot 被 `truncate` 移除时**级联 drop**（composer.rs:679 `parent.children.truncate(idx)` → Slot drop → remembered drop） | ✅ 无泄漏 |
| on_remove 触发时机 | 物化（同帧）→ compose 末尾 drain `prev_node_by_key` → `free_node_skip` → `on_remove.take()` 触发——**同帧、渲染前**（无滞后）；`take()` 防重复 | ✅ |
| Skip 恢复 | 节点保留 → on_remove 不触发 → 副作用状态保留（remember 幂等） | ✅ |
| 有限动画 | `ACTIVE_ANIMATIONS` 全局表，`update()` 返回 false 自清 | ✅ 完成自清 |
| 无限动画 | `ACTIVE_INFINITE_*` 全局表，**仅 `InfiniteTransition::dispose()` 手动移除** | ❌ **泄漏** |

## 三、缺陷 B：无限动画生命周期泄漏

### 证据

- `remember_infinite_transition`（animation.rs:519）：`ids` 挂在 remember 状态；
  `animate_float/animate_color` 把 state_id 写入 `ACTIVE_INFINITE_*` 全局表并持有
  `State` clone。
- `InfiniteTransition::dispose`（animation.rs:560）：注释明言"**组件离开组合/不再
  需要时手动调用**"——但**没有任何自动接线**（节点 free 不触发 dispose）。
- 后果：无限动画所在组合点被移除后——
  1. remember 状态释放（ids Arc 计数减），但**动画对象仍在全局表**（持有 state clone）；
  2. 每帧 `update_animations()` 继续推进（animation.rs:234）→ `state.set()` → notify →
     pending 推送 → 下一帧 compose 消费（无 slot 依赖 → 无 dirty → 无重组，**但每帧
     空转**）；
  3. **永久泄漏**：动画永不停止、永不释放——长时间运行 + 频繁创建/移除无限动画
     （如导航切换）→ 全局表无限增长 → 每帧空转成本线性增长。

### 修复方案（最小侵入）

**方案 1：on_remove 自动 dispose（推荐）**

`remember_infinite_transition` 内部创建动画作用域后，挂一个隐式 on_remove 节点：

```rust
pub fn remember_infinite_transition(&mut self) -> InfiniteTransition {
    let ids_state = self.remember(|| Arc::new(Mutex::new(Vec::new())));
    let ids = ids_state.get();
    // 自动清理：组合点移除 → on_remove 触发 → dispose（表内移除动画）
    let t = InfiniteTransition { ids: ids.clone() };
    let key = self.next_key();
    let ids2 = ids.clone();
    self.start_leaf_with_remove(key, Modifier::new(), Box::new(move || {
        let ids: Vec<u32> = ids2.lock().unwrap().drain(..).collect();
        for sid in ids { crate::animation::remove_animation_by_state(sid); }
    }));
    self.end_node();
    t
}
```

- 每帧组合重跑时同 key 复用节点、覆盖 on_remove（幂等——内部 ids Arc 相同，
  drain 已消费的 id 不会重复出现）✅
- 节点移除 → free → on_remove → 表内动画移除 → 泄漏收口 ✅
- 与 `InfiniteTransition::dispose()` 并存：显式 dispose 先行 drain 清空 ids，
  后续 on_remove 触发时 drain 空表——**双重触发安全**（drain 幂等）✅

**方案 2：动画对象绑定 remember 状态 drop**（不采用）
`Box<dyn Any>` 无 drop 钩子——需要包装类型 + 状态持有动画 id 列表 + drop 时
移除——改动面大、侵入 remember 机制，收益与方案 1 相同。

### 测试（T5/T6）

- **T5**：`test_infinite_transition_auto_dispose`——组合创建 InfiniteTransition →
  节点移除（结构变化）→ 断言 `ACTIVE_INFINITE_*` 表不含该 state_id、`update_animations()`
  返回 false（无空转）。
- **T6**：`test_infinite_transition_manual_dispose_idempotent`——显式 dispose 后
  on_remove 再次触发（模拟双重清理）→ 无 panic、表不变。

## 四、审计确认的"非缺陷"（防止重复修）

| 曾疑点 | 结论 | 证据 |
|--------|------|------|
| remember 状态泄漏 | 非缺陷——Slot 级联 drop | composer.rs:679 truncate → Slot drop |
| 协程泄漏（scope.spawn） | 非缺陷——Arc 归零 drop abort | effect.rs `impl Drop for ScopeState` |
| on_remove 滞后一帧 | 非缺陷——同帧渲染前触发 | materialize → compose 末尾回收 |
| 有限动画节点移除后继续跑 | 轻微浪费，**不修**（YAGNI）——完成自清，`state.set` 无依赖不触发重组 | animation.rs:239 `if a.update() { still.push }` |
| 动画 on_done 与结构移除竞态 | 非缺陷——on_done 写 state，无 slot 依赖无重组 | slot_deps 空 → 无 dirty |

## 五、实施清单

- [ ] 1. `remember_infinite_transition` 挂 on_remove 自动 dispose（方案 1）
- [ ] 2. T5：节点移除 → 无限动画表清空 + `update_animations()` false
- [ ] 3. T6：显式 dispose + on_remove 双重触发幂等
- [ ] 4. 全量测试 + examples 构建 + review + 提交
- [ ] 5. cleanup-plan.md 勾选 P3-2

## 六、不做（记录）

- **有限动画立即取消**（节点移除时）：Compose 语义是取消，本框架是"跑完自清"——
  差异仅在 300ms~2s 的推进空转，无逻辑错误。未来若做"动画状态绑定 remember
  生命周期"再一并处理。
- **副作用状态跨帧保留语义扩展**（rememberUpdatedState 等）：现有 remember 已满足
  当前 API 需求，按需再加。
