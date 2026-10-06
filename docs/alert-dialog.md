# AlertDialog

M3 alert dialogs: `winia/src/components/alert_dialog.rs`. Demo:
`cargo run -p winia --example alert_dialog_demo`.

## API

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

`BasicAlertDialog` is the same dialog with arbitrary content instead of the slots (Compose's
`BasicAlertDialog`); `AlertDialog` delegates to it, as `AlertDialogImpl` does.

**The action row is a TEXT BUTTON, never a filled one.** The specs page's basic-dialog anatomy calls the
element "Button label text" and the full-screen variant's "Text button"; its colour role is Primary.
`AlertDialogImpl` says the same in code — it provides `DialogTokens.ActionLabelTextColor` to the row and
notes that a TextButton "will not consume this provided content color value, and will use their own
defined or default colors" (`AlertDialog.kt:283-288`). So both the confirm and the dismiss slot want
`Button::new().style(ButtonStyle::Text)`; the dialog supplies the layout and its two default colours, not
the button style.

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
Measured on this demo with probe prints in the confirm slot: the click FIRED and the overlay count stayed
at 1. The date-picker fixtures and demo compose their dialog unconditionally for the same reason.

Slots are `Fn`, not `FnOnce`: an overlay composes its content on every frame the dialog is up
(the same bound `ModalBottomSheet`'s content uses), so a callback that moves out of a slot needs
a fresh clone per call — `clone!` inside the slot body.

## Tokens (`DialogTokens`, `AlertDialogDefaults`)

| token | value | used for |
|---|---|---|
| `ContainerShape` (`CornerExtraLarge`) | 28dp | container corners (the same radius the bottom sheet's expanded shape uses) |
| `ContainerColor` | `SurfaceContainerHigh` | container |
| `TonalElevation` | 0dp | flat by default — see the deviations and "Tonal overlay" below |
| `IconColor` / `IconSize` | `Secondary` / 24dp | the icon slot |
| `HeadlineColor` / `HeadlineFont` | `OnSurface` / `HeadlineSmall` | the title |
| `SupportingTextColor` / `SupportingTextFont` | `OnSurfaceVariant` / `BodyMedium` | the text |
| `ActionLabelTextColor` / `ActionLabelTextFont` | `Primary` / `LabelLarge` | provided to the buttons, which set their own colours (as in Compose) |
| `dialogPadding` | 24dp (20dp for a precision pointer) | the content column's padding, all sides — winia hard-codes the touch value |
| icon / title padding | 16dp each | below the icon, below the title |
| `textPadding` | 24dp | below the text |
| `DialogMinWidth` / `DialogMaxWidth` | 280dp / 560dp | the width clamp |
| button spacing | 8dp | both axes of the action row |

## Tonal overlay (on `Surface`, not on the dialog)

**Tonal overlay** is how Material 3 gives a raised surface its colour shift: instead of (or alongside) a
shadow, a `Surface` with a `tonalElevation` has `surfaceTint` blended over its own colour, more the higher the
elevation. M3 dialogs look "raised" this way.

`Surface::tonal_elevation` used to be a placeholder that did nothing. It now works, with Compose's rule
(`ColorScheme.kt:1540-1543`, `:1125-1129`):

- The tint applies **only when the surface's colour is exactly `theme.surface`** and tonal elevation is on.
  A surface with any other colour is left untouched — that is Compose's gate, verbatim.
- The blend is Compose's formula, character for character: `alpha = ((4.5 · ln(elev + 1)) + 2) / 100`, then
  `surfaceTint(alpha)` over `surface`. winia composites it with the existing `Color::overlay` (the same
  alpha lerp the state layers use).
- `WiniaTheme::with_tonal_elevation_enabled(false, …)` suppresses it for a subtree — winia's
  `LocalTonalElevationEnabled` (`ColorScheme.kt:1556`).

Over the published `ElevationTokens` levels that is `1dp → 5.1%`, `3dp → 8.2%`, `6dp → 10.8%`.

**It accumulates.** The elevation that decides the alpha is the ABSOLUTE one: each `Surface` adds its own
`tonal_elevation` to whatever its ancestors provided and tints from the sum, then provides that sum downward
(`Surface.kt:106,109` and the same pair in the other three overloads at `:211`、`:317`、`:424`; the reason is
at `Surface.kt:146-150` — *a Surface never appears to have a lower elevation overlay than its ancestors*).
So a 3 dp surface inside a 3 dp one reads 6 dp, not 3. winia previously tinted from the local number alone,
which made every surface in a stack read as flat as its parent.

**This does not change any dialog.** Every M3 dialog's container is `surfaceContainerHigh`/`surfaceVariant`,
never `surface`, so the gate excludes them all — which is also why Compose's own `AlertDialog` gets nothing
from its `tonalElevation` parameter, whose default is `0.dp` anyway (`AlertDialogDefaults.TonalElevation`,
`AlertDialog.kt:241`). Note that gate holds only for the DEFAULT container colour: `containerColor` is a
parameter, and a caller who passes `ColorScheme.surface` would get the tint. That is why `AlertDialog` does
not expose `tonalElevation` here — see "Deviations from Compose".

`Card` is not one of the tonal surfaces: Compose's `Card` passes no `tonalElevation` at all — the word does
not occur anywhere in `material3/Card.kt`, and only `shadowElevation` reaches its `Surface`
(`Card.kt:88-94`, `:149-157`). Its container depends on the variant: the default filled card is
`surfaceContainerHighest` (`FilledCardTokens.kt:24`), `ElevatedCard` is `surfaceContainerLow`, and
`OutlinedCard` is plain `surface` — so the last one *satisfies* the colour half of the gate and still shows
no tint, because it never passes `tonalElevation`: the absolute elevation stays `0.dp`, and
`surfaceColorAtElevation` (`ColorScheme.kt:1126`, `if (elevation == 0.dp) return surface`) is a no-op.
Its per-state numbers (`OutlinedCardTokens.kt:30,34`) are `shadowElevation` only, which does not feed
`applyTonalElevation`. `Menu` does pass one (`Menu.kt:403`), but its default
is `ElevationTokens.Level0 = 0.dp`. So in practice no stock Compose component tints: the capability is on
`Surface` because that is where Compose puts it, not because a stock caller uses it.

Six tests in `components::surface` pin it: the tint and the formula (with the Level2 8.24% alpha), the non-`surface`
colours staying untouched, zero elevation and the switch each doing nothing, the content colour still
following the UNTINTED base colour, the tint reaching the rendered pixels, and the accumulation across two
and three nested surfaces.

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
`Modifier::min_max_steps`: each Compose modifier is its own node and constrains into what the node
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

## Structure and behaviour

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

`confirm_button` is a slot like the rest, so a dialog without one builds (Compose's two-action
overload requires it; its `content` overload is `BasicAlertDialog` here).

The surface publishes `role = Dialog`, which is winia's landing for Compose's
`Modifier.semantics { paneTitle = dialogPaneDescription }` on the dialog Box (`AlertDialog.kt:171`) and
what `accessibility.rs` maps to the Pane UIA control type. A caller's own modifier still wins — it is
applied after ours.

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

Built on the existing `Dialog` overlay, so modality, the scrim, outside-click dismissal and
Escape come from the overlay path (`app.rs`), and the enter/exit motion is
`OverlayAnimSpec::default_enter()/default_exit()` (scale 0.8 + fade, 200ms) — the same motion
the framework's `Dialog` uses. The overlay is registered only while `visible`; no rising-edge
state is needed on top of that, because the enter animation runs on registration and
`sync_overlays` composes the current frame's content closure (the piece `ModalBottomSheet` needs
an edge for is its own sheet slide, which a dialog has none of) — and see "Composing it" above for why
`visible` has to be an argument rather than a surrounding `if`.

Tests (12) cover: no overlay when hidden and exactly one modal, centred overlay when shown; the
`dismiss_on_outside`, `dismiss_on_back_press` and `focusable` flags reaching the overlay, each with
Compose's default; the `role = Dialog` the surface publishes; the 280 floor and the 560 cap BY
DEFAULT — and, with `platform_default_width(false)`, a body wider than the cap escaping it while the
window still bounds it; a text slot taller than the window leaving the action row its own height; the
slot order with its 16/16/24 paddings; the title centring with an icon and start-alignment without
one; the button row's end alignment with the confirm action last; the WRAPPED row putting the confirm
above the dismiss; and the over-sized-slot overflow above.

The `fill = false` in the text slot's weight is pinned by `slots_stack_in_order_with_their_paddings`,
NOT by the tall-text test: with `fill = true` the box is stretched to its share whether it needs it
or not, which that test catches through its fixed 40-tall text (swapping the call for
`layout_weight(1.0)` measures 11 passed / 1 failed), while the tall-text test passes on both.

The escape gate itself is tested at the key path, not just on the flag:
`app::overlay_close_tests::escape_respects_dismiss_on_back_press` builds two overlays and asserts Escape is
consumed and closes the first, and is consumed but closes neither when `dismiss_on_back_press` is false.

## Deviations from Compose

- **No `tonalElevation` parameter.** Compose exposes one (`AlertDialog.kt:108`, default
  `AlertDialogDefaults.TonalElevation = 0.dp`, `:241`) and forwards it to its `Surface`. With the default
  container colour the gate (`ColorScheme.applyTonalElevation`, `ColorScheme.kt:1540-1543`) excludes it —
  a dialog's container is `surfaceContainerHigh`, never `surface` — so the default value is a no-op in
  Compose too, and so would be anything else winia accepted here while the container colour is the default.
  It is not a no-op for a caller who also passes `container_color` equal to `theme.surface`, which is why
  this is listed rather than dismissed. (`DialogTokens.ContainerElevation` = `Level3` is referenced nowhere
  in the alert-dialog implementations — what it belongs to is not demonstrable, so nothing here claims it.)
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
  "Structure and behaviour" for which is which.
- The tests find the dialog's nodes through `Modifier::test_tag` and a real overlay layout (the
  registered overlay's content is composed in its own `Composer`, as the app does), so they
  check the geometry the user sees rather than the builder's fields.
