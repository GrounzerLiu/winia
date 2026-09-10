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
- [x] `renderInSharedTransitionScopeOverlay` (keep bottom bar / FAB on top
  during transitions) — shipped as
  `Modifier::render_in_shared_transition_scope_overlay(&scope, z_index)`.
  While the scope has a non-terminal flight the marked **non-shared** subtree
  is a layer root again (skipped in the tree walk, re-drawn untransformed at
  the end of the layer), z-sorted by `zIndexInOverlay` against the shared
  endpoints (which default to 0.0); outside a flight it is ordinary tree
  content. This is the Compose-sanctioned answer for "content slides under
  pinned chrome", and unlike the shared element's own
  `render_in_overlay = false` it covers BOTH ends — the detached leaving ghost
  included, which tree order can never cover. Membership is decided once per
  frame (own poll + the cross-poll's window union), so a peer composer's
  chrome elevates in the same frame. Deviation: Compose's second parameter,
  the `renderInOverlay: () -> Boolean` gating lambda, is not exposed — winia
  always gates on "this scope has a non-terminal flight", which is Compose's
  default. Demo:
  `cargo run -p winia --example shared_transition_pinned_bar_demo`.
- [x] `ResizeMode::RemeasureToBounds` — shipped. The entering end is measured
  with **fixed constraints of the animated bounds** every frame, so the content
  re-lays-out instead of being scaled (`scale = 1` in the layer pass, and hit
  testing maps 1:1). Driven by a per-frame `State<FlightMeasureFrame>` read
  during measure plus a re-seeded `layout_dirty_keys` entry, so it re-measures
  without recomposing (pinned by `flight_layout_contract_matrix`, which asserts
  zero scenario rebuilds across a whole flight). Deviations: only the ENTERING
  end re-measures (the leaving end is detached and frozen — its content has no
  live layout), and because writers run after layout the layout trails the
  flight by ~1 frame. Compose's guidance is preserved: `ScaleToBounds` stays
  the default for `sharedBounds`, and text is still better off scaled.
- [x] `PlaceHolderSize::ContentSize` / `AnimatedSize` — shipped as the reported
  size the parent observes: `AnimatedSize` reports the animated size (siblings
  reflow with the flight), `ContentSize` / `JumpCut` keep the target size so
  the surrounding layout holds still. The target's natural size is captured the
  frame the end resolves. Deviation: the OUTGOING end's space is not preserved
  in a screen switch — its whole tree is gone, so there is no parent layout to
  hold open (Compose keeps it because the old screen stays composed); for the
  same-screen morph the node stays and both policies are exact.
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
- [x] Scroll-exact paint/clip/hit at resolve time (frozen per-end ancestor
  sums) — kept for opt-out targets; layer ends carry no sum and the layer
  supplies the scroll-corrected absolute origin instead (same pixels). NOT
  covered: a scroll that happens *during* a flight — a layer end is pinned to
  its frozen window rect while an opt-out end keeps following the content, so
  the two ends can diverge. No test drives a mid-flight scroll.
- [x] Single unclamped flight-t; engine-release-gated completion.
- [x] Live-endpoint hit routing; flight-id tagged teardown.
- [x] Identity-keyed morph baselines; bouncy spring overshoot renders.
- [x] Transition-layer elevation (`renderInOverlayDuringTransition`):
  in-tree paint skip + rootless layer re-render + layer-order hit routing,
  both tiers, `z_index` ordering across both ends.
