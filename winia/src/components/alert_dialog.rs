//! `AlertDialog` — Material 3 alert dialogs (Compose `AlertDialog` / `BasicAlertDialog`).
//!
//! Tokens and layout verified against androidx `AlertDialog.kt` and
//! `tokens/DialogTokens.kt`:
//! - **Container**: `DialogTokens.ContainerShape` = `CornerExtraLarge` (28dp) and
//!   `DialogTokens.ContainerColor` = `SurfaceContainerHigh`. `AlertDialogDefaults.TonalElevation`
//!   is `0.dp` and `AlertDialogImpl` passes no shadow elevation, so the CONTENT is flat.
//!   (`DialogTokens.ContainerElevation` = `Level3` is referenced nowhere in the alert-dialog
//!   implementations, so what it belongs to is not demonstrable — nothing here claims it.)
//!   winia's `Surface` implements the tint (see `surface.rs`), but Compose's gate only tints a surface
//!   whose colour is EXACTLY `surface`, and this container is `surfaceContainerHigh` — so `tonal_elevation`
//!   is not exposed here because exposing it would be a parameter that provably does nothing.
//! - **Width**: `DialogMinWidth` = 280dp .. `DialogMaxWidth` = 560dp, i.e. the content's own
//!   width clamped into that range (Compose's `sizeIn`). This is what `Modifier::max_width`
//!   was added for; `min_width` alone would let a long title grow the dialog to the window.
//!   `platform_default_width(false)` is `DialogProperties.usePlatformDefaultWidth = false`: the
//!   range is not applied at all and the content decides its own width.
//! - **Content column**: padding 24dp all round; then, in order, the icon (16dp below,
//!   centred), the title (16dp below, start-aligned — or centred when an icon is present), the
//!   text (24dp below, start-aligned; it takes the slack when a height is imposed) and the
//!   buttons (end-aligned).
//! - **Buttons**: a `FlowRow` (8dp both axes) whose LAYOUT DIRECTION IS FLIPPED while the
//!   buttons themselves keep the original direction. That is what puts the confirm action after
//!   the dismiss one in a row while keeping it ABOVE when the row wraps — the trick is
//!   Compose's `AlertDialogFlowRow`, reproduced here with `WiniaTheme::with_theme_and_direction`.
//! - **Colors**: icon `Secondary`, title `OnSurface`, text `OnSurfaceVariant`, button labels
//!   `Primary` as a default the buttons may override (Compose notes that `TextButton` uses its
//!   own colors, which is the usual choice for the two actions).
//!
//! Built on the existing `Dialog` overlay: modal scrim, outside-click dismissal, Escape
//! handling through the overlay path, and the framework's dialog enter/exit motion
//! (scale 0.8 + fade, 200ms) come from there.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::layout::{Alignment, LayoutDirection};
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::overlay::{next_overlay_id, OverlayAnimSpec, OverlayDesc, PopupPosition};
use crate::theme::WiniaTheme;
use std::sync::Arc;

/// `DialogMinWidth` — the narrowest an alert dialog gets.
pub const DIALOG_MIN_WIDTH: f32 = 280.0;
/// `DialogMaxWidth` — content wider than this is constrained to it.
pub const DIALOG_MAX_WIDTH: f32 = 560.0;
/// `DialogTokens.ContainerShape` = `ShapeKeyTokens.CornerExtraLarge` (the M3 "extra large"
/// corner, 28dp — the same radius the bottom sheet's expanded shape uses).
pub const DIALOG_CORNER_RADIUS: f32 = 28.0;
/// `AlertDialogDefaults.dialogPadding` — the content column's padding on every side.
pub const DIALOG_CONTAINER_PADDING: f32 = 24.0;
/// Space below the icon slot.
pub const DIALOG_ICON_PADDING_BOTTOM: f32 = 16.0;
/// Space below the title slot.
pub const DIALOG_TITLE_PADDING_BOTTOM: f32 = 16.0;
/// `AlertDialogDefaults.textPadding` — space below the text slot.
pub const DIALOG_TEXT_PADDING_BOTTOM: f32 = 24.0;
/// Spacing between the action buttons, on both axes (`ButtonsMainAxisSpacing` /
/// `ButtonsCrossAxisSpacing`).
pub const DIALOG_BUTTON_SPACING: f32 = 8.0;
/// `DialogTokens.IconSize` — the size a caller is expected to give the icon slot.
pub const DIALOG_ICON_SIZE: f32 = 24.0;

/// Defaults (Compose `AlertDialogDefaults`, `DialogTokens`).
pub struct AlertDialogDefaults;

impl AlertDialogDefaults {
    /// `ContainerShape` = `CornerExtraLarge`.
    pub fn shape() -> Shape {
        Shape::rounded(DIALOG_CORNER_RADIUS)
    }

