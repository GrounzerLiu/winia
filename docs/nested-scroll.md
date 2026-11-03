# Nested Scroll

Winia provides the Compose-style nested-scroll protocol: an ancestor container can consume part of a
scroll delta before or after the child does.

## Protocol

- `ScrollDelta` / `ScrollVelocity`: 2-D scroll displacement and velocity.
- `NestedScrollSource`: `Wheel`, `Drag`, `Fling`, `SideEffect`.
- `NestedScrollConnection`:
  - `on_pre_scroll`
  - `on_post_scroll`
  - `on_pre_fling`
  - `on_post_fling`
- `NestedScrollDispatcher`: dispatches pre in ancestor order and post in reverse order.

The scroll sign follows the existing winia convention: a negative vertical delta moves the content up
and increases the scroll offset.

## Current integration

- `Modifier::nested_scroll(connection)` attaches an ancestor connection.
- Wheel, debug scroll and drag moves dispatch as pre -> child partial consume -> post; every return
  value is always limited to the available delta.
- The fling after a drag runs through `dispatch_nested_scroll_fling`: pre-fling -> child fling ->
  post-fling. When the child's decay hits a clamp boundary, the animation layer hands that frame's
  instantaneous remaining velocity to ancestor post-fling, inner to outer.
- `TopAppBarScrollBehavior::{pinned, enter_always, exit_until_collapsed}` provide a standalone
  `TopAppBarState` connection; `nested_scroll_connection_with_scroll(scroll)` binds the child scroll
  state and mirrors `content_offset` from the real offset, so nothing accumulates a drift.
- The legacy `TopAppBarScrollBehavior::new(scroll, expanded_height)` is kept for pages that already
  share an offset.

`TopAppBarState` keeps its own:

- `height_offset_limit`
- `height_offset`
- `content_offset`

and derives `collapsed_fraction()`, `overlapped_fraction()`, `current_height()` and friends.

Building the behaviour:

```rust
let state = TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT);
let behavior = TopAppBarScrollBehavior::exit_until_collapsed(
    state.clone(),
    TOP_APP_BAR_LARGE_HEIGHT,
);
let connection = behavior.nested_scroll_connection().unwrap();
```

Attaching the connection to the content's scrolling node:

```rust
Column::new()
    .modifier(
        Modifier::new()
            .vertical_scroll(scroll)
            .nested_scroll(connection),
    )
    .build(ctx, content);
```

`Pinned`, `EnterAlways` and `ExitUntilCollapsed` are the supported delta-consumption models; the
legacy `TopAppBarScrollBehavior::new(scroll, height)` stays for the old shared-offset usage.

## Current boundaries

- Wheel and drag use the actual consumed delta, not a bool that means "hit = consumed".
- Fling is dispatched through the pre/post callbacks; the TopAppBar pre-fling consumes velocity
  against the current collapsible range, and when the child's decay hits the boundary it hands the
  instantaneous remaining velocity to ancestor post-fling.
- WindowInsets, overscroll, the scrollbar and accessibility semantics are outside this slice.