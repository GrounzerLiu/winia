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
| `.shared_element(state, transform, placeholder, path, z_index, render_in_overlay)` | `Modifier.sharedElement(…)` | Same content on both ends — flies + crossfades; every `PlaceHolderSize` is honoured (`JumpCut` is just what the examples pass). Compose's `sharedElement` carries no resize parameter and always re-measures, so this marker resolves to `RemeasureToBounds` — see §4.1. `path` is `Linear` / `ArcBelow` / `ArcAbove`; `z_index` orders the flying pair in the layer and is re-read every frame (so it may be flipped mid-flight); `render_in_overlay` is Compose `renderInOverlayDuringTransition` — see §6. NOTE: `render_in_overlay` defaults to `true` in *behaviour*, but Rust has no default arguments: every call site must pass it (`SharedTransitionDefaults::render_in_overlay()` is the value the framework treats as the default). The parameter order also differs from Compose's (`…, path, z_index, render_in_overlay` vs Compose's `…, renderInOverlayDuringTransition, zIndexInOverlay, clipInOverlayDuringTransition`) — porting is a loud type error, not a silent swap |
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
| `ResizeMode` | `scale_to_bounds()` / `scale_to_bounds_with(content_scale, alignment)` | Implemented, matching Compose: the content is scaled into the lerped rect without re-laying-out, fitted by `ContentScale` (`FillWidth` by default, so the aspect ratio is preserved) and placed by `ContentAlignment` (`Center` by default). `ContentScale::FillBounds` gives the old non-uniform stretch. The `clip` flag was measured as dead and deleted — the render always clips a transitioning node to the lerped rect. `ScaleToBounds` is Compose's default for `sharedBounds`, and the one to keep for text |
| `ResizeMode` | `RemeasureToBounds` | Implemented — the entering end is measured with **fixed constraints of the animated bounds** every frame, so content re-lays-out/rewraps instead of being scaled (render then skips the scale, hit testing maps 1:1). See §4.1 |
| `PlaceHolderSize` | `JumpCut` (**winia-only**) | Nothing like it exists in Compose, and the code path is identical to `ContentSize` — it just names the cheap "layout snapped to the end state" reading. For the entering end it reports the target size, like `ContentSize`; for a leaving end it means the space is released immediately |
| `PlaceHolderSize` | `ContentSize` (Compose's default name) | Reports the target size, so the parent holds still |
| `PlaceHolderSize` | `AnimatedSize` | Reports the animated size, so siblings reflow with the flight |
| `SharedKind::Element` | (no resize parameter, like Compose) | Always `RemeasureToBounds`: Compose's `sharedElement` KDoc says it "will re-measure and relayout its child layout using fixed constraints derived from its animated size". There is deliberately no way to make an Element flight scale |
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
                            true, // render_in_overlay: Compose's default
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
                            true,
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

### 4.1 Layout contract (`ResizeMode` + `PlaceHolderSize`)

Both come from the **entering end's** marker and describe what the layout does
during the flight — Compose's `ResizeMode` and `PlaceholderSize`:

| Marker | Effect during the flight |
|---|---|
| `ScaleToBounds { clip }` (default) | Content measured once at its target size, then **scaled** into the lerped rect. Nothing re-lays-out: cheapest, and Compose's advice for text |
| `RemeasureToBounds` | Content is re-measured every frame with fixed constraints = the animated size, so it reflows (text rewraps, rows resize) instead of stretching |
| `PlaceHolderSize::ContentSize` / `JumpCut` | The parent keeps seeing the **target** size: siblings stay put until the flight ends |
| `PlaceHolderSize::AnimatedSize` | The parent keeps seeing the **animated** size: siblings ride the flight (a card growing inside a column pushes the rest down smoothly) |

Three properties worth knowing before you rely on it:

- **Free when unused.** `ScaleToBounds` reports the target size and attaches no
  override at all, so the endpoint's layout is untouched (the invalidation is
  only re-seeded while an override is attached, or on the frame it is dropped).
  Pinned by `default_contract_does_not_touch_the_layout`, which asserts a
  mid-flight layout re-measures nothing.
- **Zero recomposition.** The override is driven by a measure-time `State` read
  (a LAYOUT dependency) and re-seeded per frame, never by composition: a whole
  flight decomposes nothing — pinned by `flight_layout_contract_matrix`, which
  asserts the scenario build count stays flat from the switch through teardown.
- **The layout trails the flight by one frame** (writers run after layout, so
  the layout of frame N uses the frame written at the end of frame N−1 — ~6% of
  a 300 ms flight; the reference content and the reflowed siblings are that far
  behind the painted rect, not the rect itself). Only the ENTERING end
  re-measures; the leaving ghost is frozen content, and in a screen switch the
  outgoing element's space is not held open (its tree is gone — Compose can hold
  it because the old screen stays composed). A same-screen MORPH ignores both
  markers: its size change came from layout in the first place, so re-reporting
  a lerped size would fight the layout driving it.

