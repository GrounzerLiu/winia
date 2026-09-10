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

### 1.2 Endpoint markers (`Modifier`)

| Method | Compose equivalent | Notes |
|---|---|---|
| `.shared_element(state, transform, placeholder, path, z_index)` | `Modifier.sharedElement(…)` | Same content on both ends — flies + crossfades; pass `PlaceHolderSize::JumpCut` (others degrade to it, logged, until implemented); `path` is `Linear` / `ArcBelow` / `ArcAbove`; `z_index` (default 0.0) orders retained ghosts back-to-front, in-tree targets keep tree order |
| `.shared_bounds(state, enter, exit, transform, resize, placeholder, path, z_index)` | `Modifier.sharedBounds(…)` | Different content — container morphs; `enter` plays on the appearing end, `exit` on the disappearing end (fade channels claimed per-end reproduce the crossfade; slide/scale/expand switches and distances compose on top at flight progress — NOTE: the transitions' inner `AnimationSpec`s are ignored, channels always ride the flight clock; Morph role skips both; expand ≈ scale-about-edge + clip) |

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
  lerped rect (each visual freezes its end's ancestor scroll sum).
  Scrolling *during* a flight keeps the visual rigid with its content.
- Mid-flight taps test the lerped rect: ghost taps route into the live
  target subtree; live Target/Morph endpoints (and their children) stay
  hittable. Visual misses pass through.
- Tier 1 limitation: cross-composer ghosts paint but ignore taps (the
  target lives in a peer arena); the live target stays directly hittable
  in its own composer. Overlay enter/exit animations (~200ms) are not
  folded into Tier 1 visuals.

## 7. Gaps and hard rules (see also the architecture doc §10)

- No shared markers on descendants of shared markers (nested flights
  compound both transforms).
- No `backdrop_blur` heroes (blur snapshots post-transform content).
- `vertical_scroll(reverse)` shares the pre-existing hit/render mirror
  divergence — out of scope.
- Cross-OS-window flights are out of scope (need an OS-level overlay).
- Tier 2 bitmap flights deliberately unbuilt (no trigger exists).

## 8. Tests

- `cargo test -p winia --lib ui::shared_transition` (43 tests: unit,
  headless Tier 0/Tier 1 raster probes, guard-checked regression tests
  for scroll add-back, morph hit routing, bouncy overshoot, baseline
  identity, arc paint, z-order, enter/exit slide, expand wipe, active
  flag).
- Tests driving animations hold `TEST_SERIAL` + `clear_all_animations()`.
- Behavior changes must ship a regression test that **fails pre-fix**
  (verify by temporarily reverting the fix, as done for all four).

## 9. Maintenance conventions

- Render only `peek`s flight state — never subscribe visuals (the
  zero-recomposition rule); per-frame writes go through
  `write_flight_visuals` / `write_cross_visuals`.
- Never remove slots by key (keys are positional identities resurrected
  on navigate-back); teardown frees arena nodes index+slot double-guarded
  and tag-checks visuals by flight id.
- Keep `docs/shared-element-transition.md` (architecture) and this file
  (usage) in sync when behavior changes.
