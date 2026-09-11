# Winia Key 系统设计（定稿）

> 分支：`key-stability-research`
> 状态：**已实施**（396 个单元测试全绿 + 29 个 demo 实测通过）
> 设计演变：初稿主张"编译期替换 remember/next_key/animate 为编号版本"——实施中发现 per-base 独立计数已解决漂移根因，**砍掉替换机制，只保留语句注入**（定稿，见 §9 演变记录）；后续补 seq 链哈希（修滚动卡死）与 overlay active 参数化（修闪烁/无法关闭）。

---

## 1. 背景与问题

### 1.1 key 的作用

key 是组合系统**跨帧身份**——组合、物化、布局三阶段靠 key 找回对象：

| 阶段 | 按 key 找回 | 位置 |
|---|---|---|
| 组合 | `start_slot(key)` 匹配槽位（Enter/Skip） | composer.rs |
| 组合 | `remembered.get(key)` 找回 State | composer.rs |
| 物化 | `prev_node_by_key` 恢复/复用节点 | materialize.rs |
| 物化 | `prev_nodes.get(key)` 恢复测量缓存 | materialize.rs |
| 布局 | `find_node_id_by_slot_key` 恢复焦点 | node.rs |
| dirty 传播 | `mark_dirty_path_scope(key)` | composer.rs |

**key 不稳定 = 三阶段全部错位**。

### 1.2 漂移根因与解决

**旧机制**：remember/next_key 的语句内序号由运行时计数（`remember_path_counters` 按 base 计数、每帧清零）——条件分支（`if show_placeholder`）插入/移除 remember → 平移后续**同 base** 的序号 → key 漂移 → 复用错误对象（placeholder 复用空输入节点的 `[0,0]` 测量缓存 → 渲染不可见）。

**解决（两个独立机制叠加）**：

1. **语句 id 编译期固定**（宏注入 `enter_stmt(id)`）→ base = fnv(scope, 语句id, 迭代seq) 编译期确定——不同语句的 remember **base 不同**，序号按 base **独立计数**（`path_counters`/`remember_path_counters` per-base）——插入/移除一个 remember 只影响**同语句**内的后续 remember，跨语句不漂移。
2. **同语句内**多 remember 数量变化 → 序号平移 → key 变 → **状态重置**（Compose 同语义：可预测、不冲突）。

### 1.3 实战教训（text-field-v2）

- placeholder `show_placeholder` 切换 → remember 序号平移 → alpha State 每帧新建 → 动画每帧从 0 重启 → 持续重组
- 修复路径：`ctx.key(role)` 显式固定 base（槽位）+ 组件 build 宏化（`#[composable]`——内部语句注入）
- 结论：**跨语句的序号平移是 bug（已由 per-base 计数根除）；同语句内的平移是 Compose 语义（重置）**。

---

## 2. 核心原则

1. **一切 key 派生自"调用点 key"**——组件实例不自己生成 key，由调用它的位置决定（Compose 同语义）
2. **调用点 base 编译期固定**：`fnv(scope哈希, 语句id, 迭代seq)`——宏注入语句 id；迭代 seq = **slot 树完整 child_counters 链哈希**（位置而非执行次数——start_slot 每帧无条件执行，滚动时 Skip/Enter 交替不漂移）
3. **语句内序号按 base 独立分配**（per-base counter）——跨语句互不漂移；同语句多实例由 per-base 序号 + 迭代 seq 共同区分
4. **无稳定 key 源即 panic**——不允许静默降级
5. **列表/动态结构**：用户 `ctx.key()` 显式兜底（Compose 语义）

---

## 3. key 的构成

```
最终 key = mix_key(base, per-base序号)     = fnv(base, 序号) 全 64 位混合
base     = 调用点链哈希（编译期固定）    ← 身份来源
序号     = 该 base 下第 N 次调用（运行时，per-base 独立计数） ← 同语句多实例区分
```

