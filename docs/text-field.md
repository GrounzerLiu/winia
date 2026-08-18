# TextField 组件

> 分支：`text-field-v2`（容器化重构 + 本组件迭代）
> 对标：Jetpack Compose `material3.TextField` / `OutlinedTextField`（底层 foundation `BasicTextField`）
> 状态：**可用**（输入/光标/选区/IME/撤销/视觉变换/错误态均可用），待完善项见 §5

## 1. API

```rust
// 创建（单参数——仅绑定文本状态；值变化回调默认忽略）
TextField::new(value: State<TextFieldValue>)      // 对标 BasicTextField(value, onValueChange, ...)
    .on_value_change(|v: TextFieldValue| {})       // 受控更新（编辑后回调新值）
    .modifier(Modifier)
    .font_size(...)                                 // 默认 14.sp()
    .enabled(bool)                                  // false：不可聚焦/编辑/选中
    .read_only(bool)                                // true：可聚焦可复制、不可编辑
    .placeholder(|ctx| { ... })                     // 空内容时显示（组合闭包）
    .single_line(bool)                              // true：Enter 吞掉、换行替换为空格
    .max_lines(usize) / .min_lines(usize)           // 默认 usize::MAX / 1
    .interaction_source(source)
    .is_error(bool)                                 // 错误态（边框/文字/光标 error 色）
    .filled() / .outlined() / .no_container()       // 变体（默认 Filled）
    .colors(TextFieldColors)
    .label(|ctx| { ... })                           // 悬浮/展开 label（位置+字号动画）
    .supporting_text("...")                         // 容器底部外侧辅助文本
    .visual_transformation(t)                       // 密码掩码/格式化（显示≠编辑，OffsetMapping 跨界转换）
    .leading_icon(|ctx| { ... }) / .trailing_icon(|ctx| { ... })
    .prefix(|ctx| { ... }) / .suffix(|ctx| { ... })
    .build(ctx);
```

- **受控/非受控**：状态由调用方持有（`State<TextFieldValue>`，Arc 共享）；不设
  `on_value_change` 即纯展示（非受控），设了即受控（编辑后同步外部）。
- 与 Compose 差异：Compose 要求 onValueChange 的值下一帧回喂（否则光标乱跳）；
  winia 是 `State` 共享——TextField 内部直接读写同一 State，天然满足回喂约束，
  无需用户手动回喂。
- `build` 与 Column/Row 不同：**无 content 闭包**（7 个槽位全经 setter 传入）。

## 2. 核心结构：容器 / 输入 leaf 拆分（理解一切的关键）

text-field-v2 容器化后，组件拆成两层节点，**职责分属两侧**——后续所有修复
都是围绕"焦点在容器、编辑状态在 leaf"的坐标/查找对齐：

| 侧 | 节点 | 挂载内容 |
|---|---|---|
| **容器** | `start_restartable_group`（container_modifier） | `TextFieldVisual`（含 offset_mapping、cursor_color、indicator 动画、supporting）、`Focusable`、`on_key_event`（键盘处理）、M3 padding/背景/边框/指示线 |
| **输入 leaf** | `ctx.key(Input) + start_leaf`（input_modifier） | `cached_paragraph`（文本测量）、`cursor_index`、`ime_callback`、`registrar`（选区）、`display_focused`、`text_content` |

**由此派生的关键约定**（改动时必须遵守）：
- **焦点/键盘在容器**：`focusable` 挂容器 → `apply_ime_for_focus`/Preedit 分发/
  IME 候选框区域都须**向下**找 leaf（`find_descendant_ime_callback`）。
- **paragraph/光标在 leaf**：点击定位/渲染光标从 leaf 出发，但 `TextFieldVisual`
  在容器 → **向上**找（`offset_mapping_for_node` / `text_field_visual_color`）。
- **registrar 用 leaf 的 slot_key 注册**（`input_key`，非容器 key）——渲染高亮
  `selected_range(node.slot_key)` 与拖动定位 `segment_info` 都按 leaf 查询。
