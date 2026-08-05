# Winia 设计原则与清理路线（Cleanup Plan）

> 依据：通用软件设计原则（SRP/DRY/YAGNI/SSOT…）× 2026-08 全库审计结果（5 路并行子代理）。
> 配套文档：`handover.md`（架构现状/已知缺陷/调试）、`architecture.md`（设计稿，部分过时）。
> 分支：`compose-core`（基于 `text-field`）。测试基线：`cargo test --lib` 162 个。

---

## 一、通用设计原则 × 项目现状对照

| 原则 | 含义 | winia 现状 | 治理动作 |
|------|------|-----------|---------|
| **SRP** 单一职责 | 一个模块只做一件事 | ⚠️ `composer.rs` 2952 行：组合+物化+依赖+key+Skip 全扛；`app.rs` 1348 行：事件+焦点+选择+IME 混合 | P2-2 拆分物化器；长期拆分 app.rs |
| **OCP** 开闭原则 | 对扩展开放、对修改封闭 | ⚠️ 加布局动画要改 `measure_node` 缓存；加 ModifierElement 要改多处 match | P2-1 布局动画统一机制（消灭旁路） |
| **ISP** 接口隔离 | 不依赖用不到的接口 | ⚠️ `ModifierElement` 大枚举迫使处理方全 match | 观察，暂不动 |
| **DIP** 依赖倒置 | 依赖抽象不依赖实现 | ✅ 布局依赖 `MeasurePolicy` trait；❌ 依赖注册走 thread_local 裸指针 + 全局表 | P2-3 依赖注册收敛 |
| **DRY** 不重复自己 | 消除重复 | ❌ Dp/Sp/Px 三副本、Column/Row/Stack 样板镜像、effect 三样板、段落构建双实现、Rect 三套、rich_text Style/Seg 镜像 | P1 + P2-4/5/6 |
| **KISS** 保持简单 | 概念最少 | ⚠️ key 三层、State 写方法四兄弟、peek/get 双读法 | 文档化语义，不删 |
| **YAGNI** 不做用不上的 | 死代码即负债 | ❌ 60+ 零调用符号（Paragraph 27 个透传、StreamObverse、IntOffset/IntSize…） | **P0 全清** |
| **SoC** 关注点分离 | 不同关注点拆开 | ✅ 组合/布局/渲染三层分离；⚠️ 物化逻辑仍在 composer.rs | P2-2 |
| **SSOT** 单一事实来源 | 每份数据一个权威 | ⚠️ 段落构建两处、Rect 三套、Dp/Sp/Px 与 Dimension/TextUnit 重叠 | P1-1、P2-6、P3 |
| **组合优于继承** | 用组合扩展 | ✅ 框架层大量组合；⚠️ 依赖注册隐式 | 观察 |
| **最小惊讶** | 行为符合直觉 | ⚠️ `set_no_wake`/`set_silent`/`peek` 语义微妙、Skip×副作用违反直觉 | 文档化 |
| **快速失败** | 尽早暴露错误 | ⚠️ 渲染 panic 直接崩窗口；部分错误静默降级 | P3 错误恢复 |

---

## 二、审计发现的问题汇总

### A. 完全死导出（零调用，P0 删）

| 符号 | 位置 | 备注 |
|------|------|------|
| `Key` | core/composer.rs:94，lib.rs:61 | 全项目零使用（demo 用的是 winit 的 Key） |
| `current_layout_direction` | ui/theme.rs:199，prelude | 零调用 |
| `push_animation` | animation.rs:165 | 零调用（`push_animatable` 才是真入口） |
| `Transition` | animation.rs:484 | animation_demo.rs:10 是 unused import |
| `open_window_with_close` | app.rs:1097 | 零调用 |
| `cancel_close` | app.rs:1108 | 零调用 |
| `Rect` | ui/selection_container.rs:20，ui.rs:33 | 外部零使用 |

### B. 死 API（P0 处理）

