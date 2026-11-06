# Scaffold

`Scaffold` 是 Winia 的页面布局骨架，负责组织 top bar、bottom bar、内容区、SnackbarHost 和 Floating Action Button（FAB）overlay。

## API

```rust
Scaffold::new(|ctx, padding| {
    // content slot
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

- top bar 放在顶部 system inset 之后，bottom bar 放在底部 system inset 之前。
- content 自动使用 top/bottom bar 之间的剩余宽高，并避开 system bars。
- `ScaffoldContentPadding` 是额外内容 inset；它与 system insets、已测得的 top/bottom bar 高度叠加。
- snackbar host 放在 bottom bar 上方、水平居中。
- FAB 是 overlay，不会额外缩小 content 可用区域。
- 默认 FAB 为逻辑 `BottomEnd`，与 bottom bar、system insets、额外 bottom inset、16dp edge margin 保持间距。
- RTL 下 BottomEnd 自动镜像到左侧；`ScaffoldFabPosition::Center` 则水平居中。

`Scaffold` 会自然使用 TopAppBar 的动态高度，因此 Medium/Large TopAppBar 折叠时 content 顶部会随其实际高度更新。

## 当前边界

尚未实现：

- Snackbar queue（当前 SnackbarHost 自己管理单条显示）
- Drawer
- TopAppBar 与内容的 nested-scroll delta 消费协调

content slot 当前接收 `ScaffoldContentPadding` 的显式值，但不接收运行期测得的 top/bottom bar 高度；布局 policy 会自动应用这些高度。需要根据实时 bar 高度改变内容内部 padding 时，应由 content 本身与共享状态协作。

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
