//! `AlertDialog` — Material 3 alert dialogs (Compose `AlertDialog` / `BasicAlertDialog`).
//!
//! Tokens and layout verified against androidx `AlertDialog.kt` and
//! `tokens/DialogTokens.kt`:
//! - **Container**: `DialogTokens.ContainerShape` = `CornerExtraLarge` (28dp) and
//!   `DialogTokens.ContainerColor` = `SurfaceContainerHigh`. `AlertDialogDefaults.TonalElevation`
//!   is `0.dp` and `AlertDialogImpl` passes no shadow elevation, so the CONTENT is flat.
//!   (`DialogTokens.ContainerElevation` = `Level3` is referenced nowhere in the alert-dialog
//!   implementations, so what it belongs to is not demonstrable — nothing here claims it.)
//!   winia has no tonal overlay at all (see `surface.rs`), which is why `tonal_elevation` is not
//!   exposed rather than accepted and ignored.
//! - **Width**: `DialogMinWidth` = 280dp .. `DialogMaxWidth` = 560dp, i.e. the content's own
//!   width clamped into that range (Compose's `sizeIn`). This is what `Modifier::max_width`
//!   was added for; `min_width` alone would let a long title grow the dialog to the window.
//! - **Content column**: padding 24dp all round; then, in order, the icon (16dp below,
//!   centred), the title (16dp below, start-aligned — or centred when an icon is present), the
//!   text (24dp below, start-aligned) and the buttons (end-aligned).
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
use crate::core::composer::ComposeCtx;
use crate::layout::{Alignment, LayoutDirection};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::overlay::{next_overlay_id, OverlayAnimSpec, OverlayDesc, PopupPosition};
use crate::ui::theme::WiniaTheme;
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
    pub fn container_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.surface_container_high
    }

    /// `IconColor` = `Secondary`.
    pub fn icon_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.secondary
    }

    /// `HeadlineColor` = `OnSurface`.
    pub fn title_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.on_surface
    }

    /// `SupportingTextColor` = `OnSurfaceVariant`.
    pub fn text_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.on_surface_variant
    }

    /// `ActionLabelTextColor` = `Primary`.
    pub fn button_color(theme: &crate::ui::theme::ThemeColors) -> Color {
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
    shape: Option<Shape>,
    container_color: Option<Color>,
    modifier: Modifier,
    content: Box<dyn Fn(&mut ComposeCtx)>,
}

