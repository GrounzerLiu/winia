# Scroll extras: clipping, overscroll and item animation

Three Compose 1.7/1.8 additions around a scrolling container.

## `Modifier::clip_scrollable_container(orientation)`

Compose's `clipScrollableContainer` clips a scroll container on its SCROLL axis while leaving
`MAX_SUPPORTED_ELEVATION` (30 dp) of room on the cross axis, so a shadow drawn there is not cut off.
winia already clips every scroll viewport exactly at its bounds; this modifier widens that clip by
30 dp on the cross axis only (`render_pass1`'s scroll block). The main axis stays exact, so scrolled
content still cannot paint over a sibling.

## The fling bend never showed (2026-11-02, reported by the user)

A fling that travelled to an edge applied its pull - `effect=true`, raw 33, stretch 29.6 - and the
screen did not move. The default factory was the cause: `LOCAL_OVERSCROLL_FACTORY`'s default closure
built a NEW `Arc` on every `current()` call, and a factory identity is a pointer, so every build minted
a fresh effect for the same scroll state. Measured: the same slot's effect state id changed on every
frame (213/228/230/232) while the fling pulled StateId(234) - an instance the next build discarded.

It is one static value now (`PLATFORM_FACTORY`). Re-measured: the slot keeps StateId(38), and the
fling's 25.4 px pull is painted for 76 frames. Regression tests: `the_default_factory_keeps_one_identity`
(churns the allocator between two reads) and `the_effect_is_stable_across_rebuilds` (pulls once, then
asserts the pull survives a rebuild). Both fail with a per-read factory. `a_pull_shifts_the_painted_content`
closes the loop in pixels: a 40 px pull must lift the content off the viewport bottom (red at rest,
background after the pull), and it fails if the render stops reading `stretch()`.

## Reviewed 2026-11-02: three defects the review found

- **A restored offset outlived its content.** `remember_saveable` restores 240 into a list that now
  scrolls 0; the container then draws blank space until the next gesture. Plain containers now clamp
  during measure (`measure/engine.rs`, both axes), the way the lazy policy already did. Test:
  `a_restored_offset_past_the_end_is_pulled_back`.
- **A key step fed the rubber band.** ArrowDown 20 px from the end left the offset at the end AND a
  24.97 px stretch: the spill went into `apply_scroll_delta`, which hands its leftover to the effect.
  Key steps are now clamped to the room that is left, so only a POINTER gesture bends the content.
  Test: `a_key_step_stops_at_the_end_without_bending_the_content`.
- **An inline factory made a new identity every rebuild**, so every scroll state minted a new effect
  (losing a pull in flight) and the provider dirtied its subtree each frame. `remember_overscroll_factory`
  keeps one identity; the per-state cache is also capped at four factories. Test:
  `a_remembered_factory_is_one_identity_across_rebuilds`.

## `Modifier::overscroll(effect)` (and the default)

Overscroll is ON BY DEFAULT for scroll containers, the way Compose does it: `LocalOverscrollFactory`
has a default factory (`foundation/Overscroll.kt:341-350`), and `Modifier.verticalScroll` opts into it
(`Scroll.kt:259` `useLocalOverscrollFactory = true`). winia mirrors that shape with
`winia::overscroll::remember_overscroll_effect(ctx)`: every `LazyColumn`/`LazyRow` (and so every
`Pager`) asks for it while building, so a list bends at its edges with no setup.

```rust
// a subtree without the bounce - Compose: `LocalOverscrollFactory provides null`
disable_overscroll(ctx, |ctx| { list(ctx); });
// or a different one everywhere inside
with_overscroll_factory(ctx, Some(Arc::new(|| OverscrollEffect::new().reach(40.0))), |ctx| page(ctx));
```

A hand-built `Column + Modifier::vertical_scroll(state)` gets it too: the BUILDER attaches the
ambient effect while it runs, and builders run inside the composing body, where the thread-local
provider stack already holds this subtree's factory. The instance is cached on the scroll state per
factory, so a rebuild hands back the same effect instead of one that drops a pull in flight.

Two consequences worth knowing:

- the LAST `Overscroll` in a chain wins, so `.vertical_scroll(s).overscroll(&mine)` uses `mine`;
- a modifier built OUTSIDE composition (a cached one, a test fixture) reads the default factory and
  gets an effect - that is what `CompositionLocal::current()` means everywhere else.

## Attaching it by hand (`Modifier::overscroll(effect)`)

`OverscrollEffect` (hoist it with `remember`) turns the delta a container cannot consume into a
rubber-band:

- `apply_scroll_delta` computes the unclamped target and the leftover past the start/end, feeds the
  leftover into `OverscrollEffect::pull`, and counts it as CONSUMED — so the gesture does not fall
  through to an ancestor scroll container (`app/scroll.rs`). The pull is measured in offset space
  while the consumed delta points the other way, so the absorbed amount is SUBTRACTED from the
  consumed delta; adding it would tell the ancestor the gesture went the other way, twice as far;
- the renderer translates the content by the accumulated pull along the scroll axis;
- every pull re-arms a 60 ms-delayed 220 ms tween back to 0, so releasing returns the content and
  rapid input keeps the tension. A pull that arrives while the 120 px cap is already reached re-arms
  the delay too (the value does not move), so a finger held at the cap does not see the return start
  underneath it.

A FLING that dies at a boundary bends too: the velocity left when the decay hits the limit becomes
`pull(velocity / 40)` px, so a 2000 px/s fling bends 50 px and anything past 4800 px/s reaches the
same 120 px cap. Compose hands the whole fling to the effect (`overscroll.applyToFling`,
`foundation/gestures/Scrollable.kt:717`); winia flings through its own decay and reports the remainder,
which is why the bend is a function of that remainder rather than of the fling's whole shape.
Reverse containers skip it, exactly as they skip the delta path.

The pull is DAMPED, not linear: the effect accumulates the RAW finger travel and shows
`max_pull * (1 - e^(-raw / reach))`. The first pixels are one-to-one (slope `max_pull / reach`),
every further pixel buys less, and the cap is an ASYMPTOTE - no drag reaches it, which is what makes
the edge feel like rubber instead of a wall with a hard stop. `max_pull` (default 120 px) and `reach`
(default `max_pull`; smaller = stiffer) are builder knobs, so the feel can be tuned per app without a
recompile. `pull` still returns the WHOLE leftover, so an ancestor never sees the gesture; only the
displayed shift is damped.

Deliberate scope: the content SHIFTS past the edge (there is no content scaling and no platform
glow), the pull is capped at 120 logical px, and containers with `reverseLayout` skip the effect
(their offsets are mirrored, so a signed pull would point the wrong way).

## `Modifier::animate_item()`

Place this on a lazy item's root. The lazy list keys each item (`items_from`'s key function) and
remembers, per key, the rectangle it occupied on the previous frame. When the data reorders and the
key moves, the item is painted at its OLD position and animates to the new one; the item's wrapper
node carries the animation, so the whole item moves. An item outside the composed window is kept as
an off-screen state, not treated as removed; returning to the window does not replay a fade-in.

The same marker is honored by `FlowRow` / `FlowColumn` directly: a keyed flow child whose slot
moves gets its old position as a two-dimensional paint offset, plus the fade channels in its
`ItemAnimationSpec`. Flow uses the child's stable slot key, so pair it with `ctx.key` /
`ctx.keyed_range` when items can reorder.

Scope: placement, fade-in and fade-out. Size is separate: wrap the item in `AnimatedSize` when its
cell should grow or shrink smoothly. The exit picture is captured while the item is rendered and
drawn by the owning container, so it follows that container's clip and scroll translation.
