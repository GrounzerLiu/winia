# Shared Element Transition — Design

> Status: design accepted, implementation in phases on `exp/shared-transition`.
> Source of truth for the architecture. Chat history (Chinese) holds the
> deliberation; this document holds the decisions.

## 1. Background: what Compose shared elements really are

Stripped of API sugar, Compose's `SharedTransitionLayout / sharedElement /
sharedBounds` does exactly four things:

1. **Matching**: `rememberSharedContentState(key)` links a disappearing element
   with an appearing element inside a scope, by key.
2. **Known bounds on both ends**: the Lookahead pass measures the destination
   bounds *before* the transition starts. Old bounds come from the current
   layout, new bounds from lookahead — never guessed, never a frame late.
3. **Overlay ghost**: during the flight the originals hide (placeholders keep
   space) and a "ghost" lerps its rect from start to end under a
   `BoundsTransform` (default: spring on `Rect`). `sharedElement` (same
   content) ferries content; `sharedBounds` (different content) paints content
   into the other side's bounds (container semantics).
4. **Deformation policy**: `ResizeMode.ScaleToBounds` (whole-scale, optional
   clip) vs `RemeasureToBounds` (re-layout at interpolated size);
   `PlaceHolderSize.contentSize / animatedSize`; `renderInOverlayDuringTransition`,
   `zIndexInOverlay`, `OverlayClip`; `animatedVisibilityScope` choreographs the
   non-shared content.

## 2. Winia capability mapping

| Capability | Winia status | Verdict |
|---|---|---|
| Matching keys | `ctx.key`, `remember_at_key`, statement-level key system | Reuse directly, add scope isolation |
| Destination bounds foresight | No lookahead; single measure pass + constant folding | Gap, but substitutable (§3.2) |
| Overlay rendering | Independent Composers, `OverlayDesc` (anchor/position/enter-exit/`click_passthrough`), z-ordered render | Natural ghost vehicle |
| Ghost content | None. `AnimatedContent` is single-generation (fade out → swap → fade in, never both ends coexisting) | Core design point (§3.3) |
| Rect interpolation | `AnimatableValue` is exact only for `f32` (Spring) and `Color` (Tween); `Offset`/`Size` are norm placeholders, Spring degrades to Tween | Add an exact `Bounds` animatable |
| Scale deformation | `GraphicsLayerParams` (scale/alpha/translation/clip/origin) + render-time `peek`, zero recomposition | `ScaleToBounds` works out of the box; Skia text scales crisply through the CTM |
| Remeasure deformation | Two-phase deps: measure-time `get()` registers a layout dep → re-measure without recompose, per-frame capable | Shipped (§5) — plus a per-frame `layout_dirty_keys` re-seed, because `layout()` clears `layout_dirty` tree-wide every pass |
| Absolute coordinates | `node_abs_position` (scroll-corrected) + post-layout `position`/`measured_size` | Bounds capture is free |
| Bitmap snapshot | `image_snapshot_with_bounds` (already used by backdrop blur) | Backup ghost path, not primary |
| Scope choreography | `AnimatedVisibility/Content/Size/Crossfade`, `nav.rs` all exist | v1: timing coordination only; deep integration in v2 |

## 3. Architecture: three decoupled layers

The dual-morph design (Tier 0) and the transplant design (Tier 1) each got
half right. The final architecture decouples three concerns so Tier 0 ships
first and later tiers plug in with zero rework of matching or engine code:

```
Matching layer      Who flies with whom? (scope, key) live maps per composer +
                    same-frame pending stash; no shared registry needed
    ↓
Flight engine       How does it fly? progress-driven kinematics: rect path +
                    opacity + shape + choreography. Pure function of progress.
    ↓
Content providers   What pixels fly? Tier0 live same-tree / Tier1 retain-in-owner
                    cross-composer (main ↔ overlays, same canvas). Auto-selected.
