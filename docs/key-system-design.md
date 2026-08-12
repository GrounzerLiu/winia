# Winia Key 系统重新设计

> 分支：`key-stability-research`
> 状态：设计草案（未实施）
> 目标：**所有 key 编译期确定，运行时零序号分配，无法获得稳定 key 即 panic**

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

### 1.2 现状的问题

```rust
base = fnv(scope_src, 语句id, 迭代seq)   // 宏注入（编译期）
key  = (base << 32) | 帧内序号(c)         // 运行时累计（漂移源）
```

- `c`（`path_counters`/`remember_path_counters` 按 base 计数、每帧清零）是**运行时序号**
- 条件分支（`if show_placeholder`）插入/移除 remember → 平移后续 `c` → key 漂移
- 漂移可能**撞上别的槽位/缓存** → 复用错误对象（如 placeholder 复用空输入节点的 `[0,0]` 测量缓存 → 渲染不可见）
- 结构变化时 key 漂移本身可接受（Compose 同语义：状态重置），但**漂移造成的冲突是 bug**

### 1.3 实战教训（text-field-v2）

- placeholder `show_placeholder` 切换 → remember 序号平移 → alpha State 每帧新建 → 动画每帧从 0 重启 → 持续重组
- 修复：`ctx.key(role)` 显式固定 base（槽位）+ remember 包进 ctx.key
- 结论：**运行时序号不可靠，必须编译期确定**

---

## 2. 核心原则

1. **一切 key 派生自"调用点 key"**——组件实例不自己生成 key，由调用它的位置决定（Compose 同语义）
2. **key 三项成分全部编译期固定**：`fnv(scope哈希, 语句id, 宏扫描序号)`
3. **运行时不再分配身份序号**——结构变化 = key 变 = 状态重置（不冲突）
4. **无稳定 key 源即 panic**——不允许静默降级
5. **列表/动态结构**：用户 `ctx.key()` 显式兜底（Compose 语义）

---

## 3. key 的三维身份

| 维度 | 来源 | 编译期 |
|---|---|---|
| 调用点身份 | 宏注入（scope 哈希 + 语句 id） | ✅ |
| 语句内序号 | 宏扫描编号（remember/next_key 出现顺序） | ✅ |
| 实例身份（列表） | 迭代位置 seq + 用户 `ctx.key()` | ✅ |

**实例隔离的关键**：base 用**完整调用链**（STMT_STACK 整个栈的哈希）而非栈顶——

- 组件方法宏化后，16 个字段的**组件内语句 id/scope 相同**
- 只有**父调用点**（content 闭包内独立语句 id）不同
- `base = fnv(完整调用链)` 才能实例隔离（已验证：9 个带 placeholder 字段的 alpha State id 各不相同）

---

## 4. 编译期 key 注入（宏设计）

### 4.1 key 生成

```rust
// 调用点 key（编译期）
调用点 key = fnv(父scope哈希, 语句id, 迭代seq)      // enter_stmt 提供
// 一切内部 key（编译期）
remember key = fnv(调用链, remember 宏序号)          // 宏扫描编号
节点 key     = fnv(调用链, next_key 宏序号)          // 宏扫描编号
// 组件实例 key
组件实例 key = 调用点 key                            // build 第一条 next_key
```

### 4.1.1 需要稳定 key 的 API 全集（宏扫描目标）

**A. remember 类**（内部 `next_remember_key`——宏替换为显式 key 版本）：

| API | 宏扫描后 |
|---|---|
| `ctx.remember(init)` | `remember_at_key(fnv, init)`（**运行时 API 已存在**） |
| `ctx.animate_float_as_state` | `animate_float_at_state(fnv, ...)`（新增 `_at` 变体） |
| `ctx.animate_color_as_state` | ↑ |
| `ctx.animate_dp_as_state` | ↑ |
| `ctx.animate_offset_as_state` | ↑ |
| `ctx.animate_size_as_state` | ↑ |
| `ctx.animate_int_as_state` | ↑ |
| `ctx.animate_value_as_state` | ↑ |

**B. 节点/scope 类**：

| API | 说明 |
|---|---|
| `ctx.next_key()` | 节点 key（组件容器组）→ `next_key_at(fnv)`（新增） |
| `ctx.start_scope()` / `start_scope_keyed(hash)` | scope key——已显式传 hash ✓ |
| `start_leaf(key)` / `start_container(key)` / `start_restartable_group(key)` | 已显式 key 参数（调用方 next_key 提供）✓ |

**C. 不需要 key 的（确认排除）**：

- `ctx.changed(param)`——值比较（Skip 参数判定）——不生成 key
- `set_current_node_*` / `sync_*` / `selection_registrar`——写入当前节点——不生成 key
- `open_overlay` / `composer_slot_key`——非身份 key

