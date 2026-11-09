# AlertDialog

M3 alert dialogs: `winia/src/components/alert_dialog.rs`.

## Sources

Compose paths are relative to
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/` unless stated
otherwise.

| Compose | Settles |
| --- | --- |
| `AlertDialog.kt` | the two public overloads (`AlertDialog`, `BasicAlertDialog`), `AlertDialogContent`'s slot order and paddings, `AlertDialogDefaults`, `AlertDialogFlowRow`'s direction flip, `DialogMinWidth`/`DialogMaxWidth` |
| `androidMain/androidx/compose/material3/AndroidAlertDialog.android.kt` (under `reference/compose/material3-1.5.0-beta01/`) | the `expect`/`actual` pair behind `AlertDialog`; nothing platform-specific is mirrored here |
| `tokens/DialogTokens.kt` | `ContainerShape` (`CornerExtraLarge`), `ContainerColor` (`SurfaceContainerHigh`), `IconColor` (`Secondary`), `HeadlineColor` (`OnSurface`), `SupportingTextColor` (`OnSurfaceVariant`), `ActionLabelTextColor` (`Primary`), and the three `TypographyKeyTokens` (`HeadlineSmall` / `BodyMedium` / `LabelLarge`) |
| `InteractiveComponentSize.kt` | `LocalMinimumInteractiveComponentSize`, which Compose subtracts from `ButtonsCrossAxisSpacing`; see the button-spacing bullet under Differences |
| `ColorScheme.kt` | the tonal-overlay gate and formula (`applyTonalElevation`, `:1540-1543`, `:1125-1129`), which is why no `tonalElevation` parameter exists here |

`DialogProperties` itself lives in `androidx.compose.ui.window` and is **not** in the reference tree; its
fields are listed from the call sites in `AlertDialog.kt` (the table under Behaviour).

## Variants

| Constructor | Content | When |
| --- | --- | --- |
| `AlertDialog::new(visible)` | fixed slots: icon, title, text, confirm/dismiss buttons | the M3 alert dialog; Compose's two-action `AlertDialog` |
| `BasicAlertDialog::new(visible)` | an arbitrary closure, plus shape / container colour / content padding | a dialog that brings its own surface; Compose's `BasicAlertDialog` and its `content` overload. The date picker's dialog uses it |

`AlertDialog` delegates to `BasicAlertDialog`, as Compose's `AlertDialogImpl` delegates to its content
composable. There are no height variants — Compose ships none for dialogs.

```rust
AlertDialog::new(visible)
    .on_dismiss_request({ clone!(open); move || open.set(Open::None) })
    .icon(|ctx| { /* 24dp, centred */ })
    .title(|ctx| { Text::new("Discard draft?").build(ctx); })
    .text(|ctx| { Text::new("Your draft will be deleted.").build(ctx); })
    .confirm_button(|ctx| { Button::new().on_click(/* … */).build(ctx, /* … */); })
    .dismiss_button(|ctx| { Button::new().style(ButtonStyle::Text).build(ctx, /* … */); })
    .build(ctx);