```

### 3.1 No-lookahead bounds discovery (key insight)

Winia needs no lookahead pass:

- **Old bounds**: after removal, `Composer.prev_nodes` (`slot_key →
  CachedNode{measured_size, position}`) still holds last frame's layout — the
  structural equivalent of what lookahead provides for the old end.
- **New bounds**: one switch-frame layout after Enter rebuild carries fresh
  bounds. The flight starts when both ends are known — one frame (~16 ms)
  later than Compose, imperceptible.

Flight state machine: `Idle → AwaitingBounds (one layout) → Flying
(progress 0→1) → Finishing (detach ghost) → Idle`. No composer or layout
engine changes required for bounds discovery.

### 3.2 Content providers

- The **target end is always live**: it is normally composed, fully live
  (a ticking counter keeps ticking mid-flight — impossible for bitmaps).
- The **source end**: same composer → Tier 0 (detach + freeze in the owner's
  arena and transition layer; freezing is semantically fine for leaving
  content); another composer (main tree ↔ overlay, same window/canvas) →
  Tier 1 (identical retain-in-owner; only rendering and flight ownership
  differ — see §3.4). No `DescNode` moves: all Winia composers render vector
  content from their own arenas, so cross-composer needs no transplant.
- Auto-selected by the coordinator; never user-visible.

### 3.3 Tier 0 dual-morph (v1 implementation)

At the switch frame the vanishing source slot is *retained* instead of being freed:
`detach_source` takes it out of `prev_node_by_key` (the map the prev drain walks), marks the
node AND EVERY DESCENDANT in `reused_nodes` so neither the drain nor the arena pool may reclaim
them, and unlinks the node from any parent still listing it. Its arena node is then detached
into a composer-level transition layer: excluded from all measure/place, rendered after the main
tree via an extra `render_pass1` at its frozen absolute rect. That shelter is what keeps the
ghost's CONTENT: with only the root sheltered, the pool handed a descendant's index to a node of
the new tree, so the ghost listed a fresh unmeasured node and painted an empty card — the
"animation starts fully transparent" report. Both ends then render the **same** lerped rect
`L(p) = lerp(S, T, p)` with transform origin top-left (simpler than Compose's
center origin, self-consistent):

- source: `translate = L(p) − P_s`, `scale = lerp(S_s,S_t,p)/S_s`, `alpha = 1−p`
- target: `translate = L(p) − P_t'`, `scale = lerp(S_s,S_t,p)/S_t`, `alpha = p`

Endpoint continuity is exact: at p=0 the source matches the pre-switch frame
pixel-for-pixel and the target is invisible; at p=1 the source is invisible
(freeing it is glitch-free) and the target is an identity transform. All
render-phase `GraphicsLayer` + `peek`: **zero recomposition, zero remeasure
during flight** (assertable in tests: compose count unchanged across a flight).

### 3.4 Tier 1 cross-composer (main tree ↔ overlays, shipped)

Same window, same canvas, different `Composer` (main tree vs Popup/Dialog
composers, each with independent slot trees but a shared event-loop thread).
No transplant — the source retains in the OWNER arena exactly like Tier 0;
only three things differ:

- **Matching**: per-composer live maps can't see each other, so the owner
  retain hook stashes unmatched disappearances (`pending_cross`) and a
  window-level cross-poll (after ALL composers laid out, before render) pairs
  them with freshly-appeared counterparts elsewhere (`fresh_shared`
  per-composer appearance records — peer prev maps absorb appearances before
  cross-poll runs, so frame records are the only freshness source). Stable
  duplicates (same hero shown in two places) never pair: freshness +
  Tier0-busy guards. Unmatched stashes free same-frame (invisible).
- **Ownership**: Tier1 flights live in the MAIN composer map (main outlives
  overlays); each flight records both endpoint composer ids. Owner polls skip
  Tier1; cross-poll drives progress, both-end visuals, completion and cancel.
  Overlay composers run the standard post-layout poll (Tier 0 inside dialogs
  works unchanged).
- **Frames**: flight bounds are canonical window coords; each writer subtracts
  its composer's `screen_origin` (main renders untranslated; overlays render
  translated by `screen_pos`). While a cross flight is active the main
  transition layer renders AFTER overlays (ghost above modal scrims).

Scope identity crosses composers for free: overlay content inherits the scope
handle through the existing CompositionLocal snapshot replay, so no shared
registry object is needed anywhere. Hit routing works across composers for
dialog-region clicks (overlay hit paths hit-test the live target with the
same remap); ghost clicks outside the overlay window pass through (documented
v1 limitation).

### 3.5 Tier 2 bitmap (deliberately unbuilt)

Honest scope: Tier 2 has no trigger. Retention guarantees content is never
lost (nothing to snapshot after the fact), and closing endpoints cancel
flights with atomic teardown (nothing to snapshot during teardown). The two
hypothetical cases from the original design both resolve without bitmaps, so
no Tier 2 code ships; the snapshot machinery (`capture.rs`) remains available
if a trigger ever materializes.

### 3.6 Overlay pass (Compose `renderInOverlayDuringTransition`, shipped)