**调用形态**：以上需要替换的 API 全部是 receiver=ctx 的 MethodCall（`ctx.remember(...)`、`ctx.animate_float_as_state(...)`、`ctx.next_key()`）——`stmt_uses_ctx` 自动识别语句注入 ✓——宏替换调用本身为编译期编号版本。动画 State 的跨帧稳定（动画推进 → 重组 → build 重跑 → 再次 animate_*_as_state 拿到同一 State）依赖此替换——text-field-v2 的 placeholder alpha 动画每帧从 0 重启即 remember key 漂移所致。

### 4.2 宏扫描替换（#[composable] 升级）

宏扫描函数体 AST，把运行时调用替换为编译期编号版本：

```rust
// 源码
#[composable]
fn my_ui(ctx: &mut ComposeCtx) {
    let a = ctx.remember(|| 0);
    let b = ctx.remember(|| 1);
    let k = ctx.next_key();
}

// 展开
fn my_ui(ctx: &mut ComposeCtx) {
    let __scope_guard = ctx.start_scope_guarded(fnv(签名));  // RAII（见 4.4）
    let a = ctx.remember_at(fnv(scope, 语句0, rem#0), || 0);
    let b = ctx.remember_at(fnv(scope, 语句0, rem#1), || 1);
    let k = ctx.next_key_at(fnv(scope, 语句0, key#0));
    // __scope_guard drop → end_scope
}
```

- 序号（rem#0、key#0）由宏展开时按出现顺序编号——**编译期固定**
- 结构变化（插入 remember）→ 后续序号变 → key 变 → **状态重置但不冲突**

### 4.3 两种注入模式

**模式 A：`#[composable]`（全量注入，现有升级）**

- 函数级 scope key + 每条语句 `enter_stmt(id)`
- 新增：remember/next_key 扫描替换
- 适合：复杂 UI 函数

**模式 B：`#[composable_keyed]` + `keyed_stmt!`（单语句注入，新）**

```rust
#[composable_keyed]                  // 轻量：不注入语句 id
fn light_ui(ctx: &mut ComposeCtx) {
    let x = ctx.remember(|| 0);      // remember/next_key 仍编译期编号（宏扫描）
    keyed_stmt!({                    // 标记语句 → 注入 enter_stmt
        Text::new("x").build(ctx);
    });
}
```

- 技术限制：**Rust 语句级 attribute 宏不可行**（attribute 宏只能用于 item）——用 function-like 宏 `keyed_stmt!` 包裹
- attribute 宏先展开，能看见未展开的 `keyed_stmt!(...)` 节点并替换

**可指定 ctx 参数名**：`#[composable]` 支持可选参数 `#[composable(c)]`——指定组合参数标识符（默认 `ctx`）：

```rust
#[composable(c)]                     // 指定参数名 c
fn my_ui(c: &mut ComposeCtx) {
    c.remember(|| 0);
    Column::new().build(c, |c| { ... });   // content 闭包参数名也必须跟随 c
}
```

**content 闭包识别 = 闭包参数名 == 当前宏的 ctx_ident**（显式契约，不猜测）：

- `#[composable]` → content 闭包必须是 `|ctx|`；`#[composable(c)]` → 必须是 `|c|`
- 不匹配（`|x|`）或间接调用（`fn wrap(c) { ... }` 非宏函数内 build）→ 闭包体不注入 → 内部 next_key 无稳定源 → **运行期 panic**（fail-fast，带可操作信息）
- 放弃"按 build 方法识别闭包"的方案：依赖方法名约定、可能误判——ctx 识别是显式契约，用户知道要求，不满足即 panic

### 4.4 组件方法宏化 + RAII scope（支持返回值）

**组件 build 方法加 `#[composable]`**（attribute 宏可用于方法）：

```rust
impl TextField {
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // 宏注入：内部 next_key/remember 全部编译期编号
        // 实例隔离 = 调用链（父调用点语句 id 不同）
    }
}
```

**`start_scope` 改 RAII guard**（支持返回值 + 提前 return 安全）：

```rust
// 源码（有返回值）
#[composable]
fn pick_label(ctx: &mut ComposeCtx, flag: bool) -> String {
    let style = ctx.remember(|| TextStyle::default());
    if flag { "A".to_string() } else { "B".to_string() }
}

// 展开
fn pick_label(ctx: &mut ComposeCtx, flag: bool) -> String {
    let __scope_guard = ctx.start_scope_guarded(fnv(签名));  // Drop → end_scope
    let style = { let __g = ctx.enter_stmt(0); ctx.remember_at(...) };
    let __ret = { /* 分支注入，块值保留 */ if flag { ... } else { ... } };
    __ret                                  // 尾表达式返回值（guard 此时 drop）
}
```

- 现有宏限制"不支持返回值"（lib.rs:324）被解除
- 顺带修复：显式 `end_scope` 在提前 return 时泄漏 scope 的隐患

