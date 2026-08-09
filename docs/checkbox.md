# Checkbox 组件（material3 对齐）

> 分支：`checkbox`（从 v2 分出）
> 对标：material3 1.4.0 `Checkbox`

## 1. API

```rust
Checkbox::new(checked: bool)                     // 对标 Checkbox(checked, ...)
    .on_checked_change(|checked: bool| {})       // 对标 onCheckedChange（点击回传 !checked）
    .enabled(bool)                               // 对标 enabled
    .colors(CheckboxColors)                      // 对标 colors
    .interaction_source(source)                  // 对标 interactionSource（hoist）
    .modifier(Modifier)                          // 外部修饰符
    .build(ctx);
```

- 组件本身不持有状态：`checked` 由调用方传入，点击回调收到取反值后由调用方更新
  （与 `IconToggleButton` 相同的受控语义）。

## 2. 默认值（对标 `CheckboxDefaults` / `CheckboxTokens` 1.4.0）

| 项 | 值 |
|---|---|
| 视觉尺寸 | 20×20（`CheckboxSize`） |
| 触摸目标/状态层 | 40×40（`StateLayerSize`，形状 Circle） |
| 圆角 | 2（`ContainerShape = RoundedCornerShape(2)`） |
| 描边宽度 | 2（`StrokeWidth`） |
| 波纹 | `ripple(bounded = false, radius = 20)`——unbounded 圆形 |

### 颜色（`CheckboxColors::from_theme`）

| 状态 | 容器 | 边框 | 勾号 |
|---|---|---|---|
| checked | Primary | Primary（与容器合并） | OnPrimary |
| unchecked | 透明 | OnSurfaceVariant | 透明 |
| disabled checked | OnSurface @ 38% | 同左 | OnPrimary（M3 `CheckboxColors`
  无 disabled checkmark 字段，`SelectedDisabledIconColor` token 未参与实现） |
| disabled unchecked | 透明 | OnSurface @ 38% | 透明 |

颜色只分 enabled×checked，hover/focus/press 反馈由 ripple 承担
（M3 `CheckboxImpl` 同样不读交互态取色）。

## 3. 实现细节

- 组合结构：外层 40×40 节点（clip Circle 供焦点环推断形状）→
  `clickable_with_source` + unbounded ripple → 内层 20×20 视觉盒
  （`background` + `border`，checked 时边框色=容器色合并为纯填充）。
- 勾号：Material Icons “check” 填充路径（20×20），外层 `graphics_layer`
  缩放动画（checked=1 / unchecked=0，Spring 近似 M3 `checkDrawFraction`
  过渡）；渲染期读动画值不触发重组。
- 焦点环：`theme.primary`，形状跟随 Circle 状态层。
- 禁用：不注册 clickable/ripple/focusable，取 disabled 色组。

## 4. 未实现 / 后续

- `TriStateCheckbox`（indeterminate 态）——需要 `ToggleableState` 三态枚举
  与横线 dash 路径 + 重力过渡动画。
- `Checkbox` 高级重载（`checkmarkStroke` / `outlineStroke` 自定义）。
- 容器/边框颜色过渡动画（M3 `animateColorAsState`；当前仅勾号缩放动画，
  与现有 Button/IconButton 的静态取色实现一致）。
- 键盘 Space 触发切换（依赖框架全局按键语义，聚焦后回车/空格触发 click
  已由既有 clickable 通路覆盖）。
