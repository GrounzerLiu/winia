# Shared element transitions: API, behavior and gaps

> Goal: match Jetpack Compose `sharedTransition` (`sharedElement` /
> `sharedBounds`) — a hero flies from the old screen to the new one while
> crossfading, with corner-radius morphing and spring physics.
> Architecture and flight engine internals live in
> `docs/shared-element-transition.md`; this document is the **usage guide**.
> Run the demo: `cargo run -p winia --example shared_transition_demo`
> (red circle → blue rectangle, position/size/aspect all change).

## 1. API overview

### 1.1 Scope (one per shared region)

| Item | Compose equivalent | Notes |
|---|---|---|
| `SharedTransitionLayout::new().build(ctx, \|ctx\| { … })` | `SharedTransitionLayout { … }` | Wraps both screens; single-param closure, scope read inside via `current_shared_scope()` |
| `current_shared_scope()` | `SharedTransitionScope` receiver | Returns `Option<SharedTransitionScope>`; `None` outside the layout |
| `scope.shared_content_state(key)` | `rememberSharedContentState(key)` | Pairing handle; equality is (scope, key) — same key in different scopes never pairs |
| `scope.is_transition_active()` | `SharedTransitionScope.isTransitionActive` | `State<bool>` — true while any flight in the scope is non-terminal (both tiers); subscribe for dimming/input-gating patterns |
| `.render_in_shared_transition_scope_overlay(&scope, z_index)` | `Modifier.renderInSharedTransitionScopeOverlay(…, zIndexInOverlay)` | Keeps **non-shared** content (pinned app bars, FABs) above the flying pair for as long as the scope transitions; `z_index` orders it against the endpoints (they default to `0.0`). Outside a flight it is ordinary tree content |

### 1.2 Endpoint markers (`Modifier`)

