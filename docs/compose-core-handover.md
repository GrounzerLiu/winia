# compose-core 分支总结（合并交接文档）

> 分支：`compose-core`（基于 `text-field`，46 个提交）
> 目标：按 `docs/cleanup-plan.md` 完善基础 compose 架构（P0–P3）
> 状态：**全部完成**，含计划外修复（for 循环迭代 key 稳定性、UI 测试框架、多窗口 debug 树）

---

## 一、完成的工作

### P0 — 死代码清理（9 个提交 + 2 个保留决策）
- 删 7 个完全死导出、effect.rs/unit.rs/text/ 死 API、app.rs legacy、`ElementCategory`/`ModifierNode` 自定义扩展点、订阅机制降级、动画类型降 `pub(crate)`、`RegisteredSegment.bounds`、Rect 统一
- **保留**（用户指定，非死代码）：`ThemeColors::light_from_seed/dark_from_seed`、`WiniaTheme::light/dark`、`TextStyle` 全部 builder、`Text::oblique/style`

### P1 — DRY
- 段落构建双实现合并（测量期与绘制期共用 `build_plain_paragraph`）
- Rect 统一（全项目只剩 `skia_safe::Rect`）

### P2 — 架构机制统一
- **P2-1 两段式依赖**：`layout_deps` + `layout_dirty` 节点标记——布局动画写 `get()` 读动画值即生效，**消灭 `force_remeasure` 旁路**（handover §3.1 最痛缺陷）
- **P2-2 物化器拆分**：`core/materialize.rs`（desc 树 → arena 树独立成模块），composer.rs 从 3176 → 2865 行
- **P2-3 依赖注册收敛**：thread_local 双指针 → `DEP_BUFFER` 数据缓冲 + `DepMode`——**无 unsafe、无悬垂**；`take_deps()` 时序修复（在 `register_modifier_deps_recursive` 之后）
- P2-4/P2-5 样板合并（build_container/attach_cleanup）

### P3 — 长远项
- **P3-1 Skip 结构签名**：`CachedNode.children_count` 校验——结构增删（if 分支/列表项）后放弃恢复走 Enter 重建，防旧内容缓存张冠李戴
- **P3-2 副作用生命周期**：无限动画自动 dispose（`InfiniteTransition` 生命周期与组合点对齐）
- **P3-3 错误可恢复**：`catch_unwind` + 连续 30 次 panic 停更保留最后画面 + 自愈测试
- P3-4 rich_text `Seg` 内嵌 `Style`；P3-6 `Modifier::text_content` 统一入口
- P3-5/P3-7 决策记录（见下）

### 计划外（本分支额外完成）
- **for 循环迭代 key 稳定性修复**（`7c4ebfa` + review 修复 `020d876`）——见 §三
- **UI 测试框架**（`ffcb3da`）：`tests/ui_test.rs` + `tests/ui_fixtures/`（5 个独立 fixture exe）——真实窗口 + stdin/stdout 管道驱动，不依赖 examples
- **多窗口 debug 树**（`2ebd4b4`）：`HashMap<u64, String>` 按 window_id 存，一次查询返回所有窗口
- **存量坏测试修复**：event_flow/layout_snapshot/render_snapshot 适配新 API（`app_root!` 包裹、arena 索引、`remember`+holder 模式）

---

## 二、与计划文档不一致的地方（决策差异）

| 计划项 | 计划 | 实际 | 原因 |
|--------|------|------|------|
| P0-10/P0-11 | 删 ThemeColors light/dark、TextStyle builder | **保留** | 用户指定（Material 扩展点/API 完整性，清了难找回） |
| P2-6 | Dp/Sp/Px 宏化（三副本合一） | **不做** | 用户 2026-08 决策：YAGNI，宏化收益仅代码量 |
| P3-1 | 完整树内 diff | **最小侵入** | YAGNI——只做 `children_count` 结构签名校验；"数量相同内容不同（A→B）"保持恢复（Compose 语义，不修） |
| P3-5 | TextUnit vs Dimension 统一 | **保留不合并** | 语义层不同（TextUnit=文本字体缩放；Dimension=布局空间），合并引入无意义变体 |
| P3-7 | architecture.md 原则更新 | ✅ 已做 | 宏已存在、错误恢复已做 |
| （计划外） | — | UI 测试框架 | 用户要求"补多窗口集成测试"→ 演化为完整 UI 测试体系 |
| （计划外） | — | key 稳定性修复 | 滚动 fixture 暴露的框架 bug（见 §三） |

handover.md §3 三大缺陷的解决映射：
- §3.1 布局缓存与动画旁路耦合 → **P2-1**（layout_deps/layout_dirty，`force_remeasure` 已删）
- §3.2 Skip 恢复 key 脆弱 → **P2-3 + P3-1 + key 稳定性修复**（key 机制 + 结构签名 + 迭代位置稳定）
- §3.3 副作用与 Skip 协调 → **P3-2**（无限动画自动 dispose）

---

## 三、遇到的问题（调试故事，接手必读）