⚠ 不能用拼接方案：`(base << 32) | 序号` 丢弃 base 高 32 位、`base 高 32 位 | 序号` 丢弃低 32 位——身份空间只剩 2^32（生日悖论 ~4300 调用点后 50% 碰撞概率，checkbox 循环子项即碰撞）。`mix_key = fnv(base, 序号)` 全 64 位混合，不丢熵。

**迭代 seq（for 循环多实例）**：`enter_stmt` 的 seq 分量 = slot 树完整 `child_counters` 链的 fnv 哈希（`sibling_position`）。链含父层 index——content 闭包内语句不同行实例哈希不同 → 行间不冲突。位置跨帧稳定（start_slot 每帧无条件执行），滚动时部分行 Skip/Enter 交替不漂移（替代旧执行计数——计数每 compose 清空 + Skip 帧不执行 → 漂移 → dup-key，animation_demo 滚动卡死根因）。

**base 的三个来源（优先级）**：

| 来源 | 值 | 稳定 |
|---|---|---|
| `ctx.key(id)` 显式作用域 | fnv(id, 调用链) | ✅ 用户保证 |
| 宏注入语句（`enter_stmt`） | fnv(scope, 语句id, 迭代seq) | ✅ 编译期固定 |
| 无 → **panic** | — | fail-fast |

**实例隔离的关键**：base 用**调用链**（STMT_STACK 栈顶 = 调用点语句）——组件方法宏化后，16 个字段的**组件内语句 id/scope 相同**，只有**父调用点**（content 闭包内独立语句 id）不同——`start_scope_callchain` 的 key = try_stable_base()（父调用点链）→ 多实例隔离。

---

## 4. 宏设计（只注入，不替换）

### 4.1 宏做什么

**`#[composable]` 展开（全部注入，无替换）**：

```rust
// 源码
#[composable]
fn my_ui(ctx: &mut ComposeCtx) {
    let a = ctx.remember(|| 0);
    let b = ctx.animate_float_as_state(1.0, spec);
    Column::new().build(ctx, |ctx| { Text::new("x").build(ctx); });
}

// 展开
fn my_ui(ctx: &mut ComposeCtx) {
    let __composable_scope = ctx.start_scope_callchain(fn_hash);  // RAII scope
    let a = { let __stmt_guard = ctx.enter_stmt(0u32); ctx.remember(|| 0) };
    let b = { let __stmt_guard = ctx.enter_stmt(1u32); ctx.animate_float_as_state(1.0, spec) };
    {
        let __stmt_guard = ctx.enter_stmt(2u32);
        Column::new().build(ctx, |ctx| {
            { let __stmt_guard = ctx.enter_stmt(3u32); Text::new("x").build(ctx); }  // content 闭包内也注入
        });
    }
}
```

- **`ctx.remember` / `ctx.next_key` / `ctx.animate_*_as_state` 原样保留**——不替换、无 `_at` 变体（定稿：见 §9）
- 语句 id（0/1/2/3...）编译期按源码顺序固定——结构变化不漂移
- 迭代 seq：`enter_stmt` 用 **slot 树完整 child_counters 链哈希**（`sibling_position`）——start_slot 每帧无条件执行（Skip 帧也执行）→ 位置跨帧稳定，**不随执行次数漂移**（替代旧执行计数——滚动时 Skip/Enter 交替导致执行计数漂移 → seq≠迭代位置 → key 漂移 → dup-key panic，animation_demo 滚动卡死根因）。链含父层 index：content 闭包内语句不同行实例哈希不同 → 行间不冲突

### 4.2 两种注入模式

**模式 A：`#[composable]`（全量注入，默认）**

- RAII scope（`start_scope_callchain`，Drop 自动 end_scope——支持返回值/提前 return）
- 每条**含 ctx 调用**的语句注入 `enter_stmt(id)`（智能注入——纯计算语句零 guard）
- 递归注入 content 闭包体（参数名 == ctx_ident）
- 适合：复杂 UI 函数、组件 build 方法

**模式 B：`#[composable_keyed]` + `keyed_stmt!`（轻量）**

