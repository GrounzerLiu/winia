# Switch 组件

> 分支：`switch`（从 v2 分出）
> 设计：两层 ripple 模型 + “28×28 Handle 容器”结构（用户定义）

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

## 2. 结构与默认值

| 项 | 值 |
|---|---|
| 轨道 | 52×32、CornerFull、2dp 边框（内缩绘制，不超出组件范围） |
| Handle 容器 | 固定 28×28（带 unbounded ripple），关闭 (2,2)、开启 (22,2) |
| 中心圆 | 关闭 16×16、开启 24×24、按下/拖拽 28×28（容器内居中，颜色过渡） |
| 拇指内容图标 | 16×16（`SwitchDefaults.IconSize`） |
| 波纹 | Handle 容器上 unbounded：背景圆直径=容器对角线，前景半径=对角线、裁剪到背景圆 |
| 焦点环 | Secondary（`SwitchTokens.FocusIndicatorColor`，与其它组件 Primary 不同） |

### 颜色（`SwitchColors::from_theme`）

| 状态 | 拇指 | 轨道 | 边框 |
|---|---|---|---|
| 开启 | OnPrimary | Primary | 透明 |
| 关闭 | Outline | SurfaceContainerHighest | Outline |
| 禁用开启 | Surface | OnSurface@12% 叠 Surface | 透明 |
| 禁用关闭 | OnSurface@38% 叠 Surface | SurfaceContainerHighest@12% 叠 Surface | OnSurface@12% 叠 Surface |

图标色：checked=OnPrimaryContainer、unchecked=SurfaceContainerHighest、
disabled 按 token alpha（38%/38%）叠 Surface。

## 3. 实现细节

- 组合结构：轨道（52×32，动态背景色 + 动态边框色 + clickable + 拖拽）
  → Handle 容器（28×28，动态 x 偏移 + unbounded ripple）
  → 中心圆（动态尺寸 + 动态背景色，容器内居中）。
- 过渡动画（180ms EaseOutCubic / Spring）：背景色、边框色、中心圆颜色、
  容器 x 偏移（2↔22）、中心圆尺寸（16↔28）。
- 按下：中心圆放大到 28（动画），按住保持；松手按状态回 16（关闭）
  或 24（开启）。
- 拖拽切换：轨道上挂 `on_drag_start/on_drag/on_drag_end/on_drag_cancel`——
  drag 期间容器跟随手指（偏移钳制 2..22、中心圆保持 28），释放按中点 12
  判定目标状态（超过则切换、未超过则弹回）；释放位置作为动画起点
  （`set_silent` 写入避免先弹回旧目标）。点击（slop 内）仍走 clickable 切换。
- 按压坐标：Ripple 与 clickable 不同节点时，框架把按压点换算到 Ripple
  节点本地空间（`press_interaction_down`），波纹锚定按压点。
- 禁用：不注册 clickable/ripple/focusable，取 disabled 色组。

## 4. 未实现 / 后续

- 焦点环 Secondary 形状跟随轨道 Pill（已有）。