### 3.1 for 循环迭代 key 碰撞（最曲折，已修复）
**现象**：fixture_scroll 两次滚动后，最后一行（Line 29）的 Text content 丢失（mod 为空），行节点还在。

**根因链条**（探针逐层定位）：
1. `next_group_key` 的 key = hash(scope, stmt_id, seq)——for 循环 30 次迭代的**语句 id 相同**（编译期固定源码位置）→ 30 行 key 全同 → 槽树错乱
2. 第一次修复：加 `STMT_SEQ`（语句调用序号）——但**行 content 闭包内语句只在行 Enter 时执行**（行 Skip 时 content 不跑）→ 自身计数随执行历史漂移：首帧 text29 seq=30，滚动后前 29 行 Skip、第 30 行才 Enter → text29 首次执行 seq=1 → **key = hash(scope, 7, 1) = text0 的 key** → 槽树 truncate 重建 → 内容丢失
3. 第二次修复（纯继承栈顶 seq）：把 for 体语句的迭代计数也抹平（for 语句 id 只在循环开始前执行一次，seq=1 固定）→ 30 次迭代行 key 全同
4. **最终修复**：`seq = max(自身执行计数, 栈顶外层语句 seq)`——for 体语句每次迭代执行（self=迭代位置）；content 内语句继承外层行语句的迭代位置
5. review 发现**跨函数泄漏**：不同 `#[composable]` 语句 id 各自从 0 开始 → `STMT_SEQ` 按 `(scope_src, id)` 计数（`SCOPE_SRC_STACK` thread_local 镜像，三处同步）

**已知限制**：嵌套循环（`for i { for j { … } }`）内层语句取 `max(内层次数, 外层位置)`——内层迭代与外层位置可能混淆。**正解：`ctx.key(i, ...)`**（已泛型化为 `impl Hash`，对标 Compose `key()`）。

**调试方法**：`WINIA_SLOT_TRACE`（[slot]/[slot-trunc]）、`WINIA_STMT_TRACE`（[stmt] id/seq/self/outer）、`WINIA_MAT_PROBE`（[mat]/[mat-fb]/[collect]）、`WINIA_SKIP_TRACE`——全部 env gated + `cfg(debug_assertions)`。

### 3.2 UI 测试的坑（已固化为教训）
- **stderr 管道必须被读线程消费**——不读 64KB 填满会阻塞 demo 进程（表现为超时）
- **launch 需 taskkill 同名残留 + 全局串行锁**——并行测试的 taskkill 会互杀刚启动的进程
- **树 JSON 必须单行紧凑 + 完整转义**（`\r`/`\t`/`\b`/`\f`）——控制字符生成非法 JSON → serde_json 解析失败表现为超时
- **fixture 用 `[[bin]]` 而非 `[[test]] harness=false`**——后者被 cargo test 当测试执行（跑窗口循环）导致全量测试卡死
- **debug 注入事件只作用于主窗口**——消费需双路兜底（window_event + new_events，`consume_debug_events`）
- `s 300` 是**向上**滚（offset 减少）；向下滚用负值
- 测试创建 State 必须走 `ctx.remember`（直接 `State::new` 无 owner → notify 不推送 → 增量重组不触发）

### 3.3 存量测试适配（API 变化）
- compose 闭包必须包 `winia::app_root!`（新 key 机制要求）
- `children` 索引经 arena 取值（`arena_nodes()` + `layout_root_idx()`）
- `State::new` → `ctx.remember` + holder 模式（闭包外创建的 State 无 owner）

### 3.4 doctest
- winia-macros 的 doctest 引用 winia crate——**循环依赖无法编译** → `rust,ignore` 标记
- 片段式示例（裸 `.background(...)`）→ 补全为可编译（`# use`）或 `ignore`

---

## 四、遗留与远期（不在本分支范围）

- **嵌套循环内层 key**：`ctx.key(i, ...)` 已就绪，文档化即可（无内置循环内层自动 key）
- **vsync 高频渲染**：winit `RedrawRequested` 自动合并；渲染层节流/多窗口帧调度未做（曾有 `vsync` 研究分支）
- **组合树分离**（组合树/布局树彻底分离为远期设计目标）——本分支已做"desc 树 → 物化"中间形态
- `animation-improve` 分支（ShrinkPolicy/force_remeasure）**勿搬回主线**——机制已被 P2-1 取代

---

## 五、验证方式

```bash
cargo test --features debug-server        # 全量：175 lib + 5 UI + 8 layout + 9 render + doctest
cargo run -p winia --example counter      # 主窗口 + 子窗口（多窗口树）
# UI 测试单独跑：
cargo test --test ui_test --features debug-server -- --nocapture
# 调试通道（demo 需 --features debug-server 启动）：
#   WINIA_SLOT_TRACE / WINIA_STMT_TRACE / WINIA_MAT_PROBE / WINIA_SKIP_TRACE
```

测试基线：**175 passed**（lib）+ 5 UI 集成 + 快照测试全绿；examples 0 error；工作树 LF（CRLF 0）。