| Method | Compose equivalent | Notes |
|---|---|---|
| `.shared_element(state, transform, placeholder, path, z_index, render_in_overlay)` | `Modifier.sharedElement(…)` | Same content on both ends — flies + crossfades; pass `PlaceHolderSize::JumpCut` (others degrade to it, logged, until implemented); `path` is `Linear` / `ArcBelow` / `ArcAbove`; `z_index` (default 0.0) orders the flying pair in the layer; `render_in_overlay` (default **true**) is Compose `renderInOverlayDuringTransition` — see §6 |
| `.shared_bounds(state, enter, exit, transform, resize, placeholder, path, z_index, render_in_overlay)` | `Modifier.sharedBounds(…)` | Different content — container morphs; `enter` plays on the appearing end, `exit` on the disappearing end (fade channels claimed per-end reproduce the crossfade; slide/scale/expand switches and distances compose on top at flight progress — NOTE: the transitions' inner `AnimationSpec`s are ignored, channels always ride the flight clock; Morph role skips both; expand ≈ scale-about-edge + clip); `render_in_overlay` same as above |

### 1.3 Flight shaping (`BoundsTransform`)

| Constructor | Default | Notes |
|---|---|---|
| `BoundsTransform::tween(TweenSpec)` | 300ms Linear (`TweenSpec::default()`, same as `SharedTransitionDefaults::bounds_transform()`) | Exact, no overshoot |
| `BoundsTransform::spring(SpringSpec)` | critically damped (`damping_ratio: 1.0`, stiffness 200) | `SpringSpec::bouncy()` (`damping_ratio: 0.6`) overshoots past the end rect — the Compose spring look |
| `BoundsTransform::keyframes(KeyframesSpec)` | — | Arbitrary progress curves |

Specs must be **finite** (Tween/Spring/Keyframes settle and release the
progress state). An infinite `Repeatable` spec holds the flight open
forever — completion waits for engine release (see §4).

### 1.4 Kind vocabulary

| Type | Variants | Status |
|---|---|---|
| `SharedKind` | `Element` / `Bounds { resize, placeholder }` | Both match and fly |
| `ResizeMode` | `ScaleToBounds { clip }` | Fully implemented (render scales content into the lerped rect; `clip` clips to it) |
| `ResizeMode` | `RemeasureToBounds` | **Degrades to scale** (one `debug_log!` per flight start) until per-frame remeasure lands |
| `PlaceHolderSize` | `JumpCut` | Implemented (layout snaps to end state immediately; the flying pair covers the pop) |
| `PlaceHolderSize` | `ContentSize` / `AnimatedSize` | **Deferred** |
| `PathMotion` | `Linear` | Implemented |
| `PathMotion` | `ArcBelow` / `ArcAbove` | Implemented — Compose `ArcSpline.Arc` math ported: quarter-ellipse center path, arc-length-uniform travel (101-entry table), endpoints exact, size stays linear; resolved from the target marker. Either-dimension travel falls back to linear (axis-aligned flights stay straight). API shape differs from Compose (flight-level path, not per-keyframe `using ArcMode`) |

## 2. Usage (three steps)

```rust
SharedTransitionLayout::new().build(ctx, |ctx| {
    let scope = current_shared_scope().expect("inside SharedTransitionLayout");
    Column::new().build(ctx, |ctx| {
        if show_list.get() {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(150.0, 150.0)
                        .background(Color::RED, Shape::Circle)
                        .shared_element(
                            scope.shared_content_state("hero"),
                            BoundsTransform::spring(SpringSpec::bouncy()),
                            PlaceHolderSize::JumpCut,
                            PathMotion::ArcBelow,
                            0.0,
                        ),
                )
                .build(ctx, |_| {});
        } else {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(320.0, 170.0)
                        .background(Color::BLUE, Shape::Rectangle)
                        .shared_element(
                            scope.shared_content_state("hero"), // same key → pair
                            BoundsTransform::spring(SpringSpec::bouncy()),
                            PlaceHolderSize::JumpCut,
                            PathMotion::ArcBelow,
                            0.0,
                        ),
                )
                .build(ctx, |_| {});
        }
    });
});
```

1. Wrap both screens in one `SharedTransitionLayout`.
2. Mark both ends with the **same key**. Different keys never pair;
   same key in different scopes never pairs.
3. Switch screens — the flight opens automatically. Zero per-frame
   recomposition during flight (render `peek`s the progress).

## 3. The three flight kinds

- **Screen switch (Tier 0, same composer)**: old hero is retained +
  detached (frozen ghost, alpha 1→0); new hero fades in (alpha 0→1);
  both paint the same lerped rect. Corner radii morph with it (§5).
- **Same-screen morph**: a marked node's *size* changes with no slot
  churn (e.g. layout-driven width) → an opacity-preserving morph opens
  automatically. Position-only moves (scroll, sibling shifts) never
  trigger. Baselines key on endpoint identity (scope, key), so a
  data-driven key-swap at one call site starts clean.
- **Cross-composer (Tier 1, main tree ↔ overlays)**: e.g. hero flies
  into a Dialog. Flights live in the main composer map; the ghost
  renders **above** modal scrims (Compose `zIndexInOverlay`).

## 4. Motion semantics

- Progress is scalar 0→1; the rect is derived per frame from it —
  spring overshoot (`t > 1`) flies past the endpoint, then
  settles exactly onto it. One unclamped flight-t drives rect, radii,
  clip and hit together; only opacity stays clamped.
- Completion waits for the animation engine to release the progress
  state (bouncy flights render their full overshoot), then tears down:
  retained source freed, target visuals cleared, natural render resumes.
- Retarget-lite: switching again mid-flight cancels the old flight and
  restarts from the current visual rect (seamless for Element flights;
  slide-bounds flights may snap by the live channel offset — documented
  in code).

## 5. Shape and color

- Corner radii resolve automatically from the nearest
  Background/Border/Clip shape (`Circle`/`Pill` → half min-size, same
  precedence as the focus ring) and lerp `[TL,TR,BR,BL]` — circle →
  rectangle is 75→0. Non-uniform scale turns arcs elliptical
  (inherent, not a bug).
- Color is **alpha crossfade**, not color morph: red→blue passes through
  blended purple mid-flight. True solid-color lerp (linear/Oklab space,
  Background/Border only) is a scoped future item — default stays
  crossfade.

## 6. Scroll, hit testing, interaction

- Scroll-exact: heroes inside scrolled containers paint exactly on the
  lerped rect. Each end freezes its ancestor scroll sum when the flight
  resolves; the elevated (default) ends carry a zero sum because the layer
  canvas has no ancestor translate, and the layer supplies the
  scroll-corrected absolute origin instead — same pixels either way.
  Opt-out (`render_in_overlay = false`) ends keep the classic add-back.
  Scrolling *during* a flight keeps the visual rigid with its content.
- **Overlay escape** (`renderInOverlayDuringTransition`, default true):
  while it flies, an endpoint is re-rendered from the transition layer at
  its absolute position, so it **escapes ancestor clips and ancestor layer
  transforms** (alpha/scale) and paints **above non-shared content** — a
  hero flying out of a scroll viewport is no longer cut at the viewport
  edge, and it is no longer hidden behind a later sibling (top bar, FAB).
  The element keeps its place in the tree: layout, state and hit testing
  are untouched, only paint moves. `false` restores the in-tree painting
  (ancestors clip it as before). The leaving end is always detached in
  Winia, so the flag is a no-op there — and because it renders from the
  layer it is not just un-clippable but also un-coverable: later siblings
  (pinned chrome) cannot hide it either. It paints over them with its
  opacity, i.e. a wash bounded by `1 − p` (measured: the bar strip reads
  `255·(1−p)` in its red channel), which is why a flight that must slide
  under a bar is usually arranged so the crossing happens late — at
  `p ≥ 0.83` the wash is ≤ 17% and fades to nothing. `Morph` (same-screen
  resize) never elevates.
  **What opting out looks like** (and why the demo's off-state looks
  lopsided): the entering end is clipped by its own ancestors, while the
  leaving end is detached — its tree is gone, so it cannot be clipped by
  anything and keeps painting the full lerped rect. Mid-flight the pair
  therefore shows two different clip extents (a hard rectangular edge cutting
  the entering content over the intact leaving ghost). The two are still on
  the SAME lerped rect — it is a clip difference, never a position difference
  (pinned by `tier0_overlay_opt_out_paints_in_tree_and_stays_clipped`). Compose
  is asymmetric here too (each end is clipped by its own screen's ancestors);
  the only approximation is that the leaving end uses no clip at all instead
  of the old screen's.
  **When to actually turn it off** (Compose's own framing: the flag is an
  optimization, not a clipping feature — "in some rare use cases there may be
  no clipping or layer transform that prevents shared elements from
  transitioning... in such cases it could be specified to false"):
  - Nothing on the path clips or applies a layer transform → on and off are
    **pixel-identical** (measured: 0 differing pixels), so off just skips the
    layer pass.
  - You *want* the flying element to stay in-tree: sliding **under** a later
    sibling (top bar, bottom bar, scrim) instead of over it, or staying inside
    an ancestor's alpha/layer group.
  - A `backdrop_blur` hero: the layer path snapshots post-transform content
    (documented limitation), the in-tree path behaves like any other blurred
    node.
  - Turning it off *for a destination that clips* is the one configuration
    that looks broken — that is a usage error, not a rendering bug, and it is
    what the demo's toggle shows as its counter-example.
  Two demos, one per side of the rule:
  `cargo run -p winia --example shared_transition_demo` (clipping container —
  the flag must stay on) and `shared_transition_pinned_bar_demo` (pinned bar —
  the Compose answer there is the chrome modifier below, not this flag).
- **Chrome that must stay on top** (pinned bars, FABs): mark it with
  `render_in_shared_transition_scope_overlay(&scope, 1.0)` — Compose's
  `renderInSharedTransitionScopeOverlay(zIndexInOverlay)`. The bar becomes a
  layer root while the scope transitions and is drawn after the flying pair,
  so **both** ends pass under it; outside a flight it returns to ordinary tree
  order. Prefer this over turning the shared element's `render_in_overlay` off:
  the flag only covers the entering end and exposes it to ancestor clips, while
  the chrome modifier covers the detached leaving end too. Membership is
  recomputed each poll, so a scope whose flight began in a peer composer this
  frame elevates one frame late (p≈0, invisible).
  Two things that mislead when reading a screenshot of the opt-out state:
  - **The visible part reads as "parked at the destination".** What shows is
    the lerped rect ∩ the clip, and when the clip is the destination
    container that intersection's leading corner sits ON the container corner
    and stays there for the whole flight (only the trailing edges follow the
    flight). The paint transform is moving the whole time — measured from a
    real demo frame, the visible corner stayed while the shape grew from a
    sliver to the full container.
  - **The artifact is directional.** It appears on the leg whose DESTINATION
    clips. Flying back OUT of that container is clean: the leaving end is
    detached, so the container's clip left with the old tree. With the
    overlay on (default) both legs are clean, because both ends are layer
    roots and simply coincide.
- Mid-flight taps test the lerped rect: elevated endpoints route through
  the flight transform inverse, ghost taps route into the live target
  subtree, and both stay hittable **even where their ancestors reject the
  point** (that is what the overlay pass changed). The endpoint's own
  viewport clamp still applies. Visual misses pass through.
- Tier 1 limitation: cross-composer ghosts paint but ignore taps (the
  target lives in a peer arena the single-arena search cannot see). The
  live target is now reached in its own composer at its animated rect —
  including outside that composer's ancestors — so overlay taps on a
  flying element land where Compose would land them. Overlay enter/exit
  animations (~200ms) are still not folded into Tier 1 visuals.

## 7. Gaps and hard rules (see also the architecture doc §10)

- No shared markers on descendants of shared markers (nested flights
  compound both transforms).
- No `backdrop_blur` heroes (blur snapshots post-transform content).
- `OverlayClip` / `clipInOverlayDuringTransition` (custom clip paths in
  the overlay) are not built: Compose's default derives from the parent
  `sharedBounds`, which needs nested markers. `ScaleToBounds { clip: true }`
  still clips the flying pair to the lerped rect.
- `vertical_scroll(reverse)` shares the pre-existing hit/render mirror
  divergence — out of scope.
- Cross-OS-window flights are out of scope (need an OS-level overlay).
- Tier 2 bitmap flights deliberately unbuilt (no trigger exists).

## 8. Tests

- `cargo test -p winia --lib ui::shared_transition` (46 tests: unit,
  headless Tier 0/Tier 1 raster probes, guard-checked regression tests
  for scroll add-back, morph hit routing, bouncy overshoot, baseline
  identity, arc paint, z-order, enter/exit slide, expand wipe, active
  flag, overlay escape + escape opt-out + cross-composer hit routing).
- Tests driving animations hold `TEST_SERIAL` + `clear_all_animations()`.
- Behavior changes must ship a regression test that **fails pre-fix**
  (verify by temporarily reverting the fix, as done for all seven).

## 9. Maintenance conventions

- Render only `peek`s flight state — never subscribe visuals (the
  zero-recomposition rule); per-frame writes go through
  `write_flight_visuals` / `write_cross_visuals`.
- One add-back, never two: an elevated end renders rootless from the layer
  at its scroll-corrected absolute origin and MUST carry a zero `scroll`;
  an opt-out end renders in-tree and MUST carry its frozen ancestor sum.
  Both are decided together in the writers.
- Layer membership is written, not discovered: the visual writers push
  elevated roots into `Composer::elevated_roots` (cleared every
  `poll_shared_flights`), because a Tier 1 peer's flight lives in the MAIN
  composer's map. `rebuild_layer_order` unions that with the detached
  sources and z-sorts; render and hit testing both read `transition_roots()`
  so paint order and hit order cannot drift.
- Marker values (`render_in_overlay`, `path`, `boundsTransform`, `z_index`)
  are sampled when a flight resolves, so the marker in the arena must be
  fresh. Read such a flag **inside the composable that builds the marked
  node** — a read one slot higher leaves this subtree skipped (Skip keeps the
  cached modifier) and the runtime toggle silently does nothing until
  something else rebuilds the screen. Pinned by
  `marker_flag_refreshes_only_when_read_in_the_marker_slot`.
- Never remove slots by key (keys are positional identities resurrected
  on navigate-back); teardown frees arena nodes index+slot double-guarded
  and tag-checks visuals by flight id.
- Keep `docs/shared-element-transition.md` (architecture) and this file
  (usage) in sync when behavior changes.
