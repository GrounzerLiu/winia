# SearchBar shared-morph experiment — handover

Branch: `exp/searchbar-shared-morph` (based on `af564be`). **Not merged to v2.** Working tree clean, full
suite 906 green (`cargo test -p winia --lib`).

Everything below is stated as measured or as "not verified" — nothing in between. Where a number appears, the
command or the file that produced it is named.

---

## 1. What the task was, and what Compose actually does

Compose Material3 SearchBar: the collapsed bar and the expanded panel share ONE progress value. The panel's
geometry is interpolated during layout, not flown:

```kotlin
// FullScreenSearchBarLayout
width  = constrainWidth(lerp(collapsedWidth,  constraints.maxWidth,  progress))
height = constrainHeight(lerp(collapsedHeight, constraints.maxHeight, progress))
radius = SearchBarCornerRadius * (1 - progress)      // rect once the radius rounds to zero
topPadding    = lerp(0, SearchBarVerticalPadding, progress)
bottomPadding = lerp(0, SearchBarVerticalPadding, progress)
animatedOffsetX = lerp(collapsedBounds.left, offsetX, progress)
animatedOffsetY = lerp(collapsedBounds.top,  offsetY,  progress)   // both offsets 0 here
```

Anchoring comes from `SearchBarImpl`'s `onGloballyPositioned { state.collapsedCoords = it }`, and the content
fades on a SEPARATE `contentAnimatable` (`AnimationForContentFade*`: 100ms, +50ms fading in, none out).

Timings (verified in `androidx` sources, `MotionTokens` v0_103):

| purpose | duration | curve |
|---|---|---|
| expand (`AnimationEnter*Spec`) | `DurationLong4` = 600ms, +100ms delay | `EasingEmphasizedDecelerateCubicBezier` = `CubicBezier(0.05, 0.7, 0.1, 1.0)` |
| collapse (`AnimationExit*Spec`) | `DurationMedium3` = 350ms, +100ms delay | `CubicBezier(0.0, 1.0, 0.0, 1.0)` |
| docked | `fadeIn + expandVertically` / `fadeOut + shrinkVertically` | same specs as above |

## 2. The shared-element route was probed and REJECTED — with evidence

The goal's step 1 assumed a cross-composer shared-element flight could carry this morph. It cannot, and the
measurement says so. Probe kept at `winia/examples/searchbar_morph_probe.rs` (a main-tree pill plus a Dialog
panel marking one `shared_bounds` key):

| probe shape | anim-trace result | flight? |
|---|---|---|
| pill always composed (the real SearchBar shape — the dialog merely covers it) | 259 records, all `mark:searchbar#plain` | **no** |
| pill removed while expanded | `#Source` 61 + `#Target` 61, rect 240x56 -> 385x209 -> 503x333 -> 713x552 | yes |
| pill keeps its slot, marker dropped | 134 records, no flight | **no** |

Mechanism (`shared_transition.rs`): cross-composer pairing only stashes a source for a key that LEFT the live
map, so a key that stays composed on both sides never flies. Compose's own SearchBar does not use a shared
element either — hence the manual-lerp implementation.

**Consequence: the goal's step 1 is answered (probe done, answer is "not viable"), and step 2's "real
shared-element bounds morph" wording does not match what Compose does.** The implementation follows Compose.

Also from the probe: an overlay composer DOES inherit `SharedTransitionScope` (through the CompositionLocal
snapshot replayed in `layout_overlays`), but `current_shared_scope()` must be read INSIDE
`SharedTransitionLayout::build`'s content closure — reading it outside yields `None`.

## 3. What was implemented (all on existing primitives)

