# Floating Action Button

Winia 的 `FloatingActionButton` / `ExtendedFloatingActionButton` 对齐 Material 3 的 FAB 家族（图标型 + 扩展型），使用 Builder + 内容闭包 API。

## 使用

```rust
FloatingActionButton::new()
    .on_click(|| println!("add"))
    .build(ctx, |ctx| {
        Icon::svg_path("M12 5v14M5 12h14").build(ctx);
    });
```

尺寸变体：

```rust
FloatingActionButton::small();   // 40dp，兼容变体，不再是 M3 首选
FloatingActionButton::new();     // Regular，56dp
FloatingActionButton::medium();  // 80dp，M3 Expressive
FloatingActionButton::large();   // 96dp
```

`FloatingActionButtonSize::icon_size()` 提供推荐图标尺寸：Small/Regular 为 24dp，Medium 为 28dp，Large 为 32dp。

## 颜色

默认颜色是 `primaryContainer` / `onPrimaryContainer`。也可以使用主题色映射：

```rust
FloatingActionButton::new()
    .colors(FloatingActionButtonDefaults::secondary_colors(&WiniaTheme::colors()))
    .build(ctx, |ctx| { /* icon */ });
```

可用映射包括：

- `colors`：Primary Container / On Primary Container（默认）
- `primary_colors`：Primary / On Primary
- `secondary_colors`：Secondary / On Secondary
- `tertiary_colors`：Tertiary / On Tertiary
- `from_pair`：自定义容器色和内容色

禁用态不注册点击、焦点和 Ripple，使用 `onSurface` 的 12% 容器色与 38% 内容色近似 Material 3 disabled token。

## 交互与阴影

设置 `on_click` 后，组件会注册 `clickable_with_source`、Focusable、Hoverable 和 bounded Ripple。焦点环、Ripple、背景和阴影共用同一 `Shape`。

默认 elevation 为 6dp，按下 12dp，聚焦/悬停 8dp；阴影由 Skia 原生 ambient/spot 光源绘制，默认使用 ambient 12.5% 黑（`0x20`）与 spot 31% 黑（`0x50`）。这是增强清晰度的可回退微调，旧值为 ambient 10%（`0x19`）/ spot 25%（`0x40`）；可通过 `Modifier::ambient_shadow_color` / `spot_shadow_color` 覆盖颜色；还提供：

- `FloatingActionButtonDefaults::lowered_elevation()`：附着在其他表面时使用
- `FloatingActionButtonDefaults::bottom_app_bar_elevation()`：底部应用栏场景，所有阴影为 0dp

## 设计来源

- [Material 3 FAB specs](https://m3.material.io/components/floating-action-button/specs)
- [Jetpack Compose FloatingActionButton.kt](https://github.com/androidx/androidx/blob/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt)

## 当前差异

- 项目尚无 semantics 树，因此 `contentDescription`/无障碍语义仍由上层能力补齐。
- FAB 的默认视觉形状、Ripple 和焦点环是 M3 四角圆角矩形；当前框架的 hit-test 仍使用布局矩形边界。
- 默认 shape 近似 M3 token：Small=CornerMedium（12dp）、Regular=CornerLarge（16dp）、Medium=LargeIncreased（20dp）、Large=CornerExtraLarge（28dp）。
- 圆形仅在用户显式传入 `.shape(Shape::Circle)` 时使用。

## 示例与测试

```bash
cargo run -p winia --example floating_action_button_demo
cargo test -p winia floating_action_button
cargo test -p winia --test render_snapshot
```

## ExtendedFloatingActionButton（M3 扩展 FAB，已实现）

```rust
// expanded 绑定 State<bool>——收起 56×56（仅图标居中，同 FAB）；展开显示 [icon|文本]
let expanded = ctx.remember(|| true);
ExtendedFloatingActionButton::new(
    |ctx| Text::new("Create").build(ctx),      // 文本槽（LabelLarge）
    |ctx| Icon::svg_path(PLUS).size(24.0).build(ctx),
    expanded.clone(),
)
.on_click(|| {})
.colors(...)            // 默认 PrimaryContainer / OnPrimaryContainer
.elevation(FloatingActionButtonDefaults::elevation())  // L3 / hover L4
.build(ctx);
```

### Token（androidx-main ExtendedFabPrimaryTokens / FabBaselineTokens）

| 项 | 值 | winia 常量 |
|---|---|---|
| 高度 | 56 | `EXTENDED_FAB_HEIGHT` |
| 收起宽 | 56（= FabBaseline.ContainerWidth） | `EXTENDED_FAB_COLLAPSED_WIDTH` |
| 展开最小宽 | 80 | `EXTENDED_FAB_MIN_EXPANDED_WIDTH` |
| 形状 | CornerLarge（16dp 圆角） | `Shape::rounded(16.0)` |
| 展开态 padding | start 16 / 图标-文本 12 / end 20 | 私有常量 |
| 容器/内容色 | PrimaryContainer / OnPrimaryContainer | theme 直取 |
| 文本样式 | LabelLarge | theme.typography().label_large |

### 动画语义（对齐 androidx）

- 进度 p = animate_float_as_state(expanded, FastSpatial≈stiffness400)
- 宽度 = lerp(56, max(80, 16+icon+12+text+20), p)
- 图标 x：从居中滑至 start padding 16；文本透明度 = p（FastEffects 淡入淡出）、
  位置从居中滑至图标右侧
- measure 期读进度注册 layout_deps——动画帧只重测不重组

### 典型用法：宽轨 header

M3 模式——WideNavigationRail 展开时 header 用 Extended FAB，收起时用普通 FAB
（见 examples/navigation_rail_demo.rs）。