## 5. Shape and color

- Corner radii resolve automatically from the nearest
  Background/Border/Clip shape (same precedence as the focus ring). The rule,
  which nothing documented until now:
  - Each end's corner KIND is captured when the flight resolves. A `Circle`/`Pill`
    nearest shape is a **percent** corner (`min(w,h)/2`, i.e. Compose's
    `RoundedCornerShape(50)` — on a non-square box that is a stadium, not a
    circle); anything else is a **fixed** value, and `TopRoundedRect` keeps its
    per-corner quad (only its top two corners are rounded).
  - A percent corner is resolved **against the rect being painted — the lerped
    rect** and only then mixed with the other end by progress. So a
    percent -> fixed flight fades its corners out (a `Circle` -> `Rectangle` at
    p=.25 paints `min(l)/2 * .75` on BOTH ends), a percent -> percent flight
    evaluates on the animated box, and fixed `Shape::rounded(n)` corners are
    unchanged (they lerp endpoint to endpoint, e.g. circle -> rectangle 75 -> 0).
  - Deliberate deviation from Compose: Compose interpolates the deferred
    `CornerSize` objects and resolves each end against ITS OWN box, so its two
    ends can differ mid-flight; winia resolves both against the lerped rect, so
    they always coincide. The visible difference on this repo's shape pairs is
    under 3px; the alignment is what the demos are checked against.
  - Under spring overshoot a resolved percent corner can exceed half the painted
    box. Skia's `RRect` constructor silently reduces it (Compose scales corners
    down proportionally instead), so the corner clamps while a fixed end keeps
    overshooting.
- Non-uniform scale turns arcs elliptical (inherent, not a bug).
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
  Scrolling *during* a flight: an elevated end is re-drawn from the layer at
  its frozen lerped WINDOW rect, so it stays put while the rest of the screen
  (including an opt-out end, which adds its frozen ancestor sum onto a canvas
  carrying the live translate) scrolls with the content. The two ends
  therefore diverge if the user scrolls mid-flight; ending the flight or
  re-opening one re-resolves both ends.
- **Overlay escape** (`renderInOverlayDuringTransition`, default true):
  while it flies, an endpoint is re-rendered from the transition layer at
  its absolute position, so it **escapes ancestor clips and ancestor layer
  transforms** (alpha/scale) and paints **above non-shared content** — a
  hero flying out of a scroll viewport is no longer cut at the viewport
  edge, and it is no longer hidden behind a later sibling (top bar, FAB).
  Its layout position, state and slot identity are unchanged; what moves is
  which pass paints it — and, because the layer is tested first, taps on it
  are resolved against the lerped rect and are no longer rejected by its
  ancestors (see the hit-testing bullet below). `false` restores the in-tree
  painting (ancestors clip it as before). The leaving end is always detached
  in Winia, so the flag is a no-op there. `Morph` (same-screen resize) never
  elevates.