- **空白点击兜底**：点容器 padding/文本右侧 → innermost 是容器（无 para）→
  `find_anchor_text_node` 向上找失败后**向下** DFS 找「有 para + 有 registrar」
  的输入 leaf；向下分支仅当 innermost 是 TextField 容器（`TextFieldVisual`）
  时执行，避免页面空白误命中远处 TextField。

## 3. 实现细节

### 3.1 布局（TextFieldLayout——7 角色定位）

`TextFieldLayout` 是容器 MeasurePolicy，按 `TextFieldSlotRole` 给子节点定位：
`Leading(12dp 左，容器垂直居中) | Label(悬浮顶部/展开输入位，progress 插值) |
Placeholder/Prefix(输入前) | Input(剩余宽) | Suffix(输入后) | Trailing(右 12dp 居中)`。

- **多轮测量**：第一轮 leading/trailing/prefix/suffix 宽度 → 第二轮 input 剩余宽
  （`width - left - prefix - 2 - suffix - right`）；容器宽 = min_width(280) 起步，
  超长输入按内容回算撑宽。
- **空文本最小尺寸**：空 paragraph 测量 0×0 → 渲染提前 return → 光标不显示。
  输入节点强制 `宽≥1、高≥解析后的 input lineHeight`；默认为 Typography
  `body_large` 的 24sp 行高，`.font_size()` 仍可覆盖输入字号。
- **supporting 高度**：`measure 高 = input + supporting_h`——渲染端
  `container_rect = 节点高 - supporting_h`（supporting 画在容器底部外侧 4dp）。
  `supporting_h` 取 Typography `body_small` 的固定行高；measure 必须预留该空间，
  否则多行换行后指示线/边框会上移穿过末行文字。
- **label/图标居中锚点**：`text_field_content_height` 多行时取
  `max(min_h - supporting, input_h)`——容器长高后图标/展开 label 跟随中心下移。
- 滚动容器内约束高 = f32::MAX → 用 min_height 兜底推导。

### 3.2 渲染（render.rs）

- **容器视觉**（`draw_text_field_container`）：Filled 填充 + 底部指示线
  （聚焦 1→2px 宽度动画、颜色 100ms Tween）；Outlined 边框 + label 缺口
  （按 label 子节点 placement 构造 cutout，`ly < 容器顶` 才有缺口）。
- **光标**：`display_focused || focused` 且无选区时画；宽度 1.5px 竖线；
  空文本画在内容起点（行高近似）；索引经 OffsetMapping 转显示偏移。
- **选区**：`get_rects_for_range` 高亮，颜色 = primary alpha 60。
- **组合下划线**：`composing_range` 经映射转显示偏移，行底 1px 线。
- **supporting**：容器底部外侧 4dp，默认 Typography `body_small`（12sp/16sp/Regular/0.4sp），error 时只覆盖颜色。

### 3.3 键盘（完整映射表，winia/src/ui/text_field.rs kb_handler）

| 键 | 动作 | 对标 Compose KeyCommand |
|---|---|---|
| 字符 | 插入（有选区先删选区） | — |
| Backspace / Delete | 删前一/后一 **grapheme**（选区优先） | DELETE_PREV/NEXT_CHAR |
| Ctrl+Backspace / Ctrl+Delete | 删前/后**词** | DELETE_PREV/NEXT_WORD |
| ← / → | 移前一/后一 grapheme；Shift 扩展 | LEFT/RIGHT_CHAR、SELECT_* |
| Ctrl+← / Ctrl+→ | 跳词首/词尾；Shift 扩展 | LEFT/RIGHT_WORD、SELECT_* |
| ↑ / ↓ | 上一/下一行首（`\n` 近似）；单行放行给焦点导航 | UP/DOWN |
| Home / End | 行首/行尾；Ctrl = 文首/文尾 | LINE_START/END、HOME/END |
| Enter | 插入换行；单行吞掉 | NEW_LINE |
| Tab | 放行给框架焦点导航 | TAB |
| Ctrl+A / C / X / V | 全选/复制/剪切/粘贴（单行粘贴换行→空格） | SELECT_ALL、COPY、CUT、PASTE |
| Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y | 撤销/重做 | UNDO/REDO |