```

**The action row is a TEXT BUTTON, never a filled one.** The specs page's basic-dialog anatomy calls the
element "Button label text" and the full-screen variant's "Text button"; its colour role is Primary.
`AlertDialogImpl` says the same in code — it provides `DialogTokens.ActionLabelTextColor` to the row and
notes that a TextButton "will not consume this provided content color value, and will use their own
defined or default colors" (`AlertDialog.kt:283-288`). So both the confirm and the dismiss slot want
`Button::new().style(ButtonStyle::Text)`; the dialog supplies the layout and its two default colours, not
the button style. That colour is fixed rather than a parameter — see Differences.

## Layout

The content column stacks: icon (centred), title (start-aligned — centred when an icon is
present), text (start-aligned), then the action row (end-aligned). Each slot carries its own
bottom padding, which is what the unit tests assert against the tokens rather than against
screenshots.

**Width** is the content's own width clamped into `280..560dp` — Compose's `sizeIn`. That needs
both halves of `widthIn`: `min_width` alone would let a long body grow the dialog to the window.
`Modifier::max_width` was added for it (`winia/src/modifier.rs`, `layout/node.rs`): the cap
lowers the incoming max and is then held at or above the min.

`platform_default_width(false)` is Compose's `DialogProperties.usePlatformDefaultWidth = false`:
the range is not applied, so the content sizes itself. That is how a caller asks for a dialog wider
than `DIALOG_MAX_WIDTH`, or one that fills the window. On both `AlertDialog` and `BasicAlertDialog`,
default `true`.

**The text slot takes the slack.** Compose wraps it in `Box(Modifier.weight(1f, fill = false))`
(`AlertDialog.kt:350`) and so does winia — `layout_weight_fill(1.0, false)`. When a height is
imposed, the text box is clamped to the leftover space so the action row keeps its own height.
Measured in the tests' 800x600 window with a 2000px text: without the weight the dialog is
`280x600` with the action row at `y=576, h=0`; with it, `y=536, h=40`.

When a min and a max conflict the winner is Compose's chain-order rule, replayed by
`Modifier::size_steps`: each Compose modifier is its own node and constrains into what the node
before it produced, so `.max_width(200).min_width(300)` measures 200 while the reverse measures
300. This used to be a recorded deviation — "the min always wins", which is CSS's precedence and
matches only the second order — and the clamp is what keeps `Constraints` consistent either way
(`Constraints::constrain_*` are `f32::clamp`, which panics on min > max). Verified on a 900-wide
window: the dialog stays 560.

**The action row** is a `FlowRow` whose layout direction is FLIPPED while the buttons keep the
original one (`AlertDialogFlowRow`). With the content in the order confirm-then-dismiss, that
puts the confirm action after the dismiss one across a row and above it once the row wraps —
both of which are the M3 arrangement. Reproduced with `WiniaTheme::with_theme_and_direction`
rather than a bespoke layout: with two buttons in LTR the row reads `[Cancel] [Discard]`,
end-aligned against the 24dp padding, which is what the real-window dump shows.

**RTL.** Every alignment above is a start/end or a centre, never a left/right: the icon centres, the
title starts (or centres), the text starts, and the row ends. The buttons' own direction is restored
inside the flipped flow, so their content mirrors with the window rather than with the flip.

## Behaviour

### Tokens (`DialogTokens`, `AlertDialogDefaults`)

| token | value | used for |
|---|---|---|
| `ContainerShape` (`CornerExtraLarge`) | 28dp | container corners (the same radius the bottom sheet's expanded shape uses) |
| `ContainerColor` | `SurfaceContainerHigh` | container |
| `TonalElevation` | 0dp | flat by default — see "Tonal overlay" below |
| `IconColor` / `IconSize` | `Secondary` / 24dp | the icon slot |
| `HeadlineColor` / `HeadlineFont` | `OnSurface` / `HeadlineSmall` | the title |
| `SupportingTextColor` / `SupportingTextFont` | `OnSurfaceVariant` / `BodyMedium` | the text |
| `ActionLabelTextColor` / `ActionLabelTextFont` | `Primary` / `LabelLarge` | provided to the buttons, which set their own colours (as in Compose) |
| `dialogPadding` | 24dp (20dp for a precision pointer) | the content column's padding, all sides — winia hard-codes the touch value |
| icon / title padding | 16dp each | below the icon, below the title |
| `textPadding` | 24dp | below the text |
| `DialogMinWidth` / `DialogMaxWidth` | 280dp / 560dp | the width clamp |
| button spacing | 8dp | both axes of the action row |

### Composing it: pass `visible`, do not wrap it in `if`

An overlay is released by its owner recording `active = false`, and an owner that simply **stops composing
it** counts as "skipped" and is KEPT — `composer.rs::record_overlay_active` and `app.rs::sync_overlays` both
say so, and the map is cleared every frame (`composer.rs:2817`) so a frame with no record reads as "keep, as
last frame". So the shape is:

```rust
AlertDialog::new(is_open)   // composed EVERY frame; `visible` carries open/closed
```

and **not**

```rust
if is_open { AlertDialog::new(true).build(ctx) }   // pins the overlay on screen
```

The failure is quiet and asymmetric, which is what makes it worth writing down: with the `if`, Escape still
closes the dialog — that path calls `begin_overlay_close` directly and never consults the registrar — while
every button inside it runs its handler, flips the caller's state, and leaves the dialog sitting there.
Measured on the demo with probe prints in the confirm slot: the click FIRED and the overlay count stayed
at 1. The date-picker fixtures compose their dialog unconditionally for the same reason.

Slots are `Fn`, not `FnOnce`: an overlay composes its content on every frame the dialog is up
(the same bound `ModalBottomSheet`'s content uses), so a callback that moves out of a slot needs
a fresh clone per call — `clone!` inside the slot body.

### Dialog properties

`DialogProperties` is exposed as its three cross-platform parts, each flat on the builder (Compose groups
them, but the group holds nothing winia acts on differently):

| Compose field | winia | default |
|---|---|---|
| `dismissOnClickOutside` | `dismiss_on_outside` | true |
| `dismissOnBackPress` | `dismiss_on_back_press` | true |
| `isFocusable` | `focusable` (drives the overlay's `focus_scope`) | true |
| `usePlatformDefaultWidth`, `decorFitsSystemWindows` | `platform_default_width` | `decorFitsSystemWindows` is an Android-window concept with no desktop meaning and is deliberately not stubbed |

`dismiss_on_back_press(false)` still SWALLOWS Escape rather than letting it through, so the page behind the
scrim never reacts to a key this dialog kept. **That swallow is winia's own, not a copy of Compose's** —
Compose routes Escape through the same flag (`BasicEdgeToEdgeDialog.android.kt:227-233`), but because its
dialog lives in a separate window it can simply fall through to `super.onKeyUp` (`:235`) when the flag is
false and still have no page behind to reach. winia draws the dialog as an overlay over the live page, so
letting the key through would clear the focus of the page under the scrim. (`AlertDialog` does not even go
through `BasicEdgeToEdgeDialog`; it reaches `androidx.compose.ui.window.Dialog` via
`DefaultBasicAlertDialogOverride`, `AlertDialog.kt:165-172`.)

One ordering detail worth knowing: the handler check runs first, so a dialog with
`dismiss_on_back_press(false)` **and no** `on_dismiss_request` does not swallow — with nothing to ask, the
key is left to the page.
Pinned by `app::overlay_close_tests::escape_respects_dismiss_on_back_press`, which drives `escape_key`
itself rather than only reading the flag.

### Surface, slots and semantics

The caller's `modifier` is a WRAPPER around the surface — as in Compose's
`Box(modifier.sizeIn(...))` — so a caller's padding sits outside the dialog's background instead
of eating into it alongside the 24dp content padding.

A slot is laid out inside the content box with its own bottom padding and cross-axis alignment.
A slot that demands more than the box (`.size(w, h)` larger than the dialog) is COERCED into it:
a `Size` is clamped into the incoming range, which is Compose's `size()` (`enforceIncoming = true`
— a request only overrides the range through `required_size`, Compose's `requiredSize`), so such a
slot comes out at the width it is offered and still starts at the padding. winia used to write the
request over the constraints and let the surface's `clip` hide the spill; the alignment with Compose
removed that, and `required_size` is what a caller reaches for if the spill is wanted.

`confirm_button` is a slot like the rest, so the builder still accepts a dialog without one — but a
VISIBLE dialog with neither action trips a `debug_assert!`: Compose's two-action overload requires
`confirmButton` positionally, and a dialog the user can only dismiss has no way to say what it was
for. Hidden dialogs are exempt, since keeping one built-but-not-shown is normal.

The surface publishes `role = Dialog`, which is winia's landing for Compose's
`Modifier.semantics { paneTitle = dialogPaneDescription }` on the dialog Box (`AlertDialog.kt:171`) and
what `accessibility.rs` maps to the Pane UIA control type. A caller's own modifier still wins — it is
applied after ours.

### Overlay lifetime and motion

Built on the existing `Dialog` overlay, so modality, the scrim, outside-click dismissal and
Escape come from the overlay path (`app.rs`), and the enter/exit motion is
`OverlayAnimSpec::default_enter()/default_exit()` (scale 0.8 + fade, 200ms) — the same motion
the framework's `Dialog` uses. The overlay is registered only while `visible`; no rising-edge
state is needed on top of that, because the enter animation runs on registration and
`sync_overlays` composes the current frame's content closure (the piece `ModalBottomSheet` needs
an edge for is its own sheet slide, which a dialog has none of) — and see "Composing it" above for why
`visible` has to be an argument rather than a surrounding `if`.

The overlay composer is long-lived — `sync_overlays` calls `mark_content_dirty()` and composes the
same closure again — so the slot tree survives across frames and a changed slot set shifts the keys
of everything after it. `alert_dialog_content` therefore carries `#[composable]`
(`docs/component-authoring.md` section 1.2), which keeps that shift inside the slot builder instead
of letting it move the caller's siblings.

