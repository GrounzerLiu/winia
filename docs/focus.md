# Focus: groups, properties and the change callback

winia's keyboard focus is a flag on `LayoutNode` plus two search paths:

- **Tab / Shift+Tab** walk the focusable nodes in depth-first order (`collect_focusable_ids`);
- **arrow keys** pick the nearest focusable whose centre lies in that direction
  (`collect_focus_candidates`).

This note covers the Compose modifiers layered on top: `focusGroup`, `focusProperties` and
`onFocusChanged`.

## `Modifier::focus_group()`

Marks a subtree as one traversal group. Tab still visits the group's focusable children in order,
but the group REMEMBERS the child that had focus when focus left it. When Tab (or Shift+Tab) enters
the group at its boundary — the first focusable child going forward, the last going backwards — the
remembered child is restored instead. Memory is updated by every focus change inside the group,
including a click (a programmatic focus request goes through the same path).

What it does not do: Compose's `focusProperties { next/previous = … }` overrides for custom
traversal graphs, and focus-group hints for platform bridges. Neither is ported.

## `Modifier::focus_properties_can_focus(false)`

Keeps an otherwise focusable node out of every focus path: Tab, arrow keys and programmatic
`set_focus_by_id` requests (`has_focusable_modifier` and `set_focus_by_id` both check it). The
rest of Compose's `focusProperties` (next/previous/up/down/left/right as `FocusRequester` lambdas,
`canFocus` as a state lambda) is not ported.

## `Modifier::on_focus_changed { focused -> … }`

The callback receives `true` when the node gains focus and `false` when it loses it. Delivery
happens once per frame, after the focus sync of the main composer and every overlay layer
(`Composer`-level `dispatch_focus_changed`), and is CHANGE-gated: the initial unfocused state fires
nothing. A node removed from the tree takes its callbacks with it — the loss is delivered only while
the node is still composed.

## `clearAndSetSemantics` and custom actions

See [semantics.md](semantics.md): `clear_and_set_semantics` replaces the node's declaration and hides
its subtree (an empty block hides the node itself), and `SemanticsConfig::custom_action(label, f)`
adds named actions that appear in the semantics tree and debug JSON. The Windows bridge still
exposes only the primary Invoke; UIA has no generic custom-action pattern.
