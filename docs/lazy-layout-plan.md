# Generic `LazyLayout` — scope and plan

Compose's `LazyLayout` is the low-level entry a caller uses to build their own lazy container.
`docs/foundation-gaps.md` names it as the highest-leverage item left in the container family.
This document is the survey and the plan, not the implementation.

## What Compose offers

From `reference/compose/foundation-1.13.0-beta01/commonMain/androidx/compose/foundation/lazy/layout/`:

```kotlin
public fun LazyLayout(
    itemProvider: () -> LazyLayoutItemProvider,
    modifier: Modifier = Modifier,
    prefetchState: LazyLayoutPrefetchState? = null,
    measurePolicy: LazyLayoutMeasureScope.(Constraints) -> MeasureResult,
)
```

The contract that matters is in the KDoc (`LazyLayout.kt:71-85`): items are composed DURING the
measure pass, and "composing items during the measure pass is the signal to indicate which items
are currently needed". `LazyLayoutMeasureScope` therefore has to be able to compose:
`LazyLayoutMeasurePolicy.kt:26-34` is a fun interface over `measure(constraints): MeasureResult`,
and the scope it runs in hands out measurables for an index on demand.

## What winia has

A complete lazy family, but three separate measure policies and no shared public surface:

| Piece | Where | Visibility |
| --- | --- | --- |
| `LazyListPolicy<A>` (list) | `winia/src/layout/lazy/policy.rs:11`, 566 lines | `pub(crate)` |
| `LineLayout` (grid) | `winia/src/layout/lazy/grid.rs:343` | private, `impl MeasurePolicy` |
| `StaggeredPolicy<A>` (staggered) | `winia/src/layout/lazy/staggered.rs:56` | private |
| `IntervalList` (count + key factories + content) | `winia/src/layout/lazy/interval.rs:34`, 148 lines | `pub(crate)` |
| `LazyListState` (anchor scroll model) | `winia/src/layout/lazy/state.rs` | public |
| `LazyListLayoutInfo` / `LazyListItemInfo` | `winia/src/layout/lazy/info.rs` | public |
| Concrete containers | `LazyColumn`, `LazyRow`, `LazyVerticalGrid`, `LazyHorizontalGrid`, the staggered three | public |

There are 92 tests in `winia/src/layout/lazy/tests.rs` (3871 lines) over the concrete containers.

The architectural difference is documented at `winia/src/layout/lazy.rs:29-34`: winia's composition
is imperative and has NO measure-time composition, so the model is two-phase — estimate the visible
range during compose, correct it during measure. `layout/subcompose.rs` (722 lines) does provide
measure-time subcomposition, which is the machinery Compose's model needs, but no lazy container
uses it today.

## The gap, stated precisely

A caller who wants a lazy container that is not one of the seven shipped ones has no door. To
build one they would have to copy `policy.rs` and re-implement the anchor-scroll protocol
(first visible index + offset, key remapping when items move, estimate/priced cache, beyond-bounds
window, sticky headers, item animations). That protocol is the thing `LazyLayout` exists to share.

## Proposed shape

Two traits and one composable. The two-phase model is kept: the composable owns the window, the
policy owns placement — a caller does not have to know how items get composed.

```rust
/// What a lazy layout lays out: a count, a key per index, and the content for an index.
pub trait LazyLayoutItemProvider {
    fn item_count(&self) -> usize;
    fn key(&self, index: usize) -> u64;
    fn compose_item(&self, ctx: &mut ComposeCtx, index: usize);
}

/// How a measured set of items becomes a viewport.
pub trait LazyLayoutMeasurePolicy {
    /// `sizes` are the measured main/cross extents, in index order from `first_index`.
    fn measure(
        &mut self,
        sizes: &[(usize, Size)],
        viewport: Size,
        state: &LazyListState,
    ) -> LazyLayoutMeasureResult;
}

LazyLayout::new(provider).policy(MyPolicy).state(state).build(ctx);
```

`LazyLayoutMeasureResult` carries what the existing policies already compute and hand back:
placements, the new `(first_index, offset)` anchor, and the `LazyListLayoutInfo` the state stores.

The three existing policies then become implementations of that trait. Their private knowledge —
the anchor update, the estimate/priced cache, sticky headers — moves into the shared `LazyLayout`
driver, so a custom policy inherits it.

## Phases

| Phase | Work | Buys | Test |
| --- | --- | --- | --- |
| P0 | Move the anchor-scroll driver out of `LazyListPolicy` into a `LazyLayout` composable; the list becomes `LazyLayout` + `LazyListPolicy`. No new public API yet. | One driver instead of three copies of the protocol | the existing 92 lazy tests must pass unchanged |
| P1 | Public `LazyLayoutItemProvider` + `LazyLayoutMeasurePolicy` + `LazyLayout`; `IntervalList` becomes the default provider. | A caller can build a custom lazy container | a custom two-column policy in a test, composed and scrolled |
| P2 | Port grid (`LineLayout`) and staggered (`StaggeredPolicy`) onto the trait. | One code path for all three families | their existing tests, unchanged |
| P3 | Decide prefetch and measure-time composition: keep the two-phase estimate, or move the driver onto `subcompose` the way Compose does. | Either a documented deviation or parity | a doc section plus a bench of first-frame item count |

P0 is the risky half: it touches the code path every list, grid and pager already use, and the
existing 92 tests are the safety net. P1 is where the API becomes public; nothing is exposed before
it is exercised by all three families (P2).

## Open questions

1. **Prefetch.** Compose's `LazyLayoutPrefetchState` composes ahead of a scroll that has not
   happened. winia has `LAZY_BEYOND_BOUNDS` (4 items each side, `lazy.rs:35`), which is a window,
   not a scheduler. Does the public API expose prefetch at all in P1, or is it added with the
   first caller that needs it?
2. **Sticky headers** are per-interval (`IntervalList::is_sticky`). A custom provider has no
   intervals, so either the trait grows `sticky: bool` per index or sticky stays a list-family
   feature.
3. **Saveable item state.** Compose resets it when an item leaves the window. winia has
   `SaveableStateHolder` for navigation, not for lazy items; is that part of P1 or a later phase?
4. **Item animations** (`layout/item_animation.rs`) key off appearing/disappearing placements. A
   custom policy that reports placements gets them, but only if the driver, not the policy, owns
   the appearance diff — which argues for putting it in P0's driver.

## What this document is not

It is not evidence that the API is wanted: no caller in the repo needs a custom lazy container
today. It is the answer to "what would it take", with the file-and-line inventory that makes the
estimate checkable, so the next round can start at P0 instead of re-reading 8000 lines.
