# Nav × shared elements — research and plan (branch `exp/nav-shared-transition`)

Status: research complete and implemented on this branch. §2–§5 are the research as it was written before
the code existed: the diagnosis path, not a description of the current state. §6 lists what the branch
actually does and what is still open. Compose quotes come from its sources/docs; every number labelled
measured comes from `anim-trace` (`docs/anim-trace.md`) or the debug server, and the file it came from is
named where it matters.

## 1. What Compose does

### 1.1 The bridge is a NavEntry decorator

androidx (Navigation 3) ships a decorator that turns every entry into a shared element keyed by the
entry key, and a composition local that carries the scope to the screens:

```kotlin
// A NavEntryDecorator that enables shared element transitions within a navigation scene.
// It wraps the content of a navigation entry in a Box with the sharedElement modifier.
val sharedEntryInSceneNavEntryDecorator = navEntryDecorator { entry ->
    with(localNavSharedTransitionScope.current) {
        Box(
            Modifier.sharedElement(
                rememberSharedContentState(entry.key),
                animatedVisibilityScope = LocalNavAnimatedContentScope.current,
            ),
        ) { entry.content(entry.key) }
    }
}

val localNavSharedTransitionScope: ProvidableCompositionLocal<SharedTransitionScope> =
    compositionLocalOf { throw IllegalStateException("… You must provide a SharedTransitionLayout …") }
```

(Kotlin source as carried by AndroidX documentation/community copies of `navigation3-runtime`; the
file could not be read raw from this sandbox — `developer.android.com` resolves to a non-public
address here, so the text above comes through a search extract rather than a direct fetch.)

The official guide adds the usage rules:

- wrap the `NavDisplay` in `SharedTransitionLayout`;
- hand the resulting `SharedTransitionScope` to the screen composables;
- take the `AnimatedVisibilityScope` from `LocalNavAnimatedContentScope`, "the `AnimatedContentScope`
  from the `AnimatedContent` that `NavDisplay` uses internally to animate between scenes".

### 1.2 Who owns the motion

The element's bounds animation is tied to the **nav's own transition clock** through that
`animatedVisibilityScope`: the element is visible/animating according to the scene it belongs to,
not according to an independent animation. Compose's `SharedElement` node also refuses to draw the
outgoing copy at all (`renderOnlyWhenVisible`, "only the shared element that is becoming visible
will be rendered during the transition" — recorded as an open deviation for winia in
`docs/shared-element-gaps.md`).

### 1.3 Scene-level shared elements (Navigation 3 1.1.0)

The 1.1.0 release notes state that navigation3 "now supports treating scenes as shared element
object", enabled by passing a `SharedTransitionScope` to `NavDisplay` or to `rememberSceneState`,
together with a new `SceneDecoratorStrategy`. So Compose has both levels: a hero inside two screens
(decorator + per-element markers) and the whole scene as one shared element.

## 2. What winia has today