### 4.5 智能注入（减少注入数量/展开膨胀）

**原则：只对"含 ctx 组合调用"的语句注入 `enter_stmt`**——纯计算语句零 guard、零展开：

```rust
fn stmt_uses_ctx(stmt: &Stmt, ctx_ident: &Ident) -> bool {
    // 递归遍历语句 AST：
    //  - Expr::MethodCall: receiver 是 ctx_ident（ctx.remember(...)）✓
    //  - Expr::Call: 参数含 ctx_ident（foo(ctx)、build(ctx, ...)）✓
    //  - Expr::Closure: 参数名 == ctx_ident（content 闭包——体内递归检测）✓
    //  - 递归进 if/for/while/match/block 分支
}
```

```rust
// 注入（含 ctx）——enter_stmt(id)
{
    let __g = ctx.enter_stmt(3);
    TextField::new(...).build(ctx);
}
{
    let __g = ctx.enter_stmt(9);
    let a = ctx.remember(|| 0);      // let init 内含 ctx → 注入
}

// 不注入（纯计算）——零 guard、零展开
let w = width - 32.0;
let label = format!("{} items", count);
let cfg = Config::new();
```

**收益**：纯计算密集函数（数据准备）零注入；UI 密集函数全注入（语句都含 build/remember）——展开体积与运行时开销随"实际组合调用数"而非"语句数"。

**边界与风险**（判定偏保守，方向安全）：

| 情况 | 判定 | 结果 |
|---|---|---|
| `let c = ctx; c.remember(...)`（别名） | 漏判（`c.remember` 无 ctx_ident 名） | 无语句 id → 运行期 `try_stable_base` **panic 兜底**（不静默错位） |
| `foo(ctx)`（自定义函数内部调组合 API） | 参数含 ctx → 注入 ✓（保守） | 无害（多一个 guard） |
| `let g = || { ctx.remember(...) }` | 闭包体含 ctx → 注入 ✓（递归） | 无害 |
| 普通同名变量（非 ComposeCtx） | 名匹配 → 注入 | 无害（多一个 guard） |

**方向性**：**漏判方向是 panic（安全），多注入方向无害（性能）**——判定无需精确。

**与两种模式结合**：
- `#[composable]`：智能全量——scope + 含 ctx 语句注入 + remember/next_key 扫描替换（替换只发生在含 ctx 的语句，天然对齐）
- `#[composable_keyed]` + `keyed_stmt!`：显式标记为主——可加自动检测选项（`#[composable_keyed(auto)]`：含 ctx 语句自动注入）

### 4.6 编译期检查（宏层）

`#[composable_keyed]` 宏扫描函数体——找 `keyed_stmt!` 未覆盖的**含 ctx 调用语句**（语句 AST 中出现 ctx_ident 的方法调用/参数）——未标记且含 ctx → `syn::Error::compile_error!()`（编译期报错，早于运行期 panic）。纯计算语句（不含 ctx）不检查、不注入。

### 4.7 panic 语义（fail-fast）

无法获得稳定 key 的调用点 **panic** 是特性而非缺陷：

| 用户写法 | 结果 |
|---|---|
| 宏函数内（参数名匹配） | ✅ 编译期注入（稳定） |
| `#[composable(c)]` + content 闭包 `|c|` | ✅ 编译期注入 |
| `#[composable(c)]` + content 闭包 `|x|`（不匹配） | **运行期 panic**（无法获得稳定 key） |
| 间接调用（`fn wrap(c){...}` 非宏函数） | **运行期 panic** |
| 普通闭包（`list.map(|x|...)`——非组合内容） | 不注入 ✓ |

**panic 信息必须可操作**：指明调用点不在稳定 key 链上，给出修复选项（①放回宏函数内 ②用 `ctx.key()` 包裹 ③content 闭包参数名与 `#[composable(x)]` 一致）。fail-fast 优于静默漂移（text-field-v2 的 placeholder 0×0 即静默漂移后果）。

---

## 5. 稳定 key 的判定

### 5.1 判定规则

**稳定 = 当前调用点位于"编译期已知的 key 链"上**：

```rust
fn try_stable_base(&self) -> Option<u64> {
    // ① 显式 ctx.key(id, f) 包裹——用户保证——稳定
    if let Some(&k) = self.key_override_stack.last() { return Some(k); }

    // ② STMT_STACK 有宏注入的语句——编译期固定——稳定
    //    ⚠ 用**完整调用链**（整个栈哈希）而非栈顶——实例隔离
    STMT_STACK.with(|s| {
        let s = s.borrow();
        if s.is_empty() { None } else { Some(fnv_chain(&s)) }
    })
}
```

### 5.2 判定调用点

