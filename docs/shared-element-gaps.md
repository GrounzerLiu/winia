# Shared element transitions: Compose API gap tracking

> Baseline: Compose `animation` module `SharedTransitionScope` API surface
> (2025 — including `OverlayClip`, `renderInOverlayDuringTransition`,
> `sharedElementWithCallerManagedVisibility`, `SharedTransitionDefaults`),
> checked against Winia `exp/shared-transition` at `ac6e621`.
> Shipped behavior: `docs/shared-element-transition.md` (architecture),
> usage: `docs/shared-element-usage.md`.
> Convention: `- [x]` shipped, `- [ ]` open. Update a row when its status
> changes; keep the priority column honest.

## P0 — half-day alignment (API shape, no behavior risk)

- [ ] `isTransitionActive` on the scope — true while any flight is
  non-terminal. Unlocks dimming/input-gating patterns during transitions.
  Trivial: derive from the flight maps.
- [ ] `SharedTransitionDefaults` constants object (default
  `BoundsTransform`, overlay defaults). Trivial.
- [ ] `placeHolderSize` parameter on `shared_element()` (default `JumpCut`,
  matching current behavior). Trivial.

## P1 — small behavior additions

- [ ] `PathMotion::ArcBelow` / `ArcAbove` — enums and marker fields exist;
  both push sites hardcode `Linear`. Quadratic-bezier center offset in the
  flight transform, zero layout impact.
- [ ] `zIndexInOverlay` — multiple pairs currently paint in arena order.
  Sort transition roots by z at render.
- [ ] `enter` / `exit` on `shared_bounds()` — wire the existing
  AnimatedVisibility system alongside the flight.

## P2 — medium (layout/render coordination)

- [ ] Overlay-escape semantics (`renderInOverlayDuringTransition`,
  `OverlayClip`) — the one real behavioral gap: Winia paints fully
  in-tree, so a hero flying outside a scroll viewport is cut by ancestor
  clips; Compose elevates to an overlay escaping parent clip and layer
  transforms. Fix: render transition roots in an overlay pass that skips
  ancestor clips. (`clipInOverlayDuringTransition` folds into this.)
- [ ] `renderInSharedTransitionScopeOverlay` (keep bottom bar / FAB on top
  during transitions) — Tier 1 ghost-above-scrim covers the cross-composer
  half; Tier 0 needs non-shared subtree elevation.
- [ ] `ResizeMode::RemeasureToBounds` — currently degrades to scale with
  one `debug_log!` per flight start. Needs per-frame remeasure at the
  lerped size; paragraph-cache quantization is the known risk
  (1px size quantization to protect the cache).
- [ ] `PlaceHolderSize::ContentSize` / `AnimatedSize` — only `JumpCut`
  today (layout snaps to end state, the flying pair covers the pop).
- [ ] `skipToLookaheadSize` — no lookahead system exists; the equivalent
  ("measure at end size from frame one") needs the end bounds before
  layout.

## P3 — deferred with rationale

- [ ] `sharedElementWithCallerManagedVisibility` — serves pagers/carousels
  whose visibility is not scope-driven. Winia has no pager component yet;
  revisit with it.
- [ ] True solid-color morph (linear/Oklab lerp of Background/Border) —
  scoped and discussed; default stays alpha crossfade. Explicit user hold.
- [ ] Tier 2 bitmap flights — deliberately unbuilt, no trigger exists
  (cross-window needs an OS-level overlay).
- [ ] Non-shared choreography stagger (old screen 0→0.3, new screen
  0.7→1) — far-term, needs AnimatedVisibility windows.

## Shipped (reference — do not reopen)

- [x] Tier 0 switch flights, same-screen morphs, Tier 1 cross-composer
  (main ↔ overlays), retarget-lite.
- [x] Scroll-exact paint/clip/hit (frozen per-end ancestor sums).
- [x] Single unclamped flight-t; engine-release-gated completion.
- [x] Live-endpoint hit routing; flight-id tagged teardown.
- [x] Identity-keyed morph baselines; bouncy spring overshoot renders.