| Piece | State |
|---|---|
| Entry decorator hook | Present: `NavEntryDecorator` trait and the `wrap_entry(ctx, entry, decorators)` chain, `winia/src/nav.rs:1291` — explicitly modelled on Nav3's decorator. |
| Entry content composition | All destinations compose in ONE `ctx` under `ctx.key(scene.scene_key())`, `winia/src/nav.rs:1168` (`scene.content(ctx, &layer_render)`). |
| Nav transition | The nav renders previous + current scene as two layers with its own per-layer params (alpha / translation / scale) driven by a progress `p`, `winia/src/nav.rs:1087-1250` (the two-layer `Stack` is at `:1209`). |
| Shared-transition scope | `SharedTransitionLayout` + `current_shared_scope()`, usable around any content; usage in `docs/shared-element-usage.md:17,59-60,99`. |
| Nav ↔ shared bridge | Implemented on this branch (see §6): each transition layer publishes itself as a scene (`scene_tag` + `with_nav_scene`, ids namespaced per display and layer role), the pairing picks the end in the scene that is **not leaving**, and the scene owns opacity while the flight owns the rect. Still open: an entry-level FLIGHT needs a scene arrangement that re-slots one entry (winia's nav does not do that today — the decorator does take part in the transition-scoped size morph), and `sizeTransform` is a separate gap (`docs/navigation3.md:238`; `:361` lists the decorator among the things Compose has). |

## 3. The crux — why "just mark both screens" does not work

A flight is triggered when a marked key **disappears from the live tree**: `retain_shared_sources`
collects the keys of `prev_shared_endpoints` that are not in the new `live` map
(`winia/src/ui/shared_transition.rs`, the `gone` loop), then `detach_source` freezes that node as
the leaving end.

The live map tolerated two ends with the same key but **kept only the last one** and only logged (the
code below no longer exists — it is the shape the diagnosis started from):

```rust
// winia/src/ui/shared_transition.rs — REMOVED; this is the pre-fix shape quoted for the diagnosis
if out.insert((m.scope_id, m.key.clone()), node.slot_key).is_some() {
    crate::debug_log!("[shared] duplicate live endpoint scope={} key={}", …);
}
```

During a nav transition BOTH scenes are composed at once (`nav.rs:1209-1250`), so the marked key is
present in both layers.

**MEASURED, and it corrects the first guess:** flights ARE created on a nav transition — a single
navigate produced TWO `begin_flight` calls for `key=hero` in the same transition
(`old_slot=0xc561… new_slot=0x8c86… src_idx=7` then `old_slot=0x8c86… new_slot=0x1922… src_idx=16`),
together with 143 `[shared] duplicate live endpoint` lines. So the bridge is not "missing the
flight"; it is **ambiguous which end is which**: the live map keeps the last end, the two scenes
alternate as "the last one", the winner's slot flips between them, and each flip reads as a fresh
switch — so flights are started, cancelled and retargeted repeatedly while the nav's own layer
params animate the same nodes. What a correct bridge needs is a stable pairing (the end whose scene
is becoming visible is the Target, the other is the Source) plus one owner for the marked element's
motion.

## 4. Plan

1. **Scenes carry a visibility, not just params.** The nav already computes a per-layer progress
   `p` and applies alpha/translation/scale (`nav.rs:1040-1073`). Expose that as a scene-scoped
   "am I entering / leaving / settled" signal so a marker can say which scene it belongs to.
2. **A `SharedEntryInSceneDecorator` for winia**, mirroring the Compose one: wrap each entry's
   content in a container carrying a shared marker keyed by the entry key, so the entry itself can
   fly between scenes; keep per-element markers (a hero) working inside it.
3. **Make the live map scene-aware for flights.** When two ends share a key, the one whose scene is
   becoming visible is the Target and the one whose scene is leaving is the Source — that is the
   Compose rule the `AnimatedVisibilityScope` encodes. The flight is then created from that pair
   instead of from a disappearance, and both ends stay live (no detach), which also means the
   outgoing end must keep painting through the nav's own fade.
4. **Define the ownership rule explicitly.** With a flight active for a marked element, the nav's
   layer params must not ALSO animate that element (double motion); the natural rule is that the
   shared bounds flight owns the marked element's rect, while the scene params keep applying to
   everything unmarked. Compose's split (scene transition vs shared element) is the model.
5. **Demo + verification.** An example that pushes a detail route with a hero marked in both routes,
   driven through the debug server: count `[dup-key]` in a redirected log, sample the hero's pixels
   before / mid / after the transition, and keep a headless test whose revert goes RED.

## 5. Open questions to settle while implementing

- Does the outgoing scene need to stay composed (nav already keeps it as a layer) or should the
  flight detach it like a screen switch does? Compose keeps it composed and lets the visibility
  scope decide; winia's nav already keeps both layers, which suggests "keep both, drive from
  scene visibility" — but the frozen-ghost path exists and is proven, so a fallback is available.
- The nav's transition is a plain progress `p` with no per-element visibility API yet; the decorator
  may need to hand each entry a scene-local `VisibilityTransition` pair so per-element enter/exit
  can ride it.
- `sizeTransform` (`docs/navigation3.md:238`) is a separate gap: it is the Compose knob for how a
  shared element's size animates; winia's equivalent is `ResizeMode` + `PlaceHolderSize`.

## 6. Implemented so far (branch `exp/nav-shared-transition`)

| Step | State |
|---|---|
| Reproducer demo (`examples/nav_shared_element_demo.rs`) | Done. Content must sit inside `Window::new().build(…)`; without it the tree is empty, with no panic and no fps lines (cost a bisect). |
| Scene-aware pairing | Done. `ModifierElement::SharedTransition` carries a `scene` field, but scene membership is read from a `SceneTag` on the ANCESTOR chain (`scene_of_node`) — a marker's modifier element is built once and reused, so a captured id freezes (measured: one end `scene=None`, the other reported the LEAVING scene). `with_nav_scene(NavSceneInfo { id, visibility, is_prev })` publishes a scene per composed layer (visibility is a **closure**, because the nav animates its layers from the render path and a compose-time snapshot would go stale mid-transition; the id is namespaced by host and layer role, so two displays on one route cannot collide); `nav.rs` publishes `p` for the leaving layer and `1 - p` for the entering one. `shared_live_map` resolves a duplicated key by **the end whose scene is NOT leaving** — deliberately not "the more visible end", because a leaving scene STARTS at visibility 1 and the entering one at 0, so comparing visibilities picked the leaving end at the switch frame (measured: the flight's target was the 96x96 list hero and it never grew). |
| Ownership rule | Done. `TransitionVisual::scene_alpha` makes the SCENE own opacity when an end carries one, so the flight stops crossfading the same element a second time; the rect stays the flight's. Detached sources carry their scene's fade (the scene's layer no longer applies to them), targets stay opaque unless the host elevates them too. Tier 1 (cross-composer) passes `None` — scene ids are published per composer. |
| Entry-level decorator | Done: `nav::SharedEntryInSceneDecorator`, winia's counterpart of Nav3's `sharedEntryInSceneNavEntryDecorator` — it wraps each entry's content in a `shared_bounds` keyed by the entry's stable content key, and degrades to rendering unwrapped when no `SharedTransitionLayout` is around the display (Compose throws there). **Measured: it opens no entry FLIGHT in this demo's flows** — a plain List→Detail push opens one flight per marked key (`hero`, and `badge` since the demo gained a second shared element) and none for the entry, because the two scenes hold two different entries; switching single-pane ↔ two-pane also opens no entry flight, because an entry's slot stays put: the nav wraps entries in `ctx.key(entry.content_key())`, so the same entry keeps its composition identity while the scene arrangement changes, and a flight needs a slot change. It DOES take part in a size morph, though: with the two-pane toggle flipped while a transition is running, the entry marker morphs its box (`morph_open flight:entry:… fresh 252x484 -> 168x484 at (0,136)`, measured on the running demo) — the decorator acting as a same-screen size animation, which is not a flight and therefore does not contradict the sentence above. A real entry flight would need a scene arrangement that RE-SLOTS one entry (the same entry rendered by two scenes at once, e.g. predictive-back previews or a two-pane strategy that re-parents a pane), which winia's nav does not do today; that is a nav-side feature, not a decorator bug. |

Measured (debug server + `anim-trace`, `examples/nav_shared_element_demo`; the trace files are named where
the number is not reproducible from the repository alone):

- `begin_flight` per marked key: **2 → 1** (temporary probe, reverted; one navigate now opens one flight
  per key — `hero` and `badge`);
- duplicate-endpoint lines with no scene visibility to resolve them: **0**;
- `[dup-key]`: **0**; hero centre `(74,121,177)` → `(83,138,202)` → `(89,149,219)` across the
  transition, at both the 96x96 list card and the 320x220 detail card;
- the flight's own rect and the two ends' opacity, from `tmp/trace6.ndjson`: `painted` (16,193) 96x96 →
  (16,283) 320x220, the leaving end's alpha 1.00 → 0.00 and the entering end's 0.00 → 1.00. (The `y` values
  moved when the demo gained its third toggle; the earlier 153 → 243 belongs to the two-toggle layout.)
- clicking Back **while the flight still runs** retargets instead of dying, from the rect it had reached:
  from `tmp/a1_post.ndjson`, at the cancel frame the hero is 202x155 and the new flight continues
  202x155 → 201x154 → 192x149 → 155x129 → 96x96, with the cancel recorded at p=0.46 in that run (an earlier
  run of the same click was at p=0.963 — the value depends on when the click lands);
- a window resize opens **0** morph flights (the morph gate is transition-scoped; before it, a resize
  opened one per marked node whose rect changed);
- a leaving scene that re-composes a copy of a flying key is suppressed: the copy is recorded as
  `phase=placeholder` (195 records for `hero` in `tmp/a3.ndjson`) and neither painted nor hittable;
- a scene's opacity is continuous when a transition is re-targeted mid-flight: re-targeting restarts the
  nav progress at `1 - p` on a fresh clock, because the two layers swap their roles. Measured by stable
  scene key on the demo (before: scene 466407 `0.770 → 0.216` in one frame; after: `0.781 → 0.761 → 0.749`).

## 7. What the branch also carries

- **`anim-trace`** (`docs/anim-trace.md`): the facility behind every measurement above — per-frame
  records of layout vs painted rect and three-part opacity, a file sink, a live `tr` command, a report
  tool and a headless capture API. No application code is involved.
- **`PaintDisposition`**: one enum per node (`InTree` / `InLayer` / `Placeholder`) decides where its
  pixels come from during a transition; the render walk and the hit walk ask the same question.
- **Scene ownership of opacity + flight ownership of the rect**, both measured above.
- **Morphs are transition-scoped**: a bare layout change (a window resize) opens nothing; the same
  change inside a running transition still morphs.

Test coverage: the alpha rule is pinned by a unit test (three roles × three visibilities, with and
without an enter/exit pair); scene ids, the scene registry's pruning, the placeholder's hit exclusion,
the scope-activity union and the trace's trajectory are pinned by their own tests (see the commit
bodies for each). **NOT covered by a headless test:** the pairing rule itself. An attempt to build one
composed two same-key ends in a single closure, which is NOT what a scene host does — measured, the two
copies collapse into one node, so the attempt does not even produce a duplicate. A faithful headless
scene host (two layers, both re-published every frame) is what such a test needs; until then the pairing
rule rests on the debug-server and trace measurements above.