```rust
#[composable_keyed]                  // 只注入 RAII scope，不注入语句 id
fn light_ui(ctx: &mut ComposeCtx) {
    let x = ctx.remember(|| 0);      // scope 提供稳定 base
    keyed_stmt!({                    // 标记语句 → 注入 enter_stmt（含块内递归）
        Text::new("x").build(ctx);
    });
}
```

- `keyed_stmt!` 展开时**对块内语句递归注入**（attribute 宏看不到 function-like 宏内容——keyed_stmt 自己注入），块内并列组件/嵌套 content 闭包都获得语句 id
- 语句 id 从**调用点位置哈希 + 1** 起编号（offset）——不同 keyed_stmt 调用点不串位，且与 keyed_stmt 自身 guard 的 id（=位置哈希）不重复
- 未标记的组件调用内部 next_key 无稳定源 → 运行期 panic（fail-fast）

### 4.3 指定组合参数名

`#[composable(c)]` / `#[composable_keyed(c)]` 指定组合参数标识符（默认 `ctx`）：

```rust
#[composable(c)]
fn my_ui(c: &mut ComposeCtx) {
    c.remember(|| 0);
    Column::new().build(c, |c| { ... });   // content 闭包参数名必须跟随 c
}
```

- content 闭包识别 = 闭包参数名 == 宏的 ctx_ident（显式契约，不猜测）——`|c|` 注入，`|ctx|` 不注入
- `keyed_stmt!` 支持前缀：`keyed_stmt!(c; { ... })`（纯块默认 `ctx`）

### 4.4 RAII scope（支持返回值）

- `start_scope_callchain(fallback)`：key = try_stable_base()（调用链）或 fallback（调用链空——测试/组合顶层）
- `ScopeGuard` Drop → end_scope——支持返回值函数与提前 return（修复显式 end_scope 泄漏隐患）
- fn_hash = fnv(签名@源码位置)——签名相同的方法（不同结构体 build）靠源码位置区分

### 4.5 智能注入（减少展开膨胀）

**原则：只对"含 ctx 组合调用"的语句注入 `enter_stmt`**——纯计算语句零 guard、零展开：

```rust
// 注入（含 ctx）——enter_stmt(id)
{ let __stmt_guard = ctx.enter_stmt(3); TextField::new(...).build(ctx); }
{ let __stmt_guard = ctx.enter_stmt(9); let a = ctx.remember(|| 0); }

// 不注入（纯计算）——零 guard
let w = width - 32.0;
let label = format!("{} items", count);
```

- `stmt_uses_ctx` 递归判定（方法 receiver/调用参数/content 闭包/控制流体）
- 判定偏保守，方向安全：**漏判方向是 panic（安全），多注入方向无害（性能）**

---

## 5. 稳定 key 的判定

```rust
pub(crate) fn try_stable_base(&self) -> Option<u64> {
    if let Some(&k) = self.key_override_stack.last() {
        // ctx.key(id)：显式 id 混合调用链（**不含迭代 seq**——列表重排时
        // 同 id 位置变但 base 不变，兜底稳定；16 字段同 role 靠 scope_src/sid
        // 隔离，多实例靠 per-base counter 区分）
        Some(fnv(k, chain_hash_no_seq().unwrap_or(0)))
    } else {
        chain_hash()   // STMT_STACK 栈顶（宏注入语句）→ fnv(scope, 语句id, 迭代seq)
    }
}
```

- **稳定 = 当前调用点位于编译期已知的 key 链上**（宏注入语句 / ctx.key 作用域）
- 无稳定源 → `panic_no_stable_key`（fail-fast，带可操作信息）
- 测试路径（cfg!(test)）：退化为路径哈希（测试自控结构，漂移由测试负责）

---

## 6. 三层兜底