Compose lifts a shared element into the scope's overlay for the duration of
the flight so it "escapes the parent's bounds and its layer transformations
(albeit alpha and scale)" and "renders on top of other non-shared UI
elements". Winia has no overlay node to lift into — it has something
equivalent and older: the **transition layer**, the rootless pass that
already drew detached sources after the tree walk.

The overlay pass generalizes that pass instead of adding a second one:

- **Elevation is a render-phase decision.** `TransitionVisual.elevated`
  (sampled when the end resolves — for a target from that end's own marker,
  `true` unconditionally for the detached source, forced `false` for `Morph`)
  makes `render_pass1` skip the subtree in the in-tree walk and makes the
  coordinator re-render it as a layer root via `render::render_node_at`.
  Layout position, slots and state are untouched. Hit testing moves with the
  paint (the layer is tested first, see below), which is what lets an
  element flying outside its container still receive taps.
  A canvas clip can never be un-set by a descendant, which is exactly why
  the escape has to be a separate root rather than a canvas trick.
- **Origin frame.** The layer canvas carries no ancestor translate, so a
  layer root renders at its scroll-corrected absolute origin
  (`abs_rect_upward`) and must not apply the ancestor add-back; an opt-out
  end keeps the in-tree path and its frozen ancestor sum. Both are decided
  by the same `layer_root` bit that routes the node (`render_pass1`), so
  paint, clip and hit cannot disagree.
- **Membership is written, not discovered.** The visual writers record
  elevated roots in `Composer::elevated_roots` (cleared each poll) because a
  Tier 1 peer's flight lives in the MAIN composer's map; a peer scanning its
  own flights would never see it. `rebuild_layer_order` unions those with
  the detached sources and z-sorts (`zIndexInOverlay`), with elevated
  targets painted *under* their ghost — the compositing order the
  tree-then-ghost passes used to produce. That ordering holds at EQUAL
  `z_index` only: the sort is stable and `elevated_roots` is pushed before
  `transition_layer`, so an explicit `z_index` on the target reorders it (there is
  no role tie-break, and no test covers ghost-vs-target at unequal z). Render and
  hit testing both read that one list, so **within one composer** paint order and hit
  order cannot drift.
  Across composers they do, deliberately (review 2, R3-F4, resolved by analysis):
  while a cross flight runs the app paints the main layer AFTER the overlays, so a
  ghost or elevated chrome draws above an open dialog, while input is resolved
  overlays-first — a tap in the overlap reaches the dialog. That matches Compose on
  the INPUT side (Compose's shared-element overlay is draw-only; input goes through
  the layout, and a Dialog is a separate window above it), and the paint order is a
  winia-specific choice: our overlays are composers inside the same window, and
  drawing the layer last is what keeps a Tier-1 ghost visible while it flies into a
  panel.
- **Hit routing follows paint.** `hit_test_with_flights` walks the layer
  topmost-first: a source ghost routes into its live target (fraction
  mapping, unchanged), an elevated target reverses its own flight transform
  (`remap_hit`) and descends its live subtree — with ancestors *not*
  rejecting the point, since the element is painted there. This is what
  makes a Tier 1 tap land where Compose lands it: the overlay composer's
  own hit test now finds the flying target even outside its ancestors'
  bounds, while the cross-composer ghost still ignores taps.
- **Deliberate exemptions.** The leaving end is always detached, so the
  flag is a no-op there (the layer is its only home). `Morph` (same-screen
  `animateBounds`) never elevates — it is a layout-driven resize, not a
  cross-composable flight, and Compose's `animateBounds` stays in place
  too. `OverlayClip` (a clip *inside* the overlay) is not built: its
  Compose default derives from the parent `sharedBounds`, which requires
  nested markers; absent those the default already means "no extra clip".
- **Chrome opts back in** (`renderInSharedTransitionScopeOverlay`). The
  escape above is a problem for pinned bars: a hero flying into a bar's strip
  would paint over it. Compose's answer is a modifier on the bar, not on the
  shared element, and it maps cleanly onto the same layer:
  `Modifier::render_in_shared_transition_scope_overlay(&scope, z)` makes the
  marked **non-shared** subtree a layer root for as long as the scope has a
  non-terminal flight — skipped by the tree walk, re-drawn untransformed at
  the end of the layer, z-sorted against the endpoints (which sit at 0.0).
  Because the layer is drawn after the tree, this covers the detached leaving
  ghost too, which tree order can never cover (its `1 − p` opacity is all that
  shows through, fading to nothing). Membership is decided once per frame:
  each composer's own poll passes its flight scopes and `poll_cross_flights`
  re-runs it for every participant with the window union, so a peer composer's
  chrome elevates in the same frame as the flight it does not own. Deviation
  from Compose: the `renderInOverlay: () -> Boolean` gating lambda is not
  exposed — the elevation window is always "this scope has a non-terminal
  flight".