## Differences from Compose

- **No `tonalElevation` parameter.** Compose exposes one (`AlertDialog.kt:108`, default
  `AlertDialogDefaults.TonalElevation = 0.dp`, `:241`) and forwards it to its `Surface`. With the default
  container colour the gate (`ColorScheme.applyTonalElevation`, `ColorScheme.kt:1540-1543`) excludes it —
  a dialog's container is `surfaceContainerHigh`, never `surface` — so the default value is a no-op in
  Compose too, and so would be anything else winia accepted here while the container colour is the default.
  It is not a no-op for a caller who also passes `container_color` equal to `theme.surface`, which is why
  this is listed rather than dismissed. (`DialogTokens.ContainerElevation` = `Level3` is referenced nowhere
  in the alert-dialog implementations — what it belongs to is not demonstrable, so nothing here claims it.)
  The tint itself is implemented on `Surface` (`winia/src/components/surface.rs`), not here: its module
  comment has the gate and the blend formula, and the accumulation rule (a nested surface tints from the
  absolute sum, not its own number) lives in the crate-internal absolute-elevation local under
  `winia/src/theme/provider.rs`.
- **No `buttonContentColor` parameter.** Compose offers none either: `AlertDialogImpl` hard-codes
  `DialogTokens.ActionLabelTextColor` for the row (`AlertDialog.kt:285`). winia previously exposed a
  `button_content_color` builder method; it is removed so the two agree, and
  `AlertDialogDefaults::button_color` (which the date picker dialog also uses) is what supplies the value.
