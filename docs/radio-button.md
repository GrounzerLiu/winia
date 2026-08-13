# RadioButton 组件（material3 对齐）

> 分支：`radio-button`（从 v2 分出）
> 对标：material3 1.4.0 `RadioButton`（Compose androidx-main RadioButton.kt）

## 1. API

```rust
RadioButton::new(selected: bool)              // 对标 RadioButton(selected, ...)
    .on_click(|| {})                          // 对标 onClick（None → 不可交互）
    .enabled(bool)                            // 对标 enabled
    .colors(RadioButtonColors)                // 对标 colors
    .interaction_source(source)               // 对标 interactionSource（hoist）
    .modifier(Modifier)                       // 外部修饰符
    .build(ctx);
```

- 组件本身不持有状态：`selected` 由调用方传入，点击回调由调用方决定新选中项
  （受控语义，与 `Checkbox`/`IconToggleButton` 一致）。
- `on_click` 为 `None` 时**不挂 clickable**——不可交互但正常渲染
  （对齐 Compose `onClick = null` 语义，屏幕阅读器由外层 selectable 管理场景）。
- 单选组：多个 `RadioButton` 的互斥由调用方状态维护（demo 展示了
  Compose RadioGroupSample 等价写法：一个 `State<String>` + 每项
  `selected == label` + 点击更新）。

## 2. 默认值（对标 `RadioButtonDefaults` / `RadioButtonTokens` v0_117）

| 项 | 值 |
|---|---|
| 视觉尺寸 | 20×20（`IconSize`） |
| 触摸目标/状态层 | 40×40（`StateLayerSize`，形状 Circle） |
| 圆环描边 | 2（`RadioStrokeWidth`——外圈中径半径 (20-2)/2 = 9） |
| 选中内点 | 直径 10（`RadioButtonDotSize = 12`，绘制半径 = 12/2 - 2/2 = 5） |
| 波纹 | `ripple(bounded = false, radius = 20)`——unbounded 圆形 |

### 颜色（`RadioButtonColors::from_theme`）

| 状态 | 图标色 |
|---|---|
| selected | Primary |
| unselected | OnSurfaceVariant |
| disabled selected | OnSurface @ 38% |
| disabled unselected | OnSurface @ 38% |

颜色只分 enabled×selected（`RadioButtonColors` 四字段，无 hover/focus/press
变体——交互反馈由 ripple/state layer 承担，`RadioButtonTokens` 的
Selected/Unselected Hover/Focus/Pressed IconColor 与静态色同值，Compose
`RadioButtonImpl` 同样不读交互态取色）。

## 3. 实现细节

- 组合结构：外层 40×40 节点（clip Circle 供焦点环推断形状）→
  `clickable_with_source` + unbounded ripple → 内层 20×20 视觉节点
  （`border_dynamic` 2dp Circle = 圆环描边）。
- 选中内点：10×10 叶子节点（`background` Circle 实心圆），
  `graphics_layer` scale 0↔1 动画。与 Compose 的
  `animateDpAsState(dotRadius) + drawCircle(Fill)`（半径 0→5dp 线性动画）
  视觉等价——同心圆从 0 放大到最终半径。
- 颜色动画：`animate_color_as_state` Tween 300ms Linear（对标 Compose
  `animateColorAsState(MotionScheme.DefaultEffects)` = 300ms tween；
  `push_animatable_color` 会把 Spring 降级为 `TweenSpec::default()`）；
  渲染期 `peek()` 读取动画值不触发重组。
- 内点动画规格：Spring `StiffnessMedium(400)`/NoBouncy（对齐 checkbox 勾号
  规格；Compose `MotionScheme.FastSpatial` 为 spring(600/30)，winia 无直接
  等价——近似差异同 checkbox 文档说明）。
- 像素级验证：选中/未选中/禁用三态 + 取消选中过渡帧保留色（颜色瞬切会吞
  退出动画）+ 动画推完回未选中，共 5 个测试（raster surface 断言；注意
  `raster_n32_premul` 小端像素为 BGRA 布局，读取按 (R,G,B) 语义换位）。

## 4. 与 Checkbox 的差异（对照）

| 维度 | Checkbox | RadioButton |
|---|---|---|
| 视觉 | 20×20 圆角盒 + 勾号/横线 | 20×20 圆环 + 内点 |
| 选中动画 | 勾号/横线 scale 0↔1 | 内点 scale 0↔1（半径动画等价） |
| 颜色模型 | 12 字段（box/border/checkmark×状态） | 4 字段（selected/unselected×enabled） |
| 状态 | ToggleableState（含 Indeterminate） | bool |
| 形状 | RoundedRect(2) | Circle |

## 5. 未实现项（对齐 Compose 差距）

- `Modifier.selectableGroup()` / `selectable(role = Role.RadioButton)`——
  无障碍语义分组（winia 暂无 semantics 系统，demo 用显式 key + 状态维护互斥）；
- `minimumInteractiveComponentSize()`（Compose 交互最小 48dp）——winia 以
  StateLayerSize 40×40 为触摸目标（与 checkbox 一致）；
- 焦点环/键盘方向键组内导航（框架级 focus 已支持，组语义未实现）。