## 4. Flight engine: unified kinematics

The engine knows one `Flight{id, start: Rect, end: Rect, progress: State<f32>,
spec}`. Position, size, opacity and shape are all pure functions of progress:

- **Spring rides the scalar**: `BoundsTransform::spring(stiffness, damping)`
  springs scalar progress 0→1. Overshoot lerps out of bounds — exactly the
  Compose spring look — with no vector-physics invention (sidesteps the
  `AnimatableValue` scalar-only limitation).
- **Path**: straight line + arc (Compose `ArcMode` equivalent). Position is a
  render translation, so a quadratic Bézier costs nothing; control point =
  chord midpoint offset along the normal (ArcBelow/Above).
- **Origin**: top-left by default, center optional (matches Compose feel).
- **Universal shape morph**: every `Shape` variant (Rectangle, RoundedRect,
  TopRoundedRect, Pill, Circle) normalizes to rect + four corner radii
  (Pill and Circle both = radii `min(w,h)/2` against the box — Circle is Compose's
  percent-50 shape, so it is a circle only on a square box). Morph = rect lerp
  + radii lerp; intermediates are always valid rounded rects, endpoints exact.
  Border width lerps alongside. No same-kind restriction, no mid-point snap
  patches.
- **Clip when the marker asks for it**: `sharedBounds { resize:
  ScaleToBounds { clip: true } }` clips both ends to the current lerped
  rounded rect, so container content cannot overflow. `sharedElement` (and
  `ScaleToBounds { clip: false }`) deliberately does not clip — scaling into
  the bounds is enough there, and clipping would cut shadows. The `expand`
  enter/exit channel forces the clip on (see `shared_clip_for_kind`).

## 5. Layout contract (placeholder policy)

```rust
enum ResizeMode { ScaleToBounds { clip }, RemeasureToBounds }
enum PlaceHolderSize { JumpCut, ContentSize, AnimatedSize }
```

Both are frozen from the ENTERING end's marker when the flight resolves, and
both act on the same per-frame structure: `LayoutNode.flight_measure` holding a
`State<FlightMeasureFrame> { content, reported }` that the coordinator rewrites
every frame.

