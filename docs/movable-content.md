# Movable content: supported contract and Compose differences

`ctx.remember_movable_content(content)` remembers a callable. `handle.compose(ctx)` places its content
under the current host. Moving that placement between hosts in one completed composition preserves
its remembered values and `LayoutNode.id`s. The stored subtree is owned separately from the host, so
removing the old branch does not delete state already claimed by the new branch.

## Supported behavior

- A State read inside the payload belongs to its stored reader scope, not to the caller. Invalidations
  enter both the reader and the current host; skipping an unchanged host preserves its dependencies.
- Compiler scopes, explicit keys, statement positions and local counters are isolated per handle.
  Changing invocation order or the caller's `ctx.key` does not change the payload's identity.
- CompositionLocals come from the invocation site. Moving between providers updates the payload while
  preserving its remembered state and nodes.
- Each actual invocation conservatively re-enters the stored child scopes. This applies fresh captured
  values and locals even when a clean wrapper declared no parameter changes. Unchanged hosts can skip
  the invocation completely. Precise tracked-lambda/parameter revision skipping is not implemented.
- Removing a child from the payload removes it from rendering and disposes its node. Reintroducing an
  actually removed child creates a new composition; it is not a state cache.
- A completed `Composer::compose` with no placement releases the stored subtree, even if the callable
  is still remembered or externally retained. Later placement starts fresh. In winia, this cleanup
  boundary is the composition call; Compose's Recomposer can transfer released state across intermediate
  passes within one frame and cleans up unused state at the end of that frame.
- A caught payload panic discards the failed placement's products and reads and restores caller stacks,
  key counters and parameter cursor before resuming unwind. A caught retry is allowed with fresh state;
  this is not a transactional rollback of the payload's previous remembered values or side effects.

The callable is initialized once. Replacing an unrelated closure passed to the same remember site does
not update its captures. Read dynamic values from State or a deliberately maintained latest-value
channel. The navigation suite stages the latest repeatable icon/label callbacks through a Backchannel
and invalidates its actual shape hosts on each scaffold build.

## Deliberate minimal-API restrictions

A handle supports **one live placement per composition**, and movable invocation cannot be nested.
Unsupported duplicate or nested invocation is rejected explicitly, never silently omitted. Use distinct
handles for simultaneous copies. Rejected invocation does not consume payload key ordinals.

These restrictions are **not Compose rules**. Compose allows multiple independent placements of one
callable. Surviving composition locations keep their existing instances before released instances are
assigned to new locations; excess new placements initialize fresh state. Supporting that safely needs
a placement-matching phase, not repeated references to the same nodes or one global per-frame counter.

The navigation suite retains payloads by item position, not destination key. Icon/label nodes move, but
the bar/rail-specific item wrappers do not. Keyed destinations and whole-item state migration remain
separate API work. Its optional collapse/swap/expand morph is winia-specific; `transition(false)` selects
a direct shape switch, and the two shape branches never overlap.

## Verification

- `winia/tests/movable_content.rs` exercises production keys, identities, subscribed state updates,
  payload refresh, caller keys, repeated functions, child removal, store release, caller locals,
  previous-sibling lookup, reference replacement, skipped hosts and caught panic/retry boundaries.
- `runtime::movable::tests` checks actual node parenting, loop placement, removed-child recreation,
  measured host/payload resize and the real nested invoking parent during a round trip.
- The `nav_suite_state` fixture and `a_navigation_suites_items_keep_their_state_across_a_shape_switch`
  test use explicit controls, actual tree geometry, twelve unique retained node ids, independent counters
  and fresh caller-captured labels across bar -> rail -> bar. No timer or requested-shape text is used
  as evidence that a move occurred.

## Upstream references

- [Public movableContentOf API](https://developer.android.com/reference/kotlin/androidx/compose/runtime/package-summary#movableContentOf(kotlin.Function0))
- [AndroidX design: locals and multiple placements](https://android.googlesource.com/platform/frameworks/support/+/refs/heads/androidx-main/compose/runtime/design/movable-content.md)
- [MovableContentTests](https://github.com/androidx/androidx/blob/androidx-main/compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/MovableContentTests.kt)
- [Recomposer matching and unused-state disposal](https://github.com/androidx/androidx/blob/androidx-main/compose/runtime/runtime/src/commonMain/kotlin/androidx/compose/runtime/Recomposer.kt)

Matching and disposal were also checked against cached runtime-android 1.11.4 sources:
`Recomposer.kt` calls `discardUnusedMovableContentState` after apply, clears the release pool and invokes
`disposeUnusedMovableContent`; retaining the callable is not a promise of keep-alive remembered state.
