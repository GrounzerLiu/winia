# Nested Scroll

Winia 提供 Compose 风格的 nested scroll 基础协议，用于让祖先容器在子滚动之前或之后部分消费滚动 delta。

## 协议

- `ScrollDelta` / `ScrollVelocity`：二维滚动位移和速度。
- `NestedScrollSource`：`Wheel`、`Drag`、`Fling`、`SideEffect`。
- `NestedScrollConnection`：
  - `on_pre_scroll`
  - `on_post_scroll`
  - `on_pre_fling`
  - `on_post_fling`
- `NestedScrollDispatcher`：按 ancestor 顺序调度 pre，按反向顺序调度 post。

滚动符号沿用 Winia 现有约定：负垂直 delta 表示内容向上移动、scroll offset 增加。

## 当前接入范围

- `Modifier::nested_scroll(connection)` 可挂载祖先 connection。
- wheel、debug scroll 和 drag move 通过 pre → child partial consume → post 顺序分发。
- 拖拽结束的 fling 通过 `dispatch_nested_scroll_fling` 走 pre-fling → child fling → post-fling 链。
- `TopAppBarScrollBehavior::{pinned, enter_always, exit_until_collapsed}` 提供独立 `TopAppBarState` connection。
- legacy `TopAppBarScrollBehavior::new(scroll, expanded_height)` 继续保留，供已有共享 offset 页面迁移。


`TopAppBarState` 独立保存：

- `height_offset_limit`
- `height_offset`
- `content_offset`

并提供 `collapsed_fraction()`、`overlapped_fraction()`、`current_height()` 等派生值。

行为工厂：

```rust
let state = TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT);
let behavior = TopAppBarScrollBehavior::exit_until_collapsed(
    state.clone(),
    TOP_APP_BAR_LARGE_HEIGHT,
);
let connection = behavior.nested_scroll_connection().unwrap();
```

将连接挂到内容滚动节点：

```rust
Column::new()
    .modifier(
        Modifier::new()
            .vertical_scroll(scroll)
            .nested_scroll(connection),
    )
    .build(ctx, content);
```

当前支持 `Pinned`、`EnterAlways`、`ExitUntilCollapsed` 的 delta 消费模型；TopAppBar legacy `TopAppBarScrollBehavior::new(scroll, height)` 仍保留，用于兼容旧的共享 offset 用法。

## 当前边界

- wheel 和 drag 已使用实际 consumed delta，不再用 bool 表示“命中即消费”。
- fling 已通过 `dispatch_nested_scroll_fling` 接入 pre/post fling 链（`drag_scroll_up` 触发）；但 `on_pre_fling` 目前用 `velocity/60` 近似换算，完整 snap/decay handoff 仍可继续打磨。
- WindowInsets、overscroll、scrollbar 和 accessibility semantics 不属于当前切片。