- **No `precision pointer` sizing.** Compose reads `shouldUsePrecisionPointerComponentSizing` and shrinks
  the geometry for a precise pointer: dialog padding 24 → 20dp, text padding 24 → 16dp, title
  `headlineSmall` at 20sp/26sp (`AlertDialog.kt:229-233`, `:318-326`). That flag is an Android
  input-modality signal and winia has no equivalent notion of "the user is on a mouse right now", so the
  touch-sized values are used unconditionally.
- **No minimum height.** Compose's KDoc for `BasicAlertDialog` says the dialog gets "the minimum height
  and width that the Material Design spec defines" (`AlertDialog.kt:122`, `:168`), but the code applies
  only `sizeIn(minWidth, maxWidth)` (`:147`) — there is no height constraint anywhere in the file. winia
  follows the code rather than the prose: a dialog with one short line is shorter than 280dp tall.
- **Tab goes nowhere when `focusable(false)`.** Compose's non-focusable dialog is a window that never took
  focus, so Tab belongs to whatever is behind it. winia draws the dialog over the live page, but the key
  path consumes Tab unconditionally (`app.rs:1360-1362`) and `keyboard_scope` finds no arena to move
  within when no focus-scope overlay is up (`app.rs:3346`), so focus does not reach the page behind.
  Every other key does fall through (`focus_scope_is_open` is false), so this is Tab only. Not fixed: the
  unconditional consume is load-bearing for every other overlay.
