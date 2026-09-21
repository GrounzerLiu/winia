# AlertDialog

M3 alert dialogs: `winia/src/ui/alert_dialog.rs`. Demo:
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

Slots are `Fn`, not `FnOnce`: an overlay composes its content on every frame the dialog is up
(the same bound `ModalBottomSheet`'s content uses), so a callback that moves out of a slot needs
a fresh clone per call — `clone!` inside the slot body.

## Tokens (`DialogTokens`, `AlertDialogDefaults`)

| token | value | used for |
|---|---|---|
| `ContainerShape` (`CornerExtraLarge`) | 28dp | container corners (the same radius the bottom sheet's expanded shape uses) |
| `ContainerColor` | `SurfaceContainerHigh` | container |
| `TonalElevation` | 0dp | flat by default — see the deviations |
| `IconColor` / `IconSize` | `Secondary` / 24dp | the icon slot |
| `HeadlineColor` / `HeadlineFont` | `OnSurface` / `HeadlineSmall` | the title |
| `SupportingTextColor` / `SupportingTextFont` | `OnSurfaceVariant` / `BodyMedium` | the text |
| `ActionLabelTextColor` / `ActionLabelTextFont` | `Primary` / `LabelLarge` | provided to the buttons, which set their own colours (as in Compose) |
| `dialogPadding` | 24dp (20dp for a precision pointer) | the content column's padding, all sides — winia hard-codes the touch value |
| icon / title padding | 16dp each | below the icon, below the title |
| `textPadding` | 24dp | below the text |
| `DialogMinWidth` / `DialogMaxWidth` | 280dp / 560dp | the width clamp |
| button spacing | 8dp | both axes of the action row |

## Layout

The content column stacks: icon (centred), title (start-aligned — centred when an icon is
present), text (start-aligned), then the action row (end-aligned). Each slot carries its own
bottom padding, which is what the unit tests assert against the tokens rather than against
screenshots.

**Width** is the content's own width clamped into `280..560dp` — Compose's `sizeIn`. That needs
both halves of `widthIn`: `min_width` alone would let a long body grow the dialog to the window.
`Modifier::max_width` was added for it (`winia/src/modifier.rs`, `layout/node.rs`): the cap
lowers the incoming max and is then held at or above the min. When a min and a max conflict the
min wins here, which is a DEVIATION from Compose — its `widthIn(min, max)` coerces the min down
to the max, and two separate calls are order-dependent, which winia cannot express at all
(its elements are scanned chain-wide, position-independently). Min-wins is CSS's precedence and
the reason the clamp exists: `Constraints::constrain_*` are `f32::clamp`, which panics on
min > max. Verified on a 900-wide window: the dialog stays 560.

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
A slot that demands more than the box (`.size(w, h)` larger than the dialog) OVERFLOWS it:
winia's `size()` overrides the incoming constraints where Compose's coerces them
(`requiredSize` is Compose's override), so the surface's `clip` is what keeps it inside the
rounded corners; the node still starts at the padding, so nothing else moves.

`confirm_button` is a slot like the rest, so a dialog without one builds (Compose's two-action
overload requires it; its `content` overload is `BasicAlertDialog` here).

Built on the existing `Dialog` overlay, so modality, the scrim, outside-click dismissal and
Escape come from the overlay path (`app.rs`), and the enter/exit motion is
`OverlayAnimSpec::default_enter()/default_exit()` (scale 0.8 + fade, 200ms) — the same motion
the framework's `Dialog` uses. The overlay is registered only while `visible`; no rising-edge
state is needed on top of that, because the enter animation runs on registration and
`sync_overlays` composes the current frame's content closure (the piece `ModalBottomSheet` needs
an edge for is its own sheet slide, which a dialog has none of).

Tests (8) cover: no overlay when hidden and exactly one modal, centred overlay when shown; the
`dismiss_on_outside` flag reaching the overlay; the 280 floor and the 560 cap; the slot order
with its 16/16/24 paddings; the title centring with an icon and start-alignment without one; the
button row's end alignment with the confirm action last; the WRAPPED row putting the confirm
above the dismiss; and the over-sized-slot overflow above.

## Deviations from Compose

- **No `tonalElevation` parameter.** Compose's `AlertDialogImpl` renders a `Surface` with
  `tonalElevation` (default 0) and no shadow elevation, so the CONTENT is flat.
  (`DialogTokens.ContainerElevation` = `Level3` is referenced nowhere in the alert-dialog
  implementations — what it belongs to is not demonstrable, so nothing here claims it.) winia has no
  tonal overlay at all (`surface.rs` keeps the field as a placeholder), so the parameter would
  be a silent no-op — the same defect an earlier review found in `sheet_container_color`. The
  default (0) is what you get.
- **No `weight(1f, fill = false)` on the text.** Compose gives it so the text absorbs the slack
  when the *caller* imposes a height, which puts the action row at the bottom of that height;
  winia's `layout_weight` has no `fill` flag and would stretch the node, so it is omitted and the
  slack stays BELOW the buttons instead (the column stacks from the top). A dialog sizes to its
  content by default, so this only shows with a caller-imposed height.
- **No `DialogProperties` object.** `dismiss_on_outside` is exposed directly; the other
  properties (platform-specific, e.g. `usePlatformDefaultWidth`) have no winia counterpart.
- The tests find the dialog's nodes through `Modifier::test_tag` and a real overlay layout (the
  registered overlay's content is composed in its own `Composer`, as the app does), so they
  check the geometry the user sees rather than the builder's fields.
