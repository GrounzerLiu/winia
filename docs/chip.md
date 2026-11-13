# Chip 组件

> 分支：`v2`（对齐 material3 Chip 家族）
> 参考：M3 specs（m3.material.io/components/chips/specs）+ Jetpack Compose material3 `Chip.kt`

## 1. API

```rust
// 统一入口 Chip + 变体构造（对标 material3 七个可组合项）
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
    .elevation(ChipElevation)                        // 覆盖变体默认的阴影高度
    .border(ChipBorder)                              // 覆盖变体边框（ChipBorder::none() = Compose 的 border = null）
    .horizontal_arrangement(f32)                     // 子元素间距（默认 ChipDefaults::ELEMENT_GAP = 8dp）
    .interaction_source(MutableInteractionSource)    // 调用方提供交互源（否则组件自己 remember 一个）
    .colors(ChipColors)                              // 非选择型配色（assist/suggestion）
    .selectable_colors(SelectableChipColors)         // 选择型配色（filter/input）
    .leading_icon(|ctx| { ... })                     // 前置图标（18dp）
    .trailing_icon(|ctx| { ... })                    // 后置图标
    .avatar(|ctx| { ... })                           // 头像（input 专用，24dp 12dp 圆角；优先于 leading）
    .icon(|ctx| { ... })                             // 建议图标（suggestion 的 icon = leading 别名）
    .build(ctx);
```

- 受控语义：`selected` 由调用方持有；`on_click` 只通知（切换由调用方 `State.update`，
  与 Compose 一致）。
- 无 content 闭包——label 是第一个参数。
- 两个 colors 类都有 `copy(..)`（`None` = 沿用原值，见 §5）。

## 2. 结构与默认值（M3 specs）

| 项 | 值 |
|---|---|
| 容器高 | 32dp |
| 形状 | 8dp 圆角（`ChipDefaults::shape()`） |
| icon 尺寸 | 18dp（`IconSource` 闭包子节点，尺寸由调用方控制） |
| avatar | 24dp、12dp 圆角（InputChip——specs 明确是圆角方块，不是圆） |
| 左右 padding | 无 icon 16dp / 有 icon 8dp；InputChip 带头像时左侧 **4dp** |
| 元素间距 | 8dp |
| 边框 | 1dp outline（无填充时）；**有填充（selected）0 边框——不挂 border modifier**；禁用态改画 `OnSurface @12%`（`FlatDisabledOutlineOpacity` 等），elevated 变体默认无边框 |

### 配色（`ChipDefaults`）

颜色角色以 M3 specs 页面（<https://m3.material.io/components/chips/specs>）各变体的 "color roles" 列表为准。

**AssistChip**（`assist_chip_colors()`）——边框样式：
| 状态 | 容器 | 文字 | 前导 icon | 后置 icon |
|---|---|---|---|---|
| enabled | transparent | **onSurface** | primary | onSurfaceVariant |
| disabled | transparent | onSurface@38% | onSurface@38% | onSurface@38% |

**SuggestionChip**（`suggestion_chip_colors()`）：同 assist，但文字是 **onSurfaceVariant**（specs 给 suggestion 的角色）。

**FilterChip**（`selectable_chip_colors()`）：
| 状态 | 容器 | 文字 | 前导 icon | 后置 icon | 边框 |
|---|---|---|---|---|---|
| unselected | transparent | onSurfaceVariant | primary | onSurfaceVariant | 1dp **outlineVariant** |
| selected | secondaryContainer | onSecondaryContainer | onSecondaryContainer | onSecondaryContainer | 无 |
| disabled 未选中 | transparent | onSurface@38% | onSurface@38% | onSurface@38% | 1dp `OnSurface@12%` |
| disabled 选中 | **onSurface@12%** | onSurface@38% | onSurface@38% | onSurface@38% | 无 |

