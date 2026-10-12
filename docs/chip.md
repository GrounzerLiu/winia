# Chip 组件

> 分支：`v2`（对齐 material3 Chip 家族）
> 参考：material3 1.4.0 `Chip.kt` + `tokens/{Assist,Filter,Input,Suggestion}ChipTokens.kt`

## 1. API

```rust
// 统一入口 Chip + 变体构造（对标 material3 的七个可组合项）
Chip::assist(label, on_click)                       // AssistChip（可选 leading/trailing icon）
Chip::elevated_assist(label, on_click)              // ElevatedAssistChip
Chip::filter(selected, label, on_click)             // FilterChip（selected 切换）
Chip::elevated_filter(selected, label, on_click)    // ElevatedFilterChip
Chip::input(selected, label, on_click)              // InputChip（leading/avatar/trailing 关闭）
Chip::suggestion(label, on_click)                   // SuggestionChip（可选 icon）
Chip::elevated_suggestion(label, on_click)          // ElevatedSuggestionChip

// 通用 setter
    .modifier(Modifier)
    .enabled(bool)
    .shape(Shape)                                    // 默认 8dp 圆角
    .colors(ChipColors)                              // 非选择型配色（assist/suggestion）
    .selectable_colors(SelectableChipColors)         // 选择型配色（filter/input）
    .elevation(ChipElevation)                        // None = 变体 token 默认（ChipElevation?）
    .border(ChipBorder)                              // None = 变体默认；ChipBorder::none() = 无边框
    .interaction_source(MutableInteractionSource)    // 对标 interactionSource 参数
    .leading_icon(|ctx| { ... })                     // 前置图标（18dp）
    .trailing_icon(|ctx| { ... })                    // 后置图标
    .avatar(|ctx| { ... })                           // 头像（input 专用，24dp 圆形）
    .icon(|ctx| { ... })                             // 建议图标（suggestion 的 icon = leading 别名）
    .build(ctx);
```

- 受控语义：`selected` 由调用方持有；`on_click` 只通知（切换由调用方 `State.update`，与 Compose 一致）。
- 无 content 闭包——label 是第一个参数。

## 2. 结构与默认值（token 数值，dp）

| 项 | 值 | 出处 |
|---|---|---|
| 容器高 | 32dp | `ContainerHeight` |
| 触摸目标 | 48dp | `Surface(onClick)` 的 `minimumInteractiveComponentSize()`（`Surface.kt:219`） |
| 形状 | 8dp 圆角 | `ContainerShape = CornerSmall` |
| icon 尺寸 | 18dp | `IconSize` / `LeadingIconSize` |
| avatar | 24dp、**CornerFull（圆形）**、禁用时 38% 透明 | `AvatarSize` / `AvatarShape` / `DisabledAvatarOpacity` |
| 左右 padding | assist/filter/suggestion：8dp；input：`start = 有 avatar 或无 leading ? 4 : 8`，`end = 有 trailing ? 8 : 4` | `HorizontalElementsPadding` + `inputChipPadding`（`Chip.kt:2944-2952`） |
| 元素间距 | 8dp | 同上 |
| 边框 | 未选中 1dp `OutlineVariant`；**选中 0 宽（不挂 border）**；禁用未选中 1dp `OnSurface@12%` | `FlatOutlineColor` / `FlatUnselectedOutlineWidth` / `FlatDisabledUnselectedOutlineOpacity` |

### 配色（`ChipDefaults`）

**AssistChip**（`assistChipColors()`）：容器 transparent、标签 **`OnSurface`**、前导图标 `Primary`、后置图标 `OnSurfaceVariant`；禁用容器 transparent、标签 `OnSurface@38%`。

**SuggestionChip**（`suggestionChipColors()`）：同 assist，但标签是 `OnSurfaceVariant`（`SuggestionChipTokens.LabelTextColor`）。

**Elevated assist / suggestion**：容器 `SurfaceContainerLow`，禁用容器 `OnSurface@12%`，其余同上，且**默认无边框**（`border = null`）。

**FilterChip**（`filterChipColors()`）：

| 状态 | 容器 | 标签 | 前导 icon | 后置 icon |
|---|---|---|---|---|
| unselected | transparent | `OnSurfaceVariant` | `Primary` | `OnSurfaceVariant` |
| selected | `SecondaryContainer` | `OnSecondaryContainer` | `OnSecondaryContainer` | `OnSecondaryContainer` |
| disabled 未选中 | transparent | `OnSurface@38%` | `OnSurface@38%` | `OnSurface@38%` |
| disabled 选中 | `OnSurface@12%` | `OnSurface@38%` | `OnSurface@38%` | `OnSurface@38%` |

**ElevatedFilterChip**：未选中容器改为 `SurfaceContainerLow`。

