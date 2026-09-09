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
Matching layer      Who flies with whom? scope+key registry, content-location agnostic
    ↓
Flight engine       How does it fly? progress-driven kinematics: rect path +
                    opacity + shape + choreography. Pure function of progress.
    ↓
Content providers   What pixels fly? Tier0 live same-tree / Tier1 transplanted
                    desc / Tier2 bitmap fallback. Auto-selected by coordinator.
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

```rust
enum ContentRef {
    LiveInTree { composer_id: u64, slot_key: u64 },  // Tier 0: live nodes, same composer
    Transplanted { desc_root: DescNode },            // Tier 1: desc subtree moved to overlay composer
    Snapshot { image: skia_safe::Image },            // Tier 2: last-resort bitmap (§3.5)
}
```

- The **target end is always Tier 0**: it is normally composed, fully live
  (a ticking counter keeps ticking mid-flight — impossible for bitmaps).
- The **source end**: same composer → Tier 0 (detach + freeze; freezing is
  semantically fine for leaving content); another composer (other
  window/overlay) → Tier 1 transplant (`DescNode` is plain data — `Modifier:
  Clone`, `policy: Box<dyn MeasurePolicy>` movable, `direction` snapshotted —
  and all Winia composers run on the event-loop thread, so the move is legal).
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

### 3.4 Tier 1 transplant (later, same engine)

Move the retained `DescNode` subtree into the ghost overlay's Composer and
materialize there (Skip-recovery path). State reads re-subscribe inside the
overlay composer, so transplanted content stays live. Requires a
`transplanted_keys` reclamation exemption (mirrors `reused_nodes`) and a
per-policy audit: policies holding composer-local resources beyond
`Backchannel` state must not appear in shared subtrees (documented + debug
assert; `ContentSizePolicy`-style stateful containers are banned there).

### 3.5 Tier 2 bitmap (last resort, ~30 lines)

Honest scope: Tier 2 is **not** "regret medicine for lost content" (lost
content has no pixels to snapshot — retention guarantees that never happens).
It covers exactly two cases: (a) source window being destroyed (correct action
is flight Cancel, §6 — bitmap is not even attempted); (b) pathological huge
subtrees where one rasterization beats per-frame 2× overdraw. Reuses
`capture.rs`. Documented as last resort; no main-flow design depends on it.

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

## 6. State machine and registry

Endpoints may live in different composers, so matching state is shared
out-of-band:

```rust
struct SharedScopeData {
    scope_id: u64,
    registry: Arc<Mutex<HashMap<(u64 /*scope*/, String /*key*/), Endpoint>>>,
}
```

Created by `SharedTransitionLayout`, distributed via `CompositionLocal`.
Compose-time registers identity (scope, key, slot); each composer's
post-layout fills bounds (existing per-composer compose→layout→render order
point in `app.rs`). The coordinator starts a flight on "same key, both bounds
known, one end just appeared/disappeared". A generation counter guards
against stale-frame resurrection (the classic one-frame-delay race).

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
- **Same-screen bounds change** (no slot disappearance): explicit
  `scope.animateBounds(key)`; old bounds already in `prev_nodes`. Single-sided
  morph, no opacity change, same engine.
- **Nested scopes**: `CompositionLocal` stack, innermost wins; `scope_id`
  disambiguates keys.
- **Text**: Scale tier vector-scales through the CTM (Skia stays crisp);
  Remeasure tier rebuilds paragraphs per frame — quantize sizes to 1px to
  protect the paragraph cache (v2 optimization).
- **Scroll during flight**: source frozen (correct — it left layout); target
  end re-resolved every frame (one absolute-position walk, negligible).
- **Multiple pairs** (image + title): registry-isolated by key.

## 9. Build phases

1. Registry + state-machine skeleton + `Bounds` animatable + frozen API
   (Tier 1/2 as `unimplemented!` stubs). Zero behavior change.
2. Tier 0 dual-morph + JumpCut + single spring → hero list→detail demo.
3. Retarget + same-screen `animateBounds` + hit routing.
4. Tier 1 transplant (cross-window) → Tier 2 stub → ContentSize/AnimatedSize.
5. Snapshot (start/mid/end frames) + interruption + concurrency tests.

## 10. Risks and open questions

1. Transplant policy audit (§3.4 ban list) — needed only at phase 4.
2. Paragraph-cache quantization for Remeasure text (phase 4 perf item).
3. `RemeasureToBounds` + scrollable shared content: remeasure inside a
   scroll viewport needs viewport-clamp review.
4. Cross-window flights need both windows' composers driven in the same frame
   (true in `app.rs` today — multi-window iterates all `PerWindow`s).