    /// `ContainerColor` = `SurfaceContainerHigh`.
    pub fn container_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.surface_container_high
    }

    /// `IconColor` = `Secondary`.
    pub fn icon_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.secondary
    }

    /// `HeadlineColor` = `OnSurface`.
    pub fn title_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface
    }

    /// `SupportingTextColor` = `OnSurfaceVariant`.
    pub fn text_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface_variant
    }

    /// `ActionLabelTextColor` = `Primary`.
    pub fn button_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// `IconSize` (24dp) — the token for the caller's own icon.
    pub fn icon_size() -> f32 {
        DIALOG_ICON_SIZE
    }
}

/// A dialog with arbitrary content (Compose `BasicAlertDialog`).
///
/// The content defines its own styling; this provides the surface (shape, colour, the
/// 280..560dp width range) and the dialog's behaviour.
pub struct BasicAlertDialog {
    visible: bool,
    on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
    dismiss_on_back_press: bool,
    focusable: bool,
    platform_default_width: bool,
    shape: Option<Shape>,
    container_color: Option<Color>,
    content_padding: f32,
    modifier: Modifier,
    content: Box<dyn Fn(&mut ComposeCtx)>,
}

impl BasicAlertDialog {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            on_dismiss_request: None,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            focusable: true,
            platform_default_width: true,
            shape: None,
            container_color: None,
            content_padding: DIALOG_CONTAINER_PADDING,
            modifier: Modifier::new(),
            content: Box::new(|_| {}),
        }
    }

    /// Whether the container is held inside `DialogMinWidth .. DialogMaxWidth` — Compose's
    /// `DialogProperties.usePlatformDefaultWidth`, `true` by default.
    ///
    /// `false` is how Compose builds a dialog that sizes itself: the content decides its own width
    /// and the 280..560 range is not applied. Without it there is no way to ask for a dialog wider
    /// than [`DIALOG_MAX_WIDTH`], or for one that fills the window.
    pub fn platform_default_width(mut self, v: bool) -> Self {
        self.platform_default_width = v;
        self
    }

    /// The inset between the container and the content, [`DIALOG_CONTAINER_PADDING`] by default.
    ///
    /// Compose's `BasicAlertDialog` has no padding of its own — the 24 dp belongs to `AlertDialog`'s
    /// content column — but winia's surface carries it so an alert dialog lands on the token
    /// geometry without repeating it. A caller that brings its own surface passes `0.0`: the date
    /// picker dialog's container IS its content (a 360 dp calendar with the action row under it).
    pub fn content_padding(mut self, padding: f32) -> Self {
        self.content_padding = padding;
        self
    }

    /// The dialog's body — an arbitrary subtree. `Fn`, not `FnOnce`: the overlay's content is
    /// composed on every frame the dialog is up (the same bound `ModalBottomSheet` uses).
    pub fn content(mut self, content: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.content = Box::new(content);
        self
    }

    /// Called when the user dismisses the dialog (outside click, or Escape through the overlay
    /// path). Compose's `onDismissRequest`.
    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_request = Some(Arc::new(cb));
        self
    }

    /// Whether clicking outside dismisses (Compose `DialogProperties.dismissOnClickOutside`,
    /// default true).
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
        self
    }

    /// Whether Escape / the back press dismisses (Compose
    /// `DialogProperties.dismissOnBackPress`, default true).
    ///
    /// False still SWALLOWS the key rather than letting it through: the page behind the scrim must not
    /// react to an Escape this dialog kept. Compose has no reason to swallow — its dialog is a separate
    /// window, so `onKeyUp` just falls through (`BasicEdgeToEdgeDialog.android.kt:227,235`) — which is
    /// exactly why this is winia's rule and not a quote of Compose's. See [`crate::app`]'s
    /// `escape_key`.
    pub fn dismiss_on_back_press(mut self, v: bool) -> Self {
        self.dismiss_on_back_press = v;
        self
    }

    /// Whether the dialog can take the keyboard while it is up (Compose
    /// `DialogProperties.isFocusable`, default true).
    ///
    /// False gives the page behind the scrim its ordinary key handling — for a dialog that is really a
    /// transient notice with nothing to focus. A non-focusable dialog is also skipped by the
    /// "topmost focus scope" test (`app.rs::focus_scope_is_open`), so a lower dialog does not inherit it.
    ///
    /// ⚠ **Tab is the exception, and it does not behave the way the sentence above implies.** Tab is
    /// consumed unconditionally by the key path (`app.rs:1360-1362`), and with no focus-scope overlay up
    /// `keyboard_scope` finds no arena to move within (`app.rs:3346`), so focus goes nowhere at all
    /// rather than reaching the page behind. Compose's window model does not have this case — a
    /// non-focusable dialog is a window that never took focus, and Tab belongs to whatever is behind it.
    /// Recorded in `docs/alert-dialog.md`; not fixed here because the key path's unconditional consume is
    /// load-bearing for every other overlay.
    pub fn focusable(mut self, v: bool) -> Self {
        self.focusable = v;
        self
    }

    /// Container shape (default [`AlertDialogDefaults::shape`]).
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Container colour (default [`AlertDialogDefaults::container_color`]).
    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Forward an already-boxed handler (used by [`AlertDialog`], which owns the slot and
    /// delegates here).
    pub(crate) fn dismiss_handler(mut self, cb: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        self.on_dismiss_request = cb;
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        // The id is remembered before the early return so a hidden dialog keeps its slot (and
        // re-showing it reuses the same overlay id). No rising-edge state is needed on top of
        // that: the overlay's enter animation runs when the overlay is registered, and
        // `sync_overlays` then composes this frame's content closure — the piece
        // `ModalBottomSheet` needs an edge for is its own sheet slide, which the dialog has none
        // of.
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return;
        }
        let shape = self.shape.unwrap_or_else(AlertDialogDefaults::shape);
        let container = self
            .container_color
            .unwrap_or_else(|| AlertDialogDefaults::container_color(&theme));
        let content = self.content;
        let on_dismiss = self.on_dismiss_request;
        let content_padding = self.content_padding;
        let platform_default_width = self.platform_default_width;
        let user_modifier = self.modifier;
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            // `Center` positions the overlay's own size, i.e. the dialog box, in the window.
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            // A dialog owns the keyboard while it is up: Tab works inside the dialog, and the page
            // behind the scrim cannot be reached.
            focus_scope: self.focusable,
            dismiss_on_outside: self.dismiss_on_outside,
            dismiss_on_back_press: self.dismiss_on_back_press,
            click_passthrough: false,
            // An alert dialog is centred, so there is nothing to fit around the anchor.
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss,
            enter_anim: Some(OverlayAnimSpec::default_enter()),
            exit_anim: Some(OverlayAnimSpec::default_exit()),
            content: Box::new(move |ctx| {
                // Rebuilt per call: this closure is `Fn` (the overlay composes it every frame
                // it is up), so nothing can be moved out of it.
                //
                // The width range is Compose's `sizeIn(DialogMinWidth, DialogMaxWidth)`, applied
                // unless the caller turned it off — `DialogProperties.usePlatformDefaultWidth`,
                // which is the switch that lets a dialog size itself.
                let mut surface = Modifier::new();
                if platform_default_width {
                    surface = surface.min_width(DIALOG_MIN_WIDTH).max_width(DIALOG_MAX_WIDTH);
                }
                let surface = surface
                    .background(container, shape)
                    .clip(shape)
                    // `role = Dialog` is winia's landing for Compose's
                    // `Modifier.semantics { paneTitle = dialogPaneDescription }` on the dialog Box
                    // (`AlertDialog.kt:171`): it is what tells a screen reader the overlay opened as a
                    // dialog pane, and `accessibility.rs` maps the role to the Pane UIA control type.
                    // A caller's own modifier still wins — it is applied after this one.
                    .semantics(
                        crate::semantics::SemanticsConfig::new()
                            .role(crate::semantics::SemanticsRole::Dialog),
                    )
                    // `AlertDialogDefaults.dialogPadding` — on the surface, so the background
                    // covers it and the slots lay out inside it.
                    .padding(content_padding);
                // The caller's modifier is a WRAPPER, as in Compose's
                // `Box(modifier.sizeIn(...))`: a caller's padding must sit outside the dialog's
                // background, not eat into it.
                crate::layout::components::Stack::new()
                    .modifier(user_modifier.clone())
                    .build(ctx, |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(surface)
                            .alignment(Alignment::Start)
                            .build(ctx, |ctx| content(ctx));
                    });
            }),
            local_snapshot: Vec::new(),
        });
    }
}