- **effect.rs**：`StreamObverse`（全库零调用，与 observe_watch 重复）、`LaunchedEffect::unit`、`DisposableEffect::unit`
- **unit.rs**：`IntOffset`/`IntSize`（零使用）、`Offset::plus/minus/times`（与运算符重复）、`Density::to_dp/to_sp_px/to_sp`（font_scale 恒 1 永不生效）、Dp/Sp/Px/Offset/Size 约 20 个方法（`ZERO/UNSPECIFIED/new/round/floor/ceil/abs/min/max/coerce_in` 等）
- **text/paragraph.rs**：约 27 个 skia 透传方法零调用（`min_intrinsic_width`/`alphabetic_baseline`/`get_word_boundary`/`get_fonts`/`get_path_at`…）
- **text/text_layout.rs**：6 个方法零调用（`draw`/`width`/`height`/`base_line`/`get_rects_for_range`/`inner_paragraph`）
- **text/paragraph_builder.rs**：`peek_style`/`get_paragraph_style`/`reset` 零调用
- **text/inline_drawable.rs**：`ImageDrawable` 全库零使用（SvgDrawable 才活着）
- **app.rs legacy**：`open_window`/`open_window_with_title`/`close_window_by_id`（仅内部/无调用）
- **modifier.rs**：`ElementCategory`、`ModifierNode`（`Custom.inner` never read——半成品扩展点，**决策：删还是落地**）
- **state.rs**：`SubscriberId`/`Subscription`/`record_dep`/`register_dependency`（仅文件内自用，降 `pub(crate)` 或删）
- **animation.rs**：`Animatable`/`InfiniteTransition`/`RepeatableSpec`/`RepeatMode`（pub 无外部使用，降级或删——`Animatable` 被 push_animatable 内部用，保留但降 pub(crate)）
- **selection_container.rs**：`RegisteredSegment.bounds` 只写不读
- **theme.rs**：`ThemeColors::light/dark/light_from_seed/dark_from_seed`（仅内部自用——保留为扩展点还是删，决策）
- **text.rs**：`TextStyle` 8 个 builder 方法（`font_weight`/`font_style`/`oblique`/`overflow`/`max_lines`/`underline`/`strikethrough`/`background`）+ `Text::oblique/style`

### C. 重叠/重复（P1-P2 重构）

| # | 重复 | 位置 | 治理 |
|---|------|------|------|
| C1 | 段落构建双实现（测量期 vs 绘制期各一套 ParagraphBuilder + to_sktextstyle） | node.rs:940-1016 / render.rs:481-484 | P1 合并共用 |
| C2 | Rect 三套（selection_container::Rect 死 + render 内联 + skia） | — | P1 统一 |
| C3 | Column/Row/Stack build 样板镜像 | layout_components.rs:34-55/86-105/130-143 | P2-4 |
| C4 | effect.rs 三处 remember+started+start_leaf_with_remove 样板 | effect.rs:99-134/163-192/212-251 | P2-5 |
| C5 | Dp/Sp/Px 三副本（结构/方法/运算符逐字重复） | unit.rs | P2-6 宏化 |
| C6 | rich_text Style/Seg 字段镜像 + apply_seg 手抄 | rich_text.rs:24-49/409-428/566-590 | P3 |
| C7 | TextUnit vs modifier::Dimension 职责重叠 | unit.rs:399 / modifier.rs:41 | P3 |
| C8 | Text vs TextField 各自构造 TextContent（字段重复） | ui/text.rs:225-235 / text_field.rs:240-250 | P3 |

### D. 架构缺陷（来自 handover §3）

- D1 布局缓存与动画旁路耦合（`force_remeasure` 式旁路）→ **P2-1 两段式依赖**（layout_deps：布局期 get() 注册 → 只重测不重组）
- D2 Skip 恢复依赖路径 key，结构变化脆弱 → P3
- D3 副作用（动画/回调）与 Skip 无生命周期契约 → P3

---

## 三、Checklist（按优先级）

### P0 — 死代码清理（机械、低风险、先做）

- [ ] P0-1 删 7 个完全死导出（A 表全部）
- [ ] P0-2 清 effect.rs 死 API（StreamObverse/两个 unit()）
- [ ] P0-3 清 unit.rs 死 API（IntOffset/IntSize/Offset::plus/minus/times/Density::to_dp/to_sp_px/to_sp/约 20 个 Dp/Sp/Px/Offset/Size 方法）
- [ ] P0-4 清 text/ 死 API（paragraph.rs 27 透传 + text_layout.rs 6 + paragraph_builder.rs 3 + ImageDrawable）
- [ ] P0-5 清 app.rs legacy（open_window/open_window_with_title/close_window_by_id）
- [ ] P0-6 modifier.rs：ElementCategory/ModifierNode 决策（删或落地 Custom 扩展点）
- [ ] P0-7 state.rs：SubscriberId/Subscription/record_dep/register_dependency 降 pub(crate) 或删
- [ ] P0-8 animation.rs：Animatable/InfiniteTransition/RepeatableSpec/RepeatMode 降 pub(crate)
- [ ] P0-9 删 RegisteredSegment.bounds（只写不读）
- [ ] P0-10 theme.rs：light/dark/light_from_seed/dark_from_seed 决策（保留为公开扩展点则补文档）
- [ ] P0-11 清 TextStyle 死 builder + Text::oblique/style

