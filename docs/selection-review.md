# Selection subsystem review: findings and checklist

> Review of `winia/src/components/selection_container.rs` and the machinery it drives
> (`winia/src/text/selection.rs`, the drag path in `winia/src/app/per_window/pointer.rs`, the
> registrar walk in `winia/src/runtime/composer/engine.rs`), against the Compose reference in
> `reference/compose/foundation-1.13.0-beta01/.../text/selection/`. Performed 2026-10-29 on branch
> `v2` at `00f2a1f`. Every claim below was measured with throw-away probe tests compiled inside the
> crate; the probes were deleted before the review closed, so the tree is unchanged and **no
> behaviour has been fixed** — this file is the work list.
>
> Companion: [`docs/text-review.md`](text-review.md) (the text stack this sits on).
>
> Convention: `- [x]` done and verified, `- [ ]` open. Keep the reason column honest.

## Status at a glance

| Bucket | Open | Fixed |
|---|---|---|
| Defects (wrong behaviour) | 6 | 0 |
| Deviations from Compose (deliberate or accepted) | 3 | 0 |
| Missing API surface | 7 | 0 |
| Investigated and ruled out | — | 4 |

## Defects

- [ ] **S-1 — a drag stops the moment the pointer leaves the text.** `handle_pointer_move` only
  runs the selection update when `nodes[innermost].cached_paragraph` is `Some`
  (`winia/src/app/per_window/pointer.rs:719`), and `innermost` is whatever the pointer is over. Over
  the container's own padding (or anywhere without text) that borrow is `None`, so the whole block
  is skipped and `compute_selection` is never reached. Measured: press inside "aaaa bbbb", move
  inside the text → `Some((1, 8, "aaa bbb"))`; move on into the container's padding → the move
  returns `false` and the range stays `1..8`; move further down → still `1..8`. Compose extends the
  selection to the end of the text (and auto-scrolls) instead of freezing it. Not fixed: the fix is
  a pointer-capture redesign (remember the anchor's text node and keep mapping the pointer into ITS
  paragraph, with edge snapping when outside), not a local patch.
  Consequence worth stating plainly: the tail of the last line cannot be selected by dragging past
  it, which is the ordinary way a user selects a whole line.
  Test to add when fixed: `a_drag_past_the_text_extends_to_its_end` (assert `1..=len` after dragging
  below the node).
- [ ] **S-2 — the "pointer in the container's empty area" branch of `compute_selection` is dead**
  (`winia/src/components/selection_container.rs:114-119`). It requires `cur_global_off == None`
  while the pointer is over a node carrying the SAME registrar; the only nodes that carry a
  registrar are text leaves, and every text leaf that registers is in the table, so `cur_global_off`
  is always `Some`. Its unit tests pass because they call `compute_selection` directly with
  `cur_global_off = None`. This is the same behaviour gap as S-1, seen from the other end: once S-1
  is fixed by capturing the anchor, this branch becomes the real edge-snap path and should be
  rewritten against it rather than kept as dead code with tests that cannot fail.
