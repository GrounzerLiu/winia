# Chip 组件

`Chip` 一个类型覆盖 material3 的七个可组合项：`winia/src/components/chip.rs`（测试在
`winia/src/components/chip/tests.rs`）。

## 1. Sources

Compose 路径相对
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/`。

| Compose | 定下了什么 |
| --- | --- |
| `Chip.kt:135`, `:215`, `:298` | `AssistChip` 三个重载——无内容色、部分内容色、全内容色。winia 用 `ChipColors` 一个类型承载 |
| `Chip.kt:377`, `:456`, `:538` | `ElevatedAssistChip` 同上三档 |
| `Chip.kt:635`, `:730`, `:826` | `FilterChip` 三档；`selected` 是第二个参数 |
| `Chip.kt:910`, `:1005`, `:1093` | `ElevatedFilterChip` 三档 |
| `Chip.kt:1184`, `:1298`, `:1420` | `InputChip` 三档；`avatar` 只在最低档上 |
| `Chip.kt:1528`, `:1604`, `:1683` | `SuggestionChip` 三档 |
| `Chip.kt:1761`, `:1836`, `:1914` | `ElevatedSuggestionChip` 三档 |
| `Chip.kt:1945` (`AssistChipDefaults`), `:2224` (`FilterChipDefaults`), `:2625` (`InputChipDefaults`), `:2904` (`SuggestionChipDefaults`) | 四个 defaults 对象；`ContentPadding = PaddingValues(horizontal = 8.dp)` 与 `HorizontalSpacing = 8.dp` 是 winia 的 padding 与 gap 规则的出处 |
| `Chip.kt:2054`, `:2201`, `:2390`, `:2565`, `:2806`, `:2985`, `:3125` | 七个 elevation 工厂（assist / elevated assist / filter / elevated filter / input / suggestion / elevated suggestion） |
| `Chip.kt:3586` (`ChipElevation`), `:3725` (`SelectableChipElevation`) | 两个 elevation 类的形状——非选择型只有 `default`，选择型还有 `selected` 那一列 |
| `Chip.kt:3869` (`ChipColors`), `:4012` (`SelectableChipColors`) | 两个 colours 类及其槽位 |
| `Chip.kt:4169` (`ChipBorder`) | `ChipBorder` 的字段（width + color） |
| `Chip.kt:3209`, `:3258`, `:3317` | `Role.Button`（非选择型）与 `Role.Checkbox`（可选择型）；`Surface(selected = …)` 在旁边把选中态一起发布 |
| `Surface.kt:225` | 三个 `onClick` 重载里的 `Box` 都挂 `Modifier.minimumInteractiveComponentSize()`——chip 的 48 dp 触控目标出处，且与 `enabled` 无关 |
| `InteractiveComponentSize.kt:71`, `:98`-`:140` | `minimumInteractiveComponentSize()` 的定义与 `MinimumInteractiveModifierNode.measure`：盒子取 `max(content, 48)`，内容居中 |
| `Chip.kt:3528` | `avatar != null -> avatar // An avatar takes precedence`——avatar 优先于 leading icon 的判断就这一行 |
| `Chip.kt:1205`-`:1215` | 禁用时把 avatar 包进 `graphicsLayer { alpha = … }` |
| `internal/Elevation.kt:54`-`:59` | `val spec = when { to != null -> incoming; from != null -> outgoing; else -> null }`——阴影动画按交互**配对**取 spec，只有两端都没有交互时才 snap。chip 此前把所有下降都写成 snap，就是与这里不符 |
| `tokens/ChipsTokens.kt` | `Height` (32 dp)、`LeadingIconSize` / `TrailingIconSize` (18 dp)、`AvatarSize` (24 dp)、`ContainerElevation` (Level0)、`DraggedContainerElevation` (Level4)、`DisabledLabelTextColor` / `DisabledLeadingIconColor` / `DisabledTrailingIconColor` |
| `tokens/AssistChipTokens.kt:27` | `ContainerShape = ShapeKeyTokens.CornerSmall`——8 dp 圆角 |
| `tokens/FilterChipTokens.kt` | `FlatSelectedContainerColor`、`FlatUnselectedOutlineColor`、`FlatUnselectedOutlineWidth`、`SelectedLeadingIconColor` 等选择型槽位 |
| `tokens/InputChipTokens.kt` | `SelectedLeadingIconColor`（选中时前导 icon 仍是 Primary）、`AvatarSize`、`AvatarShape`、`DisabledAvatarOpacity = 0.38f` |
| `tokens/ElevationTokens.kt` | `Level0` 0 / `Level1` 1 / `Level2` 3 / `Level4` 8 dp |

