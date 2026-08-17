# Button 组件：API、实现细节与未实现项

> 目标：对标 Jetpack Compose material3 1.4.0 的 `Button` 一族
> （`Button` / `ElevatedButton` / `FilledTonalButton` / `OutlinedButton` /
> `TextButton`），在 winia 上提供等价的声明式 API 与接近的视觉/交互语义。
> 本文档随实现演进，改动 API 或行为时同步更新。

## 1. API 总览

### 1.1 构造变体

winia 没有为每种 M3 变体单独建 composable，而是用 `Button` builder 的构造函数表达：

| 构造 | 等价 material3 | 差异点 |
|---|---|---|
| `Button::filled()`（同 `new()`） | `Button` | 默认 Filled 样式 |
| `Button::elevated()` | `ElevatedButton` | `ButtonStyle::Elevated`（SurfaceContainerLow 容器 + primary 内容）+ `ButtonElevation::elevated()` |
| `Button::filled_tonal()` | `FilledTonalButton` | `ButtonStyle::Tonal` |
| `Button::outlined()` | `OutlinedButton` | `ButtonStyle::Outlined`（透明底 + 1px outline 边框） |
| `Button::text()` | `TextButton` | `ButtonStyle::Text`（透明底、紧凑内边距） |

### 1.2 链式参数（`Button` builder）

| 方法 | 对标 | 说明 |
|---|---|---|
| `on_click(Fn())` | `onClick` | 点击回调（`Send + Sync + 'static`） |
| `enabled(bool)` | `enabled` | 禁用：容器/内容色切换 + 不注册 clickable/ripple（不可聚焦） |
| `style(ButtonStyle)` | —— | Filled / Tonal / Outlined / Text |
| `colors(ButtonColors)` | `colors: ButtonColors` | 默认由 `ButtonDefaults::button_colors(theme, style)` 生成 |
| `elevation(ButtonElevation)` | `elevation: ButtonElevation` | 阴影高度按交互状态取值，经 180ms 动画过渡 |
| `interaction_source(...)` | `interactionSource` | hoist 交互源；未传则内部 `remember` |
| `shape(Shape)` | `shape` | 容器/边框/阴影/焦点环/波纹统一形状；默认胶囊 |
| `content_padding((s,t,e,b))` | `contentPadding` | 每边支持动态 `SizeValue`；默认按 style（24/8 或 12/8） |
| `min_size(w,h)` | M3 内部 `defaultMinSize` | 默认 58×40；支持动态 `SizeValue` |
| `border(ButtonBorder)` | `border: BorderStroke` | 覆盖 style 默认边框 |
| `modifier(Modifier)` | `modifier` | 追加在外层，可覆盖默认样式 |

### 1.3 值类型与默认值

- `ButtonStyle`：Filled / Tonal / Outlined / Text。
- `ButtonColors`：`container / content / disabled_container / disabled_content`。
- `ButtonElevation`：`default / pressed / focused / hovered / disabled`。
  - `default_elevation()`：全 0（Filled 系）。
  - `elevated()`：`1 / 4 / 1 / 3 / 0`（基础对齐 M3 1.4.0 `ElevatedButtonTokens`：
    rest/focus = Level1、hover = Level2、disabled = Level0；**pressed 有意偏离 M3 的
    Level1 回落**——M3"按下压下去"隐喻依赖 hover，触屏无 hover，按下不升高则
    Elevated 按钮在触屏上无阴影反馈。本框架语义：按下升高（1<3<4）、释放降低，
    鼠标与触屏一致）。
  - 按下分支取 `max(pressed, hovered)`：自定义配置 press < hover 时按下也不回落。
- `ButtonBorder`：`width + color`（对标 `BorderStroke`）。
- `ButtonDefaults`：
  - `shape()` = 胶囊（对标 `CornerFull`）；
  - `min_width() = 58`、`min_height() = 40`；
  - `content_padding(style)`：非 Text 24/8/24/8，Text 12/8/12/8；
  - `button_colors(theme, style)`、`button_elevation()`、`elevated_button_elevation()`。

## 2. 实现细节

### 2.1 build 流程与 modifier 链顺序

`Button::build` 每帧执行：

1. `ctx.changed(&self.style)` / `ctx.changed(&self.enabled)` 声明 Skip 参数；
2. 取主题与颜色（`ButtonDefaults::button_colors`），读交互状态；
3. 构造阴影动画 State（`ctx.animate_float_as_state`，180ms EaseOutCubic）；
4. 组装默认 modifier：`min_size → padding_sides → background/border/clip`；
5. 阴影经 `graphics_layer` 动态闭包（渲染期读动画值，不触发重组）；
6. `modifier.then(用户 modifier)`（用户外层可覆盖）；
7. enabled 时追加 `clickable_with_source` + `ripple_with_shape`；
8. `start_restartable_group`（Skip 时 content 不执行，但新 modifier 经
   `set_skip_modifier` 应用到节点——因此 shape/elevation/border 等变化
   即使 Skip 也生效，不需要全部声明进 `changed()`）；