/// A Material 3 alert dialog: an optional icon, title and text, and one or two action buttons
/// (Compose `AlertDialog`).
pub struct AlertDialog {
    visible: bool,
    on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
    dismiss_on_back_press: bool,
    focusable: bool,
    platform_default_width: bool,
    icon: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    title: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    text: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    confirm_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    dismiss_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    shape: Option<Shape>,
    container_color: Option<Color>,
    icon_content_color: Option<Color>,
    title_content_color: Option<Color>,
    text_content_color: Option<Color>,
    button_content_color: Option<Color>,
    modifier: Modifier,
}

impl AlertDialog {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            on_dismiss_request: None,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            focusable: true,
            platform_default_width: true,
            icon: None,
            title: None,
            text: None,
            confirm_button: None,
            dismiss_button: None,
            shape: None,
            container_color: None,
            icon_content_color: None,
            title_content_color: None,
            text_content_color: None,
            button_content_color: None,
            modifier: Modifier::new(),
        }
    }

    /// The confirming action. Compose requires it with the two-action signature; here it is a
    /// slot like the rest, and the row is end-aligned either way.
    pub fn confirm_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.confirm_button = Some(Box::new(button));
        self
    }

    /// The dismissing action, shown before the confirm button in a row.
    pub fn dismiss_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.dismiss_button = Some(Box::new(button));
        self
    }

    /// Icon above the title (centred; the title centres with it).
    pub fn icon(mut self, icon: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.icon = Some(Box::new(icon));
        self
    }

    /// Dialog title. Not mandatory — the text alone is often enough.
    pub fn title(mut self, title: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.title = Some(Box::new(title));
        self
    }

    /// The dialog's message.
    pub fn text(mut self, text: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.text = Some(Box::new(text));
        self
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_request = Some(Arc::new(cb));
        self
    }

    /// Whether clicking outside dismisses (Compose `DialogProperties.dismissOnClickOutside`,
    /// default true).
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
        self
    }

    /// Whether Escape / the back press dismisses (Compose
    /// `DialogProperties.dismissOnBackPress`, default true). False still swallows the key rather than
    /// letting the page behind react to it.
    pub fn dismiss_on_back_press(mut self, v: bool) -> Self {
        self.dismiss_on_back_press = v;
        self
    }

    /// Whether the dialog takes the keyboard while it is up (Compose
    /// `DialogProperties.isFocusable`, default true).
    pub fn focusable(mut self, v: bool) -> Self {
        self.focusable = v;
        self
    }

    /// Whether the container is held inside `DialogMinWidth .. DialogMaxWidth` — Compose's
    /// `DialogProperties.usePlatformDefaultWidth`, `true` by default. See
    /// [`BasicAlertDialog::platform_default_width`].
    pub fn platform_default_width(mut self, v: bool) -> Self {
        self.platform_default_width = v;
        self
    }

    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    /// Icon tint (default `Secondary`).
    pub fn icon_content_color(mut self, color: Color) -> Self {
        self.icon_content_color = Some(color);
        self
    }

    /// Title colour (default `OnSurface`).
    pub fn title_content_color(mut self, color: Color) -> Self {
        self.title_content_color = Some(color);
        self
    }

    /// Text colour (default `OnSurfaceVariant`).
    pub fn text_content_color(mut self, color: Color) -> Self {
        self.text_content_color = Some(color);
        self
    }

    /// Content colour provided to the buttons (default `Primary`). `Button`/`TextButton` set
    /// their own colours, so this mainly reaches custom content — as in Compose.
    pub fn button_content_color(mut self, color: Color) -> Self {
        self.button_content_color = Some(color);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        let has_icon = self.icon.is_some();
        let shape = self.shape;
        let container_color = self.container_color;
        let icon_color = self.icon_content_color.unwrap_or_else(|| AlertDialogDefaults::icon_color(&theme));
        let title_color = self.title_content_color.unwrap_or_else(|| AlertDialogDefaults::title_color(&theme));
        let text_color = self.text_content_color.unwrap_or_else(|| AlertDialogDefaults::text_color(&theme));
        let button_color = self.button_content_color.unwrap_or_else(|| AlertDialogDefaults::button_color(&theme));
        let title_style = WiniaTheme::typography().headline_small;
        let text_style = WiniaTheme::typography().body_medium;
        let button_style = WiniaTheme::typography().label_large;
        let slots = DialogSlots {
            icon: self.icon,
            title: self.title,
            text: self.text,
            confirm_button: self.confirm_button,
            dismiss_button: self.dismiss_button,
        };

        let mut dialog = BasicAlertDialog::new(self.visible)
            .dismiss_on_outside(self.dismiss_on_outside)
            .dismiss_on_back_press(self.dismiss_on_back_press)
            .focusable(self.focusable)
            .platform_default_width(self.platform_default_width)
            .modifier(self.modifier)
            .dismiss_handler(self.on_dismiss_request)
            .content(move |ctx| {
                let colors = DialogColors {
                    icon: icon_color,
                    title: title_color,
                    text: text_color,
                    button: button_color,
                };
                let styles = DialogStyles {
                    title: title_style.clone(),
                    text: text_style.clone(),
                    button: button_style.clone(),
                };
                alert_dialog_content(ctx, &slots, direction, has_icon, colors, &styles);
            });
        if let Some(shape) = shape {
            dialog = dialog.shape(shape);
        }
        if let Some(color) = container_color {
            dialog = dialog.container_color(color);
        }
        dialog.build(ctx);
    }
}