| 层 | 判定 | 结果 |
|---|---|---|
| 编译期（宏） | 语句注入 + `#[composable(c)]` 参数名校验 | 注入即稳定；参数名不匹配 → 编译 panic |
| 运行期（composer） | `try_stable_base` | 有稳定源 = 继续；无 = **panic**（带修复选项） |
| 物化期（defense） | `collect_node_keys` dup-key 检测 | 同 slot_key 两节点 = **panic**（带节点位置/尺寸） |
| 组合尾（invariant repair，`compose()`） | `prune_stale_child_links`（`core/materialize.rs`） | **Slot identity rule**: a node must be reachable from `arena.root` or from `transition_layer` — exactly once. A detached flight ghost is a legitimate root, so the layer seeds the walk. Listings owned by an unreachable parent are stale and are cleared; a repeated child index inside a reachable parent collapses to one. It runs for EVERY composer on EVERY compose — the call sits in `compose()` right after `retain_shared_sources`, not inside a hook that can return early — and BEFORE the prev drain, so a stale listing's node is still reclaimed. `parent_id` is deliberately NOT consulted: a measured case had it naming a node that no longer existed. |

---

## 7. 组件宏化清单（现状）——什么需要宏化

### 7.1 判定标准（两个条件都要满足）

**宏化 = 组件成为"自包含组合单元"**。前提：

1. **内部用 `remember`/`next_key`/`animate_*`**——否则宏化没意义（白加 scope/注入开销）
2. **内部 State 的失效不需要父容器感知**——宏化引入内部 scope 成为依赖注册目标，若内部 `State.get()` 的失效需冒泡给父（父重跑），内部 scope 会拦截 → 父 Skip → 逻辑断

**判定实操**：看内部 `State.get()` 注册的依赖"谁需要知道"——只有自己 → 宏化；需要父级/内容闭包 → 不宏化。

### 7.2 现状清单（实测验证）

| 类别 | 组件 | 状态 |
|---|---|---|
| 自包含叶子/交互组件 | Text/Button/Card/Checkbox/TriStateCheckbox/`checkbox_impl`/Icon/IconButton/IconToggleButton/Image/RichText/SelectionContainer/Switch/TextField/Window `build` | ✅ `#[composable]` |
| effect | `LaunchedEffect::build`/`DisposableEffect::build`/`remember_coroutine_scope`/`observe_watch`/`attach_cleanup` | ✅ `#[composable]` |
| overlay | Popup/Dialog/DropdownMenu `build` | ✅ `#[composable]` |
| **布局容器** | **Column/Row/Stack** | ❌ **不宏化**——content 闭包顶层 `State.get()`（如 ps 计算）必须注册到容器自身（子项变化 → 容器 Enter → 内容重跑）；宏化引入内部 scope 拦截 → 父 Skip → 联动断（checkbox_demo 全选不联动即此） |
| **动画容器** | **AnimatedVisibility/Crossfade/AnimatedContent/AnimatedSize** | ❌ **不宏化**——内部 `progress.get()` 每帧检测（淡出完成/removed）需父容器每帧重跑；宏化 → 父 Skip → 检测冻结 → 内容残留/动画断 |

**经验教训**：容器类组件（有 content 闭包、内部状态依赖需冒泡给父）**不能宏化**；叶子/自包含组件（内部状态只自己消费）**宏化收益大**。已宏化的 16 组件 + effect + overlay 均满足两条件。

### 7.3 overlay 生命周期（Popup/Dialog 参数化）

**Popup/Dialog 必须 `new(visible)` 参数化**（对齐 `DropdownMenu::new(expanded)`）——build **总执行**并调用 `ctx.record_overlay_active(id, visible)` 记录 active 状态：

- **主动关闭**（`visible=false`）→ sync_overlays 按 active=false **删除**（先触发 on_dismiss）
- **注册方 Skip**（主树无变化帧，build 未执行 → 本帧无记录）→ **保留**

**为什么不能 `if visible { Popup::new()...build() }` 包裹**：build 不执行时，主树 Skip 帧与主动关闭在 slot 层**无法区分**——旧实现用 recomposed+alive 推断关闭，Skip 帧被误判为关闭 → 删掉 → 下帧重建 → **闪烁**（overlay_demo Popup 闪烁根因）。组合期显式记录 active 是唯一正解。

---

## 8. 与 Compose 对标

