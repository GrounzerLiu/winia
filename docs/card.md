# Card 组件：API、实现细节与未实现项

> 目标：对标 Jetpack Compose material3 1.4.0 的 `Card` 一族
> （`Card` / `ElevatedCard` / `OutlinedCard`），在 winia 上提供等价的
> 声明式 API 与接近的视觉/交互语义。
> 本文档随实现演进，改动 API 或行为时同步更新。

## Sources

Compose 路径相对于
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/`。

| Compose | 决定什么 |
|---|---|
| `Card.kt` | 六个重载（`Card` / `ElevatedCard` / `OutlinedCard`，各有静态与 `onClick` 两版）、`CardDefaults` 的 shape/elevation/colors/border、`CardElevation.shadowElevation`（`:657`，`interactionSource` 为 `null` 时取 `defaultElevation`）与私有的 `animateElevation`（`:668`，interactions 列表 `lastOrNull` 的状态解析） |
| `Card.kt:498`、`:546`、`:597`（`defaultCardColors` / `defaultElevatedCardColors` / `defaultOutlinedCardColors`） | 三种 style 的容器色与**禁用**容器色，包括 `compositeOver` 的用法 |
| `tokens/{Filled,Elevated,Outlined}CardTokens.kt` | 容器色、六个状态的高度、`ContainerShape`、`DisabledContainerOpacity`、`DisabledOutlineOpacity`、`FocusIndicatorColor` |
| `tokens/ShapeKeyTokens.kt` + `tokens/ShapeTokens.kt:70` | `CornerMedium` = `RoundedCornerShape(12.0.dp)` |
| `tokens/ElevationTokens.kt:24-37` | Level0/1/2/3/4 → 0/1/3/6/8 dp 的换算（**不是**序号） |
| `Surface.kt:202`（`Surface(onClick = …)` 重载）、`:225`（其 Box 上的 `minimumInteractiveComponentSize()`） | 可点击卡片为什么有 48dp 触控目标 |
| `internal/Elevation.kt:46-62`（`animateElevation`） | 高度过渡曲线按**交互对**选择，而非固定时长 |

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
| `elevation(CardElevation)` | `elevation` | 阴影高度按交互状态取值，过渡曲线由交互对（`elevation_spec_for_pair`）决定，非固定时长 |
| `interaction_source(...)` | `interactionSource` | hoist 交互源；未传则内部 `remember` |
| `shape(Shape)` | `shape` | 容器/边框/阴影/焦点环/波纹统一形状；默认 12dp 圆角（`CornerMedium`） |
| `border(CardBorder)` | `border: BorderStroke` | 覆盖 style 默认边框（Outlined 默认 1dp outline 色） |
| `modifier(Modifier)` | `modifier` | 追加在外层，可覆盖默认样式 |

### 1.3 值类型与默认值

- `CardStyle`：Filled / Elevated / Outlined。
- `CardColors`：`container / content / disabled_container / disabled_content`。
- `CardElevation`：`default / pressed / focused / hovered / dragged / disabled`
  （六状态——比 ButtonElevation 多 dragged，对齐 M3 `CardElevation`）。
- `CardBorder`：`width + color`（对标 `BorderStroke`）。
- `CardDefaults`：
  - `shape()` = 12dp 圆角（三种 style 的 `ContainerShape` 都是 `CornerMedium`，
    `ShapeTokens.kt:70`：`RoundedCornerShape(12.0.dp)`）；
  - `card_colors(theme, style)`——按 style 从主题推导：
    | style | container | content | disabled_container | disabled_content |
    |-------|-----------|---------|-------------------|-----------------|
    | Filled | SurfaceContainerHighest | OnSurface | SurfaceVariant@38% | content@38% |
    | Elevated | SurfaceContainerLow | OnSurface | Surface（不透明） | content@38% |
    | Outlined | Surface | OnSurface | Surface（= 容器） | content@38% |
  - `card_elevation()` = 0/0/0/1/6/0 dp（FilledCardTokens）；
  - `elevated_card_elevation()` = 1/1/1/3/8/1 dp（ElevatedCardTokens）；
  - `outlined_card_elevation()` = 0/0/0/1/6/0 dp（OutlinedCardTokens）；
  - `outlined_border(theme, enabled)` = 1dp OutlineVariant（disabled 为 Outline@12%）。

  > 数值是 Dp，不是 token 的 level 序号。`ElevationTokens` 的 Level0/1/2/3/4 分别对应
  > 0/1/3/6/8 dp —— 两者只有 Level0 和 Level1 相同，历史上这里存过序号，导致 Filled 卡
  > 拖拽只升 3dp 而非 6dp、Elevated 只升 4dp 而非 8dp。

## 2. 布局与测量

- **测量策略**：`ColumnLayout::new().direction(dir)`——一个 12dp 圆角的容器，内容按
  **垂直 Column** 排布、交叉轴 `Start` 对齐、主轴 `Arrangement::Start`、间距 0。
  Compose 是 `Surface { Column(content) }`，列本身不带 padding，间距由内容自己给。
- **尺寸**：容器随内容（`fill` 用调用方的 `Modifier`）。没有内部固定高度；
  `Card::size` 之类的参数也不存在——尺寸一律走 `Modifier`。
- **内容间距由内容自己控制**（对标 M3：Card 无 `contentPadding` 参数——叶子节点的
  `Modifier.padding` 已完整支持，测量回加 + 渲染偏移）。
- **可点击卡片的额外包装**：`on_click` 存在时，外面套一个
  `build_container` + `minimum_interactive_component_size()`（48dp）+ `BoxLayout`
  居中，并设 `propagate_min_constraints(true)`。后者是必须的：父级若 FORCE 一个宽度，
  内层卡片才跟着变成那个宽度；否则外层报告的是被强制的尺寸，而内层仍按自己的内容宽度，
  焦点环、波纹、容器三者对卡片宽度失去共识。
  - 注意包装只看 `on_click.is_some()`，**不看 enabled** —— 与 Compose 的
    `Surface(onClick = …)` 一致，它也无条件应用最小交互尺寸，所以禁用的可点击卡片
    仍占 48dp。纯展示卡片没有这个包装，也就在 Compose 里没有该最小值。
- **RTL**：Column 的主轴是 y，镜像的是**交叉轴**（x）。`measure_flex` 在
  `direction == Rtl` 时把每个子节点的 `position.x` 换成
  `cross_size - x - width`（`flex.rs:513-518`）。主轴方向不受影响，因为 Column 的
  主轴是垂直的；`Arrangement::Absolute(_)` 的镜像豁免在 Column 上没有可观察差异。
- **Intrinsic 尺寸**：`ColumnLayout` 覆写四个 intrinsic 查询
  （`reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/RowColumnImpl.kt:261-369`
  的 `IntrinsicMeasureBlocks` 移植——注意它不在 material3 下），`weight` 走与测量相同的算术。

## 3. 行为与实现细节

### 3.1 build 流程与 modifier 链顺序

`Card::build` 每帧执行：

1. `ctx.changed(&self.style)` / `ctx.changed(&self.enabled)` 声明 Skip 参数；
2. 取主题与颜色（`CardDefaults::card_colors`），读交互状态；
3. 构造阴影动画 State（`ctx.animate_float_as_state`）——过渡曲线由**交互对**决定
   （`elevation_spec_for_pair(from, to)`，对标 `material3/internal/Elevation.kt:46-62` 的
   `animateElevation`），
   不是固定时长的单曲线；进入 120ms、离开 150ms、idle→idle 直接 snap；
4. 组装默认 modifier：`background(container, shape)` → `border`（Outlined）；
5. 阴影经 `graphics_layer` 动态闭包（渲染期读动画值，不触发重组）；
6. **可点击时**声明 semantics（`role(Button)` + `merge_descendants(true)` + `enabled` 状态）；
7. `modifier.then(用户 modifier)`（用户外层可覆盖，semantics 也在其后可被覆盖）；
8. enabled 且 on_click 非空时追加 `clickable_with_source` + `ripple_with_shape`；
9. `start_restartable_group`（Skip 时 content 不执行，但新 modifier 经
   `set_skip_modifier` 应用到节点）；
10. `set_current_node_focus_color(theme.secondary)` 写入 desc 通道
    （M3 `FilledCardTokens.FocusIndicatorColor` = Secondary——与 Button 的 primary 有意不同）。

链顺序（内→外）：容器(background/border) → shadow(graphics_layer) →
semantics → 用户 modifier → clickable/ripple。

- **内容 = 顶部对齐 Column**（对标 M3：`Surface { Column(content) }`）；
  内容色经 `WiniaTheme::with_content_color` 下传——`Icon::tint(Auto)`
  取卡片内容色。
- **三种 style 都有不透明容器色**（Filled=SurfaceContainerHighest、
  Elevated=SurfaceContainerLow、Outlined=Surface——M3 Card 无透明底变体，
  与 Button 的 Outlined/Text 透明底不同）。

### 3.2 状态与取色

- 容器/内容色只区分 enabled/disabled（M3 `CardColors` 语义）；
  hover/press/focus 的视觉反馈由 ripple 状态层绘制，不叠加在背景色上。
- M3 token 配色（1.4.0）：Filled = SurfaceContainerHighest/OnSurface；
  Elevated = SurfaceContainerLow/OnSurface；Outlined = Surface/OnSurface。
- 禁用：Filled 容器 SurfaceVariant@38%（alpha 直乘近似 M3 的
  compositeOver）；Elevated 容器换成**不透明的 Surface**（M3 是
  `Surface@0.38 compositeOver Surface`，结果即为 Surface 本身，与启用的
  SurfaceContainerLow 不同色）；Outlined 容器不变（M3 的
  DisabledContainerColor 就是 ContainerColor）。内容统一 OnSurface@38%。

### 3.3 阴影（hover/按下升高动画）

- 阴影值由 `graphics_layer.shadow_elevation` 动态闭包驱动，过渡曲线按**交互对**选择：
  进入 120ms、离开 150ms、离开 hover 120ms、idle→idle 直接 snap。
- `for_state` 优先级：disabled > **dragged** > pressed > hovered > focused > default
  —— 拖拽必须排在按压之前，因为拖拽一定伴随按压，反过来 `dragged` 永远不可达。
- 阴影由 Skia 原生 ambient/spot 光源绘制，默认颜色约为 ambient 10% 黑、spot 25% 黑；
  阴影形状跟随 `Card::shape`。
- `graphics_layer` 只影响外观，不参与命中测试（语义已注释）。

### 3.4 波纹与裁剪

- `ripple_with_shape(interaction, contentColor, bounded, shape)`：显式容器形状，
  波纹按容器形状（默认 12dp 圆角）裁剪。

### 3.5 焦点系统

- 焦点环：宽 3、完全外置（内侧距组件 2）、颜色 = 主题 secondary（M3 Card
  FocusIndicatorColor 对齐；组合期经 desc 通道捕获，渲染期 CompositionLocal
  已退出）、180ms 淡入淡出。
- 键盘激活：Enter/Space 只触发**聚焦节点自身**的 `on_click`，不向祖先冒泡
  （与 Button 一致）。

### 3.6 无障碍

- **可点击卡片**：声明 `role(SemanticsRole::Button)` + `merge_descendants(true)` +
  `SemanticsState::new().enabled(self.enabled)`。声明位置在追加调用方 `modifier`
  **之前**，所以调用方的 `semantics(..)` 能逐字段覆盖、同时保留 role。
  `merge_descendants` 让卡片的文字成为这个节点的名字，而不是独立子节点。
- **不可点击卡片**：不声明任何语义，保持语义树里的透明（`component-authoring.md`
  §9 第二条）。给静态卡片加 Button 角色等于在每张卡片前放一个无名按钮。
- 禁用但可点击的卡片**保留 role 并上报 `enabled = false`** —— 丢掉任一半都会让
  屏幕阅读器既读不出"这是按钮"也读不出"它不可用"。

### 3.7 性能要点

- Skip 判定 = style/enabled 参数相等 + modifier 参数相等；动画值走
  `graphics_layer`，不经过重组。
- 波纹按时间在渲染期计算，无额外动画状态。

## 4. 与 material3 1.4.0 的差距（未实现/近似）

### 4.1 组件与 API 形态

- **独立 composable**：M3 的 `ElevatedCard` / `OutlinedCard` 是独立函数；
  winia 用构造变体近似（后续可拆，API 不变）。
- **`BorderStroke`**：仅 width + color；无 Brush/渐变/虚线。
- **`CardColors.copy()` 逐参数覆盖**：winia 以 `CardColors::new(4 色)` 覆盖。
- **`ColumnScope`**：M3 的 content 是 `@Composable ColumnScope.() -> Unit`，卡片内可以
  `Modifier.weight()` / `align()`；winia 的 `Card::build` 取
  `impl FnOnce(&mut ComposeCtx)`，调用方拿不到 ColumnScope。`ColumnScope` 类型本身已
  存在（`layout/column.rs:29`，带 `weight`/`weight_fill`/`align`/`align_by`/
  `align_by_baseline`，且 `DerefMut` 到 `ComposeCtx`，换成它不会破坏现有 `|ctx| …`
  调用点），只是还没有组件用它。

### 4.2 主题与 Token

- **形状 token**：CornerMedium = 12dp，对应 `Shape::rounded(12.0)`，无主题级
  shape 系统（同 Button）。
- **disabled 容器色**：Filled 用 alpha 直乘近似 M3 的 compositeOver（项目统一
  约定，同 checkbox/button）；Elevated 取不透明的 Surface 作为精确结果（叠加同色
  后本就该不透明，用 alpha 近似反而会做出半透明卡片）；Outlined 与 M3 一致。
- **内容色**：M3 `contentColorFor(container)` 按对比度计算；winia 统一取
  OnSurface（三种容器均为 surface 系，对比结果一致）。
- **tonal overlay**：M3 在容器色为 `surface` 时按 elevation 叠一层 primary；winia
  未实现（这是 `Surface` 层的缺口，非 Card 特有）。

### 4.3 交互与渲染

- **阴影动画触发源**：M3 用 interaction 流 + `animateElevation`（interactions
  列表 lastOrNull 语义）；winia 用 `ComponentState` 快照 + 按交互对选曲线
  （`elevation_spec_for_pair`）——状态进入/离开的过渡曲线与 M3 的
  `animateElevation` 不完全相同（近似），但两套可达路径（hover→press、
  press→hover）已对齐。
- **波纹**：只有 bounded 波纹；无 unbounded、无 hover 涟漪。
- **a11y**：已实现 role + enabled + merge_descendants（§3.6）。**缺**无障碍标签
  （`contentDescription`）与自定义 action。
- **命中测试**：`graphics_layer` 外观不参与命中（已注释为语义）。

### 4.4 测试缺口

- 键盘激活、Tab/IME 同步、方向键导航均无 app 级集成测试（现有覆盖为
  单元级——与 Button 相同缺口）。
- 焦点环形状/颜色的渲染断言未接入 UI 测试框架。
- 无 `fixture_all` 场景（13 个场景中没有 card），因此 UI 测试套件跑不到本组件。

## 5. 测试

```bash
# 本组件的全部单元测试
cargo test -p winia --lib components::card

