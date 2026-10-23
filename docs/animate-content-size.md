# animateContentSize

Compose's `Modifier.animateContentSize()` animates the box a node reports to its parent. winia
exposes the same behaviour as a composition-owned modifier helper:

```rust
let animated = ctx.animate_content_size(SpringSpec::default());
Column::new()
    .modifier(animated)
    .build(ctx, |ctx| { /* content whose height may change */ });
```

`ComposeCtx::animate_content_size` creates the two handles the pure `Modifier` cannot own:
the animated `Size` and its last target. The returned modifier also clips the node to its animated
bounds, so content that has already taken its new size does not paint outside a box still growing or
shrinking.

## Behaviour

- First measurement snaps to the content's size.
- A later target change starts from the current animated size.
- Only the node's reported box is animated. Child placements stay where the last layout put them;
  the clip keeps overflow out of the animated box.
- Every frame the animation runs, the node is re-measured and the parent sees the animated size.
- The value is clamped into the incoming constraints, like Compose's `animateContentSize`.

## Scope

The helper animates the node it is attached to, not an arbitrary position in a modifier chain.
A pure `Modifier::animate_content_size()` is not provided: a modifier is data built without a
`ComposeCtx`, and winia's modifier nodes cannot yet measure a child and decide their own layout.
`AnimatedSize` remains the container form with the same mechanics; this helper is the convenient
modifier-shaped entry point.

## Tests

```bash
cargo test -p winia --lib animate_content_size_springs_the_reported_box
cargo test -p winia --test ui_test --features debug-server flow_features_demo_animates_a_reflow
```