impl BasicAlertDialog {
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            on_dismiss_request: None,
            dismiss_on_outside: true,
            shape: None,
            container_color: None,
            modifier: Modifier::new(),
            content: Box::new(|_| {}),
        }
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
        let user_modifier = self.modifier;
        ctx.open_overlay(OverlayDesc {
            id: id.get(),
            anchor_slot: None,
            // `Center` positions the overlay's own size, i.e. the dialog box, in the window.
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            dismiss_on_outside: self.dismiss_on_outside,
            click_passthrough: false,
            on_dismiss,
            enter_anim: Some(OverlayAnimSpec::default_enter()),
            exit_anim: Some(OverlayAnimSpec::default_exit()),
            content: Box::new(move |ctx| {
                // Rebuilt per call: this closure is `Fn` (the overlay composes it every frame
                // it is up), so nothing can be moved out of it.
                let surface = Modifier::new()
                    .min_width(DIALOG_MIN_WIDTH)
                    .max_width(DIALOG_MAX_WIDTH)
                    .background(container, shape)
                    .clip(shape)
                    // `AlertDialogDefaults.dialogPadding` — on the surface, so the background
                    // covers it and the slots lay out inside it.
                    .padding(DIALOG_CONTAINER_PADDING);
                // The caller's modifier is a WRAPPER, as in Compose's
                // `Box(modifier.sizeIn(...))`: a caller's padding must sit outside the dialog's
                // background, not eat into it.
                crate::ui::layout_components::Stack::new()
                    .modifier(user_modifier.clone())
                    .build(ctx, |ctx| {
                        crate::ui::layout_components::Column::new()
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

    /// Whether clicking outside dismisses (default true).
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
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
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
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
    title: crate::ui::text::TextStyle,
    text: crate::ui::text::TextStyle,
    button: crate::ui::text::TextStyle,
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
/// Deviation: Compose gives the text `weight(1f, fill = false)` so it absorbs the slack when
/// the CALLER imposes a height. winia's `layout_weight` has no `fill` flag and would stretch
/// the node, so it is omitted — an alert dialog sizes to its content here.
fn alert_dialog_content(
    ctx: &mut ComposeCtx,
    slots: &DialogSlots,
    direction: LayoutDirection,
    has_icon: bool,
    colors: DialogColors,
    styles: &DialogStyles,
) {
    use crate::ui::layout_components::{FlowRow, Stack};
    use crate::ui::text::ProvideTextStyle;

    // A slot: an inner box carrying the padding and the alignment within the Column.
    fn slot(
        ctx: &mut ComposeCtx,
        padding_bottom: f32,
        align: Alignment,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        Stack::new()
            .modifier(Modifier::new().padding_bottom(padding_bottom).align_self(align))
            .build(ctx, content);
    }

    if let Some(icon) = &slots.icon {
        WiniaTheme::with_content_color(colors.icon, ctx, |ctx| {
            slot(ctx, DIALOG_ICON_PADDING_BOTTOM, Alignment::Center, icon);
        });
    }
    if let Some(title) = &slots.title {
        WiniaTheme::with_content_color(colors.title, ctx, |ctx| {
            // Compose centres the title when an icon sits above it, and starts it otherwise.
            let align = if has_icon { Alignment::Center } else { Alignment::Start };
            slot(ctx, DIALOG_TITLE_PADDING_BOTTOM, align, |ctx| {
                ProvideTextStyle(styles.title.clone(), ctx, |ctx| title(ctx));
            });
        });
    }
    if let Some(text) = &slots.text {
        WiniaTheme::with_content_color(colors.text, ctx, |ctx| {
            slot(ctx, DIALOG_TEXT_PADDING_BOTTOM, Alignment::Start, |ctx| {
                ProvideTextStyle(styles.text.clone(), ctx, |ctx| text(ctx));
            });
        });
    }
    let (confirm, dismiss) = (&slots.confirm_button, &slots.dismiss_button);
    if confirm.is_some() || dismiss.is_some() {
        WiniaTheme::with_content_color(colors.button, ctx, |ctx| {
            slot(ctx, 0.0, Alignment::End, |ctx| {
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
    use crate::core::composer::Composer;
    use crate::layout::Constraints;
    use crate::ui::layout_components::Stack;

    /// Compose an AlertDialog (invisible unless `visible`) and hand back the composer.
    fn compose_dialog(dialog: AlertDialog) -> Composer {
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::ui::adaptive::set_window_size(800.0, 600.0);
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
    fn a_slot_wider_than_the_dialog_overflows_and_is_positioned_at_the_padding() {
        // winia's `size()` OVERRIDES the incoming constraints where Compose's coerces them (the
        // override is Compose's `requiredSize`), so a slot that demands more than the dialog is
        // measured at its request; the surface's `clip` is what keeps it inside the rounded
        // container. Recorded here because a slot author has to know it, and pinned so the
        // behaviour cannot change silently.
        let mut c = compose_dialog(AlertDialog::new(true).title(fixed_slot("wide", 900.0, 20.0)));
        let inner = lay_out_overlay(&mut c);
        let (x, _, w, _) = abs_rect(&inner, "wide");
        assert_eq!(x, DIALOG_CONTAINER_PADDING, "still laid out at the padding");
        assert_eq!(w, 900.0, "and measured at its request, past the dialog's cap");
    }
}
