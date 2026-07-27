# [P1] State 裸指针解耦与全局队列重构

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 进度






| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. focus_next() 裸指针 → node.id 收集，消除 3 处 unsafe + 4 处裸指针 | ✅ 完成 | 消除了 node.rs 中 3 处 unsafe + 4 处裸指针。focus_next 改为收集 node.id (u64)，set_focus_by_ptr 替换为 set_focus_by_id，find_by_focus_id_immut 替换为 find_node_id_by_focus_requester 返回 Option<u64>。collect_focusable 改为 collect_focusable_ids 收集 id。 |
| 2. 删除 CURRENT_COMPOSER 死代码（set/clear/static，约 15 行） | ✅ 完成 | 删除了 CURRENT_COMPOSER thread_local（state.rs 14-27 行）+ set_current_composer/clear_current_composer 函数 + RefCell 导入。同时删除 composer.rs 中的 import、ComposeCtx::new 中的裸指针存储、以及空的 Drop impl。净削减约 18 行。 |
| 3. PENDING_STATES 改为 per-Composer 队列，消除全局状态泄漏 | ✅ 完成 | PENDING_STATES (全局 Mutex<Vec<u32>>) + GLOBAL_DIRTY (AtomicBool) 替换为 per-Composer 队列。

state.rs: 新增 COMPOSER_REGISTRY (LazyLock<Mutex<Vec<Weak<...>>>>) + register_composer_queue() + 改进的 notify_state_changed()。删除 take_pending_states/has_pending_states/take_global_dirty/set_global_dirty。

composer.rs: 新增 pending_states: Arc<Mutex<Vec<u32>>> 字段、has_pending_states() 公开方法。Composer::new 中创建队列并注册到全局表。recompose() 改为消费自身队列。

app.rs: 改用 self.composer.has_pending_states()。

多窗口安全：每个 Composer 独立队列，State 变化通过全局注册表通知所有活动 Composer，Composer drop 后 Weak 引用自动清理。 |
| 4. 运行测试验证，确保 32 测试通过 | ✅ 完成 | 32 tests passed, 0 failed. 无新增编译警告。0 unsafe + 0 裸指针残留。 |

- **进度类型**：sequential（顺序）
- **完成进度**：4/4（当前：4/4）

## 实施思路

### 第 1 步：focus_next 裸指针 → node.id

**现状**：`focus_next()` 在 `node.rs:382-394` 通过 `*const LayoutNode` 收集可聚焦节点，再用 `unsafe { (**p).focused }` 读取。
**问题**：收集裸指针后若树结构变化 → 悬垂 UB。
**方案**：改为收集 `node.id: u64`（NEXT_NODE_ID 分配），用 `find_by_id()` 查找。删除 `set_focus_by_ptr`、`find_by_focus_id_immut` 等辅助函数。

### 第 2 步：删除 CURRENT_COMPOSER 死代码

**现状**：`state.rs` 中 `CURRENT_COMPOSER: RefCell<Option<*const ()>>` 只在 set/clear 中使用，从未被读取。
**方案**：删除 static + `set_current_composer`/`clear_current_composer` 函数，清理 `composer.rs` 中的 import 和调用。

### 第 3 步：PENDING_STATES 改为 per-Composer

**现状**：`static PENDING_STATES: Mutex<Vec<u32>>` 全局共享，多窗口会互相窃取通知。
**方案**：每个 Composer 持有 `Arc<Mutex<Vec<u32>>>` 作为自己的通知队列。新增全局 `STATE_OBSERVERS: LazyLock<Mutex<Vec<Weak<Mutex<Vec<u32>>>>>>` 注册表。State 变化时通知所有活动 Composer。`GLOBAL_DIRTY` 随之消除。