# 只看无障碍声明那两条
cargo test -p winia --lib a_clickable_card_declares_the_button_role_and_its_enabled_state

# 回归：依赖 Card 的组件与 demo
cargo check -p winia --examples
```

12 个单元测试，各钉住一条不变量：

| 测试 | 钉住什么 |
|---|---|
| `a_clickable_card_declares_the_button_role_and_its_enabled_state` | 可点击 → `Button` 角色 + `enabled` 状态 + `merge_descendants`；禁用可点击 → 保留角色且上报 `false`；不可点击 → 完全不声明（对照组） |
| `a_clickable_card_reserves_a_48_dp_touch_target` | 可点击卡片有 48dp 最小交互尺寸，纯展示卡片没有（对照组） |
| `the_card_elevation_spec_keys_on_the_interaction_pair` | 过渡曲线按交互对：进入 120ms、离开 150ms、idle→idle snap |
| `a_dragged_filled_card_raises_the_shadow_it_paints` | 拖拽中的 Filled 卡真的画出 6dp 阴影（不是序号 3） |
| `test_card_default_shape_is_corner_medium` | 默认圆角 = 12dp，注释与断言都指向 `ShapeTokens.kt:70` |
| `test_card_colors_match_m3_tokens` | 三种 style 的容器/内容色；Elevated 禁用容器是**不透明的 Surface**，且断言 `Surface != SurfaceContainerLow`（否则该断言无意义） |
| `test_card_colors_state_resolution` | `ComponentState` → 禁用优先取 `disabled_container` / `disabled_content` |
| `test_card_elevation_defaults` | 三套 elevation 的六个数值（Dp，非 level 序号） |
| `test_card_elevation_priority` | `for_state` 的 disabled > dragged > pressed > hovered > focused > default |
| `test_outlined_border_uses_theme_outline_variant` | Outlined 默认边框 1dp OutlineVariant，禁用为 Outline@12% |
| `test_card_variant_constructors` | 三个构造函数各自设定的 style 与 elevation |
| `test_card_builder` | builder 各字段的往返与默认值 |

固定装置：**无** `fixture_all` 场景（见 §4.4），UI 测试套件不会跑到本组件。

## 6. 维护约定

- 新增 builder 参数：默认值放 `CardDefaults`，在 `build` 中经 modifier
  携带即可（不需要加进 `changed()`）；同步补 getter、单元测试与本文档。
- 修改视觉语义（颜色/形状/焦点）时保持"对标 M3 token 优先、近似需注释"原则。
- 涉及焦点/IME/键盘的分支改动，四条路径（Tab/方向键/点击/Escape）必须一起更新。
- 改无障碍声明时同步 `docs/semantics.md` 的组件表（若该表后续加入 Card 行）。
