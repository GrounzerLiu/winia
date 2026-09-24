# Semantics (accessibility)

> Status: **the model and a debug consumer are in; the platform bridge is not.**
> Companion to `docs/semantics-gap.md`, which is the original analysis of why this was hard and what
> it would take — its "do it when a consumer exists" recommendation is what this slice follows.

## What exists

`winia::semantics` resolves what a node *is* into a tree, at query time, from a modifier element:

```rust
Modifier::new().semantics(
    SemanticsConfig::new()
        .role(SemanticsRole::Tab)
        .state(SemanticsState::new().selected(true)),
)
```

- `semantics_tree(nodes, root) -> Vec<SemanticsNode>` walks one composer's arena (main tree or an
  overlay's) and returns the elements.
- Each `SemanticsNode` carries `role`, `name`, `state`, `clickable`, `focused` and absolute `bounds`,
  so a consumer can address an element by id without hit-testing.
- `click_node_by_id(nodes, id)` fires the element's click action — the primitive a platform bridge's
  Invoke pattern needs.
- `semantics_json(...)` is the debug-channel form; the debug server answers `sem` with it (stdin and
  WebSocket, prefix `SEMANTICS:`), and `winia/tests/ui/mod.rs` exposes `UiTest::semantics()`.

### Two rules do most of the work

1. **A node that claims its subtree absorbs it.** A click target claims automatically (Compose's
   `clickable` does the same inside foundation), and a component that IS one control declares
   `merge_descendants(true)` — a *disabled* button has no clickable, so without the explicit claim its
   label would hang under it as a child instead of naming it. The absorbed subtree's role and state
   fill in only what the claiming node left open, so a clickable row wrapping a switch reports both
   the click and the switch's checked state.
2. **A node that declares nothing and claims nothing is transparent.** A `Column` never appears
   between a panel and its buttons.

Declarations along one modifier chain merge by position: a later field wins, an earlier one fills the
gaps. That is why components declare before appending the caller's modifier —
`modifier.semantics(role).then(user_modifier)` — so a caller adding a content description keeps the
role.

### What the components declare

| Component | role | state |
|---|---|---|
| `Button`, `IconButton`, `FloatingActionButton` (and Extended) | `Button` | `enabled` |
| `TriStateCheckbox` | `Checkbox` | `checked` (off / on / indeterminate) |
| `Switch` | `Switch` | `checked`, `enabled` |
| `RadioButton` | `RadioButton` | `selected`, `enabled` |
| `SegmentedButton` | `RadioButton` (single-choice) / `Checkbox` (toggleable) | `selected` / `checked` |
| `Tab` | `Tab` | `selected`, `enabled` |
| `Icon`, `Image` | `Image` | — |
| `Text`, `RichText` | *(none — a name and no role)* | — |

`Icon::content_description` / `Image::content_description` used to be inert parameters ("winia has no
semantics tree — reserved", as the old comment said); they now name the element, and an image or icon
**without** a description stays out of the tree entirely — decorative, not "announced as empty", which
is Compose's distinction too.

## Deliberate differences from Compose

| Compose | winia | why |
|---|---|---|
| A merged tree **and** an unmerged one | one merged tree | The merged tree is what a screen reader consumes; the unmerged one exists for tests that want the raw declarations. Adding it is mechanical once something needs it. |
| `SemanticsPropertyKey` set (hundreds, custom keys) | role, name, state, click | An API that grows to fit real consumers beats one copied whole from a surface with no consumer here. |
| `SemanticsActions` incl. `SetProgress`, `ScrollBy`, `SetText` | click only | Each action needs a platform pattern behind it (UIA invoke / range value); click is the one wired end to end. |
| Collections (`collectionInfo`, `indexForKey`), `liveRegion`, `heading`, custom actions | absent | Same reason. |
| `ContentDescription` and `Text` are separate properties | one `name` | One field until a consumer needs the distinction, which is what a screen reader's Name property resolves to anyway. |
| Semantics merge during composition into a `SemanticsConfiguration` | merged at read time from the modifier chain | winia's modifier is plain data read per query; nothing has to be kept in sync. |

Dead ends kept from the analysis: `docs/semantics-gap.md`.

## What is NOT here yet: the platform bridge

Nothing publishes this tree to Windows UI Automation, AT-SPI or macOS AX. That is the next slice, and
the doc-comment order in `semantics-gap.md` still holds: one platform, end to end, however crude.

What a Windows bridge needs, now that the model is settled:

1. `WM_GETOBJECT` (`OBJID_CLIENT` / `UiaRootObjectId`) — winit does not forward it, so the HWND needs
   `SetWindowSubclass` over the handle from `window.window_handle()`.
2. `IRawElementProviderSimple` + `IRawElementProviderFragment( Root)` over the snapshot, mapping
   role → `ControlType`, name → `Name`, `clickable` → `InvokePattern`, `checked`/`selected` →
   `ToggleState`/`SelectionItem`, `enabled` → `IsEnabled`, bounds → `BoundingRectangle` (logical →
   screen coordinates).
3. The snapshot is already published per frame, so the provider can read it without touching the
   composer — that part is done.

Known gaps in the channel worth knowing before writing that bridge:

- The snapshot is built **once per frame** while the `debug-server` feature is on (the tree JSON
  already worked this way). Under the feature that is a traversal plus a string per frame per window;
  without it, everything compiles away. If the bridge needs it in a normal build, it should ask for a
  snapshot lazily (the `screenshot_requested` one-shot pattern) instead of publishing every frame.
- Overlay contents are in the snapshot (`{"overlays":[{"id":N,"tree":[…]}]}`), but a dialog gets no
  `Dialog` role marker of its own yet — the role is declared and mapped, nothing declares it on the
  container.
- Status text is absent: `snackbar` and `loading_indicator` announce nothing, and there is no
  `liveRegion` equivalent to announce them with.