9. `set_current_node_focus_color(theme.primary)` 写入 desc 通道。

链顺序（内→外）：min → padding → 容器(background/border/clip) →
shadow(graphics_layer) → 用户 modifier → clickable/ripple。

- **尺寸变体**：`ButtonSize`（XSmall 32 / Small 40 / Medium 56 / Large 96 /
  XLarge 136，`.size(...)` 设置，默认 Small）。随尺寸联动：min-height =
  容器高、水平 padding 16/24/24/48/64、图标间距 8/8/8/12/16、
  Outlined 边框宽 1/1/1/2/3、建议图标尺寸 20/20/24/32/40
  （`icon_size()`）；`ButtonDefaults::content_padding_for(size, style)`。

- **内容 = 居中 Row**（对标 M3：图标/文字并排）；内容色经
  `WiniaTheme::content_color()` 下传——`Icon::tint(Auto)` 取按钮内容色
  （如 Filled 内图标自动 on_primary）。
- **图标间距**：内部 Row 自动应用 8dp（对标 IconLabelSpace；M3 需手动
  spacing，本框架自动）。
- **带图标 padding**：`ButtonDefaults::button_with_icon_content_padding()`
  （左 16/右 24）与 `text_button_with_icon_content_padding()`（左 12/右 16），
  对标 M3 同名常量——带图标按钮请传入，否则默认 24/8/24/8。
- **内容统一包在 Row 内**：`fill_max_width`/`weight` 等子节点在内部 Row
  上下文中解析（与 M3 内容即 Row 一致）；自定义排布可自建 Row 作为内容。

### 2.2 状态与取色

- 容器/内容色只区分 enabled/disabled（M3 `ButtonColors` 语义）；
  hover/press/focus 的视觉反馈由 ripple 状态层绘制，不叠加在背景色上。
- M3 token 配色（1.4.0）：Filled = Primary/OnPrimary；Elevated =
  SurfaceContainerLow/Primary；Tonal = SecondaryContainer/OnSecondaryContainer；
  Outlined = 透明/OnSurfaceVariant；Text = 透明/Primary（M3 实现如此，
  token 标注待修正）。
- 禁用：Filled/Elevated 容器 OnSurface@10%、Tonal 容器 OnSurface@12%、
  Outlined/Text 容器透明；内容 OnSurface(Variant)@38%。
- Outlined 边框：`OutlineVariant`（非 outline），disabled 为
  OutlineVariant @ DisabledContainerOpacity(0.1)；
  宽度随尺寸变体 1/1/1/2/3。

### 2.3 阴影（hover 升高动画）

- 阴影值由 `graphics_layer.shadow_elevation` 动态闭包驱动，180ms tween 平滑过渡。
  Filled 默认全 0；Elevated 对齐 M3 token：悬停 2 / 聚焦 1 / 按下 1 / rest 1 / disabled 0，
  移出后回落。graphics-layer 阴影使用 Skia 原生 ambient/spot 光源，默认颜色约为
  ambient 12.5% 黑（`0x20`）、spot 31% 黑（`0x50`）。这是为增强原生 Skia 阴影
  清晰度的微调；若视觉过重，可在 `modifier.rs` 将默认值恢复为 ambient 10%（`0x19`）/
  spot 25%（`0x40`）。阴影颜色也可通过 `Modifier::ambient_shadow_color` /
  `spot_shadow_color` 覆盖。
- 阴影形状跟随 `Button::shape`；全 0 阴影（Filled 默认）不创建图层。
- `graphics_layer` 只影响外观，不参与命中测试（语义已注释）。

### 2.4 波纹与裁剪

- `ripple_with_shape(interaction, contentColor, bounded, shape)`：显式容器形状，
  Outlined/Text 无 Background 元素时波纹仍按胶囊裁剪（不再回退矩形）。
- 通用 `ripple()` 不传 shape 时从链上最近的 Background/Border 推断。

### 2.5 焦点系统

- 焦点环：宽 3、完全外置（内侧距组件 2）、颜色 = 主题 primary（组合期经
  desc 通道捕获，渲染期 CompositionLocal 已退出）、180ms 淡入淡出
  （1.15× 大环收缩到贴合，组件中心锚点）。