- `ResizeMode::ScaleToBounds` (default, Compose's too): the content is measured
  naturally and scaled into the lerped rect.
- `ResizeMode::RemeasureToBounds`: `content` carries the animated size, and the
  measure pass applies it as **fixed constraints** before the policy runs — the
  subtree reflows (text rewraps, rows resize). The resulting size IS the lerped
  size, so the render scale is 1 and hit testing maps 1:1.
- `PlaceHolderSize::AnimatedSize`: `reported` carries the animated size, so the
  PARENT reflows and siblings ride the flight.
- `PlaceHolderSize::ContentSize` / `JumpCut`: `reported` keeps the target size
  (captured the frame the end resolves), so the surrounding layout holds still.
  `JumpCut` is winia-only and shares this code path; Compose names its default
  `ContentSize`.
- `SharedKind::Element` resolves to `RemeasureToBounds` with no way to opt out,
  mirroring Compose's `sharedElement` ("will re-measure and relayout its child
  layout using fixed constraints derived from its animated size").

`measured_size` keeps ONE meaning — the box the node's own layout/paint
occupies; `reported` leaves the measure as the RETURN value, which is what
parents place by. That matters because parents write their placement size back
into `measured_size` (`place()`), so the two have to be stored apart:
`flight_content_size` carries the content box for the frames where the parent
was told something else, and render, clip, radii, `abs_rect_upward` and hit
testing all read that instead.

Zero recomposition: reading the frame during measure registers a LAYOUT
dependency, and the coordinator re-seeds the node's `layout_dirty_keys` entry
every frame while an override is attached — necessary because `layout()` resets
`layout_dirty` on the whole tree at the start of every pass, so a folded parent
would never descend into the override. The seed is skipped while nothing is
attached, so the default contract costs no layout at all. Every teardown that
drops an override (Tier 0 completion/cancel, and both Tier 1 paths through
`clear_transition_for_slot`) seeds once more, so the natural size returns in the
next pass; the clear also matches the flight id, because a successor flight can
own the same slot key by then.

Timing: writers run after layout, so the layout of frame N is driven by the
frame written at the end of frame N−1 — one frame, ~6% of a 300 ms flight.
EXCEPT on the frame a flight FIRST attaches its override: there the parent would
have been laid out from the entering end's NATURAL size (nothing constrained it
yet, and resolving the flight needs that frame's measurement), so the content
below the hero dipped and snapped back — reported from
`shared_transition_image_demo` with `PlaceHolderSize::AnimatedSize`. The app loop
now consumes a per-composer "override freshly attached" flag and re-lays-out
once, inside the same frame, which is winia's stand-in for Compose's lookahead
pass: `switch_frame_layout_uses_the_animated_size` pins it (measured: without the
extra pass the probe leaf lands at y=240, the detail hero's natural height; with
it, under the animated 80px source hero).

## 6. State machine and matching

Matching runs on per-composer live maps (`(scope, key) → slot`, rebuilt from
marker walks — no shared registry object exists; scope identity crosses
composers through the scope handle itself, which overlays inherit via the
CompositionLocal snapshot):

- **Tier 0** (same composer): `detect_switch(prev, live)` pairs a vanished
  slot with an appeared slot under one key; the source detaches immediately.
- **Tier 1** (main ↔ overlays): the owner retain hook stashes unmatched
  disappearances; a window-level cross-poll (after ALL composers laid out,
  before render) pairs them with freshly-appeared counterparts elsewhere
  (`fresh_shared` per-composer appearance records — peer prev maps absorb
  appearances before cross-poll runs, so frame records are the only freshness
  source). Stable duplicates never pair (freshness + Tier0-busy guards).
  Unmatched stashes free same-frame.
- A per-frame generation discipline (frame-fresh records overwritten every
  retain; baselines keyed by live slots) guards against stale-frame
  resurrection (the classic one-frame-delay race).

```
Idle → AwaitingBounds → Flying → Finishing → Idle
                     ↘ Retarget (reverse/redirect mid-flight: new flight from
                       current visual rect — always well-defined) → Flying
                     ↘ Cancelled (scope disposed / participating window closed:
                       release retained, atomic cleanup) → Idle
```

`push_animatable`'s "same state, new target evicts old animation + inherits
velocity" gives interruption physical continuity for free. Concurrent pairs
are key-isolated; z-order defaults to launch order with explicit `z_index`
override.

### Slot identity rule (root-caused 2026-09, Phase 2)

**Never remove a slot from the slot tree by key.** Keys are positional
identities: navigating back resurrects the same key on a NEW slot object, so
key-based removal murders the live successor (observed: switch-back collapse
— Column recovered with `kids=[]`, self-perpetuating through Skip recovery).
Stale slots need no manual teardown: `truncate` (same-position replacement)
and Enter-group `retain(visited)` pruning already remove them, and key
resurrection IS state restoration — it must be preserved. Flight teardown
frees only arena nodes (index + slot double-guarded) and clears visuals;
slot hygiene is left to the stock mechanisms.

## 7. Final API (frozen)

```rust
SharedTransitionLayout::new().build(ctx, |ctx| { /* scope via current_shared_scope() */ })
scope.shared_content_state(key) -> SharedContentState
scope.is_transition_active() -> State<bool>
Modifier::shared_element(state, transform, placeholder, path, z_index, render_in_overlay)
Modifier::shared_bounds(state, enter, exit, transform, resize, placeholder, path, z_index, render_in_overlay)
Modifier::render_in_shared_transition_scope_overlay(&scope, z_index)
BoundsTransform::{tween, spring, keyframes}
ResizeMode::{ScaleToBounds(clip), RemeasureToBounds}
```
Passing the scope to the layout closure is load-bearing-wrong (it degrades
keys to positional); the single-param closure plus `current_shared_scope()` is
the supported shape.

## 8. Remaining semantics and their homes

- **Non-shared choreography** (deep `animatedVisibilityScope`): map enter/exit
  onto progress windows (old screen fades 0→0.3, new screen 0.7→1 — the
  Compose stagger). No visibility-system rewrite.
- **Hit testing**: clicks test the *current visual* (lerped) rect and route
  into the live target subtree (in-tree endpoints remap into layout space for
  child descent; detached source ghosts fraction-map into the target's natural
  rect with a root-anchored path for bubbling fidelity). Visual misses pass
  through. Remap is single-application per level (re-entering a transitioning
  node would invert the transform twice). Live endpoints (Target/Morph)
  stay hittable mid-flight through their own remap — only Source-role
  visuals skip input (their ghost routes to the live target instead).
- **Same-screen bounds change** (no slot disappearance): automatic size-delta
  detection per marked endpoint (scroll-immune: position-only moves never
  trigger). Baselines key on endpoint identity (scope, key), not the slot.
- **Nested scopes**: `CompositionLocal` stack, innermost wins; `scope_id`
  disambiguates keys.
- **Text**: Scale tier vector-scales through the CTM (Skia stays crisp);
  Remeasure tier rebuilds paragraphs per frame — quantize sizes to 1px to
  protect the paragraph cache (future optimization).
- **Scroll during flight**: source frozen (correct — it left layout); flight
  ends are fixed at open (v1 — per-frame end tracking is future work).
  In-tree endpoints paint scroll-exact: bounds are scroll-corrected window
  coords while the render canvas carries translate(-S) from scrolled
  ancestors, so each visual freezes its end's ancestor scroll sum at resolve
  time and the flight transform adds it back (paint, clip and hit agree in
  the lerped frame).
- **Spring overshoot**: one unclamped flight-t drives rect, radii, clip and
  hit together (only opacity stays clamped); completion waits for the engine
  to release the progress state, so bouncy springs render their full
  overshoot before teardown.
- **Multiple pairs** (image + title): key-isolated in matching and flights.

## 9. Build phases

1. ~~Skeleton + state machine + Bounds animatable + frozen API~~ → shipped as
   live-map matching + pure state machine (P1).
2. ~~Tier 0 dual-morph~~ → shipped with spring progress + hero demo (P2).
3. ~~Retarget + same-screen morph + hit routing~~ → shipped, retarget-lite +
   automatic size morph + visual-rect routing (P3).
4. Tier 1 cross-composer (main ↔ overlays, retain-in-owner, NO transplant) →
   shipped (P4). Tier 2 deliberately unbuilt (no trigger exists — §3.5).
   `ContentSize`/`AnimatedSize` placeholders and `RemeasureToBounds` shipped in
   the layout contract (§5).
5. ~~Snapshot (start/mid/end frames) + interruption + concurrency tests.~~
6. ~~Overlay pass: `renderInOverlayDuringTransition` on both markers, layer
   elevation for both ends, layer-order hit routing, z ordering across ends~~
   → shipped (§3.6), default on, opt-out preserving the pre-overlay path.
   `OverlayClip` deferred with nested markers.

## 10. Risks and open questions

1. ~~Transplant policy audit~~ — moot (no transplant design anymore).
2. Paragraph-cache quantization for Remeasure text: STILL OPEN and now live —
   the layout contract re-measures text at arbitrary f32 sizes every frame with
   no 1px bucket, so a mis-sized paragraph cache would thrash; Compose's own
   advice is to keep `ScaleToBounds` for text, which is still the default.
3. `RemeasureToBounds` + scrollable shared content: CLOSED — a re-measured
   scroll container re-derives its viewport from the animated constraints
   (`scroll_viewport_*`), so render's clip and the hit clamp follow the box.
4. FIXED WITH A REPRODUCER (review 2, R2-F5): the Tier-0 writers resolve their
   endpoint by the FROZEN slot key with no identity re-check, so a recomposition
   that moves the target's positional slot hands the flight's visual + layout
   override to an unrelated node for one pass — measured: an inserted 20x20 leaf
   is laid out at the flight's reported size with the animated content
   constraints. It self-heals (the same event that changes the element under the
   key also retargets/cancels, clearing that slot and seeding it). Slot keys are
   node-bound and duplicate-free, so an override can never migrate to another
   node object — it rides its node to a different ELEMENT for one pass.
5. CORRECTED TWICE (review 2, R2-F2): the Tier-1 teardown routing through
   `clear_transition_for_slot` IS load-bearing. An earlier note here claimed the
   "peer frozen forever" leak did not reproduce and blamed node rebuilds; both
   halves were wrong. Measured: (a) dropping a marker reuses the SAME node object
   (`materialize` only replaces the modifier), so a surviving node does keep its
   override; (b) the revert experiment was green only because a Tier-1 peer with
   `PlaceHolderSize::AnimatedSize` opened a PHANTOM same-screen morph every frame,
   whose idle frame deleted the override (the cross-poll rewrote it, so it was a
   per-frame drop/rewrite churn). That phantom is now guarded in
   `poll_layout_morphs` (a node carrying an override is the flight's, not a
   morph), and with the guard in place reverting BOTH Tier-1 clears makes
   `stale_cancel_drops_the_surviving_peers_layout_override` fail with the
   override still attached.
6. RESOLVED: the `clip` flag on `ResizeMode::ScaleToBounds` was dead. The render clips
   EVERY transitioning node to its morph-shaped lerped rect (`tf_clip_rr`), so a pair that
   would spill does not, whether or not the marker asked for the clip — verified by
   building a hero with a 500px painted band inside a 120px box and probing outside the
   lerped rect: the probe reads background either way. DECIDED (the unconditional clip is
   the contract, and a parameter that silently does nothing is worse than none): the flag
   and its `TransitionVisual` plumbing are DELETED, `shared_clip_for_kind` returns false,
   and the docs no longer present clipping as an option.
7. RESOLVED BY ANALYSIS, NO CODE CHANGE (review 2, R3-F4): across composers, PAINT order
   and HIT order disagree. During a cross flight the app draws the main transition layer
   AFTER `render_overlays` (`app.rs`), i.e. over an open dialog/scrim, while the hit path
   asks `hit_overlay` FIRST and only then the main tree — so a tap in the overlap reaches
   the dialog that the ghost/chrome is painted over. Checked against Compose before
   touching it: Compose's shared-element overlay is DRAW-ONLY for input (hits go through
   the layout, whose node follows the animated position) and a Dialog is a separate
   window above that screen's overlay — so overlays-first INPUT is correct, and it is the
   PAINT order that is winia-specific (our overlays are composers in the same window, and
   drawing the layer last keeps a Tier-1 ghost visible while it flies into a panel). The
   invariant is therefore scoped to one composer in §3.6 and in the usage guide, with the
   cross-composer behaviour documented instead of silently contradicting the claim.
