# Typography

Winia 新增了独立的 Material 3 Typography token 层，参考 AndroidX Compose：

- [Typography.kt](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Typography.kt)
- [TypeScaleTokens.kt](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/TypeScaleTokens.kt)
- [MaterialTheme.kt](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/MaterialTheme.kt)

## Token 角色

`Typography::material3_default()` 提供标准 15 个角色：

```text
display_large / display_medium / display_small
headline_large / headline_medium / headline_small
title_large / title_medium / title_small
body_large / body_medium / body_small
label_large / label_medium / label_small
```

ListItem 最重要的三个角色对齐 AndroidX `ListTokens`：

| 角色 | 字号 | 行高 | 字距 | 字重 |
|---|---:|---:|---:|---|
| body_large | 16sp | 24sp | 0.5sp | Regular |
| body_medium | 14sp | 20sp | 0.2sp | Regular |
| label_small | 11sp | 16sp | 0.5sp | Medium |

## 使用

```rust
let typography = WiniaTheme::typography();
ProvideTextStyle(typography.body_large.clone(), ctx, |ctx| {
    Text::new("Body Large").build(ctx);
});
```

也可以覆盖局部子树：

```rust
WiniaTheme::with_typography(custom_typography, ctx, |ctx| {
    // 子树中的组件读取 custom_typography
});
```

`WiniaTheme::with_theme_and_typography` 和
`WiniaTheme::with_theme_typography_and_direction` 可同时设置颜色、Typography 与布局方向。
现有 `with_theme`、`light`、`dark`、`auto` API 保持不变。

## 组件映射

| 组件 / slot | Typography role | 说明 |
|---|---|---|
| Button 内容 | `label_large` | 状态色覆盖 color，保留 14/20/Medium/0.1sp |
| Badge label | `label_small` | 保留 11/16/Medium/0.5sp |
| ListItem headline / supporting / overline | `body_large` / `body_medium` / `label_small` | 对齐 ListTokens |
| Chip label | `label_large` | 14/20/Medium/0.1sp，selected/disabled 只覆盖颜色 |
| TextField input / placeholder / prefix / suffix | `body_large` | 默认 16/24/Regular/0.5sp；TextField `.font_size()` 可覆盖输入字号 |
| TextField floating label | `body_large` → `body_small` | 保留展开到悬浮的 16sp→12sp 动画，并插值行高与字距 |
| TextField supporting text | `body_small` | 独立渲染路径也携带字重、字距与固定行高 |
| IconButton / fixed-size FAB | 无 | icon-only 组件只提供 `LocalContentColor`；Extended FAB 另行设计 |


`TextStyle` 现在可以继承：

- `font_size`
- `font_weight`
- `font_style`
- `letter_spacing`
- `line_height`
- `soft_wrap`
- `color`
- `overflow`
- `max_lines`

优先级与 Compose 语义一致：

```text
Text builder 参数 > Text::style(...) > ProvideTextStyle > Text 默认值
```

`letter_spacing` 和 `line_height` 会进入现有 Paragraph 测量/渲染链；`line_height` 使用 `TextUnit` 表示，普通 `f32` 参数按 Sp 解释，最终按字号转换成 Skia 行高倍数。使用 `Px` 时会先按当前 `Density` 转换为逻辑像素。

## 兼容策略

主题入口会提供默认 Typography local，但**不会自动用 `body_large` 包裹整个主题子树**。因此现有裸 `Text` 仍保持原来的 14sp 默认行为，不会因引入 M3 token 导致全局布局和截图变化。

`TextStyle` 是公开结构体，本次新增了 `soft_wrap`、`letter_spacing` 与 `line_height` 字段。外部代码若直接使用未带 `..TextStyle::default()` 的完整 struct literal，需要补齐这三个字段；推荐使用 builder API 或 struct update 语法，以避免后续字段扩展造成同类源码兼容问题。

当前限制：

- Typography 尚未接入 RichText 的全部 span 样式；RichText 的 `height_multiple` 与 Plain Text 的固定行高单位不同。
- 主题尚未提供字体族、locale、字体宽度等完整 Compose 字体属性。
- 组件应显式读取所需 token，而不是依赖主题自动改变所有文本。