/// The four content colours, grouped so the content function stays readable.
struct DialogColors {
    icon: Color,
    title: Color,
    text: Color,
    button: Color,
}

/// The three text styles, likewise (`HeadlineSmall` / `BodyMedium` / `LabelLarge`).
struct DialogStyles {
    title: crate::text::TextStyle,
    text: crate::text::TextStyle,
    button: crate::text::TextStyle,
}

/// The slot closures, grouped so the content function takes one argument instead of six.
struct DialogSlots {
    icon: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    title: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    text: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    confirm_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    dismiss_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
}

/// The M3 content column: icon, title, text, then the action row (Compose
/// `AlertDialogContent`).
///
/// Each slot is wrapped in its own box so it can carry the slot's bottom padding and
/// cross-axis alignment; the Column itself belongs to [`BasicAlertDialog`].
///
/// The text slot carries `layout_weight_fill(1.0, false)`, matching Compose's
/// `Box(Modifier.weight(1f, fill = false))` (`AlertDialog.kt:350`): when the column is height
/// constrained, the text box is clamped to the leftover space so the action row keeps its own
/// height. Without it a tall text takes its full content height, the column overflows and the
/// buttons are crushed — measured at 0 px in the 800x600 window the tests lay out in, with the fix
/// putting the row back at its own 40 px.
///
/// `fill = false` is the load-bearing half, and the tests that pin it are the ones with a SHORT
/// text, not the tall one: with `fill = true` the box is forced to its share either way, so
/// `a_tall_text_leaves_the_action_row_its_height` passes on both. Swapping the call for
/// `layout_weight(1.0)` fails `slots_stack_in_order_with_their_paddings` (measured: 11 passed, 1
/// failed, at its `confirm_y` assertion), because its fixed 40-tall text would be stretched to the
/// whole share. That is the behaviour `fill = false` buys: the share is a MAXIMUM main-axis size,
/// so text shorter than its share leaves the column shorter as well.
///
/// The other slots have no weight; Compose gives none either.
fn alert_dialog_content(
    ctx: &mut ComposeCtx,
    slots: &DialogSlots,
    direction: LayoutDirection,
    has_icon: bool,
    colors: DialogColors,
    styles: &DialogStyles,
) {
    use crate::layout::components::{FlowRow, Stack};
    use crate::components::text::ProvideTextStyle;

    // A slot: an inner box carrying the padding and the alignment within the Column, and — for the
    // text slot only — the weight that lets it take the slack when a height is imposed.
    fn slot(
        ctx: &mut ComposeCtx,
        padding_bottom: f32,
        align: Alignment,
        weight: Option<f32>,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        let mut modifier = Modifier::new();
        if let Some(w) = weight {
            // `fill = false`: the share is the box's MAXIMUM main-axis size, so text shorter than
            // its share leaves the column shorter too — Compose's `weight(1f, fill = false)`.
            modifier = modifier.layout_weight_fill(w, false);
        }
        Stack::new()
            .modifier(modifier.padding_bottom(padding_bottom).align_self(align))
            .build(ctx, content);
    }

    if let Some(icon) = &slots.icon {
        WiniaTheme::with_content_color(colors.icon, ctx, |ctx| {
            slot(ctx, DIALOG_ICON_PADDING_BOTTOM, Alignment::Center, None, icon);
        });
    }
    if let Some(title) = &slots.title {
        WiniaTheme::with_content_color(colors.title, ctx, |ctx| {
            // Compose centres the title when an icon sits above it, and starts it otherwise.
            let align = if has_icon { Alignment::Center } else { Alignment::Start };
            slot(ctx, DIALOG_TITLE_PADDING_BOTTOM, align, None, |ctx| {
                ProvideTextStyle(styles.title.clone(), ctx, |ctx| title(ctx));
            });
        });
    }
    if let Some(text) = &slots.text {
        WiniaTheme::with_content_color(colors.text, ctx, |ctx| {
            slot(ctx, DIALOG_TEXT_PADDING_BOTTOM, Alignment::Start, Some(1.0), |ctx| {
                ProvideTextStyle(styles.text.clone(), ctx, |ctx| text(ctx));
            });
        });
    }
    let (confirm, dismiss) = (&slots.confirm_button, &slots.dismiss_button);
    if confirm.is_some() || dismiss.is_some() {
        WiniaTheme::with_content_color(colors.button, ctx, |ctx| {
            slot(ctx, 0.0, Alignment::End, None, |ctx| {
                // Compose's `AlertDialogFlowRow`: the FLOW lays out in the flipped direction
                // while the buttons inside keep the original one. The content order (confirm
                // first) then reads dismiss-then-confirm across a row and confirm-above-dismiss
                // once the row wraps — the M3 arrangement in both cases.
                let flipped = match direction {
                    LayoutDirection::Ltr => LayoutDirection::Rtl,
                    LayoutDirection::Rtl => LayoutDirection::Ltr,
                };
                WiniaTheme::with_theme_and_direction(WiniaTheme::colors(), flipped, ctx, |ctx| {
                    FlowRow::new()
                        .main_spacing(DIALOG_BUTTON_SPACING)
                        .cross_spacing(DIALOG_BUTTON_SPACING)
                        .build(ctx, |ctx| {
                            WiniaTheme::with_theme_and_direction(
                                WiniaTheme::colors(),
                                direction,
                                ctx,
                                |ctx| {
                                    ProvideTextStyle(styles.button.clone(), ctx, |ctx| {
                                        if let Some(confirm) = confirm {
                                            confirm(ctx);
                                        }
                                        if let Some(dismiss) = dismiss {
                                            dismiss(ctx);
                                        }
                                    });
                                },
                            );
                        });
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::layout::components::Stack;

    /// Compose an AlertDialog (invisible unless `visible`) and hand back the composer.
    fn compose_dialog(dialog: AlertDialog) -> Composer {
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(800.0, 600.0);
            dialog.build(ctx);
        });
        c
    }

    /// Run the registered overlay's content in its own Composer and lay it out, the way the app
    /// does — an overlay composes in a separate Composer and is positioned by the overlay layer.
    fn lay_out_overlay(c: &mut Composer) -> Composer {
        let overlays = c.take_overlays();
        assert_eq!(overlays.len(), 1, "exactly one overlay");
        let mut inner = Composer::new();
        let content = &overlays[0].content;
        inner.compose(|ctx| content(ctx));
        inner.layout(Constraints::new(0.0, 800.0, 0.0, 600.0));
        inner
    }

    /// Absolute rect of the node carrying `tag`, with the ancestors' offsets summed (a node's
    /// own `position` is parent-relative).
    fn abs_rect(c: &Composer, tag: &str) -> (f32, f32, f32, f32) {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            y: f32,
            tag: &str,
        ) -> Option<(f32, f32, f32, f32)> {
            let node = &nodes[idx];
            let (ax, ay) = (x + node.position.x, y + node.position.y);
            if node.modifier.get_test_tag() == Some(tag) {
                return Some((ax, ay, node.measured_size.width, node.measured_size.height));
            }
            node.children
                .iter()
                .find_map(|&child| walk(nodes, child, ax, ay, tag))
        }
        let root = c.layout_root_idx().expect("laid out");
        walk(c.arena_nodes(), root, 0.0, 0.0, tag).expect("the tagged node is in the overlay tree")
    }

    /// A slot whose size is fixed, so layout assertions do not depend on text metrics.
    fn fixed_slot(tag: &'static str, w: f32, h: f32) -> impl Fn(&mut ComposeCtx) + 'static {
        move |ctx| {
            Stack::new()
                .modifier(Modifier::new().test_tag(tag).size(w, h))
                .build(ctx, |_| {});
        }
    }

    #[test]
    fn invisible_registers_no_overlay_and_visible_registers_one_modal() {
        let mut hidden = compose_dialog(AlertDialog::new(false).title(fixed_slot("t", 100.0, 20.0)));
        assert!(hidden.take_overlays().is_empty(), "hidden: no overlay");

        let mut shown = compose_dialog(
            AlertDialog::new(true)
                .title(fixed_slot("t", 100.0, 20.0))
                .dismiss_on_outside(false),
        );
        let overlays = shown.take_overlays();
        assert_eq!(overlays.len(), 1, "visible: one overlay");
        assert!(overlays[0].modal, "an alert dialog is modal (scrim + dismissal)");
        assert!(!overlays[0].dismiss_on_outside, "the flag is forwarded");
        assert!(
            matches!(overlays[0].position, PopupPosition::Center),
            "centred in the window"
        );
    }

    /// `DialogProperties.dismissOnBackPress` and `isFocusable` reach the overlay, and both default to
    /// what Compose defaults them to (true).
    #[test]
    fn the_dialog_properties_reach_the_overlay() {
        let mut plain = compose_dialog(AlertDialog::new(true).title(fixed_slot("t", 100.0, 20.0)));
        let overlays = plain.take_overlays();
        assert!(overlays[0].dismiss_on_back_press, "dismissOnBackPress defaults true");
        assert!(overlays[0].focus_scope, "isFocusable defaults true");

        let mut off = compose_dialog(
            AlertDialog::new(true)
                .title(fixed_slot("t", 100.0, 20.0))
                .dismiss_on_back_press(false)
                .focusable(false),
        );
        let overlays = off.take_overlays();
        assert!(!overlays[0].dismiss_on_back_press, "the flag is forwarded");
        assert!(!overlays[0].focus_scope, "and so is isFocusable");
    }

    /// The dialog publishes `role = Dialog`, which is winia's landing for Compose's `paneTitle`
    /// semantics (`AlertDialog.kt:171`) and what maps to the Pane UIA control type.
    #[test]
    fn the_dialog_publishes_the_dialog_role() {
        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("t", 100.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        // The role rides the surface, which is the column carrying the background — not the overlay's
        // root node, which is the caller's modifier wrapper around it.
        let role = inner
            .arena_nodes()
            .iter()
            .map(|node| node.modifier.semantics_config().role_value())
            .find(|role| role.is_some());
        assert_eq!(
            role,
            Some(Some(crate::semantics::SemanticsRole::Dialog)),
            "the dialog surface must announce itself as a dialog pane"
        );
    }

    #[test]
    fn the_surface_is_the_minimum_width_for_narrow_content() {
        // Compose clamps into `DialogMinWidth .. DialogMaxWidth`: a 100-wide title is not the
        // dialog's width, 280 is (`sizeIn`).
        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("t", 100.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().expect("laid out");
        assert_eq!(
            inner.arena_nodes()[root].measured_size.width, DIALOG_MIN_WIDTH,
            "narrow content still gets {DIALOG_MIN_WIDTH}"
        );
    }

    #[test]
    fn the_surface_is_capped_at_the_maximum_width() {
        // 900 of content (plus the 24dp side padding) must come out at the 560 cap, not the
        // window width — this is what `Modifier::max_width` was added for.
        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("wide", 900.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().expect("laid out");
        assert_eq!(
            inner.arena_nodes()[root].measured_size.width, DIALOG_MAX_WIDTH,
            "content wider than {DIALOG_MAX_WIDTH} is constrained to it"
        );
    }

    #[test]
    fn the_width_clamp_is_what_platform_default_width_turns_off() {
        // Compose's `DialogProperties(usePlatformDefaultWidth = false)` is how a dialog sizes
        // itself. With it off the 560 cap does not apply: 900 of content (plus the 24dp padding)
        // comes out wider than `DIALOG_MAX_WIDTH` and stays inside the 800 window.
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .platform_default_width(false)
                .title(fixed_slot("wide", 900.0, 20.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().expect("laid out");
        let width = inner.arena_nodes()[root].measured_size.width;
        assert!(
            width > DIALOG_MAX_WIDTH,
            "with the platform default width off the cap must not apply, got {width}"
        );
        assert!(width <= 800.0, "and the window still constrains it, got {width}");
    }

    #[test]
    fn a_tall_text_leaves_the_action_row_its_height() {
        // Compose wraps the text slot in `weight(1f, fill = false)` (`AlertDialog.kt:350`): when
        // the column is height-constrained, the text box is clamped to the leftover space so the
        // action row keeps its own. Without that the text takes its full content height, the
        // column overflows and the buttons are crushed: this dialog came out `280x600` in the
        // 800x600 window below, with the action row at y=576 and height 0. With the weight it is
        // at y=536 with its full height.
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .title(fixed_slot("t", 100.0, 20.0))
                .text(fixed_slot("body", 200.0, 2000.0))
                .confirm_button(fixed_slot("ok", 80.0, 40.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let (_, y, _, h) = abs_rect(&inner, "ok");
        assert_eq!(h, 40.0, "the action row keeps its own height");
        assert_eq!(y, 536.0, "and sits above the container's bottom padding");
    }

    #[test]
    fn slots_stack_in_order_with_their_paddings() {
        // icon | title | text | buttons, with 16dp below the icon, 16dp below the title and
        // 24dp below the text (Compose's AlertDialogContent order).
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .icon(fixed_slot("icon", 24.0, 24.0))
                .title(fixed_slot("title", 100.0, 20.0))
                .text(fixed_slot("text", 200.0, 40.0))
                .confirm_button(fixed_slot("confirm", 60.0, 40.0))
                .dismiss_button(fixed_slot("dismiss", 70.0, 40.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let (_, icon_y, _, _) = abs_rect(&inner, "icon");
        let (_, title_y, _, _) = abs_rect(&inner, "title");
        let (_, text_y, _, _) = abs_rect(&inner, "text");
        let (_, confirm_y, _, _) = abs_rect(&inner, "confirm");
        assert_eq!(icon_y, DIALOG_CONTAINER_PADDING, "the icon starts at the 24dp padding");
        assert_eq!(
            title_y,
            icon_y + 24.0 + DIALOG_ICON_PADDING_BOTTOM,
            "16dp below the icon"
        );
        assert_eq!(
            text_y,
            title_y + 20.0 + DIALOG_TITLE_PADDING_BOTTOM,
            "16dp below the title"
        );
        assert_eq!(
            confirm_y,
            text_y + 40.0 + DIALOG_TEXT_PADDING_BOTTOM,
            "24dp below the text"
        );
    }

    #[test]
    fn the_icon_centres_the_title_and_a_lone_title_starts() {
        // Compose centres the title when an icon sits above it; without one it starts.
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .icon(fixed_slot("icon", 24.0, 24.0))
                .title(fixed_slot("title", 100.0, 20.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().unwrap();
        let dialog_w = inner.arena_nodes()[root].measured_size.width;
        let (icon_x, _, icon_w, _) = abs_rect(&inner, "icon");
        let (title_x, _, title_w, _) = abs_rect(&inner, "title");
        let inner_left = DIALOG_CONTAINER_PADDING;
        let inner_w = dialog_w - 2.0 * DIALOG_CONTAINER_PADDING;
        assert!(
            (icon_x - (inner_left + (inner_w - icon_w) / 2.0)).abs() < 0.5,
            "the icon is centred in the content box (x={icon_x})"
        );
        assert!(
            (title_x - (inner_left + (inner_w - title_w) / 2.0)).abs() < 0.5,
            "and so is the title (x={title_x})"
        );

        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("title", 100.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        let (title_x, _, _, _) = abs_rect(&inner, "title");
        assert_eq!(
            title_x, DIALOG_CONTAINER_PADDING,
            "without an icon the title starts at the padding"
        );
    }

    #[test]
    fn the_buttons_sit_at_the_end_with_the_confirm_action_last() {
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .title(fixed_slot("title", 100.0, 20.0))
                .confirm_button(fixed_slot("confirm", 60.0, 40.0))
                .dismiss_button(fixed_slot("dismiss", 70.0, 40.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().unwrap();
        let dialog_w = inner.arena_nodes()[root].measured_size.width;
        let (confirm_x, _, confirm_w, _) = abs_rect(&inner, "confirm");
        let (dismiss_x, _, dismiss_w, _) = abs_rect(&inner, "dismiss");
        assert_eq!(
            dismiss_x,
            dialog_w - DIALOG_CONTAINER_PADDING - confirm_w - DIALOG_BUTTON_SPACING - dismiss_w,
            "dismiss, then the 8dp gap, then confirm, all against the end padding"
        );
        assert_eq!(
            confirm_x + confirm_w,
            dialog_w - DIALOG_CONTAINER_PADDING,
            "the confirm action ends at the dialog's end padding"
        );
    }
    #[test]
    fn a_wrapped_action_row_puts_the_confirm_above_the_dismiss() {
        // The other half of the flipped-direction trick: once the row wraps, the confirm action
        // (first in content order) is on the FIRST line, i.e. above the dismiss — Compose's
        // `AlertDialogFlowRow` behaviour. 260 + 8 + 260 exceeds the 512-wide content box of a
        // dialog already at its 560 cap — with two 150-wide buttons the dialog would simply have
        // grown to 356 and kept them on one line.
        let mut c = compose_dialog(
            AlertDialog::new(true)
                .title(fixed_slot("title", 300.0, 20.0))
                .confirm_button(fixed_slot("confirm", 260.0, 40.0))
                .dismiss_button(fixed_slot("dismiss", 260.0, 40.0)),
        );
        let inner = lay_out_overlay(&mut c);
        let root = inner.layout_root_idx().unwrap();
        let dialog_w = inner.arena_nodes()[root].measured_size.width;
        let (confirm_x, confirm_y, confirm_w, _) = abs_rect(&inner, "confirm");
        let (dismiss_x, dismiss_y, dismiss_w, _) = abs_rect(&inner, "dismiss");
        assert!(
            confirm_y + 40.0 <= dismiss_y,
            "wrapped: confirm on top (confirm y={confirm_y}, dismiss y={dismiss_y})"
        );
        for (x, w, which) in [(confirm_x, confirm_w, "confirm"), (dismiss_x, dismiss_w, "dismiss")] {
            assert_eq!(
                x + w,
                dialog_w - DIALOG_CONTAINER_PADDING,
                "each wrapped line is end-aligned ({which})"
            );
        }
    }

    #[test]
    fn a_slot_wider_than_the_dialog_is_coerced_into_it_and_keeps_its_padding() {
        // A slot asking for more width than it is offered comes out at the offered width — Compose's
        // `size()` is `enforceIncoming = true` (`SizeNode.measure` runs
        // `constraints.constrain(Constraints.fixed(...))`). winia used to write the request over the
        // constraints instead, so a 900 dp slot measured 900 and depended on the surface's `clip` to
        // hide the spill; that override is `required_size` now, for a caller who really wants it.
        //
        // The coerced width is the dialog's own cap reached through its padding:
        // `DIALOG_MAX_WIDTH` 560 less the 24 dp content padding on each side.
        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("wide", 900.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        let (x, _, w, _) = abs_rect(&inner, "wide");
        assert_eq!(x, DIALOG_CONTAINER_PADDING, "still laid out at the padding");
        assert_eq!(
            w,
            DIALOG_MAX_WIDTH - 2.0 * DIALOG_CONTAINER_PADDING,
            "coerced to what it is offered, not measured at its 900 dp request"
        );
    }
}