数值层面 winia 的 `ChipDefaults`（`chip.rs:201`-`240`）一一对应：`HEIGHT = 32.0`、
`CORNER_RADIUS = 8.0`、`PADDING_NO_ICON = 16.0`、`PADDING_WITH_ICON = 8.0`、
`INPUT_AVATAR_LEFT_PADDING = 4.0`、`ELEMENT_GAP = 8.0`、`BORDER_WIDTH = 1.0`、
`AVATAR_SIZE = 24.0`、`AVATAR_CORNER_RADIUS = 12.0`、`DISABLED_OUTLINE_OPACITY = 0.12`、
`DISABLED_SELECTED_CONTAINER_OPACITY = 0.12`、`DISABLED_AVATAR_OPACITY = 0.38`、
`CONTAINER_FADE_MILLIS = 180`，以及私有的 `SHADOW_OPAQUE_ALPHA = 250`（winia 自己的判据，
Compose 没有对应物，见 §5）。

## 2. Variants

| Constructor | 高度 | 何时用 |
| --- | --- | --- |
| `Chip::assist(label, on_click)` | 32 dp | 触发一个与内容相关的辅助动作；容器透明 + 1 dp outline |
| `Chip::elevated_assist(label, on_click)` | 32 dp | 同上，但需要从背景里抬起来——容器 `SurfaceContainerLow`、无 outline |
| `Chip::filter(selected, label, on_click)` | 32 dp | 多选筛选；`selected` 切换容器填充并去掉 outline |
| `Chip::elevated_filter(selected, label, on_click)` | 32 dp | 同上，抬升版 |
| `Chip::input(selected, label, on_click)` | 32 dp | 一条用户输入（实体、属性）；可带 `avatar` 与关闭用的 `trailing_icon` |
| `Chip::suggestion(label, on_click)` | 32 dp | 给用户的动态建议；文字是 `OnSurfaceVariant` 而非 `OnSurface` |
| `Chip::elevated_suggestion(label, on_click)` | 32 dp | 同上，抬升版 |

七个共享同一组 setter：

```rust
Chip::filter(selected, label, on_click)
    .modifier(Modifier)                              // 最后追加
    .enabled(bool)
    .shape(Shape)                                    // 默认 8dp 圆角，与 Compose 同值
    .elevation(ChipElevation)                        // 覆盖变体默认的阴影高度
    .border(ChipBorder)                              // ChipBorder::none() = Compose 的 border = null
    .horizontal_arrangement(f32)                     // 子元素间距，默认 ChipDefaults::ELEMENT_GAP
    .interaction_source(MutableInteractionSource)    // 调用方提供交互源，否则组件自己 remember 一个
    .colors(ChipColors)                              // 非选择型（assist / suggestion）
    .selectable_colors(SelectableChipColors)         // 选择型（filter / input）
    .leading_icon(|ctx| { ... })                     // 前置图标，18dp
    .trailing_icon(|ctx| { ... })                    // 后置图标
    .avatar(|ctx| { ... })                           // 头像，24dp / 12dp 圆角，优先于 leading icon
    .icon(|ctx| { ... })                             // suggestion 的 icon，是 leading_icon 的别名
    .build(ctx);
```

**受控语义**：`selected` 由调用方持有，`on_click` 只通知，切换由调用方 `State.update` 完成——与
Compose 的 `onClick` 一致。chip 自己不持有任何状态。

**没有 content 闭包**：label 是第一个参数。`label` / `leading_icon` / `trailing_icon` / `avatar`
四个闭包由调用方提供并原样交给主题 scope，这正是 chip 不标 `#[composable]` 的理由（§4 的「不宏化」，
以及 `docs/key-system-design.md` §7 的排除名单）。

**两个 colours 类都有 `copy(..)`**，`None` 表示沿用原值（§5 说明这与 Compose 的载体不同）。

## 3. Layout