| commit | what |
|---|---|
| `642b9a5` | `SearchBarState.progress` + `content_progress`; `drive_expansion()` pushing the Compose specs through `animation::push_animatable`; `collapsed_size` reported by `Modifier::on_size_changed` (the `onGloballyPositioned` counterpart); morph arithmetic as pure functions (`expansion_lerp` / `expansion_size` / `expansion_corner_radius` / `expansion_shape` / `expansion_vertical_padding`) |
| `4e2aefc` | the panel's width AND height come from the morph (`Modifier::size(SizeValue::Dynamic, ...)`) instead of `fill_max_size`; the panel's overlay-level animation dropped from `expand_fade` (which scaled it a second time) to a plain content fade |
| `bcd3bb2` | sawtooth fix (below) + content fade split onto its own clock |
| `47b3da5` | `Dialog` gained `position`/`offset`/`anchor_slot` (it was hard-coded to `Center`); `anchor_slide` positioning |
| `0f58516` | the slide moved to BOTH axes |
| `6889689` | filter fix, close fix, auto-focus (below) |
| `ce5f4dd` | docked = fade + vertical reveal (was `slide_down`, DropdownMenu's motion) |

### Measured results

Expansion geometry, debug server sampling the overlay root size per frame (window 420x700, bar at `(0,69)`
420x56):

```
+ 28ms 420x56      (the bar's own height)
+199ms 420x567
+711ms 420x700     (full height)          <- 600ms motion + 100ms delay
```

With the pill constrained to 260 wide (temporary edit, reverted) BOTH axes move:
`+31ms 260x56 -> +154ms 366x481 -> +495ms 418x692 -> +669ms 420x700`.

Per-frame `progress` during the expansion (temporary probe inside the size closure):

```
+ 97.8ms p=0.0082      (still in the delay)
+105.3ms p=0.4148      <- sawtooth: one frame carrying 41% of the travel
... then 0.0001-scale crawling, repeating 8 times
```

after the fix:

```
+ 99.5ms p=0.0171
+107.9ms p=0.1738      monotonic; max single-frame step 0.157 (was 0.41)
+201.2ms p=0.7490      first 8 steps average 0.0636 falling to 0.00013 at the end
```

### The three reported defects (all verified fixed on the running demo)

| report | root cause | evidence |
|---|---|---|
| typing did not filter | the panel body's `start_restartable_group` declared NO parameters, so `params_equal` was trivially true and the group Skipped every frame (`WINIA_SKIP_TRACE`: `pending_len=0`), never re-running the content closure | after: typing "bl" leaves 3 rows (Blackberry, Blueberry) of 34; before: the list kept its first rows |
| picking a result did not close | `if !active { return; }` stopped `Dialog::build`, so `active=false` was never recorded and `sync_overlays` kept the overlay for good | before: panel shrank to 420x56 and stayed, 1 overlay still present 1.5s later; after: overlay count 0 and stays 0 |
| scrim persisted | same leak as above (the scrim belongs to the overlay) | same measurement |

Also fixed: the expanded input field now takes focus on first expansion (Compose's
`LaunchedEffect { focusRequester.requestFocus() }`); without it the keyboard had no target.

## 4. NOT verified — read this before trusting the branch

1. **The docked reveal was never measured frame by frame.** The dropdown was measured appearing at its final
   size (420x280, 1 overlay). `reveal_top` is a RENDER-time clip while the debug server reports post-layout
   sizes, so the growth is invisible to that channel; the pixel probe that could see it hung twice on this
   machine. Covered only by spec assertions.
2. **The `layout_overlays` wiring has no test.** Reverting the anchor slide to the Y-only form keeps the whole
   suite green: `anchor_slide_lerp` is tested, but the call site that feeds it both axes is not. (I started
   extracting the placement into a pure `overlay_screen_pos(..)` so this becomes testable — that edit was
   interrupted and is NOT in the tree.)
3. **The filter fix has no test.** A SearchBar-level test was written and DELETED: composing the panel body
   through `take_overlays` re-entered the group every frame, so removing `ctx.changed(&query_now)` still
   passed it. A test that cannot fail is worse than none. The group mechanism itself IS pinned by
   `core::composer::tests::group_without_declared_params_reenters_or_skips` (measured: a group with no declared
   parameters Skips on frame 2 — runs stayed 1).
4. **Keyboard input cannot be driven over the debug server.** It has `k` (named keys only); `DebugEvent::Text`
   exists but has NO handler in `app.rs`. So "type and observe" is not automatable. I added a `text` command,
   found it would have returned a fake `ok`, and removed it. To verify input behaviour I temporarily gave the
   demo a scripted typing hook (`WINIA_DEMO_TYPE`), measured, then reverted it (`git diff` clean).

## 5. Measurement tooling gaps (the root cause of the churn in this branch)

- No channel for RENDER-time values (reveal fraction, alpha) — only post-layout node trees. This is why (1)
  above is unverifiable, and it is the single highest-value thing to add.
- No keyboard/text injection.
- The screenshot/pixel path (`p` command) hung repeatedly; treat it as unreliable here.

## 6. Test + code inventory

New/changed tests: `expansion_geometry_interpolates_with_progress`, `expansion_specs_match_composes_timings`,
`expansion_steps_are_monotonic_and_bounded` (each with teeth verified by breaking the code and seeing it fail),
`content_fade_is_separate_from_the_geometry_clock`, `docked_transition_is_fade_plus_vertical_reveal`,
`anchor_slide_interpolates_both_axes_from_the_anchor_to_the_window_corner`, `anchor_slide_clamps_its_progress`,
`group_without_declared_params_reenters_or_skips`.

Files touched (verified with `git diff --stat af564be..HEAD -- winia/src`): `winia/src/ui/search_bar.rs`
(744 lines changed — the component), `winia/src/ui/overlay.rs` (+132; `Dialog` placement, `AnchorSlide`),
`winia/src/core/composer.rs` (+44; the group test), `winia/src/app.rs` (+17; resolving the anchor slide),
`winia/src/ui/tooltip.rs` and `winia/src/ui/bottom_sheet.rs` (+1 each; the new `OverlayDesc` field), plus
`winia/examples/searchbar_morph_probe.rs` (the probe — DELETE IT or keep it as documented evidence; it is
currently committed).

Deliberate deviation recorded in code: winia's `TweenSpec` has no `delayMillis`, so Compose's 100ms delay is a
`KeyframesSpec` that holds the start value then follows the curve — with LINEAR segment interpolators (curved
ones replayed the easing per segment and produced the sawtooth).

## 7. If you pick this up

Do first, in this order:

1. **Add a render-time inspection channel** to the debug server (reveal fraction, alpha, and ideally the
   interpolated size for a node) — without it, items 4.1 and 4.2 stay unverifiable.
2. **Finish the `overlay_screen_pos` extraction** in `overlay.rs` and cover it, closing item 4.2.
3. Then re-run the demo and confirm the expansion and the docked reveal visually.

Known-good command for driving the demo:

```
cargo build -p winia --example search_bar_demo --features "debug-server anim-trace"
WINIA_DEBUG_PORT=9997 ./target/debug/examples/search_bar_demo.exe
# then over ws://localhost:9997:  t (tree)  c X Y (click)  q (quit)
```

The bar sits at `(0,69) 420x56`; results appear at panel-local `y=81/137`, which on screen is the bar's top
plus those offsets.
