# Layout: a container with your own measure policy

Compose's `Layout(modifier, measurePolicy) { content }`. Every built-in container in winia is this plus a
policy, so a caller who needs a layout the library does not ship writes a `MeasurePolicy` and builds it
here instead of assembling the composition glue (`ctx.start_restartable_group` / `end_restartable_group`)
by hand.

## API

```rust
Layout::new(policy)
    .modifier(Modifier::new().fill_max_width())
    .build(ctx, |ctx| { /* content */ });
```

`policy` is any `MeasurePolicy` (the same trait Row/Column/Stack implement: `measure` returns a size and
one `Placement` per child, `place` writes them into the arena). The four intrinsic methods default to
measuring the content, so a policy only overrides them when the approximation is wrong for it.

## Rules

- The content is a PASS-THROUGH group. A state read that decides the content must happen inside the
  content closure; a read that decides the MEASUREMENT may happen inside `measure`, where it registers a
  layout dependency and a write re-measures.
- The policy is installed when the group enters. A different policy on the same call site replaces it as
  soon as the caller re-enters; a caller who changes the policy from state must therefore read that state
  in a scope that composes when it changes (the same rule as the built-in containers).
- The container's size is whatever the policy reports. The engine does not clamp it (only a scroll
  container clamps its own box), which is why `Row`/`Column` floor at the incoming minimum themselves.

## Example: the smallest complete policy

`layout/custom.rs`'s test carries a `TopLeftStack` policy: measure every child with the incoming maximum,
report the widest and tallest child, place all of them at the origin.

## Tests

`cargo test -p winia --lib -- layout::custom` — the custom policy measures and places its content, and the
container's modifier applies.
