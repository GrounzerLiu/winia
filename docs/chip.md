# Chip 组件

> 分支：`v2`（对齐 material3 Chip 家族）
> 参考：M3 specs（m3.material.io/components/chips/specs）+ Jetpack Compose material3 `Chip.kt`

## 1. API

```rust
// 统一入口 Chip + 变体构造（对标 material3 AssistChip/FilterChip/InputChip/SuggestionChip）
Chip::assist(label, on_click)                       // 辅助操作（可选 leading/trailing icon）
Chip::filter(selected, label, on_click)             // 筛选（selected 切换）
Chip::input(selected, label, on_click)              // 输入信息（leading/avatar/trailing 关闭）
Chip::suggestion(label, on_click)                   // 建议（可选 icon）

// 通用 setter
    .modifier(Modifier)
    .enabled(bool)
    .shape(Shape)                                    // 默认 8dp 圆角
    .colors(ChipColors)                              // 非选择型配色（assist/suggestion）
    .selectable_colors(SelectableChipColors)         // 选择型配色（filter/input）
    .leading_icon(|ctx| { ... })                     // 前置图标（18dp）
    .trailing_icon(|ctx| { ... })                    // 后置图标
    .avatar(|ctx| { ... })                           // 头像（input 专用，24dp 12dp 圆角）
    .icon(|ctx| { ... })                             // 建议图标（suggestion 的 icon = leading 别名）
    .build(ctx);
```

- 受控语义：`selected` 由调用方持有；`on_click` 只通知（切换由调用方 `State.update`，
  与 Compose 一致）。
- 无 content 闭包——label 是第一个参数。

## 2. 结构与默认值（M3 specs）

| 项 | 值 |
|---|---|
| 容器高 | 32dp |
| 形状 | 8dp 圆角（`ChipDefaults::shape()`） |
| icon 尺寸 | 18dp（`IconSource` 闭包子节点，尺寸由调用方控制） |
| avatar | 24dp、12dp 圆角（InputChip） |
| 左右 padding | 无 icon 16dp / 有 icon 8dp |
| 元素间距 | 8dp |
| 边框 | 1dp outline（无填充时）；**有填充（selected）0 边框——不挂 border modifier** |

### 配色（`ChipDefaults`）

**非选择型（Assist/Suggestion）**——边框样式：
| 状态 | 容器 | 文字 | icon |
|---|---|---|---|
| enabled | transparent | onSurfaceVariant | primary |
| disabled | transparent | onSurface@38% | onSurface@38% |

**选择型（Filter/Input）**：
| 状态 | 容器 | 文字 | icon | 边框 |
|---|---|---|---|---|
| unselected | transparent | onSurfaceVariant | primary | 1dp outline |
| selected | secondaryContainer | onSecondaryContainer | onSecondaryContainer | 无 |
| disabled | transparent | onSurface@38% | onSurface@38% | 无交互 |

## 3. 实现细节

- **统一容器**：`build_chip`（内部）——32dp 高 + shape 背景 + 条件边框 +
  clickable/ripple + 内容垂直居中（`Row` 显式 `Alignment::Center`——默认 Start
  会顶对齐，有 icon 时文字不居中）。
- **背景色过渡动画**：`animate_color_as_state`（180ms EaseOutCubic）——selected/
  unselected/disabled 切换时容器色平滑渐变。⚠ `background` 接受动态闭包
  （`impl Fn() -> Color`），必须传 `move || bg.get()` 而非 `bg.get()` 的值——
  否则冻结在 build 时（动画不显示）；渲染期每帧求值。
- **边框消失**：`border_width > 0` 才挂 border modifier——`border(0.0)` 仍会
  绘制 0 宽线（可见伪影），selected 时直接不挂。
- **变体差异**（`ChipVariant` 分派）：Assist/Suggestion 走 ChipColors（非选择）；
  Filter/Input 走 SelectableChipColors（selected 双态）+ selected 0 边框。
- **内部结构**：容器（背景/边框/clickable/ripple）→ Row（icon → label → trailing，
  间距 8）→ 内容经 `WiniaTheme::with_content_color` 下传各元素颜色（Icon tint Auto
  跟随）。label 默认通过 `ProvideTextStyle(WiniaTheme::typography().label_large)`
  下传 Material 3 LabelLarge（14sp/20sp/Medium/0.1sp），状态色只覆盖其 color。
- **宏化判据**：未宏化（与 Column/Row/Button 一致——content 闭包 + 内部 scope
  会拦截依赖注册）。

## 4. 已知限制 / 待改进

- **elevation**（ElevatedAssistChip/ElevatedFilterChip）：未实现（无阴影层）。
- **hover/focus/press 配色**：Compose ChipColors 有 hover/focus/drag 状态色，
  winia 仅 enabled/disabled/selected（ripple 已覆盖 press 视觉）。
- **disabled 边框**：Compose 用 onSurface@12% 边框 + 0.12 opacity；当前禁用态
  未画边框。
- **交互源**：未暴露 `interaction_source`（Button 有）。
- **contentPadding 定制**：写死 16/8。
- 视觉验证：chip_demo 展示 4 变体 + selected 切换 + 过渡动画（WS 实测点击切换
  生效、动画渐变正常）。