- **词边界** = `is_alphanumeric`（对齐 Compose `isLetterOrDigit`，非 UAX#29——
  "a.b" 中标点是边界）。
- **每键处理前快照**（对齐 `forceNextSnapshot`）：同文本只更新 selection
  （光标移动合并，不产生新撤销条目）。
- **编辑前结束 IME 组合**（`FinishComposingTextCommand` 语义）；Ctrl+A/C 除外。
- 修饰键判定：Ctrl（macOS Cmd）；**Windows 的 Meta（⊞）不并入**（避免与
  系统快捷键冲突）。

### 3.4 IME

- **开启**：`node_or_descendant_wants_ime`——焦点容器向下找 `ime_callback` 声明。
- **Preedit 分发**：焦点节点向下找 `ime_callback`（`find_descendant_ime_callback`），
  回调插入组合文本 + 设 `composing_range` + 光标恒单点。
- **候选框区域**：渲染后 `request_ime_update(cursor_area)`——用输入 leaf 的
  paragraph/cursor 计算（容器无 paragraph），每帧跟随光标。
- **Commit**：逐字符走容器 `on_key_event` 的 Character 插入路径。

### 3.5 选区与拖动

- 点击定位：`handle_pointer_down` → `find_anchor_text_node`（向上/向下）→
  `get_closest_grapheme_cluster_cluster_at`（skia 段落定位，支持多行）→
  OffsetMapping 转编辑偏移 → `cursor_index` + 回调。
- 拖动：`handle_pointer_move` 复用 SelectionContainer 拖动管线
  （`compute_selection` + registrar）——down 记 anchor，move 按当前 index 取
  min..max，写回 reg（显示空间）经映射转编辑空间。
- **滚动一致性**：`node_abs_position` 累加时减祖先 scroll 偏移（与 hit_test
  同一坐标空间）——否则滚动容器内多行点击/拖拽全落到第一行。

### 3.6 光标闪烁

- tokio 任务 100ms 轮询，**仅聚焦时翻转**（`is_focused()` 检查——非聚焦
  set 会触发 17 个字段全量重组）；翻转后重置计时器（防狂闪）。
- 点击/键盘操作 `blink_reset`：立即可见 + 重置相位。

### 3.7 视觉变换（visual_transformation）

- 显示文本 = `filter(编辑文本)`；**一切跨界转换走 OffsetMapping**：
  渲染光标/选区/组合下划线 = `original_to_transformed`；点击/拖动定位 =
  `transformed_to_original`。
- `TextFieldVisual`（含 offset_mapping）在容器、leaf 查找不到 → 用
  `offset_mapping_for_node` 沿 parent 链向上找（裸 TextField 用
  `TextFieldOffsetMapping` 元素挂载）——否则掩码字符（'•' 3 字节）显示偏移
  直写 selection → `replace_range` 越界 panic。

### 3.8 其他

- UndoManager：双栈 + 同文本 selection 合并 + 上限 100 条 + 新编辑清 redo。
- 剪贴板：arboard（临时打开、无长驻句柄）。
- 视觉默认值：Filled（`new` 默认）；`no_container()` 裸输入（仍支持变换）。
- demo 14 展示响应式 `is_error`：`value.get()` 注册依赖 → 编辑自动重组变色。

## 4. 对照 Compose 的差距清单（改进方向）

### P0（行为正确性）

