# TopAppBar

Winia 的 `TopAppBar` 参考 AndroidX Material 3：

- [`AppBar.kt`](https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/AppBar.kt)
- [`AppBarSmallTokens.kt`](https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/AppBarSmallTokens.kt)
- [`AppBarMediumTokens.kt`](https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/AppBarMediumTokens.kt)
- [`AppBarLargeTokens.kt`](https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/AppBarLargeTokens.kt)
- [`arrow_back` SVG](https://github.com/google/material-design-icons/blob/master/src/navigation/arrow_back/materialicons/24px.svg)
- [`more_vert` SVG](https://github.com/google/material-design-icons/blob/master/src/navigation/more_vert/materialicons/24px.svg)

## 变体

```rust
TopAppBar::new(title)            // Standard, 64dp
TopAppBar::center_aligned(title) // CenterAligned, 64dp
TopAppBar::medium(title)         // expanded 112dp, collapsed 64dp
TopAppBar::large(title)          // expanded 152dp, collapsed 64dp
```

所有变体都支持：

```rust
TopAppBar::large(|ctx| Text::new("Settings").build(ctx))
    .navigation_icon(|ctx| Icon::svg_path("M20 12H4").build(ctx))
    .subtitle(|ctx| Text::new("Account preferences").build(ctx))
    .actions(|ctx| Icon::svg_path("M12 5v14M5 12h14").build(ctx))
    .build(ctx);
```

标题 Typography：

- Standard / CenterAligned：`title_large`
- Medium expanded：`headline_small`
- Large expanded：`headline_medium`
- Medium/Large collapsed：`title_large`
- subtitle：`body_medium`

布局对齐 AndroidX 的两种基础结构：

- Standard / CenterAligned 使用专用单行三 slot policy：navigation、title、actions。
- Medium / Large 使用固定 64dp collapsed row 加独立 expanded row；collapsed row 负责 navigation、collapsed title 和 actions，expanded row 只负责 expanded title 与 subtitle。

navigation 采用 48dp touch slot，actions group 的最小高度也为 48dp；两端保留 4dp horizontal inset。标题测量宽度会扣除 12dp title inset、navigation 和 actions 宽度，并默认使用单行 ellipsis，避免长标题撑高或覆盖 action。

CenterAligned 标题先按完整 app bar 的几何中心定位，再限制在两侧 slot 的安全区域中；因此不等宽 navigation/actions 不会把标题变成“剩余空间居中”。RTL 下 start/end slot 与 title placement 都由专用布局策略镜像。

Medium/Large 使用单一 title slot：它从 expanded row 的底部锚点连续移动到 collapsed 64dp row 的垂直中心。展开时 subtitle 位于 title 下方；它是独立 expanded-only layer，按 collapse fraction 淡出并由 root bounds clip，因此完全折叠后不会参与 title 的布局或把 title 从 navigation/actions 的中心线挤开。Medium 使用 24dp bottom inset，Large 使用 28dp bottom inset。

## 折叠行为

TopAppBar 可绑定调用方持有的 `ScrollState`：

```rust
let scroll = ctx.remember(|| ScrollState::new()).get();
let behavior = TopAppBarScrollBehavior::new(scroll.clone(), TOP_APP_BAR_LARGE_HEIGHT);

TopAppBar::large(|ctx| Text::new("Title").build(ctx))
    .scroll_behavior(behavior)
    .build(ctx);

Column::new()
    .modifier(Modifier::new().vertical_scroll(scroll))
    .build(ctx, content);
```

折叠比例为：

```text
(scroll.offset / (expanded_height - 64dp)).clamp(0, 1)
```

当前实现是共享 offset 驱动：顶部栏读取外部滚动状态，外框高度与单一 title placement 随 fraction 连续插值，subtitle 作为独立 expanded-only layer 淡出。它不会复制滚动状态，也不会修改 LazyColumn 的测量策略。

当前未实现 Compose 的 nested-scroll pre-scroll/post-scroll 消费链，因此顶部栏不会优先消费部分滚动 delta。需要严格 nested-scroll 语义时，应在后续独立切片中扩展滚动分发器。

## 测试

```bash
cargo test -p winia --lib top_app_bar
cargo test -p winia --test visual_matrix
cargo test -p winia --features debug-server --test ui_test top_app_bar_
```