| 项 | 值 |
| --- | --- |
| 容器高 | 32 dp（`ChipsTokens.Height`） |
| 形状 | 8 dp 圆角（`ChipDefaults::shape()`，`AssistChipTokens.ContainerShape` = CornerSmall） |
| icon 尺寸 | 18 dp（`LeadingIconSize` / `TrailingIconSize`）；尺寸由调用方的闭包子节点决定 |
| avatar | 24 dp、12 dp 圆角（`AvatarSize` / `AvatarShape`——specs 明确是圆角方块，不是圆） |
| 左右 padding | 无 icon 16 dp；有 icon 8 dp；InputChip 带头像时左侧 **4 dp** |
| 元素间距 | 8 dp（`ELEMENT_GAP`，可用 `.horizontal_arrangement(..)` 覆盖） |
| 边框 | 1 dp outline（无填充时）；selected 有填充时 **0 边框——不挂 border modifier**；禁用态改画 `OnSurface @12%`；elevated 变体默认无边框 |

padding 是**按边**算的，不是一对数字：`InputChip` 左边看 avatar（4）→ leading icon（8）→ 都没有（16），
右边只看 trailing icon（8 / 16）；其余变体看任一侧有没有 icon，有就两边都是 8，否则两边都是 16。
两侧分别用 `padding_start` / `padding_end` 挂到链上，所以 RTL 下自动镜像，不需要额外的方向分支。

内部结构是一个 `Row`（`spacing(gap)` + `Alignment::Center`）：avatar **或** leading icon → label →
trailing。交叉轴必须显式 `Center`——`Row` 默认 `Alignment::Start` 会顶对齐，有 18 dp 图标时文字就不居中了。

**48 dp 触控目标**：chip 的节点是**独立的一层 wrapper**，32 dp 的容器是它内层的视觉盒——与 Button / Card
同一个形状。Compose 的七个变体都建立在 `Surface(onClick = …)` 上，该重载的 `Box` 无条件挂
`Modifier.minimumInteractiveComponentSize()`（`Surface.kt:225`），所以 clickable 与 disabled 的 chip 都有
48 dp 的可点区域，winia 此前只有 32 dp。用 wrapper 而不是往 chip 自己的链上加一个元素，是因为渲染器把撑大
后的节点的绘制缩回 `minimum_interactive_content`，而该字段只在链上带**成对的固定尺寸**时才发布
（`measure/engine.rs:1380-1385`）——chip 的宽度随标签，`fixed_size()` 是 `None`，什么都不会发布，
`paint_rect` 于是取整个节点，容器会被画成 48 dp 的色块。wrapper 还接走调用方 modifier 里的 parent-data 那半
（`split_parent_data`），否则 `Row::weight` 落深一层、带权重的 chip 会塌回触控目标的大小。

## 4. Behaviour

### 配色（`ChipDefaults`）

颜色角色以 M3 specs 页面（<https://m3.material.io/components/chips/specs>）各变体的 "color roles" 列表为准。

**AssistChip**（`assist_chip_colors()`）——边框样式：

| 状态 | 容器 | 文字 | 前导 icon | 后置 icon |
|---|---|---|---|---|
| enabled | transparent | **onSurface** | primary | onSurfaceVariant |
| disabled | transparent | onSurface@38% | onSurface@38% | onSurface@38% |

**SuggestionChip**（`suggestion_chip_colors()`）：同 assist，但文字是 **onSurfaceVariant**（specs 给
suggestion 的角色）。

**FilterChip**（`selectable_chip_colors()`）：

| 状态 | 容器 | 文字 | 前导 icon | 后置 icon | 边框 |
|---|---|---|---|---|---|
| unselected | transparent | onSurfaceVariant | primary | onSurfaceVariant | 1dp **outlineVariant** |
| selected | secondaryContainer | onSecondaryContainer | onSecondaryContainer | onSecondaryContainer | 无 |
| disabled 未选中 | transparent | onSurface@38% | onSurface@38% | onSurface@38% | 1dp `OnSurface@12%` |
| disabled 选中 | **onSurface@12%** | onSurface@38% | onSurface@38% | onSurface@38% | 无 |

**InputChip**（`input_chip_colors()`）：与 FilterChip 相同，**唯一差别**是选中时前导 icon 保持
**primary**（specs 把 input chip 的前导 icon 定为 Primary 角色；`InputChipTokens.SelectedLeadingIconColor` 同）。

