# Scaffold

`Scaffold` 是 Winia 的页面布局骨架，负责组织 top bar、bottom bar、内容区、SnackbarHost 和 Floating Action Button（FAB）overlay。

## API

```rust
Scaffold::new(|ctx, padding| {
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding_sides(padding.start, padding.top, padding.end, padding.bottom)
                .padding(24.0),
        )
        .build(ctx, |ctx| {
            Text::new("Content").build(ctx);
        });
})
.top_bar(|ctx| {
    TopAppBar::new(|ctx| Text::new("Title").build(ctx)).build(ctx);
})
.bottom_bar(|ctx| {
    // 任意 bottom bar 内容
})
.snackbar_host(|ctx| {
    SnackbarHost::new(snackbar_state.clone()).build(ctx);
})
.floating_action_button(|ctx| {
    FloatingActionButton::new().build(ctx, |ctx| {
        Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z").build(ctx);
    });
})
.content_padding(ScaffoldContentPadding::new(0.0, 0.0, 0.0, 0.0))
.content_window_insets(Insets::ZERO)
.fab_position(ScaffoldFabPosition::End)
.build(ctx);
```

`content` 收到的 `ScaffoldContentPadding` 就是 Compose 的 `PaddingValues`：content 根节点必须自己应用它。

`content_window_insets` 默认使用 `ScaffoldDefaults::content_window_insets()`，也就是当前窗口的 system bars。需要自己处理 Insets 时传 `Insets::ZERO`。

## 布局规则

`Scaffold` 以五个稳定 slot 进行测量和放置：

```text
Scaffold
├── top_bar
├── bottom_bar
├── content
├── floating_action_button
└── snackbar_host
```

- top bar 放在顶部，bottom bar 放在底部；bar 自己负责处理 system insets，和 Compose 一致。
- content 按全尺寸测量并放在 `(0, 0)`，通过 `ScaffoldContentPadding` 自己避让 bar 和 system insets。
- `content` 的 padding 计算方式：
  - `top`：有 top bar 时是 top bar 高度，没有 top bar 时是顶部 system inset。
  - `bottom`：有 bottom bar 时是 bottom bar 高度，没有 bottom bar 时是底部 system inset。
  - `start` / `end`：始终来自 system insets。
  - `.content_padding(...)` 是额外的显式 inset，会加在以上数值上。
- content padding 在布局阶段写入 `State`，content 会在下一帧带着真实 bar 高度重新组合。
- snackbar host 放在 bottom bar（或底部 system inset）上方、FAB 上方，水平居中。
- FAB 是 overlay，不会缩小 content。
- 默认 FAB 为逻辑 `BottomEnd`，与 bottom bar（或底部 system inset）、额外 bottom inset、16dp edge margin 保持间距。
- RTL 下 BottomEnd 自动镜像到左侧；`ScaffoldFabPosition::Center` 则水平居中。

`Scaffold` 会自然使用 TopAppBar 的动态高度，因此 Medium/Large TopAppBar 折叠时 content 的 `padding.top` 会随其实际高度更新。

## 当前边界

尚未实现：

- Snackbar queue（当前 SnackbarHost 自己管理单条显示）
- Drawer
- TopAppBar 与内容的 nested-scroll delta 消费协调
- content 的第一帧会先以默认 padding 组合，布局测得 bar 高度后再重组一次；真正的 Compose 在 measure 内 subcompose，不存在这一帧。

## Deferred

- `ScaffoldFabPosition::Start` / `EndOverlay` — upstream `FabPosition` is a four-value value class
  (`Start` / `Center` / `End` / `EndOverlay`, `Scaffold.kt:307-331`); winia's enum
  (`winia/src/components/scaffold.rs:18`) exposes `End` and `Center` only. `EndOverlay` is the
  position that overlaps the bottom bar (`Scaffold.kt:231`); `Start` pins the FAB to the start edge.

## 测试

```bash
cargo test -p winia --lib scaffold
cargo test -p winia --test visual_matrix scaffold_
cargo test -p winia --features debug-server --test ui_test scaffold_
```