| # | 差距 | Compose 行为 | winia 现状 |
|---|---|---|---|
| 1 | **双击选词 / 三击选段** | 1/2/3 击 = None/Word/Paragraph（鼠标共用） | 无（每次点击重定位光标） |
| 2 | **Shift+点击扩展选区** | 按下即从 anchor 扩选 | 无（Shift 仅键盘扩展） |
| 3 | **↑/↓ 光标按列对齐** | 用 TextLayoutResult 保持水平偏移 | `\n` 近似（跳行首），无布局信息 |
| 4 | **删除按 code point** | emoji/代理对边界 | grapheme（接近，但组合字符偏保守） |
| 5 | **撤销合并语义** | staging 合并（连续插入/同向删除/换行不合并/5s 超时） | 仅"同文本更新 selection" |
| 6 | **Ctrl+H** | DELETE_PREV_CHAR | 未映射（H 被当字符输入） |
| 7 | **Alt+Delete / Ctrl+Shift+Backspace** | DELETE_TO_LINE_END / 删前词 | 未映射 |
| 8 | **PageUp/PageDown、Ctrl+↑/↓（段）** | PAGE_UP/DOWN、PREV/NEXT_PARAGRAPH | 未映射 |

### P1（API 面）

| # | 差距 | Compose | winia |
|---|---|---|---|
| 9 | **imeAction / 键盘动作** | KeyboardOptions.imeAction + KeyboardActions(onDone/onNext/...) | 无 |
| 10 | **inputTransformation** | 过滤用户输入（键盘/粘贴/拖放），不改程序化 setText | 仅 output（visualTransformation），无 input 过滤 |
| 11 | **scrollState** | 内部滚动状态（超长输入水平滚动） | 无（超长溢出裁剪） |
| 12 | **contentPadding / shape 定制** | 可定制 | 固定 4dp 圆角、padding 按变体写死 |
| 13 | **labelPosition** | Inside / Cutout | 仅 Outlined cutout 风格 |

### P2（视觉细节）

| # | 差距 | Compose | winia |
|---|---|---|---|
| 14 | 光标厚度 | 2dp（floor 到 px，奇数半像素居中） | 1.5px 竖线 |
| 15 | 选区色 | handle 0xFF4286F4 + bg alpha 0.4 | primary alpha 60（写死） |
| 16 | indicator 动画 | 宽度弹簧（stiffness 1400）、颜色弹簧 | Tween（100ms/150ms） |
| 17 | placeholder alpha | FastEffects/SlowEffects 双向 | 已有（150ms 双向）✓ |
| 18 | label 行高插值 | lerp(24, 16, progress) | 字号动画已实现，行高未单独插值 |
| 19 | 拖拽自动滚动 | 拖动超边界自动滚 | 无 |

### 已知限制（设计取舍）

- ↑/↓ 无 TextLayoutResult（winia 无公开布局查询 API）——近似行首。
- 无 selection handles（桌面触摸场景未做）。
- Undo 快照间隔/换行不合并等 Compose 高级语义未全实现。
- `single_line` 时 Up/Down 放行给框架焦点导航（与 Compose 一致）。

## 5. 改进建议路线（按依赖排序）

1. **P0-1 双击/三击选词选段**：gesture 层已有 tap/double-tap/long-press 识别，
   在 `detect_click`/gesture 处加 click-count 状态 → 复用 registrar 选区 +
   `get_word_boundary`/段落边界。
2. **P0-2 Shift+点击扩展**：`handle_pointer_down` 记录 shift → anchor 已有，
   点按后 `selection = min(anchor, idx)..max`。
3. **P0-3 ↑/↓ 列对齐**：需布局查询 API（`onTextLayout` 等价物）——
   跨组件设计（text layout result 缓存暴露），改动大、排后。
4. **P0-4~8 键位补齐**：直接对照 §3.3 表补 KeyCommand 分支（低风险）。
5. **P1-9 imeAction**：kb_handler 暴露 Enter/action 回调 + `keyboard_actions`
   setter。
6. **P2-14/15 视觉**：光标 2dp、选区色 token 化。