8. FIXED (review 2, R1): the keyboard focus ring used to be drawn from the node's own
   nearest shape, so a focused hero mid-flight showed corners that did not match the
   background it surrounds — for a percent shape (Circle/Pill) the radius was resolved on
   the un-transformed content box and the flight's `canvas.scale` stretched it into an
   ellipse while the background painted `min(lerped)/2`. `draw_focus` now takes an
   optional device-space radii override and the render passes the flight's `radii_pairs`
   when the node is flying (the ring's geometry was already right: it is drawn inside the
   flight's canvas transform). NOT covered by a test — the ring needs focus AND a flight
   at once, which the headless harness does not drive; the non-flying path is unchanged
   and covered by the existing focus tests.
9. Cross-WINDOW flights (separate OS windows) remain out of scope — they need
   OS-level overlay, not framework composition.
10. Mid-flight reversal opens a reverse flight through the same match path
   (stashed source × fresh counterpart — tested both directions); a reversal
   with no counterpart anywhere cancels atomically with no replacement.
11. Tier1 ghosts paint but ignore taps (target lives in a peer arena the
   single-arena hit search cannot see); the live target stays directly
   hittable in its own composer.
12. Overlay enter/exit animations are not folded into Tier1 visuals — the
   flight leads/lags the panel transform while it animates (~200ms).
13. No shared markers on descendants of shared markers (nested flights would
   compound both transforms). This also gates `OverlayClip`, whose Compose
   default resolves through the parent `sharedBounds`.
14. Elevated endpoints escape ancestor clips *by design* — an element that
   is intentionally clipped by a container (an image inside a rounded card)
   will now spill while it flies. That is Compose's semantics; the opt-out
   (`render_in_overlay = false`) is the escape hatch, and a real
   `OverlayClip` is the eventual fine-grained answer.
15. Layer membership is written per frame by the visual writers and cleared
    in `poll_shared_flights`; a composer that stops being polled would keep
    a stale order. Every composer (main + overlays + headless tests) polls
    each frame, so this is a invariant to preserve rather than a bug today.
16. FIXED, PINNED BY MECHANISM ONLY: the landed hero used to replay the whole flight
    (reported from the demo). Cause: the phantom-morph guard from the same review round
    skipped the morph loop's BASELINE update along with the morph decision, so the
    baseline stayed at the take-off rect and the landing compared the hero against where
    it started — a fresh same-screen morph. The guard now skips the decision only.
    `morph_detector_skips_the_decision_but_updates_the_baseline` pins both halves (an
    early return fails the baseline half, dropping the guard fails the phantom-flight
    half), but the END-TO-END replay still has no headless reproducer: it needs a flight
    that resolves in the frame it opens with the flights polled before the morph poll, so
    the override is attached before the first poll that sees the new rect. Until that
    exists, the demo check is the only end-to-end evidence.
17. RESOLVED (review 2, R1): `ScaleToBounds` now matches Compose's DEFAULT. Compose's
    signature is `scaleToBounds(contentScale: ContentScale = ContentScale.FillWidth,
    alignment: Alignment = Center)` (API reference), i.e. uniform scaling by WIDTH and
    centred placement — deliberately not `Image`'s `Fit`; winia used to hard-code
    `FillBounds` (non-uniform stretch) with top-left placement, so a hero whose aspect
    ratio changes between ends was visibly DISTORTED where Compose scales uniformly.
    Landed in three steps: the two types + their geometry rule
    (`content_scale_factors_match_compose` pins every mode, the degenerate-bounds guard
    and the centring math); the plumbing (`ResizeMode::ScaleToBounds { content_scale,
    alignment }` + `scale_to_bounds()`/`scale_to_bounds_with()`, `TransitionVisual`
    carrying `scale_parts`, `paint_scale` through `factors()`, the new `paint_offset` in
    the flight transform, and `remap_hit` mirroring both — committed with the call sites
    still passing the old behaviour explicitly, so that commit changed no numbers); then
    the default flip to `scale_to_bounds()`.
    The full suite was unchanged at 878 across the flip, because nothing encoded the old
    geometry. NOTE, correcting two earlier claims: the demos do NOT change looks — they
    use `shared_element` (the `Element` kind, hardwired to `RemeasureToBounds`), so
    `ScaleToBounds` is reachable only through `shared_bounds`; only `shared_bounds` users
    see the new default. `ContentScale::FillBounds` still reproduces the old look.
18. FIXED, FROM ROUND 3 (ScaleToBounds reviewer): the `scaleToBounds` work above left
    `radii_pairs` dividing corners by the plain axis ratios while the render scaled by
    `paint_scale`. Under the new `FillWidth` default the two differ whenever the aspect
    ratio changes, so background, border, clip and focus-ring corners painted ELLIPTICAL
    (measured 8x24 device px for an intended 8x8 round corner; 8.7% vertical stretch in a
    live width-preserving flight). `radii_pairs` now divides by `paint_scale`, the
    test-only `scale_parts: None` fallback goes through `ContentScale::FillBounds` instead
    of re-deriving the old formula, and `circle_radius_follows_the_lerped_rect_on_both_ends`
    asserts BOTH axes. Both follow-ups from that reviewer are now CLOSED:
    - the cap question: measured (`skia_clamps_an_oversized_rrect_radius_to_half_the_shorter_side`,
      probe first: r=51 and r=150 on a 300x100 rect both store 50) — Skia silently clamps to
      half the shorter side, so capping at `min(lerped)/2` matches the geometry of the rect
      the corner is drawn into instead of over-shrinking it. RESIDUAL, documented not fixed:
      with `OverlayClip::None` the content may overflow the lerped rect, so the corner
      belongs to the SCALED CONTENT box and the cap can then be tighter than that box allows
      (an exotic combination — a percent shape, `Crop`/`Fit`, and an un-clipped overlay —
      and no raster case was built for it);
    - the ghost half: `circle_radius_follows_the_lerped_rect_on_both_ends` now scans the
      arena for the `Source`-role visual and asserts the same invariant there. MEASURED
      HONESTY NOTE: that half is an invariant guard, NOT a discriminator — with the defect
      restored and the target assertion disabled it still passes, because a detached source
      end's axis ratios coincide with its paint scale.
19. FIXED, FROM ROUND 3 (layout reviewer): a cancelled Tier-1 flight whose SOURCE is the
    peer (overlay -> main, the "hero returns" direction) never released the surviving MAIN
    node's layout override — the writer stamped `all[0].composer_id` while
    `cancel_cross_flight` cleared with `a.source_cid`, so the node stayed frozen at the
    outgoing size and, because the morph guard keys on `flight_measure.is_some()`, its morph
    detection stayed dead too. `peer_sourced_tier1_cancel_drops_the_mains_override` pins it.
20. FROM ROUND 3 (test reviewer) — three claims this branch made were FALSE and are
    corrected in code and comments: `mid_flight_paint_stays_inside_the_lerped_rect`'s
    "the band probe still discriminates SCALED vs CROPPED" (it did not; the two spill
    probes the previous round deleted are restored and catch the crop class), the two
    default-contract tests whose added live-count control had DISARMED the assertion it was
    meant to strengthen (the control's `layout()` flushed the seeded key; it runs after the
    assertion now), and `demo_shape_shared_hero_composable_still_opens_a_switch`'s comment
    naming an assertion that does not fail on the relevant revert. Lesson recorded: a
    "repair" that deletes an assertion must PROVE the survivor still has teeth by mutating
    the code it guards, which is exactly how all three were caught.
