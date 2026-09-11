# Nav × shared elements — research and plan (branch `exp/nav-shared-transition`)

Status: research complete, implementation not started. Everything below is either quoted from
Compose's own sources/docs or verified against winia's code with a file:line; nothing here was
measured by running a nav + shared-element app, because that is what this branch is going to build.

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
| Entry decorator hook | Present: `NavEntryDecorator` trait and the `wrap_entry(ctx, key, entry, decorators)` chain, `winia/src/nav.rs:1124-1167` — explicitly modelled on Nav3's decorator. |
| Entry content composition | All destinations compose in ONE `ctx` under `ctx.key(scene.scene_key())`, `winia/src/nav.rs:1084-1090` (`scene.content(ctx, &layer_render)`). |
| Nav transition | The nav renders previous + current scene as two layers with its own per-layer params (alpha / translation / scale) driven by a progress `p`, `winia/src/nav.rs:1040-1119`. |
| Shared-transition scope | `SharedTransitionLayout` + `current_shared_scope()`, usable around any content; usage in `docs/shared-element-usage.md:17,59-60,99`. |
| Nav ↔ shared bridge | MISSING. `docs/navigation3.md:238` lists `SharedTransitionScope` / `sizeTransform` as an open gap, and `:356` lists `sharedTransitionScope + SharedEntryInSceneNavEntryDecorator` among the things Compose has and winia does not. |

## 3. The crux — why "just mark both screens" does not work

A flight is triggered when a marked key **disappears from the live tree**: `retain_shared_sources`
collects the keys of `prev_shared_endpoints` that are not in the new `live` map
(`winia/src/ui/shared_transition.rs`, the `gone` loop), then `detach_source` freezes that node as
the leaving end.

The live map tolerates two ends with the same key but **keeps only the last one** and only logs:

```rust
// winia/src/ui/shared_transition.rs:1909-1924
if out.insert((m.scope_id, m.key.clone()), node.slot_key).is_some() {
    crate::debug_log!("[shared] duplicate live endpoint scope={} key={}", …);
}
```

During a nav transition BOTH scenes are composed at once (`nav.rs:1092-1119`), so the marked key is
present in both layers: it never disappears, no flight is created, and the marker animates only
through the nav's own layer params. That is the gap to close.

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