- [ ] **S-3 — a horizontal cross-container drag snaps to the wrong end**
  (`winia/src/components/selection_container.rs:126`). The edge is chosen by `scene_y < down_y`
  only, so a purely horizontal drag (a Row of selectable columns) always answers "the end".
  Measured: two containers in a Row, press inside B's text at `(208, 10)`, drag LEFT into A's text
  at `(100, 10)` → B's selection is `(1, 9)` = to the END of B, where dragging left should reach the
  START. The same call with the pointer to the right gives the identical `(40, 100)`. Compose picks
  the edge from the direction of travel in both axes. Fix: compare against the anchor's own box
  (`down_x`/`down_y` and the anchor node's rect), not y alone.
  Test to add: `dragging_left_across_containers_reaches_the_anchor_start`.
- [ ] **S-4 — `selected_range` reports a collapsed caret as a selection; `build_selection` does
  not** (`winia/src/text/selection.rs:170-179` vs `:189-199`). The guard is
  `local_start >= seg.text_len || local_end == 0`, so a caret at offset 0 answers `None` while a
  caret at offset 5 answers `Some(5..5)`. Measured: `set_selection(5, 5)` → `selected_range(1) =
  Some(5..5)`, `build_selection() = None`; `set_selection(0, 0)` → `None`. The two functions disagree
  about what a zero-width selection is. One consumer branches on `.is_some()` and would paint a
  **full-width** highlight rect for a caret: `winia/src/render/text_field.rs:342`. That consumer is
  the uncached fallback (`render/pass.rs:1044`, the `else` of `if let Some(para)`), reached only for
  a text node with no cached paragraph — the cached paths at `render/pass.rs:925` and `:1092` guard
  with `range.start < range.end`, which is why nothing has been seen on screen. So the reachable
  consequence today is the API disagreeing with itself, not a painted band; the band is latent.
  Fix: return `None` from `selected_range` when `local_start == local_end`.
  Test to add: `a_collapsed_selection_reports_no_range`.
- [ ] **S-5 — the selection does not follow its characters when a sibling appears**
  (`winia/src/text/selection.rs:146-164`). Offsets are clamped after `reindex`, never remapped, and
  the range is a single global byte span. Measured: select `BBB` (`0..3`), insert `AAA` above it →
  the range stays `0..3`, which now reads **"AAA"**. The mirror case, removing the earlier text,
  clamped `3..6` down to `total = 3` and the selection became `None`. Compose anchors on
  `(selectableId, offset)` per end, so the selection stays on its own characters. Fix needs anchors
  in `Selection`, not a clamp.
  Test to add: `inserting_a_sibling_keeps_the_selection_on_its_own_text`.
- [ ] **S-6 — stitching is in tree order, not reading order**
  (`winia/src/runtime/composer/engine.rs:1431-1454` collects slots by a pre-order walk). Measured: a
  Row of two Columns ("A1","A2") and ("B1","B2") stitches `"A1A2B1B2"`, while the reading order on
  screen is `A1 B1 A2 B2`. Compose sorts selectables geometrically (y first, then x, with an
  `inARow` heuristic in `SelectionRegistrarImpl.sort`). Any multi-column selectable layout therefore
  copies text in the wrong order today.
  Test to add: `a_two_column_layout_stitches_in_reading_order`.

## Deviations from Compose

- [ ] **D-1 — clearing a selection is silent.** `fire_on_change` only runs `build_selection()`, which
  returns `None` for an empty range, so the callback is never told the selection went away
  (`winia/src/text/selection.rs:224-231`, documented at `selection_container.rs:33-36`). Measured:
  after `set_selection(0, 5)` + `fire_on_change` the callback saw `"hello"`; after
  `clear_selection()` + `fire_on_change` it saw nothing; after collapsing to `(3, 3)` it saw nothing.
  Combined with D-2 this means a "N characters selected" label stays at N forever. Compose's
  `SelectionState.selection` becomes observable `null`, so its consumers can reset. Reason it is not
  filed as a defect: the doc comment states the intent, so this is a designed limitation that has
  turned out to be the wrong design for any consumer that displays the selection.
- [ ] **D-2 — tapping elsewhere does not clear the selection.** `handle_pointer_down` clears the
  registrar of the node it hit (`pointer.rs:311`), which is the wrong node whenever the tap does not
  land on a selectable text: measured, after selecting in A, pressing a plain (unselectable) Text
  below and pressing A's own padding both left A at `Some((1, 9))`. Compose clears on any pointer-up
  that is not a drag end (`SelectionManager.modifier` → `onClearSelectionRequested { onRelease() }`)
  and on focus loss. Measuring also showed two containers holding a selection at the same time
  (`A = Some((1, 9))`, `B = Some((1, 9))`), which Compose's single-focus rule prevents.
  Fix: dispatch the clear through the registrar that owns the OPEN selection (or through the
  container's own hit area), not through the registrar found at the press point.
- [x] **D-3 — the multiset of texts a container reports is correct.** Ruled out as a deviation: a
  text that leaves the tree loses its segment (`reindex` drops slots the walk did not report, and
  `total` went `6 -> 3` when the first text was removed), and a text that Skips keeps it (`6 -> 6`).
  The `reindex` note in `selection_container.rs:55-61` is accurate — the earlier assumption that a
  removed text lingers came from a probe whose State was read OUTSIDE the group, so the subtree
  never re-entered and the node was never removed. Record the trap: reading the flag inside the
  `Column` body (as the passing T-1 test does) is what makes the removal real.

## Missing API surface

- [ ] **M-1 — no way to copy a CONTAINER selection.** The write path now exists
  (`winia/src/clipboard.rs`: a component requests a copy, `AppState::window_event` hands it to
  winit's `set_clipboard` as the event finishes), and `TextField`'s Ctrl+C / Ctrl+X use it. What is
  still missing is the container side: `SelectionContainer` does not listen for Ctrl+C, so a
  selected paragraph cannot be copied, and there is no context menu either. Compose wires
  `isCopyKeyEvent(it) { copy() }` into `SelectionManager.modifier`.
  Measured 2026-10-29: a temporary example queued a known string before `run_app`, the first window
  event drained it, and the OS clipboard went from a `Set-Clipboard` sentinel to that string; it
  STILL held it after the process was killed, which is winit flushing through `OleFlushClipboard`
  (`arboard` writes a snapshot and would not survive). The component half is pinned by
  `app::per_window::clipboard_copy_tests` (Ctrl+C copies the selection, a caret copies nothing,
  Ctrl+X copies and clears). What remains for M-1 itself: bind Ctrl+C in `SelectionContainer`: the
  selection is global across the container, so the handler needs the container's own registrar, not
  a text node's — and that is the same missing piece as D-2's clear dispatch.
  The repo now pins winit **0.31.0-beta.4**, which adds
  `ActiveEventLoop::clipboard()` / `set_clipboard()` over the data-transfer API, implemented on
  Windows as well (changelog `winit/src/changelog/v0.31.md`; the beta.4 sources are in the local
  cargo registry). It type-checks against this crate today: `clipboard()` returns
  `Result<Option<DataTransferId>>`, `fetch_data_transfer(id, &TypeHint::Plaintext)` asks for the
  bytes, and `WindowEvent::DataTransferReceived { id, serial, value }` delivers them as
  `value.try_as_string() -> Option<String>`; writing is
  `DataTransferSendBuilder::new(text).with_type(TypeHint::Plaintext, ..).build()` into
  `set_clipboard`. The WRITE half is adopted; the READ half is not, because winit's read is
  asynchronous (a paste becomes a round trip through the event loop where `arboard` answers
  synchronously inside the key handler) — that is the next step, not this one.
- [ ] **M-2 — no `SelectionState`.** Compose exposes `rememberSelectionState()` with
  `state.selection`, `selectedText`, `selectAll()` and `clear()`; winia has only the
  `on_selection_change` callback and no programmatic handle on the selection.
- [ ] **M-3 — no selection handles.** Compose draws and drags a start/end handle
  (`SelectionHandles.kt`, `manager.handleDragObserver`); winia has none, so an existing selection
  cannot be adjusted, only re-dragged from scratch.
- [ ] **M-4 — no context menu / toolbar.** Compose adds `ContextMenuArea` + text-toolbar components
  (`addSelectionContainerTextContextMenuComponents`); winia has no selection menu at all.
- [ ] **M-5 — no word / paragraph selection.** No double-click-to-select-word, no triple-click, no
  `SelectionAdjustment.Word` / `Paragraph`. Verified by grep: `pointer.rs` has no click-count
  tracking and there is no `SelectionAdjustment` equivalent in the crate.
- [ ] **M-6 — no `DisableSelection`.** Compose can opt a subtree out; winia has no equivalent, so the
  only way to keep a text unselectable is to keep it out of any `SelectionContainer`.
- [ ] **M-7 — the highlight colour is hard-coded.** `Color::from_argb(80, 100, 150, 255)` at
  `render/pass.rs:929`, `render/pass.rs:1095` and `render/text_field.rs:344`. Compose reads
  `LocalTextSelectionColors` (handle colour, background colour), so a themed selection is
  impossible. Cheap to fix: one `CompositionLocal<TextSelectionColors>` read at compose time into
  the node, like `focus_color`.

## Investigated and ruled out

- [x] **A removed text keeps its segment** — false; measured `6 -> 3` and the selection clamped. See
  D-3 for why the first probe misread it.
- [x] **`register()` corrupts offsets when a text grows** — false; the
  `next_global_offset.max(offset + text_len)` advance keeps the table contiguous, and `reindex`
  rebuilds from the tree at the end of every layout anyway.
- [x] **Nested containers lose their inner texts** — false, but fragile: measured
  `outer total = 23` ("outer before" + "outer after") and `inner total = 10`, i.e. the inner
  container's text is NOT part of the outer's selection, which is the intended nesting. The
  fragility: the inner `build` ends with `ctx.clear_selection_registrar()`
  (`selection_container.rs:73`), so a text built AFTER the inner container inside the outer one sees
  `ctx.selection_registrar() == None` (measured). It still registers correctly because
  `Text::build` falls back to `LOCAL_SELECTION_REGISTRAR.try_current()`. Any future consumer that
  reads only the composer-level registrar will silently miss it.
- [x] **A stale callback survives a rebuild that drops `on_selection_change`** — TRUE but narrow:
  measured, frame 0 installs the callback (1 fire), frame 1 rebuilds without it and the callback
  still fires (2 fires), because `set_on_change` is only called when `on_change` is `Some`
  (`selection_container.rs:48-50`). Not filed as a defect because dropping the callback is rare and
  the stale closure cannot observe anything it did not already own. Fix when convenient: call
  `set_on_change` unconditionally with an empty closure, or add a `clear_on_change`.

## How to re-check

Every measurement above came from a probe module compiled into the crate and deleted afterwards.
To reproduce one: add `#[cfg(test)] mod probe_step;` next to `mod pointer;` in
`winia/src/app/per_window.rs`, build a `PerWindow` with the layout under test, then drive it with
`pointer::handle_pointer_down` / `handle_pointer_move` (both `pub(crate)`) and read the registrar
through `LOCAL_SELECTION_REGISTRAR.current()`. Read any `State` INSIDE the group that must
re-enter (see D-3's trap) and run with `--test-threads=1`, since the suite shares global state.