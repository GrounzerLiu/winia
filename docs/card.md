# Card 组件：API、实现细节与未实现项

> 目标：对标 Jetpack Compose material3 1.4.0 的 `Card` 一族
> （`Card` / `ElevatedCard` / `OutlinedCard`），在 winia 上提供等价的
> 声明式 API 与接近的视觉/交互语义。
> 本文档随实现演进，改动 API 或行为时同步更新。

## 1. API 总览

### 1.1 构造变体

winia 没有为每种 M3 变体单独建 composable，而是用 `Card` builder 的构造函数表达
（与 Button 的 filled/elevated/outlined 同模式）：

| 构造 | 等价 material3 | 差异点 |
|---|---|---|
| `Card::filled()`（同 `new()`） | `Card` | 默认 Filled 样式（SurfaceContainerHighest 容器） |
| `Card::elevated()` | `ElevatedCard` | `CardStyle::Elevated`（SurfaceContainerLow 容器）+ `CardElevation::elevated()` |
| `Card::outlined()` | `OutlinedCard` | `CardStyle::Outlined`（Surface 容器 + 1dp outline 边框） |

### 1.2 链式参数（`Card` builder）

| 方法 | 对标 | 说明 |
|---|---|---|
| `on_click(Fn())` | `onClick` | 点击回调（`Send + Sync + 'static`）；**None = 纯展示卡片**（不可交互、无波纹） |
| `enabled(bool)` | `enabled` | 禁用：容器/内容色切换 + 不注册 clickable/ripple（不可聚焦） |
| `style(CardStyle)` | —— | Filled / Elevated / Outlined |
| `colors(CardColors)` | `colors: CardColors` | 默认由 `CardDefaults::card_colors(theme, style)` 生成 |
| `elevation(CardElevation)` | `elevation` | 阴影高度按交互状态取值，经 180ms 动画过渡 |
| `interaction_source(...)` | `interactionSource` | hoist 交互源；未传则内部 `remember` |
| `shape(Shape)` | `shape` | 容器/边框/阴影/焦点环/波纹统一形状；默认 8dp 圆角 |
| `border(CardBorder)` | `border: BorderStroke` | 覆盖 style 默认边框（Outlined 默认 1dp outline 色） |
| `modifier(Modifier)` | `modifier` | 追加在外层，可覆盖默认样式 |

### 1.3 值类型与默认值

- `CardStyle`：Filled / Elevated / Outlined。
- `CardColors`：`container / content / disabled_container / disabled_content`。
- `CardElevation`：`default / pressed / focused / hovered / dragged / disabled`
  （六状态——比 ButtonElevation 多 dragged，对齐 M3 `CardElevation`）。
- `CardBorder`：`width + color`（对标 `BorderStroke`）。
- `CardDefaults`：
  - `shape()` = 8dp 圆角（对标 `CardTokens.ContainerShape` = CornerMedium）；
  - `card_colors(theme, style)`——按 style 从主题推导：
    | style | container | content | disabled_container | disabled_content |
    |-------|-----------|---------|-------------------|-----------------|
    | Filled | SurfaceContainerHighest | OnSurface | SurfaceVariant@38% | content@38% |
    | Elevated | SurfaceContainerLow | OnSurface | 容器不变 | content@38% |
    | Outlined | Surface | OnSurface | 容器不变 | content@38% |
  - `card_elevation()` = 0/0/0/1/3/0（FilledCardTokens）；
  - `elevated_card_elevation()` = 1/1/1/2/4/1（ElevatedCardTokens）；
  - `outlined_card_elevation()` = 0/0/0/0/3/0（OutlinedCardTokens）；
  - `outlined_border(theme, enabled)` = 1dp OutlineVariant（disabled 为 Outline@12%）。

## 2. 实现细节

### 2.1 build 流程与 modifier 链顺序

`Card::build` 每帧执行：

1. `ctx.changed(&self.style)` / `ctx.changed(&self.enabled)` 声明 Skip 参数；
2. 取主题与颜色（`CardDefaults::card_colors`），读交互状态；
3. 构造阴影动画 State（`ctx.animate_float_as_state`，180ms EaseOutCubic）；
4. 组装默认 modifier：`background(container, shape)` → `border`（Outlined）；
5. 阴影经 `graphics_layer` 动态闭包（渲染期读动画值，不触发重组）；
6. `modifier.then(用户 modifier)`（用户外层可覆盖）；
7. enabled 且 on_click 非空时追加 `clickable_with_source` + `ripple_with_shape`；
8. `start_restartable_group`（Skip 时 content 不执行，但新 modifier 经
   `set_skip_modifier` 应用到节点）；
9. `set_current_node_focus_color(theme.secondary)` 写入 desc 通道
   （M3 CardTokens.FocusIndicatorColor = Secondary——与 Button 的 primary 有意不同）。

链顺序（内→外）：容器(background/border) → shadow(graphics_layer) →
用户 modifier → clickable/ripple。