| 维度 | Compose | Winia（定稿） |
|---|---|---|
| 调用点 key | 编译器生成（源码位置哈希） | 宏注入语句 id（fnv 调用链） |
| 语句内序号 | 编译器组栈 | per-base 运行时计数（独立不漂移） |
| 迭代 seq（循环多实例） | 编译器组栈位置 | **slot 树 child_counters 链哈希**（位置，跨帧稳定） |
| 列表实例 | `key(index)` 用户显式 | `ctx.key()` 用户显式 |
| 状态重置（结构变化） | 是（可预测） | 是（可预测） |
| 未覆盖场景 | 编译期 | 运行期 panic（fail-fast） |

---

## 9. 设计演变记录（2026-08-12）

**初稿主张**（`docs/key-system-design.md` 旧版）：remember/next_key/animate 宏替换为编译期编号版本（`remember_at(FN_HASH, SID, SEQ)` 三参 + animate `_at` 变体 7 个）——"运行时零序号分配"。

**实施结论**：替换机制**砍掉**——per-base 独立计数已根除跨语句漂移（§1.2），同语句内序号变化是 Compose 语义（重置）。替换机制引入 fn_hash/sid/seq 三参、7 个 animate `_at` 变体、宏替换器（约 300 行）——复杂度高、收益有限。

**定稿**：

- 宏只做**语句注入**（enter_stmt + RAII scope）——概念最简（"调用点位置 = key 身份"）
- `remember`/`next_key`/`animate_*_as_state` **原样保留**——无 `_at` 变体、无 fn_hash/sid/seq
- 运行时 `try_stable_base` + per-base 序号是唯一 key 来源
- 新增：`#[composable(c)]` 参数名指定、`keyed_stmt!(c; ...)` 前缀、keyed_stmt 块内递归注入
- 收益：宏代码量约减半（-300 行替换器），API 面不变（用户代码零迁移），风险更低

**演进保留项**（对比初稿）：

| 演进 | 说明 |
|---|---|
| scope key = 调用链哈希 | `start_scope_callchain`——多实例隔离落实到 scope 层（初稿用签名哈希） |
| fn_hash = 签名@源码位置 | 区分不同结构体同签名 build 方法 |
| `ctx.key()` 混合调用链 | 同 id 跨调用点（16 字段同 role Label）不碰撞 |
| `compose!` 宏 | 测试/嵌套场景根闭包（line:column 哈希） |
| dup-key 物化 panic | debug 告警升级为 hard panic（fail-fast） |

**2026-08-13 补充（滚动卡死 + overlay 修复后的演进）**：

| 演进 | 说明 |
|---|---|
| **迭代 seq = slot 树链哈希** | `enter_stmt` 的 seq 从"执行计数"（STMT_SEQ，每 compose 清空 + Skip 帧不执行 → 滚动漂移 → dup-key）改为**完整 child_counters 链的 fnv**（`sibling_position`）——start_slot 每帧无条件执行 → 位置跨帧稳定。删除 STMT_SEQ/SCOPE_SRC_STACK。修 animation_demo 滚动卡死（WS 复现 60 滚动 → 0 dup-key） |
| **mix_key 全 64 位混合** | key = `fnv(base, 序号)`（`mix_key`）——拼接方案（`<< 32` 或 `高 32 位 |`）都把 base 截到 32 位身份空间（2^32 碰撞概率高），混合不丢熵（review 发现，`acf3e6a`） |
| **overlay active 参数化** | Popup/Dialog 改 `new(visible)`（对齐 DropdownMenu::new(expanded)）——build 总执行 + `record_overlay_active` 组合期记录 active；sync 按 active=false 删除（主动关闭），无记录保留（Skip 帧）。修 overlay 闪烁（retain 误删）+ Dialog 无法关闭 |

---

## 10. 参考

- 现有实现：`try_stable_base`/`panic_no_stable_key`（composer.rs）、`enter_stmt`/`start_scope_callchain`（composer.rs）、`collect_node_keys`（materialize.rs）
- 宏实现：winia-macros `#[composable]`/`#[composable_keyed]`/`keyed_stmt!`/`compose!`/`app_root!`
- 实战教训记录：`docs/developer-guide.md` 5.3 稳定 Key