**leading 与 trailing 用各自的色位**，不是同一个 `icon_color`：assist 是 primary / onSurfaceVariant。
三个 `trailing_icon` 槽位此前被 `ChipDefaults` 写好却从没被读过，修好后才有上面这一列。

**四个 disabled icon 槽位是独立的**：两个 colours 类各有 `disabled_leading_icon` /
`disabled_trailing_icon`，默认仍是 `OnSurface` @ 38 %（M3 的 `DisabledLeadingIconColor` /
`DisabledTrailingIconColor`），所以像素不变，变的是调用方能单独覆盖——此前禁用色的两个图标只能
跟着 label 走，一个都改不了。

### 容器色渐变

`animate_color_as_state`（180 ms `EaseOutCubic`）在 selected / unselected / disabled 之间混合容器色。
这是 winia 的新增（§5）。注意 `background` 接受动态闭包，必须传 `move || bg.get()` 而不是
`bg.get()` 的值——否则冻结在 build 时，动画不显示。

### 边框消失

`border_width > 0` 才挂 border modifier——`border(0.0)` 仍会绘制 0 宽线（可见伪影），所以 selected
时直接不挂，elevated 变体默认也不挂。

### 阴影

`graphics_layer` 的动态闭包在渲染期读动画值（不触发重组），形状跟随 chip shape。数值（dp）：

| 变体 | 默认 / 按下 / 聚焦 / hover / 拖动 / 禁用 |
|---|---|
| assist / suggestion / input（flat） | 0 / 0 / 0 / 0 / **8** / 0 |
| filter（flat） | 0 / 0 / 0 / **1** / **8** / 0 |
| 三个 Elevated 变体 | **1** / 1 / 1 / **3** / **8** / **0** |

数值取自 token 层级（`ElevatedContainerElevation` = Level1、`ElevatedHoverContainerElevation` =
Level2、`DraggedContainerElevation` = Level4 = 8 dp）；`Level2 = 3dp`。

**只有容器不透明时才画**（`ChipElevation::shadow_for(state, container_alpha, elevation)`）——透明
背景后面的阴影看起来是一块脏印，而不是抬升。判据是 `container_alpha < SHADOW_OPAQUE_ALPHA`（250），
所以上面表里平铺 assist / suggestion 和未选中的 filter 实际永远不显示阴影，这是有意行为（§5）。

判定读的是容器**动画色**的 alpha，过渡期因此自然正确：变不透明时等颜色到位才出现，变透明时阴影立即消失。

**两种下降要分开**。`ChipElevation::shadow_is_gated(container_alpha)` 持同一套阈值，`shadow_for` 也
建立在它之上，所以阈值只写一处：

- 容器变透明压下来的阴影**必须当帧归零**——它托着的那块填充已经在走了，悬空的抬升就是一块脏印；
- 交互结束（hover / focus / press 离开）压下来的阴影走 Compose 的 `elevation_spec_for_pair`，
  与 Button / Card 同一套配对规则（进入 120 ms `fast_out_slow_in`，离开 120 / 150 ms outgoing）。

此前 `wanted <= shown` 一刀切成 `Snap`，结果是 chip 的 hover 离开是硬切、旁边的 Button 却是淡出。
回归：`a_hover_lift_ramps_in_and_fades_out` 与 `a_container_going_transparent_drops_the_shadow_at_once`
各管一半。

### 变体与内容

- **变体差异**（`ChipVariant` 分派）：Assist / Suggestion 走 `ChipColors`（非选择）；
  Filter / Input 走 `SelectableChipColors`（selected 双态）+ selected 0 边框；
  Elevated* 三个变体容器 `SurfaceContainerLow` 且默认无边框。
- **内部结构**：容器（背景 / 边框 / 阴影 / clickable / ripple）→ `Row`（avatar **或** leading icon →
  label → trailing，间距 8）→ 内容经 `WiniaTheme::with_content_color` 下传各元素颜色（`Icon` tint Auto
  跟随）。
- **avatar 优先于 leading icon**：两者同时给时只画 avatar，对标 `Chip.kt:3528`
  `avatar != null -> avatar // An avatar takes precedence`（此前两个都画，实测图标数 2）。
- label 默认通过 `ProvideTextStyle(WiniaTheme::typography().label_large)` 下传 Material 3 LabelLarge
  （14sp / 20sp / Medium / 0.1sp），状态色只覆盖其 color。

### 内容色的参数声明

