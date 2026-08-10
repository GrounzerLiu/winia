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

```rust
TriStateCheckbox::new(state: ToggleableState)    // 对标 TriStateCheckbox(state, ...)
    .on_click(|| {})                             // 对标 onClick（状态迁移由调用方决定）
    .enabled(bool)
    .colors(CheckboxColors)
    .interaction_source(source)
    .modifier(Modifier)
    .build(ctx);
```

- 组件本身不持有状态：`checked` 由调用方传入，点击回调收到取反值后由调用方更新
  （与 `IconToggleButton` 相同的受控语义）；`TriStateCheckbox` 传 `ToggleableState`
  （Off / On / Indeterminate），点击回调由调用方决定下一个状态。
- `Checkbox` 内部委托 `TriStateCheckbox(state = ToggleableState(checked),
  onClick = { onCheckedChange(!checked) })`——与 M3 结构一致。

## 2. 默认值（对标 `CheckboxDefaults` / `CheckboxTokens` 1.4.0）

| 项 | 值 |
|---|---|
| 视觉尺寸 | 20×20（`CheckboxSize`） |
| 触摸目标/状态层 | 40×40（`StateLayerSize`，形状 Circle） |
| 圆角 | 2（`ContainerShape = RoundedCornerShape(2)`） |
| 描边宽度 | 2（`StrokeWidth`） |
| 波纹 | `ripple(bounded = false, radius = 20)`——unbounded 圆形 |

### ToggleableState

| 值 | 含义 | 视觉 |
|---|---|---|
| `Off` | 未选中 | 透明底 + 2dp 边框，无标记 |
| `On` | 选中 | Primary 填充 + 白色 check |
| `Indeterminate` | 部分选中 | Primary 填充 + 白色横线（check 中段压平） |

### 颜色（`CheckboxColors::from_theme`）

| 状态 | 容器 | 边框 | 勾号 |
|---|---|---|---|
| On | Primary | Primary（与容器合并） | OnPrimary |
| Off | 透明 | OnSurfaceVariant | 透明 |
| disabled On | OnSurface @ 38% | 同左 | OnPrimary（M3 `CheckboxColors`
  无 disabled checkmark 字段，`SelectedDisabledIconColor` token 未参与实现） |
| disabled Off | 透明 | OnSurface @ 38% | 透明 |
| disabled Indeterminate | OnSurface @ 38% | OnSurface @ 38% | OnPrimary |

颜色只分 enabled×state（Off/On/Indeterminate），hover/focus/press 反馈由 ripple 承担
（M3 `CheckboxImpl` 同样不读交互态取色）。

## 3. 实现细节

- 组合结构：外层 40×40 节点（clip Circle 供焦点环推断形状）→
  `clickable_with_source` + unbounded ripple → 内层 20×20 视觉盒
  （`background` + `border`，checked 时边框色=容器色合并为纯填充）。
- 容器/边框颜色：`animate_color_as_state` Spring 过渡（对标 M3
  `animateColorAsState`），Off/On/Indeterminate 切换时渐变而非跳变；
  渲染期 `peek()` 读取动画值不触发重组。
- 勾号：Material Icons “check” 填充路径（20×20），外层 `graphics_layer`
  缩放动画（checked=1 / unchecked=0，Spring 近似 M3 `checkDrawFraction`
  过渡）。勾号 tint 固定为选中色（OnPrimary），未选中静止态由 scale=0
  隐藏——保证取消选中时缩放退出动画可见（tint 若瞬切透明会吞掉动画）。
- Indeterminate：横线路径（M3 drawCheck 中段 0.2w..0.8w、y=0.5h），与 check
  图标各持一个缩放动画（On=1/0、Indeterminate=0/1），近似 M3
  `crossCenterGravitation` 形态过渡（v1 为交叉缩放而非路径形变）。
- 焦点环：`theme.primary`，形状跟随 Circle 状态层。
- 禁用：不注册 clickable/ripple/focusable，取 disabled 色组。

## 4. 未实现 / 后续

- `Checkbox` 高级重载（`checkmarkStroke` / `outlineStroke` 自定义）。
- On ↔ Indeterminate 的路径形变过渡（当前为两个图标的交叉缩放，
  需 Canvas/pathMeasure 才能做 M3 的连续形变）。
- 键盘 Space 触发切换（依赖框架全局按键语义，聚焦后回车/空格触发 click
  已由既有 clickable 通路覆盖）。
