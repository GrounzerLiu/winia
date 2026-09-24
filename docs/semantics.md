# Semantics (accessibility)

> Status: **the model, a debug consumer, and the Windows UI Automation bridge are in.**
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
| `LinearProgressIndicator`, `CircularProgressIndicator` (and the wavy pair) | `ProgressBar` | `progress(current, min, max)` when determinate |
| `LoadingIndicator` | `ProgressBar` | — (a spinner has no value) |
| `Text`, `RichText` | *(none — a name and no role)* | — |

A progress bar reports its **value**, not just its role: `UIA_RangeValue` with `value`/`min`/`max`
and `IsReadOnly` true (progress is reported, not set). Only a determinate bar has one — an
indeterminate bar announces itself as a progress bar and no percentage, because "in progress" and
"0 percent" are different things a screen reader must not conflate.

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

## The Windows bridge (feature `accessibility`)

```bash
cargo build --example alert_dialog_example --features accessibility
# then, from any UIA client — Narrator, Inspect.exe, or the probe in tools/:
powershell -File tools/verify_uia.ps1 -ProcessName alert_dialog_demo -Invoke "Settings"
```

Off by default, and Windows-only: it needs the `windows` crate, and a plain build should not carry a
COM server it does not use. Off Windows (or without the feature) `crate::accessibility` is a no-op
module with the same two functions, so call sites in `app.rs` stay unconditional.

How it reaches a client:

1. `SetWindowSubclass` hooks the window procedure, because **winit does not forward `WM_GETOBJECT`**
   — and that message is the whole entry point.
2. The hook answers exactly one spelling of the request, `lParam == UiaRootObjectId`. Measured: also
   answering `OBJID_CLIENT` (the older MSAA convention) makes UIA accept the provider but never walk
   past the first element.
3. `UiaReturnRawElementProvider` hands UIA the provider; everything after that is COM calls into
   `IRawElementProviderSimple` / `_Fragment` / `_FragmentRoot`, served from the published snapshot.
4. Actions go back the other way: `IInvokeProvider::Invoke` and `IFragment::SetFocus` **queue** a
   `UiAction` instead of running application code, and the frame loop performs it
   (`consume_ui_actions` in `app.rs`), running the same callback a real click or Tab would. A provider
   is called at a moment the app does not control — it must not run a callback, and it must not block.

Things that are load-bearing, each of which was measured rather than assumed:

- **Providers are cached per `(window, path)`.** UIA compares the providers it gets back: an element it
  has already seen must come back as the same instance, or the walk collapses to a single child.
- **Only the root returns a host provider** (`HostRawElementProvider`). Answering with the window's
  host for every element made each child advertise the window's `WindowPattern` and `TransformPattern`.
- **`GetRuntimeId` is derived from the element's own `node_id`**, so an element keeps its identity
  across frames. (UIA does not appear to call it for these fragments — it derives identity from the
  host — but a client that does gets a stable answer.)
- **Unsupported properties answer with `UiaGetReservedNotSupportedValue`**, not an error: the protocol
  distinguishes "no value" from "not supported", and an error fails the whole property read.
- **A pattern an element does not have answers S_OK with a NULL interface** (`no_pattern`), which is
  what "not implemented" means for `GetPatternProvider` — a client asking the wrong pattern is routine.

Role mapping:

| semantics | UIA control type | pattern |
|---|---|---|
| Button | Button | Invoke |
| Checkbox, Switch | CheckBox | Toggle |
| RadioButton, single-choice SegmentedButton | RadioButton | SelectionItem |
| Tab | TabItem | SelectionItem |
| Image | Image | — |
| ProgressBar | ProgressBar | RangeValue (read-only) when a value is reported |
| Dialog | Pane | — |
| *(no role, but named)* | Text | — |
| the window itself | Window | — |

UIA has no switch control type, so a switch lands on CheckBox — the nearest honest neighbour, since
what it reports (a toggleable state) is what the control type means.

**An element can offer several patterns at once**, and what it offers follows from what it can do: a
click target offers Invoke, a checked control offers Toggle, a selected one offers SelectionItem. A
checkbox is therefore Invoke *and* Toggle — a client may click it or read and change its state, and
`IsInvokePatternAvailable` is true alongside a working Toggle. (An earlier version returned a single
pattern per element, which left an enabled checkbox offering only Invoke while its Toggle state sat
unreachable.)

Two gaps the bridge surfaces in the examples themselves:

- An icon-only control has no text to be named by, so it needs `content_description`. The shared
  example chrome's settings button was announced as the window's own title until it was given one.
- A control whose label is a SIBLING rather than a child reports an empty name: `Column { Text("Row 1");
  Checkbox(..) }` composes a checkbox with nothing inside it, so the element has no name to announce.
  Naming it is `Modifier::semantics(SemanticsConfig::new().content_description("Row 1"))` on the
  control (Compose has the same requirement — a label next to a control is not associated with it
  automatically). The checkboxes in `checkbox_demo` read as `''` for exactly this reason.
- Nothing announces status changes: there is no `liveRegion` equivalent, so a snackbar is silent —
  it appears and disappears without a word. A `LoadingIndicator` at least reads as a progress bar
  now; what it cannot do is ANNOUNCE that it appeared. That is the next thing a screen-reader user
  would notice.

Known limits of this slice:

- One-way events: there is no `UiaRaiseStructureChangedEvent` / `AutomationFocusChanged`, so a client
  that caches the tree learns about changes by re-reading. The snapshot is rebuilt every frame, so a
  re-read is always current.
- Toggle and SelectionItem fire, but they do it the way Invoke does — by queueing a click
  (`UiAction::Invoke`), because a click is winia's one activation path. The element's own handler
  decides the new value, which is why the provider does not compute it. `AddToSelection` /
  `RemoveFromSelection` are refused with `UIA_E_NOTSUPPORTED`: winia's selectable controls are
  single-choice, so "add to the selection" has no meaning and saying so beats faking it.
- A refusal must be an ERROR, not S_OK. Measured: answering `AddToSelection` with the same empty
  `Error` that means "no pattern" made the client see success — the empty `Error` maps to S_OK, which
  is what `GetPatternProvider` needs for "this element does not have that pattern", but a lie for a
  method the provider declines to perform.
- The window's own rectangle and the native frame come from the host provider; the semantics tree
  describes the client area.
