# Switch 组件（material3 对齐）

> 分支：`switch`（从 v2 分出）
> 对标：material3 1.4.0 `Switch`

## 1. API

```rust
Switch::new(checked: bool)                        // 对标 Switch(checked, ...)
    .on_checked_change(|checked: bool| {})        // 对标 onCheckedChange（点击回传 !checked）
    .enabled(bool)
    .colors(SwitchColors)
    .interaction_source(source)
    .modifier(Modifier)
    .build(ctx, |ctx| { /* thumbContent：拇指内图标（空闭包 = 无） */ });
```

- 受控语义与 Checkbox 相同：状态由调用方持有，点击回调收到取反值。
- `build` 的 content 闭包对应 M3 `thumbContent`；内部 Icon 的 tint Auto
  自动跟随 `SwitchColors.icon_color(...)`。

## 2. 默认值（`SwitchTokens` 1.4.0）

| 项 | 值 |
|---|---|
| 轨道 | 52×32、CornerFull、2dp 边框 |
| 拇指 | 圆形：checked=24 / unchecked=16 / pressed=28 |
| 拇指偏移 | unchecked=4、checked=24；pressed 时内收 2（22 / 2） |
| 拇指内容图标 | 16×16（`SwitchDefaults.IconSize`） |
| 波纹 | 拇指上 unbounded（radius = StateLayerSize/2 = 20）；轨道 toggleable 无 indication |
| 焦点环 | Secondary（`SwitchTokens.FocusIndicatorColor`，与其它组件 Primary 不同） |

### 颜色（`SwitchColors::from_theme`）

| 状态 | 拇指 | 轨道 | 边框 |
|---|---|---|---|
| checked | OnPrimary | Primary | 透明 |
| unchecked | Outline | SurfaceContainerHighest | Outline |
| disabled checked | Surface | OnSurface@12% 叠 Surface | 透明 |
| disabled unchecked | OnSurface@38% 叠 Surface | SurfaceContainerHighest@12% 叠 Surface | OnSurface@12% 叠 Surface |

图标色：checked=OnPrimaryContainer、unchecked=SurfaceContainerHighest、
disabled 按 token alpha（38%/38%）叠 Surface。

## 3. 实现细节

- 组合结构：轨道节点（52×32，`background`+`border`+`clickable_with_source`）
  → 拇指节点（动态 `size` + 动态 `offset` + Circle 背景 + unbounded ripple）。
- 拇指动画：`animate_float_as_state` 驱动尺寸（16/24/28）与水平偏移（4/22/24），
  Spring 近似 M3 `ThumbNode` 的 FastSpatial（pressed 的 Snap 统一用 Spring）；
  y 偏移 = (轨道高 - 拇指尺寸)/2 跟随尺寸动画，实现“左对齐 + 垂直居中”。
- 禁用：不注册 clickable/ripple/focusable，取 disabled 色组。

## 4. 未实现 / 后续

- 拖拽滑动切换（M3 TODO 同样未做，b/223797571）。
- 轨道/拇指颜色过渡动画（M3 `SwitchImpl` 本身静态取色，暂无差距）。
- 焦点环 Secondary 形状跟随轨道 Pill（已有）。