- **内容 = 顶部对齐 Column**（对标 M3：`Surface { Column(content) }`）；
  内容色经 `WiniaTheme::with_content_color` 下传——`Icon::tint(Auto)`
  取卡片内容色。
- **内容间距由内容自己控制**（对标 M3：Card 无 contentPadding 参数——
  叶子节点 `Modifier.padding` 已完整支持，测量回加 + 渲染偏移）。
- **三种 style 都有不透明容器色**（Filled=SurfaceContainerHighest、
  Elevated=SurfaceContainerLow、Outlined=Surface——M3 Card 无透明底变体，
  与 Button 的 Outlined/Text 透明底不同）。

### 2.2 状态与取色

- 容器/内容色只区分 enabled/disabled（M3 `CardColors` 语义）；
  hover/press/focus 的视觉反馈由 ripple 状态层绘制，不叠加在背景色上。
- M3 token 配色（1.4.0）：Filled = SurfaceContainerHighest/OnSurface；
  Elevated = SurfaceContainerLow/OnSurface；Outlined = Surface/OnSurface。
- 禁用：Filled 容器 SurfaceVariant@38%（alpha 直乘近似 M3 的
  compositeOver）、Elevated/Outlined 容器不变；内容 OnSurface@38%。

### 2.3 阴影（hover/按下升高动画）

- 阴影值由 `graphics_layer.shadow_elevation` 动态闭包驱动，180ms tween 平滑过渡。
- `for_state` 优先级：disabled > pressed > dragged > hovered > focused > default
  （与 material3 的"最近交互优先"一致；各状态取独立配置值）。
- 阴影由 Skia 原生 ambient/spot 光源绘制，默认颜色约为 ambient 10% 黑、spot 25% 黑；
  阴影形状跟随 `Card::shape`。
- `graphics_layer` 只影响外观，不参与命中测试（语义已注释）。

### 2.4 波纹与裁剪

- `ripple_with_shape(interaction, contentColor, bounded, shape)`：显式容器形状，
  波纹按容器形状（默认 8dp 圆角）裁剪。

### 2.5 焦点系统

- 焦点环：宽 3、完全外置（内侧距组件 2）、颜色 = 主题 secondary（M3 Card
  FocusIndicatorColor 对齐；组合期经 desc 通道捕获，渲染期 CompositionLocal
  已退出）、180ms 淡入淡出。
- 键盘激活：Enter/Space 只触发**聚焦节点自身**的 `on_click`，不向祖先冒泡
  （与 Button 一致）。

### 2.6 性能要点

- Skip 判定 = style/enabled 参数相等 + modifier 参数相等；动画值走
  `graphics_layer`，不经过重组。
- 波纹按时间在渲染期计算，无额外动画状态。

## 3. 与 material3 1.4.0 的差距（未实现/近似）

### 3.1 组件与 API 形态

- **独立 composable**：M3 的 `ElevatedCard` / `OutlinedCard` 是独立函数；
  winia 用构造变体近似（后续可拆，API 不变）。
- **`BorderStroke`**：仅 width + color；无 Brush/渐变/虚线。
- **`CardColors.copy()` 逐参数覆盖**：winia 以 `CardColors::new(4 色)` 覆盖。

### 3.2 主题与 Token

- **形状 token**：CornerMedium(8dp) 直接对应 `Shape::rounded(8.0)`，无主题级
  shape 系统（同 Button）。
- **disabled 容器色**：M3 是 DisabledContainerColor @ Opacity compositeOver
  容器色；winia 用 alpha 直乘近似（项目统一约定，同 checkbox/button）。
- **内容色**：M3 `contentColorFor(container)` 按对比度计算；winia 统一取
  OnSurface（三种容器均为 surface 系，对比结果一致）。

### 3.3 交互与渲染

- **阴影动画触发源**：M3 用 interaction 流 + `animateElevation`（interactions
  列表 lastOrNull 语义）；winia 用 `ComponentState` 快照 + 180ms tween——
  状态进入/离开的过渡曲线与 M3 的 `animateElevation` 不完全相同（近似）。
- **波纹**：只有 bounded 波纹；无 unbounded、无 hover 涟漪。
- **a11y**：无 semantics 树/无障碍标签（全框架缺口，非 Card 特有）。
- **命中测试**：`graphics_layer` 外观不参与命中（已注释为语义）。

### 3.4 测试缺口

- 键盘激活、Tab/IME 同步、方向键导航均无 app 级集成测试（现有覆盖为
  单元级——与 Button 相同缺口）。
- 焦点环形状/颜色的渲染断言未接入 UI 测试框架。

## 4. 维护约定

- 新增 builder 参数：默认值放 `CardDefaults`，在 `build` 中经 modifier
  携带即可（不需要加进 `changed()`）；同步补 getter、单元测试与本文档。
- 修改视觉语义（颜色/形状/焦点）时保持"对标 M3 token 优先、近似需注释"原则。
- 涉及焦点/IME/键盘的分支改动，四条路径（Tab/方向键/点击/Escape）必须一起更新。
