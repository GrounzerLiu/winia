# Shared element transitions: Compose API gap tracking

> Baseline: Compose `animation` module `SharedTransitionScope` API surface
> (2025 — including `OverlayClip`, `renderInOverlayDuringTransition`,
> `sharedElementWithCallerManagedVisibility`, `SharedTransitionDefaults`),
> checked against Winia `exp/shared-transition` at `ac6e621`; the overlay
> pass added after that commit is tracked below as P2.
> Shipped behavior: `docs/shared-element-transition.md` (architecture),
> usage: `docs/shared-element-usage.md`.
> Convention: `- [x]` shipped, `- [ ]` open. Update a row when its status
> changes; keep the priority column honest.

## P0 — half-day alignment (API shape, no behavior risk)

- [x] `isTransitionActive` on the scope — true while any flight is
  non-terminal. Unlocks dimming/input-gating patterns during transitions.
  Trivial: derive from the flight maps.
- [x] `SharedTransitionDefaults` constants object (default
  `BoundsTransform`, overlay defaults). Trivial.
- [x] `placeHolderSize` parameter on `shared_element()` (default `JumpCut`,
  matching current behavior). Trivial.

## P1 — small behavior additions

- [x] `PathMotion::ArcBelow` / `ArcAbove` — Compose `ArcSpline.Arc` math
  ported (quarter ellipse, arc-length-uniform travel, either-dimension
  degenerate rule); `path` parameter on both markers, resolved from the
  target marker. API shape intentionally differs (flight-level path, not
  per-keyframe `using ArcMode`). Deviation: overshoot pins the center at
  the nearer endpoint (documented in code).
- [x] `zIndexInOverlay` — `z_index` parameter on both markers (default
  0.0); the transition layer sorts back-to-front (stable — equal z keeps
  insertion order). Both ends are layer roots by default since the
  overlay-escape pass landed, so a flight's target and its ghost order
  against each other and against other flights; `z_index` on a marker
  also participates in that ordering.
- [x] `enter` / `exit` on `shared_bounds()` — target plays enter, source
  plays exit (each side declares its own); fade channels claimed per-end
  (fade defaults reproduce the crossfade); slide/scale evaluated at flight
  progress in device space (before the flight scale); expand ≈
  scale-about-edge + forced clip; Morph skips channels. `Element` endpoints
  always crossfade (no enter/exit, like Compose).

## P2 — medium (layout/render coordination)

- [x] Overlay-escape semantics (`renderInOverlayDuringTransition`) — shipped
  as the transition-layer elevation pass. Both ends of a flight render in
  the layer by default (`render_in_overlay = true`), escaping ancestor
  clips and ancestor layer transforms (alpha/scale) and painting above
  non-shared content; elevated endpoints are hit at their lerped rect even
  outside their ancestors' bounds. `render_in_overlay = false` keeps the
  old in-tree painting. Two documented deviations: the leaving end is
  always detached in Winia, so the flag is a no-op there; `Morph`
  (same-screen `animateBounds`) never elevates, matching Compose's
  animateBounds which stays in place.
- [ ] `OverlayClip` / `clipInOverlayDuringTransition` — the clip *inside*
  the overlay. Not built: Compose's default is the parent `sharedBounds`'
  resolved clip path, and nested shared markers are still unsupported
  (§10.8 of the architecture doc), so the Compose default resolves to
  "no extra clip" — which is already what the layer does. `ScaleToBounds
  { clip: true }` keeps clipping the flying pair to the lerped rect.
  Revisit together with nested markers.
- [ ] `renderInSharedTransitionScopeOverlay` (keep bottom bar / FAB on top
  during transitions) — Tier 1 ghost-above-scrim covers the cross-composer
  half, and elevated endpoints now paint above the tree; Tier 0 non-shared
  subtrees (a bar that must stay ABOVE a flying hero) still need their own
  elevation. The layer machinery is now in place to build it on.
- [ ] `ResizeMode::RemeasureToBounds` — currently degrades to scale with
  one `debug_log!` per flight start. Needs per-frame remeasure at the
  lerped size; paragraph-cache quantization is the known risk
  (1px size quantization to protect the cache).
- [ ] `PlaceHolderSize::ContentSize` / `AnimatedSize` — only `JumpCut`
  today (layout snaps to end state, the flying pair covers the pop);
  with elevation both ends are layer roots, which is compatible with
  either placeholder contract when it lands.
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
- [x] Scroll-exact paint/clip/hit (frozen per-end ancestor sums) — kept
  for opt-out targets; elevated ends carry a zero sum and the layer
  supplies the scroll-corrected absolute origin instead (same pixels).
- [x] Single unclamped flight-t; engine-release-gated completion.
- [x] Live-endpoint hit routing; flight-id tagged teardown.
- [x] Identity-keyed morph baselines; bouncy spring overshoot renders.
- [x] Transition-layer elevation (`renderInOverlayDuringTransition`):
  in-tree paint skip + rootless layer re-render + layer-order hit routing,
  both tiers, `z_index` ordering across both ends.