`build_chip` 里三路内容色（label / leading / trailing）在 group 之前用 `ctx.changed` 声明，**且三条
声明放在 `next_key()` 之前**（与 `split_button.rs:281` 同序）；两个图标色还要在内容闭包内、`Row`
启动**之前**再声明一次——`Row` 是它自己的 restartable group，只比较 spacing / alignment，参数不变就
Skip，其 content 不执行，`with_content_color` 根本没被调用。实测（只让 `leading_icon` 变色、三帧计数
闭包）：修复前闭包 3 帧只跑 1 次、颜色恒为 `101,85,143`；补齐后 `seen=["101,143","200,20"]`。

这是 `docs/component-authoring.md` §3.2 的 caller 侧陷阱，只是这次发生在组件内部。

### 不宏化

chip 不标 `#[composable]`，理由是它是**透传 caller 闭包的容器**（`label` / `leading` / `trailing` /
`avatar` 原样交给主题 scope，闭包顶层 `State.get()` 必须注册到父容器才刷新），这条记在
`docs/key-system-design.md` §7 的排除名单里。**不是**因为「与 Button 一致」——Button 是
`#[composable]` 的；依赖注册这里靠上面的 `ctx.changed` 显式声明。

### 语义（无障碍）

每个 chip 是**一个**无障碍元素（label 与 icon 都在里面，`merge_descendants(true)`），role 按 material3：

| 变体 | role | state |
|---|---|---|
| Assist / ElevatedAssist / Suggestion / ElevatedSuggestion | `Role.Button`（`Chip.kt:3209`） | `enabled` |
| Filter / ElevatedFilter / Input | `Role.Checkbox`（`Chip.kt:3258`、`:3317`） | `enabled` + **`selected`**（true / false 都发声，不是「沉默」） |

调试通道 `sem` 与语义树里可直接读到：`role: "checkbox"` + `state.selected`。

## 5. Differences from Compose

不是 bug，是 winia 的判断或还没建的东西。这里保持沉默就等于声称一致。

- **容器色是渐变过去的，M3 直接赋值。** `animateColorAsState` 在 M3 的 `Chip.kt` 里不存在，容器色是
  当帧应用的；winia 用 180 ms `EaseOutCubic` 过渡 selected / unselected / disabled 之间。代价是所有
  「容器透明就不画阴影」的判断必须读动画色而不是目标色——这恰好给出了过渡期的正确行为（§4）。
- **「容器透明就不画阴影」是 winia 的规则，M3 没有。** Compose 的 chip 在平铺变体上也有 elevation
  token，只是默认值都是 Level0 所以看不出来；winia 把它写成了显式判据（`SHADOW_OPAQUE_ALPHA = 250`），
  理由是透明背景后面的阴影读作脏印而不是抬升。这条规则的副作用是选中态切换时阴影会随容器色进出。
- **阴影下降的 spec 分两种情况，M3 只按交互配对分。** `internal/Elevation.kt:54-59` 的 `when` 只看
  `from` / `to` 有没有交互；winia 在其上多一层 alpha gate：容器变透明导致的下降当帧归零（§4）。
  M3 的 chip 容器本来透明，没有这个问题可解，所以这是 winia 为「容器会渐变成透明」付的账。
- **`copy(..)` 用 `Option<Color>` 承载「沿用」，M3 用 `Color.Unspecified` 哨兵值。** winia 的 `Color`
  没有 unspecified 变体，所以每个槽位都是 `Option`，`None` 表示「保留原值」——契约相同，载体不同，
  好处是全 `None` 的 copy 是恒等。
- **没有 hover / focus / press 状态色。** Compose 的 `ChipColors` / `SelectableChipColors` 有
  hover / focus / drag 状态槽位，winia 只有 enabled / disabled / selected（ripple 已覆盖 press 视觉；
  `Chip.kt` 的默认工厂同样只用这两态，所以默认观感一致，差别在调用方能不能单独覆盖）。
- **`contentPadding` 没有开放成参数。** 16 / 8 / 4 写死按 specs 规则算（§3），
  `horizontalArrangement` 已开放为 `.horizontal_arrangement(f32)`。
- **没有 `Elevation*` 之外的自定义 elevation 重载。** Compose 的七个 `*Elevation(...)` 工厂接受逐状态
  的 dp 与动画 spec；winia 用 `ChipElevation`（六个数字）+ `.elevation(..)` 覆盖，走的是同一套
  `elevation_spec_for_pair`，不能换 spec。

