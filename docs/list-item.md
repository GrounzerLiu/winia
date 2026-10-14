# ListItem

Winia 的 `ListItem` 对齐 AndroidX Material 3 `ListItem` 的 slot 结构和高度 token，适合设置页、消息列表和可滚动内容列表。

实现参考：

- [`ListItem.kt`](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/ListItem.kt)
- [`ListItemDefaults.kt`](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/ListItemDefaults.kt)
- [`ListTokens.kt`](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/ListTokens.kt)
- [`ListSamples.kt`](https://cs.android.com/androidx/platform/frameworks/support/+/androidx-main:compose/material3/material3/samples/src/main/java/androidx/compose/material3/samples/ListSamples.kt)

## 高度和间距

| 形态 | 高度 | 用法 |
|---|---:|---|
| 一行 | 56dp | 只有 headline |
| 二行 | 72dp | headline 加 overline 或 supporting |
| 三行 | 88dp | 同时提供 overline 和 supporting |

The horizontal inset is 16dp, and the gap between the leading slot, the text column and the trailing slot
is 16dp as well (`LeadingContentEndPadding` / `TrailingContentStartPadding`). The vertical inset is 8dp for
the one- and two-line layouts and 12dp for the three-line one, matching Compose's
`verticalPadding(listItemType)`. The component fills its parent's width by default; an external `modifier`
is applied after the default one, so it can add size, background or test tags.

## Slots

```rust
ListItem::new(|ctx| {
    Text::new("Headline").build(ctx);
})
.overline_content(|ctx| Text::new("Overline").build(ctx))
.supporting_content(|ctx| Text::new("Supporting").build(ctx))
.leading_content(|ctx| Icon::svg_path("M12 5v14M5 12h14").build(ctx))
.trailing_content(|ctx| Text::new("›").build(ctx))
.on_click(|| {})
.build(ctx);
```

headline 使用 Typography 的 `body_large`，supporting 使用 `body_medium`，overline 使用 `label_small`。leading、trailing 和辅助文本默认使用 `on_surface_variant`；禁用状态使用约 38% 内容色，且不会注册点击、焦点或 ripple 交互。

## LazyColumn

ListItem 可直接作为 `LazyColumn` 的 item 内容。列表会依据测量后的实际高度更新缓存，因此一行、二行和三行项目可以混排：

```rust
LazyColumn::new()
    .modifier(Modifier::new().fill_max_size())
    .items_plain(3, |ctx, index| {
        ListItem::new(move |ctx| Text::new(format!("Item {index}")).build(ctx))
            .build(ctx);
    })
    .build(ctx);
```

当前版本实现了标准 slot、颜色、enabled、interaction source 和 clickable overload；selected/checked 变体、semantics、baseline 对齐和 Material Expressive 扩展仍属于后续范围。