**P0 验收**：`cargo test --lib` 162 全绿；`cargo check --examples` 全绿；`cargo build -p winia` 警告数不增加；git diff 无 CRLF 噪音；一次提交一个文件组，每个提交独立可回滚。

### P1 — DRY 重复合并（中风险，逐项验证）

- [ ] P1-1 段落构建双实现合并（node.rs 测量期与 render.rs 绘制期共用一套 ParagraphBuilder 流程 + to_sktextstyle）
- [ ] P1-2 Rect 统一（删 selection_container::Rect，render.rs 内联计算收敛到一处）

**P1 验收**：文本测量/绘制行为不变（text_demo/rich_text_demo/text_field_demo 截图对比）；测试全绿；`cargo check --examples` 干净。

### P2 — 架构机制统一（核心重构，先设计后实施）

- [ ] P2-1 **两段式依赖（布局动画统一机制）**——state.rs 记录分流（IN_LAYOUT）+ composer.rs `layout_deps` + node.rs `layout_dirty`；布局动画写 `get()` 读动画值即生效，消灭 force_remeasure 式旁路。验收：写一个布局动画测试（高度随动画 State 收缩，下方节点跟随，且组合不重跑——用组合计数断言）
- [ ] P2-2 物化器拆分（composer.rs 拆出 `materialize` 模块：desc 树 → arena 树独立成模块）——SRP
- [ ] P2-3 依赖注册收敛（slot_deps/layout_deps 统一管理；评估去掉 thread_local 裸指针桥接）——DIP/SSOT
- [ ] P2-4 Column/Row/Stack build 样板合并（宏或共享辅助函数）
- [ ] P2-5 effect.rs 三处样板合并
- [ ] P2-6 Dp/Sp/Px 宏化（三副本合一）

**P2 验收**：每项有对应测试（组合计数/依赖注册断言/布局动画行为）；13 个 demo 全回归；162+ 测试全绿。

### P3 — 长远（记录方向，不在本次范围）

- [ ] P3-1 Skip 语义 Compose 化（参数相等跳过为主导，dirty 为辅）——D2
- [ ] P3-2 副作用生命周期契约（动画/回调与 Skip 协调）——D3
- [ ] P3-3 错误可恢复（渲染崩溃边界）——原则 6
- [x] P3-4 rich_text Style/Seg 镜像合并（C6）——✅ 2026-08（Seg 内嵌 Style，apply_seg → Style::apply）
- [x] P3-5 TextUnit vs Dimension 统一（C7）——✅ 2026-08 决策：**保留不合并**——语义层不同（TextUnit=文本字体缩放 Sp/Px；Dimension=布局空间 Fixed/Dp/Px/Fill/Auto），合并会让双方出现无意义变体；仅"Px"概念共享属正确分层
- [x] P3-6 Text vs TextField TextContent 构造合并（C8）——✅ 2026-08（Modifier::text_content 统一入口）
- [x] P3-7 文档对齐：architecture.md 原则更新为现实（宏已存在、错误恢复未做）——✅ 2026-08（compose-core 分支）

### 保留清单（扩展点/基础设施，**勿删**）

- `CompositionLocal`（外部扩展主题/密度需要）
- `InlineDrawable` trait（外部自定义图片需要；`ImageDrawable` 实现可删，trait 保留）
- `SelectionRegistrar`（app.rs/render.rs 内部依赖）
- `Composer`/`Constraints`/`LayoutDirection`/`ThemeColors`（内部活跃，examples 零引用属正常）
- `StmtGuard`（`#[composable]` 宏展开必需）
- `DpExt/SpExt/PxExt`（`.dp()/.sp()/.px()` 被 demo 调用，f32/u32 变体保留）

---

## 四、执行顺序与提交策略

```
P0（一次一个文件组提交，每提交：cargo test + cargo check --examples）
  → P1（每项一个提交，带截图回归）
    → P2（每项独立分支或独立提交，先写设计说明再实施）
```

提交信息格式：`refactor(cleanup): <文件组> 死代码清理` / `refactor(dry): 段落构建合并` / `refactor(core): 两段式依赖`。