**InputChip**（`inputChipColors()`）：与 FilterChip 相同，**只有一处不同**——选中时前导图标保持 `Primary`（`InputChipTokens.SelectedLeadingIconColor`）。

### 阴影（`ChipElevation`，dp）

| 变体 | 默认 / 按下 / 聚焦 / hover / 拖动 / 禁用 |
|---|---|
| assist / suggestion / input（flat） | 0 / 0 / 0 / 0 / **8** / 0 |
| filter（flat） | 0 / 0 / 0 / **1（仅选中态）** / **8** / 0 |
| 三个 Elevated 变体 | **1** / 1 / 1 / **3** / **8** / **0** |

filter 的 hover 分档：`FlatSelectedHoverContainerElevation = Level1`、`FlatUnselectedHoverContainerElevation = Level0`——**未选中时不抬升**。未选中的 filter chip 容器是透明的，抬到 1dp 会让阴影从透明背景后面透出来（实机反馈："变成未选中背景透明会漏出阴影"）。material3 自己的 `filterChipElevation()` 两个状态都用选中值，这里按 token 对分开，属于有意偏差。

拖动恒为 `DraggedContainerElevation = Level4 = 8dp`（这也是为什么平铺 chip 也保留图层）。升落用 `animation::elevation_spec_for_pair`——与 Button/Card 同一套交互配对规则。

### 语义

- Assist / Suggestion / Elevated 系列：`Role.Button`；
- Filter / Input / ElevatedFilter：`Role.Checkbox` + `selected` 状态（`Chip.kt:1963` / `:2010`）。

## 3. 实现细节

- **统一容器**：`build_chip`（内部）——外层 48dp 触摸目标 → 内层 32dp 容器（背景/边框/阴影/
  clickable/ripple）→ `Row`（前导 → 标签 → 后置，间距 8dp，显式 `Alignment::Center`）。
- **Container colour fade**: `animate_color_as_state`（180ms EaseOutCubic）在
  selected/unselected/disabled 之间渐变容器色。这是 winia 的补充——M3 的 `Chip.kt` 直接取色，
  没有 `animateColorAsState`。`background` 接受动态闭包（`impl Fn() -> Color`），必须传
  `move || bg.get()` 而非取值，否则冻结在 build 时。
- **阴影与容器色同步**（`Chip::build` 的 gate）：容器色渐变期间**不显示新升起的阴影**——
  背景还是半透明时阴影会从后面透出来（"两个状态过渡过程中可以砍刀"）。规则：
  - 变透明（选中 → 未选中）：阴影**立即**归零（`Snap`），不等颜色渐变结束；
  - 变不透明（未选中 → 选中）：等**容器色真正到位**（逐通道比较动画色与目标色，容差 1）后才升阴影。

  判定用的是动画色本身而非动画注册表：gate 只依赖"当前画的是什么"，并在颜色落位的那一帧自动放开（
  build 里读了动画色，因此每个颜色帧都会重跑一次 build）。
- **边框**：`ChipBorder::resolve(enabled, selected)` 返回 `Option<(宽度, 颜色)>`——宽度为 0 时
  **不挂** border modifier（挂 0 宽线仍会绘制，可见）。
- **阴影**：`graphics_layer` 动态闭包读动画值（渲染期求值，不触发重组），形状跟随 chip shape。
  升起用 `elevation_spec_for_pair`（与 Button/Card 同一套交互配对规则），落下用 `Snap`。
- **label 排版**：`ProvideTextStyle(WiniaTheme::typography().label_large)`（M3 LabelLarge
  14sp/20sp/Medium/0.1sp），状态色只覆盖其 color。
- **宏化判据**：未宏化（与 Column/Row/Button 一致——content 闭包 + 内部 scope 会拦截依赖注册）。

## 4. 已知偏差 / 限制

- **48dp 目标的边缘不是点击区**：clickable/ripple 留在 32dp 视觉节点上，外层 48dp 只提供占位
  （`fire_click_along_path` 只认带 `on_click` 的节点）。把 clickable 移到 48dp 节点会让波纹被裁成
  48dp 圆角矩形——每次按压都比 chip 高 8dp 的可见回归，所以先保留现状；要真正扩大命中区需要
  "波纹留在视觉节点、命中区在外层"的框架支持。
- **hover/focus/drag 配色**：`ChipColors` 只有 enabled/selected 两态（M3 的同名类同样只有这两态，
  token 里的 `HoverLabelTextColor` 等未被 `Chip.kt` 使用），故与 Compose 一致；ripple 覆盖按压视觉。
- **icon 出现/消失动画**：M3 的 `AnimatingChipContent` 会在 leading/trailing icon 从无到有时做过渡，
  winia 是瞬时出现。
- **contentPadding 定制**：写死上面的 token 规则，未开放成参数（Compose 也没有公开参数）。