- 焦点环形状：从 modifier 最近的 Background/Border/Clip 推断；
  Text 按钮显式 `clip(shape)` 兜底（否则回退矩形）。
- 键盘激活：Enter/Space 只触发**聚焦节点自身**的 `on_click`，不向祖先冒泡
  （对标 Compose clickable；与鼠标点击路径的冒泡行为有意不对称）。
- Tab/Shift+Tab 循环聚焦；方向键按“方向半平面 + 方向距离 + 垂直偏离×2”
  评分导航（候选为可见焦点节点的视觉中心，含 scroll 偏移）。
- IME：框架只做机械转发——节点声明 `ime_callback` 才开启输入法。
  Tab/方向键/点击聚焦/Escape 清焦四条路径都经 `apply_ime_for_focus` 同步。
- 文本字段按键：多行模式消费 Up/Down（按 `\n` 近似行移动，Shift 扩选区）；
  单行模式放行给方向键焦点导航。

### 2.6 动态尺寸（SizeValue）

- `content_padding` / `min_size` 支持 `&State<f32>` / `State<f32>` / 闭包；
  在 measure 期求值并注册 layout 依赖——动画只触发重测，不重组。
- 布局语义：min 约束提升 incoming min 且受 max 夹住（对标 `widthIn/heightIn`）。

### 2.7 性能要点

- Skip 判定 = style/enabled 参数相等 + modifier 参数相等；动画值走
  `graphics_layer`/layout 依赖，不经过重组。
- 波纹按时间在渲染期计算，无额外动画状态；按下/释放只在交互源上发事件。

## 3. 与 material3 1.4.0 的差距（未实现/近似）

### 3.1 组件与 API 形态

- **独立 composable**：M3 的 `ElevatedButton` / `FilledTonalButton` /
  `OutlinedButton` / `TextButton` 是独立函数；winia 用构造变体近似，签名与
  `Button` 相同（后续可拆成独立组件，API 不变）。
- **`IconButton` / `FilledIconButton` / `OutlinedIconButton` /
  `TextButton` icon-only 形态**：未实现（无独立 `Icon` 组件）。
- **`BorderStroke`**：仅 width + color；无 Brush/渐变/虚线。
- **`ButtonDefaults.buttonColors(...)` 逐状态参数**：winia 以
  `ButtonColors::new(4 色)` 覆盖；没有 M3 的 `containerColor/contentColor/
  disabledContainerColor/disabledContentColor` 命名参数形态。

### 3.2 主题与 Token

- **形状 token**：`CornerFull` 用 `Shape::Pill` 近似（短边一半圆角），
  无主题级 shape 系统（`MaterialTheme.shapes` 对应物未实现）。
- **字体**：M3 Button 强制 `LabelLarge`；winia 跟随 `ProvideTextStyle`，
  不自动应用 LabelLarge。
- **焦点指示器颜色**：M3 默认 secondary，winia 用主题 primary（有意选择，
  颜色经 desc 通道可扩展为 token 化）。
- **禁用色 alpha**：50% 近似（M3 12%/38%），项目统一约定，未逐 token 对齐。

### 3.3 交互与渲染

- **行移动**：多行 Up/Down 是 `\n` 分隔的行首近似，无布局感知的列保持；
  换行/软换行/溢出场景行为与真实文本编辑器不同。
- **波纹**：只有 bounded 波纹；无 unbounded（M3 某些组件用）、无
  hover 涟漪（M3 也仅在 press 触发，此项与 M3 一致）。
- **动画**：阴影/尺寸/内边距可动画；无容器色动画
  （M3 的 containerColor 默认也不做 crossfade，可接受）。
- **a11y**：无 semantics 树/无障碍标签（全框架缺口，非 Button 特有）。
- **命中测试**：`graphics_layer` 外观不参与命中（已注释为语义），
  3D 变换/透明度下的点击区域与视觉不一致属已知取舍。

### 3.4 测试缺口

- 键盘激活（Enter/Space 仅自身）、Tab/IME 同步、方向键导航均无
  app 级集成测试（现有覆盖为 helper/单元级）。
- 焦点环形状/颜色的渲染断言未接入 UI 测试框架。

## 4. 维护约定

- 新增 builder 参数：默认值放 `ButtonDefaults`，在 `build` 中经 modifier
  携带即可（不需要加进 `changed()`）；同步补 getter、单元测试与本文档。
- 修改视觉语义（颜色/形状/焦点）时保持“对标 M3 token 优先、近似需注释”原则。
- 涉及焦点/IME/键盘的分支改动，四条路径（Tab/方向键/点击/Escape）必须一起更新。