- **What opting out looks like** (the demo's off-state): the entering end is
  clipped by its own ancestors, while the leaving end is detached — its tree
  is gone, so it cannot be clipped by anything and keeps painting the full
  lerped rect. Mid-flight the pair therefore shows two different clip extents
  (a hard rectangular edge cutting the entering content over the intact
  leaving ghost). The two are still on the SAME lerped rect — it is a clip
  difference, never a position difference (pinned by
  `tier0_overlay_opt_out_paints_in_tree_and_stays_clipped`). Compose is
  asymmetric here too (each end is clipped by its own screen's ancestors);
  the only approximation is that the leaving end uses no clip at all instead
  of the old screen's.
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
    overlay on (default), and with the chrome modifier below, both legs are
    clean.
- **When to actually turn it off** (Compose's own framing: the flag is an
  optimization, not a clipping feature — "in some rare use cases there may be
  no clipping or layer transform that prevents shared elements from
  transitioning... in such cases it could be specified to false"):
  - Nothing on the path clips or applies a layer transform → on and off are
    **pixel-identical** (measured: 0 differing pixels), so off just skips the
    layer pass.
  - You *want* the flying element to stay in-tree: staying inside an
    ancestor's alpha/layer group, or keeping an unusual z relationship that
    the chrome modifier below cannot express.
  - Turning it off *for a destination that clips* is the one configuration
    that looks broken — a usage error, not a rendering bug: the demo
    `cargo run -p winia --example shared_transition_demo` shows it as a
    counter-example. For "content slides under pinned chrome" the right tool
    is the chrome modifier below, not this flag:
    `shared_transition_pinned_bar_demo` demonstrates it.
- **Chrome that must stay on top** (pinned bars, FABs): mark it with
  `render_in_shared_transition_scope_overlay(&scope, z_index)` — Compose's
  `Modifier.renderInSharedTransitionScopeOverlay(zIndexInOverlay)`. The
  subtree becomes a layer root while the scope transitions and is drawn after
  the flying pair, so **both** ends pass under it (including the detached
  leaving ghost, which tree order can never cover, and whose `1 − p` opacity
  wash is the one thing the bar cannot hide — it fades to nothing). Outside a
  flight it returns to ordinary tree order. Membership is recomputed per poll
  from the window's live flights, so a peer composer's chrome elevates in the
  same frame as the flight (never "one frame later"). Deviation from Compose:
  the gating lambda `renderInOverlay: () -> Boolean` is not exposed — the
  elevation is always "while this scope has a non-terminal flight"; the
  `z_index` argument is the same and shared endpoints sit at `0.0`.
- Mid-flight taps follow PAINT order: the layer is tested topmost-first, so
  an elevated endpoint is hit at its lerped rect (through the flight transform
  inverse, ignoring ancestors that would reject the point), and elevated
  chrome is hit before the flights painted under it — a button on a pinned bar
  stays live when a hero slides under it. Ghost taps route into the live
  target subtree. Each node's own viewport clamp still applies; visual misses
  pass through to the tree.
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
- `OverlayClip` is shipped as a marker parameter
  (`shared_bounds_with_overlay_clip`): `Bounds` (default, the drawn end's own corner quad
  on the lerped rect), `Rectangle`, `RoundedCorner(radius)`, and `None` — a winia addition
  for content that must overflow the animated bounds. Compose's own default (the parent
  `sharedBounds`' resolved clip path) still needs nested markers, so it stays out of
  reach; the winia default is the equivalent for a single marker.
- `vertical_scroll(reverse)` shares the pre-existing hit/render mirror
  divergence — out of scope.
- Cross-OS-window flights are out of scope (need an OS-level overlay).
- Tier 2 bitmap flights deliberately unbuilt (no trigger exists).
- ~~`ResizeMode::ScaleToBounds` does not implement Compose's shape~~ **CLOSED**: it now
  carries Compose's `contentScale` + `alignment`, and `scale_to_bounds()` reproduces
  Compose's defaults exactly (`ContentScale.FillWidth` + `Alignment.Center`), so the
  content keeps its aspect ratio and the leftover axis is centred. All seven
  `ContentScale` members are implemented (`Fit`, `Crop`, `FillBounds` — the old
  hard-coded behaviour, still available, `FillWidth`, `FillHeight`, `Inside`, `None`).
  `RemeasureToBounds` remains the mode to reach for when the content itself must re-flow.
- A same-screen MORPH ignores `resize`/`placeholder` (see §4.1).
- `PlaceHolderSize` is a closed enum: Compose's `PlaceholderSize` is a policy
  value (`calculateSize(contentSize, animatedSize)`), so a Compose user cannot
  plug in a custom rule here.
- Not pixel-pinned: the SCALED-versus-CROPPED distinction rests on ONE raster
  probe (a green band whose scaled height is ~39px of the lerped rect, vs 60px
  when the content is drawn 1:1); the box/scale contract itself is pinned by the
  layout assertions (the matrix's exact sizes,
  `remeasure_end_is_never_scaled_by_the_frame_delta`), not by pixels. A hero whose
  content is a single solid rounded rect looks the same either way.

## 8. Tests

- `cargo test -p winia --lib ui::shared_transition` (82 tests: unit,
  headless Tier 0/Tier 1 raster probes, guard-checked regression tests
  for scroll add-back, morph hit routing, bouncy overshoot, baseline
  identity, arc paint, z-order, enter/exit slide, expand wipe, active
  flag, overlay escape + escape opt-out + cross-composer hit routing,
  chrome elevation (same-composer, peer-composer, equal-z, double-paint),
  the directional artifact, marker freshness, and the layout contract —
  `flight_measure_frame_reaches_the_parent_layout` (the channel itself),
  `flight_layout_contract_matrix` (the 4-way resize/placeholder matrix, pinned
  at t=.5 with exact values), `mid_flight_cancel_restores_the_natural_size`
  (the teardown seed), `default_contract_does_not_touch_the_layout` /
  `tier1_default_contract_does_not_touch_the_peer_layout` (the default costs no
  layout, both tiers, each with a live-count control),
  `shared_element_re_measures_and_honours_animated_size`
  (the Compose parity decision), `mid_flight_paint_stays_inside_the_lerped_rect`
  (SCALED vs CROPPED), `ghost_tap_maps_identity_into_a_remeasure_target`,
  `remeasure_end_is_never_scaled_by_the_frame_delta`,
  `weighted_hero_keeps_its_allocation_while_flying`,
  `restored_node_keeps_its_content_box`, `circle_radius_follows_the_lerped_rect_on_both_ends`,
  `percent_to_fixed_corners_stay_aligned_end_to_end`,
  `corner_endpoints_are_exact_for_every_shape_pair`,
  `circle_shape_fills_a_non_square_box_like_pill`,
  `writer_never_clobbers_another_flights_override` (both the write and the clear
  direction), `tier0_writer_skips_a_node_that_is_not_the_shared_endpoint`,
  `peer_sourced_tier1_cancel_drops_the_mains_override`,
  `morph_detector_skips_the_decision_but_updates_the_baseline` and
  `teardown_reaches_a_slot_that_left_the_tree`).
- Tests driving animations hold `TEST_SERIAL` + `clear_all_animations()`.
  Newer tests PIN the flight progress (`progress.set(t)`) instead of sampling the
  wall clock, and drive at most one extra `layout()` to consume the writer's
  seed; the remaining completion loops are wall-clock bounded, so they can only
  fail by not finishing within the loop cap.
- Behaviour changes must ship a regression test that **fails pre-fix** (verify by
  temporarily reverting the fix). Where NO failure reproduces — the fix is a
  consistency change, or the contract can only be pinned as an end state — say so
  in the commit message instead of implying a reproducer exists; `a9eb990` and
  `128b700` are the examples, and `128b700`'s premise was later MEASURED wrong
  (the leak did reproduce; a phantom peer morph had been cleaning it up), which is
  exactly why the claim has to be checked rather than asserted.

## 9. Maintenance conventions

- Render only `peek`s flight state — never subscribe visuals (the
  zero-recomposition rule); per-frame writes go through
  `write_flight_visuals` / `write_cross_visuals`.
- One add-back, never two: the same `layer_root` bit that routes a node to
  the layer also suppresses its ancestor scroll add-back (`render_pass1`),
  so an elevated end can never add a sum the layer canvas does not carry.
- Layer membership is written, not discovered: the visual writers push
  elevated roots into `Composer::elevated_roots` (cleared every
  `poll_shared_flights`), because a Tier 1 peer's flight lives in the MAIN
  composer's map. `rebuild_layer_order` unions that with the detached
  sources and the elevated chrome, then z-sorts; render AND hit testing both
  read `transition_roots()`, so paint order and hit order cannot drift **within a
  composer** (chrome entries are hit through their own subtree in the same walk).
  Across composers the two orders differ on purpose: the main layer paints above an
  open overlay (so a ghost stays visible flying into a panel) while input is resolved
  overlay-first — which is what Compose does (its overlay is draw-only for input, and
  a Dialog is a separate window above it). See the architecture doc §3.6.
- Chrome elevation is a window-level decision: each composer's poll passes
  its own flight scopes, and `poll_cross_flights` re-runs it for every
  participant with the union — a peer cannot see a Tier1 flight otherwise.
  Do not reintroduce a dependency on `is_transition_active()` (an
  app-facing state that only exists once somebody subscribes).
- Marker values are sampled when a flight resolves (`render_in_overlay`,
  `path`, `boundsTransform`, and now `resize`/`placeholder`), so the marker in
  the arena must be fresh. Read
  such a flag **inside the composable that builds the marked node** — a read
  one slot higher leaves this subtree skipped (Skip keeps the cached
  modifier) and the runtime toggle silently does nothing until something else
  rebuilds the screen. `z_index` is the exception: it is re-read from the
  arena every poll, so it takes effect on the next frame. Pinned by
  `marker_flag_refreshes_only_when_read_in_the_marker_slot`.
- Never remove slots by key (keys are positional identities resurrected
  on navigate-back); teardown frees arena nodes index+slot double-guarded
  and tag-checks visuals by flight id.
- The flight layout override is a per-frame contract, not a subscription:
  the writers re-seed the endpoint's `layout_dirty_keys` entry EVERY frame
  (because `layout()` clears `layout_dirty` on the whole tree at the start of
  every pass, so a folded parent would never descend into it), and every
  teardown that drops an override must seed once more. Both Tier 0 and Tier 1
  route through `clear_transition_for_slot` for that reason — it is the single
  place that also matches the flight id (a successor flight can own the same
  slot key by then).
- Keep `docs/shared-element-transition.md` (architecture) and this file
  (usage) in sync when behavior changes.