- **Text slot weight — ALIGNED (was a deviation).** Compose gives the text slot
  `weight(1f, fill = false)` so it absorbs the slack when a height is imposed; winia now does the
  same, and two tests cover the two halves: a 2000px text in a 600px window leaves the action row at
  its own 40px instead of crushing it to 0, and a 40px text is not stretched to its share.
- **No `DialogProperties` object** — the cross-platform fields are flat on the builder
  (`dismiss_on_outside`, `dismiss_on_back_press`, `focusable`, `platform_default_width`) rather than
  grouped. `decorFitsSystemWindows` and the rest of the Android-window fields have no desktop
  meaning; `usePlatformDefaultWidth` does, and is `platform_default_width`. See the table under
  Behaviour for which is which.
- **Button spacing ignores `MinimumInteractiveComponentSize`.** Compose computes
  `ButtonsCrossAxisSpacing - (LocalMinimumInteractiveComponentSize - ButtonDefaults.MinHeight)`,
  coerced to `0..8dp` (`AlertDialog.kt:264-269`), so a platform that inflates the touch target shrinks
  the vertical gap to compensate. winia has no such local, so both axes are a flat 8dp.

## Tests

```bash
cargo test -p winia --lib components::alert_dialog
```

15 unit tests, all over the arena (`Modifier::test_tag` + a real overlay layout — the registered
overlay's content is composed in its own `Composer`, as the app does), so they check the geometry the
user sees rather than the builder's fields:

- no overlay when hidden; exactly one modal, centred overlay when shown
- `dismiss_on_outside`, `dismiss_on_back_press` and `focusable` reaching the overlay, each with Compose's
  default
- the `role = Dialog` the surface publishes
- the 280 floor and the 560 cap by default, and with `platform_default_width(false)` a body wider than the
  cap escaping it while the window still bounds it
- a text slot taller than the window leaving the action row its own height
- the slot order with its 16/16/24 paddings
- the title centring with an icon and start-alignment without one
- the button row's end alignment with the confirm action last
- the wrapped row putting the confirm above the dismiss
- an over-sized slot being coerced into the dialog
- a visible dialog with neither action tripping the debug assertion, and a hidden one not
- changing the slot set across frames on one long-lived composer keeping the tree valid

The `fill = false` in the text slot's weight is pinned by `slots_stack_in_order_with_their_paddings`,
NOT by the tall-text test: with `fill = true` the box is stretched to its share whether it needs it
or not, which that test catches through its fixed 40-tall text (swapping the call for
`layout_weight(1.0)` measures 11 passed / 1 failed), while the tall-text test passes on both.

The escape gate is tested at the key path, not just on the flag:
`app::overlay_close_tests::escape_respects_dismiss_on_back_press` builds two overlays and asserts Escape is
consumed and closes the first, and is consumed but closes neither when `dismiss_on_back_press` is false.

**No fixture scenario, and no demo.** `winia/tests/ui_fixtures/fixture_all.rs` has no alert-dialog
scenario, and `winia/examples/alert_dialog_demo.rs` does not exist — `docs/rendering-backends.md` and
`docs/semantics.md` still reference it, so those two lines are stale. The unit suite above is what
runs today.