## 6. Tests

```bash
# Every unit test of this component (25, all in chip/tests.rs)
cargo test -p winia --lib components::chip
```

| Test | What it pins |
| --- | --- |
| `a_trailing_icon_takes_the_trailing_role` | the trailing glyph takes `On surface variant`, not the leading `Primary` — the two used to share one colour slot |
| `moving_only_an_icon_colour_recolours_the_glyph` | a colour that moves reaches the glyph: declared on the chip's group AND before the inner `Row` starts |
| `two_modifier_calls_append_to_the_chain` | the second `.modifier(..)` appends instead of replacing the first |
| `an_avatar_wins_over_a_leading_icon` | avatar + leading icon together paint ONE leading slot (the avatar) |
| `the_disabled_icon_slots_are_separate_from_the_label` | the two disabled icon colours are independent slots, not the label's colour |
| `the_disabled_icons_default_to_the_on_surface_role` | the defaults stay at M3's disabled `OnSurface` role, so the pixels do not move |
| `copy_replaces_exactly_the_slots_it_is_given` | `ChipColors::copy(..)` replaces what it is given and keeps every `None` |
| `selectable_copy_replaces_exactly_the_slots_it_is_given` | the same contract on `SelectableChipColors` |
| `a_callers_border_overrides_the_variants_own` | `.border(..)` wins over the variant, and `ChipBorder::none()` detaches it |
| `a_border_can_be_added_to_a_variant_that_has_none` | an elevated chip can be given one |
| `horizontal_arrangement_changes_the_laid_out_gap` | the spacing moves the laid-out width by exactly the difference |
| `a_disabled_input_chips_avatar_is_dimmed` | a disabled input chip's avatar is drawn at `DISABLED_AVATAR_OPACITY` |
| `the_padding_is_the_specs_rule` | 16 / 8 / 4 per side: the avatar-or-icon rule on the left, the trailing icon on the right |
| `the_outline_role_follows_the_variant` | a flat chip is outlined and an elevated one is not; `selected` detaches it |
| `elevated_chips_fill_and_drop_the_outline` | the elevated variants paint `SurfaceContainerLow` (or `SecondaryContainer` when selected) |
| `assist_and_suggestion_labels_are_different_roles` | `OnSurface` vs `OnSurfaceVariant` — the one thing that separates the two transparent variants |
| `an_input_chips_selected_leading_icon_stays_primary` | the input chip keeps `Primary` on its leading icon when selected, unlike the filter chip |
| `label_uses_typography_label_large` | the label inherits M3 LabelLarge through `ProvideTextStyle` |
| `the_variants_declare_the_roles_material3_gives_them` | `Role.Button` for the four non-selectable variants, `Role.Checkbox` + `selected` for the three selectable ones |
| `a_disabled_chip_outlines_with_on_surface_at_twelve_percent` | the disabled outline is `OnSurface @12%`, not the enabled role's colour |
| `a_disabled_selected_chip_fills_with_on_surface_at_twelve_percent` | a disabled SELECTED chip keeps a 12% fill |
| `the_elevation_sets_are_the_tokens_in_dp` | `flat` / `hover_lifting` / `elevated` are the token LEVELS in dp (`Level2 = 3`, not 2) |
| `a_transparent_container_never_shadows` | the alpha gate: transparent shows nothing, opaque shows the token's value |
| `a_hover_lift_ramps_in_and_fades_out` | hover fades out over the outgoing spec instead of cutting, like `Button` and `Card` |
| `a_container_going_transparent_drops_the_shadow_at_once` | the gate outranks the outgoing spec: a container fade steps straight to zero |
| `a_chip_reserves_a_48_dp_touch_target` | the node a parent measures is 48 dp on both axes while the box that paints the container is still 32 — and a disabled chip reserves it too |

Fixtures: **no** `fixture_all` scenario, so the UI test suite never reaches this component. Its own
page is `examples/chip_demo.rs`; `examples/animated_visibility_demo.rs` and
`examples/animated_content_demo.rs` also build chips, as the content they animate.

`chip_demo.rs` walks the seven variants, the `selected` state, the three slots (leading icon,
trailing icon, avatar — including the avatar-over-leading rule), the border / gap / colour overrides,
the shadow rule, and a hoisted interaction source with a disabled row.