**InputChip**（`input_chip_colors()`）：与 FilterChip 相同，**唯一差别**是选中时前导 icon 保持 **primary**（specs 把 input chip 的前导 icon 定为 Primary 角色；`InputChipTokens.SelectedLeadingIconColor` 同）。

## 3. 实现细节

- **统一容器**：`build_chip`（内部）——32dp 高 + shape 背景 + 条件边框 +
  clickable/ripple + 内容垂直居中（`Row` 显式 `Alignment::Center`——默认 Start
  会顶对齐，有 icon 时文字不居中）。
- **Container colour fade**: `animate_color_as_state` (180 ms EaseOutCubic) blends the container
  colour across selected/unselected/disabled. That fade is a winia addition — M3's `Chip.kt` applies
  the container colour directly and has no `animateColorAsState`. Note `background` takes a dynamic
  closure
  （`impl Fn() -> Color`），必须传 `move || bg.get()` 而非 `bg.get()` 的值——
  否则冻结在 build 时（动画不显示）；渲染期每帧求值。
- **边框消失**：`border_width > 0` 才挂 border modifier——`border(0.0)` 仍会
  绘制 0 宽线（可见伪影），selected 时直接不挂；elevated 变体默认也不挂。
- **阴影**：`graphics_layer` 动态闭包读动画值（渲染期求值，不触发重组），形状跟随 chip shape。
  升高用 `elevation_spec_for_pair`（与 Button/Card 同一套交互配对规则），回落用 `Snap`。
  **只有容器不透明时才画**（`ChipElevation::shadow_for(state, container_alpha, elevation)`）——
  透明背景后面的阴影看起来像一块脏印。判定读的是容器**动画色**的 alpha，因此过渡期行为自然正确：
  变透明时阴影立即消失，变不透明时等颜色到位才出现。
- **变体差异**（`ChipVariant` 分派）：Assist/Suggestion 走 ChipColors（非选择）；
  Filter/Input 走 SelectableChipColors（selected 双态）+ selected 0 边框；
  Elevated* 三个变体容器 `SurfaceContainerLow` 且默认无边框。
- **内部结构**：容器（背景/边框/阴影/clickable/ripple）→ Row（avatar **或** leading icon →
  label → trailing，间距 8）→ 内容经 `WiniaTheme::with_content_color` 下传各元素颜色（Icon tint Auto
  跟随）。**leading 与 trailing 用各自的色位**（assist：主色 / onSurfaceVariant），不是同一个
  `icon_color`——三个 `trailing_icon` 槽位此前被 `ChipDefaults` 写好却从没被读过。
  **avatar 优先于 leading icon**：两者同时给时只画 avatar，对标 `Chip.kt:3528` 的
  `avatar != null -> avatar // An avatar takes precedence`（此前两个都画，实测图标数 2）。
  label 默认通过 `ProvideTextStyle(WiniaTheme::typography().label_large)`
  下传 Material 3 LabelLarge（14sp/20sp/Medium/0.1sp），状态色只覆盖其 color。
- **内容色的参数声明**：`build_chip` 里三路内容色（label / leading / trailing）在 group 之前
  用 `ctx.changed` 声明，**且三条声明放在 `next_key()` 之前**（与 `split_button.rs:281` 同序）；
  两个图标色还要在内容闭包内、`Row` 启动**之前**再声明一次——`Row` 是它自己的 restartable
  group，只比较 spacing/alignment，参数不变就 Skip，其 content 不执行，`with_content_color`
  根本没被调用。实测（只让 `leading_icon` 变色、三帧计数闭包）：修复前闭包 3 帧只跑 1 次、
  颜色恒为 `101,85,143`；补齐后 `seen=["101,143","200,20"]`。
  这是 `docs/component-authoring.md` §3.2 的 caller 侧陷阱，只是这次发生在组件内部。
