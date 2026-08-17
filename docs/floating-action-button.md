# Floating Action Button

Winia 的 `FloatingActionButton` 对齐 Material 3 的图标型 FAB 家族，使用 Builder + 内容闭包 API。

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

默认阴影为 3dp，悬停时为 4dp；还提供：

- `FloatingActionButtonDefaults::lowered_elevation()`：附着在其他表面时使用
- `FloatingActionButtonDefaults::bottom_app_bar_elevation()`：底部应用栏场景，所有阴影为 0dp

## 设计来源

- [Material 3 FAB specs](https://m3.material.io/components/floating-action-button/specs)
- [Jetpack Compose FloatingActionButton.kt](https://github.com/androidx/androidx/blob/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/FloatingActionButton.kt)

## 当前差异

- 本次只实现图标型 FAB；`ExtendedFloatingActionButton`（图标 + 文本、展开/收起动画）留待后续组件切片。
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