```rust
let base = match self.try_stable_base() {
    Some(b) => b,                    // 稳定：继续（+ 宏扫描的编译期序号）
    None => {
        #[cfg(test)] { ...路径哈希（测试自控）... }
        panic!("无法获得稳定 key：调用点不在 #[composable]/keyed_stmt! 注入内，也无 ctx.key() 包裹");
    }
};
```

---

## 6. panic 兜底（三层）

| 层 | 判定 | 结果 |
|---|---|---|
| 编译期（宏） | 扫描 remember/next_key/.build(ctx) | 扫描到 = 编译期编号；未标记的 `.build` = compile_error! |
| 运行期（composer） | `try_stable_base` | 有稳定源 = 继续；无 = **panic** |
| 物化期（defense） | `collect_node_keys` dup-key 检测 | 同 slot_key 两节点 = **panic**（现状 debug 告警，升级） |

---

## 7. 实施计划

1. **composer.rs**：`remember_at`/`next_key_at` 运行时 API（显式 key 版本）+ `try_stable_base` 判定 + `start_scope_guarded`（RAII）
2. **winia-macros**：`#[composable]` 升级——remember/next_key 扫描替换 + scope 改 RAII + 放开返回值
3. **winia-macros**：新 `#[composable_keyed]` + `keyed_stmt!`
4. **组件方法宏化**：TextField::build 等加 `#[composable]`（内部编译期编号）
5. **dup-key panic 升级**：物化期同 key 覆盖 → panic
6. 迁移：现有组件（Button/Card/Checkbox 等）方法宏化；examples/tests 适配
7. 测试：key 稳定性回归（跨帧/结构变化/列表增删/多实例隔离）

---

## 8. 与 Compose 对标

| 维度 | Compose | Winia（新设计） |
|---|---|---|
| 调用点 key | 编译器生成（源码位置哈希） | 宏注入（scope 哈希 + 语句 id） |
| 语句内序号 | 编译器组栈 | 宏扫描编号 |
| 列表实例 | `key(index)` 用户显式 | `ctx.key()` 用户显式 |
| 状态重置（结构变化） | 是（可预测） | 是（可预测） |
| 运行时序号 | 无 | **无**（消除） |
| 未覆盖场景 | 编译期（非 @Composable 内调用 @Composable） | 编译期检查 + 运行期 panic |

---

## 9. 收益 / 代价 / 已知限制

### 收益
- 运行时零 key 身份分配——"帧内序号漂移冲突"类 bug 根除（text-field-v2 持续重组/placeholder 0×0 即此类）
- 结构变化 = 状态重置（Compose 语义，可预测、不冲突）
- 未覆盖场景立即 panic（编译期或运行期），不静默错位——fail-fast 带可操作信息
- 编译期 key 可审计/缓存（同一源码产物固定）
- **智能注入**：展开体积/运行时开销随"实际组合调用数"而非"语句数"——纯计算函数零注入
- **单语句模式**（`#[composable_keyed]` + `keyed_stmt!`）：轻量函数按需标记，不强制全量注入
- **支持返回值函数**（RAII scope guard）：现有宏"不支持返回值"限制解除，顺带修复提前 return 的 scope 泄漏

### 代价
- 宏复杂度上升：扫描替换 remember/next_key + `stmt_uses_ctx` 判定 + 两种注入模式（`#[composable]` / `#[composable_keyed]` + `keyed_stmt!`）
- 组件方法宏化——展开膨胀、编译时间上升
- 所有组件 build 需加 `#[composable]`（迁移工作：Button/Card/Checkbox/TextField 等）
- content 闭包参数名成为显式契约（`#[composable(c)]` 要求 `|c|`）——用户需按规范写，否则 panic

### 已知限制
- 运行时动态调用 `ctx.remember`/`next_key`（闭包内间接调用、函数指针、别名 `let c = ctx`）无法被宏扫描——**运行期 panic 兜底**（fail-fast，不静默错位）
- `keyed_stmt!` 的 id 用扫描顺序——插入/删除标记语句平移后续 id（漂移 = 状态重置，不冲突）
- 嵌套循环内层语句的 seq 混淆（现有文档化限制）——需显式 `ctx.key()`
- content 闭包识别依赖参数名匹配——`#[composable]` 内写 `|x|` 的非 build 闭包不被注入（误写 → panic，符合 fail-fast 语义）
- 智能注入的别名漏判（`let c = ctx; c.remember(...)`）→ panic——建议规范写法避免

---

## 10. 参考

- 现有实现：`next_group_key`/`next_remember_key`（composer.rs:1085/:451）、宏注入（winia-macros `inject_stmt_ids`）、`ctx.key`（composer.rs:250）
- 实战教训记录：`docs/developer-guide.md` 5.3 稳定 Key
- 相关调研：`cargo +nightly expand` 实测 content 闭包注入、字段实例隔离
