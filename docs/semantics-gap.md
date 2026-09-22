# The semantics gap (accessibility)

> Status: **known gap, deliberately not started.** Written down so the next person does not have to
> rediscover the shape of it, and so the components that want semantics have somewhere to point.
> Companion: `docs/segmented-button-plan.md` (difference #1 there is this document).

## What is missing

winia has no accessibility tree. Nothing a component composes is exposed to Windows UI Automation, Linux
AT-SPI, or anything else, and nothing in the crate models what Compose calls *semantics*: the roles,
names, states and actions a screen reader (or a test) reads off the UI.

Concretely, the components whose Compose counterparts carry semantics have no way to say so here:

| Component | Compose's semantics | what winia has |
|---|---|---|
| `SegmentedButton` | `selectableGroup()`, `Role.RadioButton` / `Role.Checkbox`, `selectable` / `toggleable` state | nothing |
| `Slider` / `RangeSlider` | `progressBarRangeInfo`, `setProgress` / `stepBy` actions, one node per thumb | nothing |
| `Switch`, `Checkbox`, `RadioButton` | `toggleable` / `selectable` state + role | nothing |
| `Text`, `Icon` | name (the text), `contentDescription` for an icon | nothing |
| `TextField` | `editableText`, `setText`, IME action | nothing (IME works, but only through winia's own focus path) |

## Why it is two problems, not one

1. **The model.** A way to declare semantics in composition — in Compose, `Modifier.semantics { ... }`
   plus a merged/unmerged tree, `MergeDescendants` for a container that reads as one node, actions with
   handlers, and collections. This is a data structure plus a traversal, and winia has no `Modifier`
   element for it.
2. **The bridge.** Something that publishes that model to the platform: UI Automation on Windows
   (`IRawElementProviderSimple` and friends), AT-SPI on Linux, macOS AX. That is a per-platform project
   in its own right — winit offers nothing here — and it is where most of the work and all of the
   platform risk lives.

They can be built in either order, and the choice matters:

- **Bridge first** means publishing an ad-hoc tree (say, from the layout arena plus hand-written rules
  per component) and then discovering that the model has to change to fit what the bridge needs — which
  is how every "just expose the tree" attempt ends up rewritten.
- **Model first** means designing `MergeDescendants`, actions and collections with **no consumer** to
  check them against. Compose's own API is the only reference, and it evolved for years against real
  screen readers; copying its surface without the feedback loop risks a model that is both large and
  wrong.

The recommendation here is therefore neither: **do it when a consumer exists** — a first bridge for one
platform, however crude, on top of a deliberately small model (role, name, state, one action list), and
grow both together. The components that already want it are listed above; the smallest useful slice is
probably `Text`/`Icon` names plus `SegmentedButton`'s role and selected state, because those are what a
keyboard-only user notices first.

## What is NOT a semantics substitute

- **Focus** is already real in winia (`Modifier::focusable`, Tab traversal, per-node key events) and is
  the part of accessibility that needs no tree: keyboard operation works today. The gap is the *reported*
  state, not the interaction.
- **Hit testing** is unrelated; a semantics node is not a hit target.

## If someone picks this up

1. Decide the platform(s). One bridge, one platform, end to end, beats a model with no consumer.
2. Keep the model minimal on the first pass: `role`, `name`, `state` (selected / checked / expanded /
   disabled), `value` for ranges, and `actions` with handlers. `MergeDescendants` only if a real case
   needs it (a segmented button reading as one node is one).
3. Wire two or three components, not twenty: `SegmentedButton`, `RangeSlider`, `Text`.
4. Test it through the bridge itself on the chosen platform (a UIA query in an integration test), not by
   asserting the internal tree — the tree is the easy half and testing it proves nothing about the part
   that ships.
