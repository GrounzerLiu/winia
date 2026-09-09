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
| Remeasure deformation | Two-phase deps: measure-time `get()` registers a layout dep → re-measure without recompose, per-frame capable | Mechanism exists, only needs driving |
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

At the switch frame the source slot is *retained* instead of truncated
(`retained_keys`, consulted by `collect_desc_tree` skip logic and the
`free_node_skip` reclamation set — same pattern as `reused_nodes`). Its arena
node is detached into a composer-level transition layer: excluded from all
measure/place, rendered after the main tree via an extra `render_pass1` at its
frozen absolute rect. Both ends then render the **same** lerped rect
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
  (Pill = radii `min(w,h)/2`, Circle = equal sides + same). Morph = rect lerp
  + radii lerp; intermediates are always valid rounded rects, endpoints exact.
  Border width lerps alongside. No same-kind restriction, no mid-point snap
  patches.
- **Clip always holds**: both ends clip to the current lerped rounded rect —
  `sharedBounds` content overflow cannot occur by construction.

## 5. Layout contract (placeholder policy)

```rust
enum PlaceHolderSize { JumpCut, ContentSize, AnimatedSize }
```

- `JumpCut` (v1): layout snaps to end state immediately; the flying pair
  covers the pop. An explicitly named cheap option, not a "simplification".
- `ContentSize`: source keeps old space (free under Tier 0 — the retained node
  is already there).
- `AnimatedSize`: container size follows progress; reuse the proven
  `ContentSizePolicy` pattern from `AnimatedContent` (layout-dep remeasure).

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
SharedTransitionLayout::new().build(ctx, |ctx, scope| { /* ... */ })
scope.shared_content_state(key) -> SharedContentState
Modifier::shared_element(state, bounds_transform, fade_mode?, z_index?, path_motion?...)
Modifier::shared_bounds(state, enter, exit, resize_mode, placeholder_size, ...)
BoundsTransform::{tween, spring, keyframes}
ResizeMode::{ScaleToBounds(clip), RemeasureToBounds}
```

## 8. Remaining semantics and their homes

- **Non-shared choreography** (deep `animatedVisibilityScope`): map enter/exit
  onto progress windows (old screen fades 0→0.3, new screen 0.7→1 — the
  Compose stagger). No visibility-system rewrite.
- **Hit testing**: clicks test the *current visual* (lerped) rect and route
  into the live target subtree (in-tree endpoints remap into layout space for
  child descent; detached source ghosts fraction-map into the target's natural
  rect with a root-anchored path for bubbling fidelity). Visual misses pass
  through. Remap is single-application per level (re-entering a transitioning
  node would invert the transform twice).
- **Same-screen bounds change** (no slot disappearance): automatic size-delta
  detection per marked slot (scroll-immune: position-only moves never trigger).
- **Nested scopes**: `CompositionLocal` stack, innermost wins; `scope_id`
  disambiguates keys.
- **Text**: Scale tier vector-scales through the CTM (Skia stays crisp);
  Remeasure tier rebuilds paragraphs per frame — quantize sizes to 1px to
  protect the paragraph cache (future optimization).
- **Scroll during flight**: source frozen (correct — it left layout); flight
  ends are fixed at open (v1 — per-frame end tracking is future work).
- **Multiple pairs** (image + title): key-isolated in matching and flights.

## 9. Build phases

1. ~~Skeleton + state machine + Bounds animatable + frozen API~~ → shipped as
   live-map matching + pure state machine (P1).
2. ~~Tier 0 dual-morph~~ → shipped with spring progress + hero demo (P2).
3. ~~Retarget + same-screen morph + hit routing~~ → shipped, retarget-lite +
   automatic size morph + visual-rect routing (P3).
4. Tier 1 cross-composer (main ↔ overlays, retain-in-owner, NO transplant) →
   shipped (P4). Tier 2 deliberately unbuilt (no trigger exists — §3.5).
   ContentSize/AnimatedSize placeholders deferred (JumpCut only for now).
5. Snapshot (start/mid/end frames) + interruption + concurrency tests.

## 10. Risks and open questions

1. ~~Transplant policy audit~~ — moot (no transplant design anymore).
2. Paragraph-cache quantization for Remeasure text (phase 4 perf item).
3. `RemeasureToBounds` + scrollable shared content: remeasure inside a
   scroll viewport needs viewport-clamp review.
4. Cross-WINDOW flights (separate OS windows) remain out of scope — they need
   OS-level overlay, not framework composition.
5. Mid-flight reversal of a Tier1 flight snaps instead of flying back
   (staleness-cancel + no reverse flight — same-frame Tier0 takes over when
   the reversal is same-composer; cross-composer reversal is the gap).