- **不宏化**：chip 不标 `#[composable]`，理由是它是**透传 caller 闭包的容器**
  （`label`/`leading`/`trailing`/`avatar` 原样交给主题 scope，闭包顶层 `State.get()` 必须注册到
  父容器才刷新），这条记在 `docs/key-system-design.md` §7 的排除名单里。**不是**因为
  「与 Button 一致」——Button 是 `#[composable]` 的；依赖注册这里靠上面的 `ctx.changed` 显式声明。

### 阴影（`ChipElevation`，dp）

| 变体 | 默认 / 按下 / 聚焦 / hover / 拖动 / 禁用 |
|---|---|
| assist / suggestion / input（flat） | 0 / 0 / 0 / 0 / **8** / 0 |
| filter（flat） | 0 / 0 / 0 / **1** / **8** / 0 |
| 三个 Elevated 变体 | **1** / 1 / 1 / **3** / **8** / **0** |

数值取自 token 层级（`ElevatedContainerElevation = Level1`、`ElevatedHoverContainerElevation = Level2`、
`DraggedContainerElevation = Level4 = 8dp`）；`Level2 = 3dp`。以上都是"容器不透明时"的值——平铺
assist/suggestion 和未选中的 filter 容器透明，实际永远不显示阴影，这是有意行为（见上）。

## 4. 语义（无障碍）

每个 chip 是**一个**无障碍元素（label 与 icon 都在里面，`merge_descendants(true)`），role 按 material3：

| 变体 | role | state |
|---|---|---|
| Assist / ElevatedAssist / Suggestion / ElevatedSuggestion | `Role.Button`（`Chip.kt:1963`） | `enabled` |
| Filter / ElevatedFilter / Input | `Role.Checkbox`（`Chip.kt:2010`） | `enabled` + **`selected`**（true/false 都发声，不是"沉默") |

调试通道 `sem` 与语义树里可直接读到：`role: "checkbox"` + `state.selected`。

## 5. 已知限制 / 待改进

- **hover/focus/press 配色**：Compose ChipColors 有 hover/focus/drag 状态色，
  winia 仅 enabled/disabled/selected（ripple 已覆盖 press 视觉；`Chip.kt` 的默认工厂同样只用这两态）。
- **contentPadding 定制**：写死 16/8/4（specs 的规则），未开放成参数。
  （`horizontalArrangement` 已开放为 `horizontal_arrangement(f32)`，见 §1。）

以下五项此前记在这里，现已对齐 material3：

- `border` —— `ChipBorder` + `.border(..)`，`ChipBorder::none()` 对应 Compose 的 `border = null`；
- `horizontalArrangement` —— `.horizontal_arrangement(spacing)`；
- `ChipColors::copy(..)` / `SelectableChipColors::copy(..)` —— `Option<Color>` 承载「沿用原值」，
  与 `CheckboxColors::copy` 同一契约；
- 四个 disabled icon 槽位 —— 两个 colors 类各有 `disabled_leading_icon` / `disabled_trailing_icon`，
  默认仍是 `OnSurface` @ 38 %（M3 的 `DisabledLeadingIconColor` / `DisabledTrailingIconColor`），
  所以像素不变，变的是调用方能单独覆盖；
- InputChip avatar 禁用降透明度 —— `ChipDefaults::DISABLED_AVATAR_OPACITY`（0.38），
  对标 `InputChipTokens.DisabledAvatarOpacity`。

## 6. Tests

```bash
# Every unit test of this component
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

Fixtures: **no** `fixture_all` scenario, so the UI test suite never reaches this component. Its own
page is `examples/chip_demo.rs`; `examples/animated_visibility_demo.rs` and
`examples/animated_content_demo.rs` also build chips, as the content they animate.

`chip_demo.rs` walks the seven variants, the `selected` state, the three slots (leading icon,
trailing icon, avatar — including the avatar-over-leading rule), the border / gap / colour overrides,
the shadow rule, and a hoisted interaction source with a disabled row.
