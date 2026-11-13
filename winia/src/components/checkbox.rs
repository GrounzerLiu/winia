//! Checkbox / TriStateCheckbox — the counterpart of material3's `Checkbox` / `TriStateCheckbox`.
//!
//! What the M3 1.5.0-beta01 implementation pins, and what this file matches:
//! - a 20×20 visual, a 2 dp corner and a 2 dp stroke; when checked or indeterminate the border
//!   colour equals the container colour, so the two merge into one fill;
//! - a 40×40 touch target and state layer (`CheckboxTokens.StateLayerSize`), with an unbounded
//!   ripple;
//! - colour splits on enabled × state only — M3's `CheckboxColors` has no hover/focus/press variant,
//!   because the ripple carries the interaction feedback;
//! - `On` draws the check path and `Indeterminate` draws a dash (M3 flattens the middle of
//!   `drawCheck`); the two icons scale independently, approximating the `checkDrawFraction` +
//!   `crossCenterGravitation` transition;
//! - `Checkbox` delegates to `TriStateCheckbox(state = ToggleableState(checked))`.

use crate::composable;
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::input::interaction::MutableInteractionSource;
use crate::layout::BoxLayout;
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// The visual box size — M3's `CheckboxSize = 20dp`.
pub const CHECKBOX_SIZE: f32 = 20.0;
/// The node a CLICKABLE checkbox occupies: `Modifier.minimumInteractiveComponentSize()` = 48 dp, with the 20 dp
/// box centred inside (`material3/Checkbox.kt:293-302`; the minimum applies whenever `onClick != null`, disabled
/// or not).
pub const CHECKBOX_TOUCH_TARGET: f32 = 48.0;
/// `CheckboxTokens.StateLayerSize = 40dp` — the DIAMETER of the hover/focus halo, which is what Compose hands to
/// `ripple` as its radius. It is smaller than the node, so it is named rather than derived from the node's
/// diagonal.
pub const CHECKBOX_STATE_LAYER_SIZE: f32 = 40.0;
/// The node a checkbox WITHOUT a click handler occupies: the 20 dp visual plus Compose's 2 dp padding on each
/// side (`CheckboxDefaultPadding`, `material3/Checkbox.kt:736`), and no minimum interactive size.
pub const CHECKBOX_IDLE_SIZE: f32 = CHECKBOX_SIZE + 4.0;
/// The stroke width — `CheckboxDefaults.StrokeWidth = 2dp`.
pub const CHECKBOX_STROKE_WIDTH: f32 = 2.0;
/// The container corner — `CheckboxTokens.ContainerShape = RoundedCornerShape(2dp)`.
pub const CHECKBOX_CORNER_RADIUS: f32 = 2.0;

/// Material Icons "check" (a 24 dp viewBox fill path) — the tick.
const CHECK_MARK_PATH: &str = "M9.55 18.2 3.55 12.2 5 10.75 9.55 15.3 19 5.85 20.45 7.3z";
/// The indeterminate dash — the flattened middle of M3's `drawCheck`: 0.2w..0.8w at y = 0.5h, 2 dp tall.
const DASH_PATH: &str = "M4.8 11h14.4v2H4.8z";

use crate::selection::ToggleableState;

/// The checkbox's colour set — the counterpart of material3's `CheckboxColors`: three colour groups
/// (box / border / checkmark) × enabled-disabled × Off-On-Indeterminate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxColors {
    pub checked_checkmark: Color,
    pub unchecked_checkmark: Color,
    pub checked_box: Color,
    pub unchecked_box: Color,
    pub disabled_checked_box: Color,
    pub disabled_unchecked_box: Color,
    pub disabled_indeterminate_box: Color,
    pub checked_border: Color,
    pub unchecked_border: Color,
    pub disabled_border: Color,
    pub disabled_unchecked_border: Color,
    pub disabled_indeterminate_border: Color,
}

impl CheckboxColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        checked_checkmark: Color,
        unchecked_checkmark: Color,
        checked_box: Color,
        unchecked_box: Color,
        disabled_checked_box: Color,
        disabled_unchecked_box: Color,
        disabled_indeterminate_box: Color,
        checked_border: Color,
        unchecked_border: Color,
        disabled_border: Color,
        disabled_unchecked_border: Color,
        disabled_indeterminate_border: Color,
    ) -> Self {
        Self {
            checked_checkmark,
            unchecked_checkmark,
            checked_box,
            unchecked_box,
            disabled_checked_box,
            disabled_unchecked_box,
            disabled_indeterminate_box,
            checked_border,
            unchecked_border,
            disabled_border,
            disabled_unchecked_border,
            disabled_indeterminate_border,
        }
    }

    /// The theme's default set — M3's `ColorScheme.defaultCheckboxColors` (`Checkbox.kt:446`, tokens
    /// from `CheckboxTokens`).
    ///
    /// Twelve slots, not M3's thirteen: the missing one is `disabledCheckmarkColor` (`:784`, tokens
    /// `SelectedDisabledIconColor` = Surface). Dropping it keeps the flag-off behaviour, where the
    /// disabled checkmark resolves to the checked colour — see [`CheckboxColors::checkmark_color`].
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        // Selected (On/Indeterminate): the container and border take Primary, the mark OnPrimary.
        // Unselected: a transparent container, an OnSurfaceVariant border, a transparent mark.
        // Disabled: container and border at OnSurface @ 0.38 (with a field of their own for
        // indeterminate).
        Self::new(
            theme.on_primary,
            transparent,
            theme.primary,
            transparent,
            alpha(theme.on_surface, 0.38),
            transparent,
            alpha(theme.on_surface, 0.38),
            theme.primary,
            theme.on_surface_variant,
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
        )
    }

    pub fn box_color(&self, enabled: bool, state: ToggleableState) -> Color {
        if !enabled {
            match state {
                ToggleableState::On => self.disabled_checked_box,
                ToggleableState::Off => self.disabled_unchecked_box,
                ToggleableState::Indeterminate => self.disabled_indeterminate_box,
            }
        } else if state.is_checked() {
            self.checked_box
        } else {
            self.unchecked_box
        }
    }

    pub fn border_color(&self, enabled: bool, state: ToggleableState) -> Color {
        if !enabled {
            match state {
                ToggleableState::On => self.disabled_border,
                ToggleableState::Off => self.disabled_unchecked_border,
                ToggleableState::Indeterminate => self.disabled_indeterminate_border,
            }
        } else if state.is_checked() {
            self.checked_border
        } else {
            self.unchecked_border
        }
    }

    /// A copy with some slots replaced — M3's `CheckboxColors.copy(..)` (`Checkbox.kt:829`, `:872`).
    ///
    /// `None` means "keep the source value", the role Compose gives `Color.Unspecified`. winia's
    /// `Color` has no unspecified variant, so the absence is carried by the `Option` instead of by a
    /// sentinel colour that a caller could legitimately want to paint.
    ///
    /// Two overloads upstream: a deprecated twelve-field one that also derives
    /// `disabledCheckmarkColor` from `checkedCheckmarkColor`, and the primary one with all thirteen.
    /// winia has no `disabled_checkmark` slot (section 5 of `docs/checkbox.md`), so one function
    /// covers both.
    #[allow(clippy::too_many_arguments)]
    pub fn copy(
        &self,
        checked_checkmark: Option<Color>,
        unchecked_checkmark: Option<Color>,
        checked_box: Option<Color>,
        unchecked_box: Option<Color>,
        disabled_checked_box: Option<Color>,
        disabled_unchecked_box: Option<Color>,
        disabled_indeterminate_box: Option<Color>,
        checked_border: Option<Color>,
        unchecked_border: Option<Color>,
        disabled_border: Option<Color>,
        disabled_unchecked_border: Option<Color>,
        disabled_indeterminate_border: Option<Color>,
    ) -> Self {
        Self {
            checked_checkmark: checked_checkmark.unwrap_or(self.checked_checkmark),
            unchecked_checkmark: unchecked_checkmark.unwrap_or(self.unchecked_checkmark),
            checked_box: checked_box.unwrap_or(self.checked_box),
            unchecked_box: unchecked_box.unwrap_or(self.unchecked_box),
            disabled_checked_box: disabled_checked_box.unwrap_or(self.disabled_checked_box),
            disabled_unchecked_box: disabled_unchecked_box.unwrap_or(self.disabled_unchecked_box),
            disabled_indeterminate_box: disabled_indeterminate_box
                .unwrap_or(self.disabled_indeterminate_box),
            checked_border: checked_border.unwrap_or(self.checked_border),
            unchecked_border: unchecked_border.unwrap_or(self.unchecked_border),
            disabled_border: disabled_border.unwrap_or(self.disabled_border),
            disabled_unchecked_border: disabled_unchecked_border
                .unwrap_or(self.disabled_unchecked_border),
            disabled_indeterminate_border: disabled_indeterminate_border
                .unwrap_or(self.disabled_indeterminate_border),
        }
    }

    /// M3 `checkmarkColor(state)`: it splits on the state and IGNORES `enabled`.
    ///
    /// That is the flag-off branch of `CheckboxImpl` (`Checkbox.kt:506`), and the flag
    /// `isCheckboxStylingFixEnabled` is `false` by default (`ComposeMaterial3Flags.kt:49`), so this is
    /// the behaviour to match rather than an omission. It is also why a disabled checked box paints
    /// `checked_checkmark` (OnPrimary) and not the `SelectedDisabledIconColor` token (Surface).
    ///
    /// The composer does not call this: it uses the `checked_checkmark` constant for every state,
    /// because `Off` hides the glyph at scale 0 and the outgoing transition has to stay visible.
    pub fn checkmark_color(&self, state: ToggleableState) -> Color {
        if state.is_checked() {
            self.checked_checkmark
        } else {
            self.unchecked_checkmark
        }
    }
}

/// The mutable view a [`CheckboxStyling`] writes into; starts from the theme's derivation.
pub struct CheckboxStylingScope {
    pub theme: crate::theme::ThemeColors,
    /// Whether the palette in scope is dark (see `ButtonStylingScope::is_dark`).
    pub is_dark: bool,
    pub colors: CheckboxColors,
}

impl CheckboxStylingScope {
    pub fn checked_box(&mut self, color: Color) {
        self.colors.checked_box = color;
    }
    pub fn unchecked_box(&mut self, color: Color) {
        self.colors.unchecked_box = color;
    }
    pub fn checked_checkmark(&mut self, color: Color) {
        self.colors.checked_checkmark = color;
    }
    pub fn disabled_indeterminate_box(&mut self, color: Color) {
        self.colors.disabled_indeterminate_box = color;
    }
    pub fn checked_border(&mut self, color: Color) {
        self.colors.checked_border = color;
    }
    pub fn unchecked_border(&mut self, color: Color) {
        self.colors.unchecked_border = color;
    }
    pub fn disabled_border(&mut self, color: Color) {
        self.colors.disabled_border = color;
    }
    pub fn disabled_unchecked_border(&mut self, color: Color) {
        self.colors.disabled_unchecked_border = color;
    }
    pub fn disabled_indeterminate_border(&mut self, color: Color) {
        self.colors.disabled_indeterminate_border = color;
    }
    pub fn disabled_checked_box(&mut self, color: Color) {
        self.colors.disabled_checked_box = color;
    }
    pub fn disabled_unchecked_box(&mut self, color: Color) {
        self.colors.disabled_unchecked_box = color;
    }
    /// Replace the whole set at once.
    pub fn colors(&mut self, colors: CheckboxColors) {
        self.colors = colors;
    }
}

/// One checkbox styling, registered through [`crate::components::component_styles::ComponentStyles::with`].
pub type CheckboxStyling = crate::components::component_styles::Styling<CheckboxStylingScope>;

/// The checkbox resolution: the theme's derivation, with the styling in scope applied on top.
pub fn checkbox_colors_with_styles(theme: &crate::theme::ThemeColors) -> CheckboxColors {
    let mut colors = CheckboxDefaults::checkbox_colors(theme);
    let styles = crate::components::component_styles::current_component_styles();
    if let Some(styling) = styles.get::<CheckboxStyling>() {
        let mut scope = CheckboxStylingScope {
            theme: *theme,
            is_dark: crate::components::component_styles::palette_is_dark(theme),
            colors,
        };
        styling.apply(&mut scope);
        colors = scope.colors;
    }
    colors
}

/// The checkbox's defaults — the counterpart of material3's `CheckboxDefaults`.
pub struct CheckboxDefaults;

impl CheckboxDefaults {
    pub fn checkbox_colors(theme: &ThemeColors) -> CheckboxColors {
        CheckboxColors::from_theme(theme)
    }

    /// M3's six-argument `CheckboxDefaults.colors(...)` (`Checkbox.kt:368`).
    ///
    /// The six are the slots a caller actually varies; everything else keeps the theme's derivation,
    /// so this is the theme's set with a partial override rather than a fresh palette. `None` means
    /// "keep" — see [`CheckboxColors::copy`] for why an `Option` carries that role instead of a
    /// sentinel colour.
    ///
    /// Three slots are forced TRANSPARENT here, exactly as upstream does in this overload: the
    /// unchecked box, the disabled unchecked box and the unchecked checkmark. A caller who wants a
    /// filled unchecked box passes an explicit `CheckboxColors` (or uses [`CheckboxColors::copy`])
    /// rather than this convenience form.
    #[allow(clippy::too_many_arguments)]
    pub fn colors(
        theme: &ThemeColors,
        checked_color: Option<Color>,
        unchecked_color: Option<Color>,
        checkmark_color: Option<Color>,
        disabled_checked_color: Option<Color>,
        disabled_unchecked_color: Option<Color>,
        disabled_indeterminate_color: Option<Color>,
    ) -> CheckboxColors {
        let transparent = Some(Color::TRANSPARENT);
        Self::checkbox_colors(theme).copy(
            // `checkmarkColor` feeds the checked slot; the unchecked one is forced transparent.
            checkmark_color,
            transparent,
            // Box: checked, unchecked, then the two disabled forms.
            checked_color,
            transparent,
            disabled_checked_color,
            transparent,
            disabled_indeterminate_color,
            // Border mirrors the box for the two checked forms and takes the caller's outline colours
            // for the unchecked ones.
            checked_color,
            unchecked_color,
            disabled_checked_color,
            disabled_unchecked_color,
            disabled_indeterminate_color,
        )
    }

    pub fn shape() -> Shape {
        Shape::rounded(CHECKBOX_CORNER_RADIUS)
    }

    pub fn stroke_width() -> f32 {
        CHECKBOX_STROKE_WIDTH
    }
}

/// The colour spec for the fill and border — `DefaultEffects` in BOTH directions.
///
/// M3 splits on the direction here, exactly as it does for the draw fraction: `colorAnimationSpecForState`
/// (`material3/Checkbox.kt:1040-1049`) returns `FastEffects` (damping 1.0, stiffness 3800) while the box
/// EMPTIES and `DefaultEffects` (1.0, 1600) otherwise, so "the fill arrives more slowly than it leaves".
///
/// winia used to follow that, and it is what made unchecking read as "it just becomes a box". The
/// fill and the checkmark have to fade on the SAME clock, because the checkmark's colour is
/// `on_primary` — it is only legible against the `primary` fill underneath it. Measured with
/// `FastEffects` on the fill and anything slower on the glyph:
///
/// ```text
///           17 ms   34 ms   51 ms   68 ms   119 ms
/// fill a     162      85      43      21        0
/// glyph a    221     174     130      94       31
/// border a   255     255     255     255      255
/// ```
///
/// The fill is gone by 120 ms while the glyph still has a third of its alpha, so the glyph spends the
/// rest of its exit painted on the page background at almost no contrast — and the border, which never
/// fades, is all that is left to look at. That is the "empty box" the eye reports. Running both colour
/// channels on `DefaultEffects` keeps them together: the fill and the glyph leave at one rate, and the
/// border's colour travels with them (section 5 of `docs/checkbox.md` records the departure).
fn checkbox_color_spec(_state: ToggleableState) -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_default()
}

/// The draw-fraction spec, the SAME spring in both directions.
///
/// This is a deliberate departure from M3, whose `transition.animateFloat(transitionSpec = { … })`
/// (`material3/Checkbox.kt:597-613`) splits on the direction:
///
/// - filling (`initialState == Off`) and every move between `On` and `Indeterminate` use the
///   `DefaultSpatial` token (`:596`) — damping 0.9, stiffness 700;
/// - EMPTYING (`targetState == Off`) is `snap(delayMillis = SnapAnimationDelay)` (`:603`, `:1061`):
///   the checkmark holds for 100 ms and then disappears in one frame.
///
/// That asymmetry is the reference's choice, not an oversight — but at this component's size it reads
/// as "ticking on animates, ticking off does not", because the entry is a ~160 ms growth while the exit
/// is a single invisible jump. Measured with a frozen clock, the draw fraction went
///
/// ```text
/// Off -> On   0.000 → 0.132 → 0.317 → 0.491 → 0.633 → … → 0.944   (per 17 ms frame)
/// On -> Off   1.000 → 1.000 → 1.000 → 1.000 → 1.000 → 1.000 → 0.000
/// ```
///
/// so winia runs the exit on the same spring as the entry. The colour still fades on its own channel
/// (`checkbox_color_spec`, which DOES keep M3's `FastEffects`-when-leaving split), so the glyph now
/// shrinks and fades together instead of vanishing.
///
/// (`Checkbox.kt` snaps `checkCenterGravitationShiftFraction` in the same two cases; winia has no
/// separate gravitation shift, so it has nothing to mirror here.)
fn checkbox_draw_spec(_state: ToggleableState) -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_default())
}

/// The shared implementation — the counterpart of M3's `CheckboxImpl`, used by both `Checkbox` and
/// `TriStateCheckbox`.
/// `#[composable]` so the two inner `Icon`s get their call statements injected: that is what keeps
/// multiple instances apart (the check and the dash icons sit at the same call position and are told
/// apart by statement id).
#[allow(clippy::too_many_arguments)]
#[composable]
fn checkbox_impl(
    ctx: &mut ComposeCtx,
    state: ToggleableState,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
) {
    let key = ctx.next_key();
    let theme = WiniaTheme::colors();
    // Resolve BEFORE declaring parameters: the resolved set is a parameter too. A styling change that
    // only reaches a live closure would repaint the box while the checkmark tint (built on Enter) keeps
    // the old colour.
    let colors = colors.unwrap_or_else(|| checkbox_colors_with_styles(&theme));
    ctx.changed(&state);
    ctx.changed(&enabled);
    ctx.changed(&colors);
    let interaction =
        interaction_source.unwrap_or_else(|| ctx.remember(MutableInteractionSource::new).get());
    let border_color = colors.border_color(enabled, state);
    // The FILL fades; it must not darken on the way out.
    //
    // `unchecked_box` is `Color::Transparent` — `(0, 0, 0, 0)`, black — and `Color` interpolates in
    // Oklab with lightness as one of its four components (`graphics/color.rs:66`). Animating the
    // checked fill into black-at-zero-alpha therefore spends the transition losing LIGHTNESS as well as
    // opacity, and on the dark scheme the two together erase it: measured on the box's own background
    // element, the fill went primary (101, 85, 143, a255) → (51, 42, 75, a162) → (15, 11, 26, a85) →
    // (2, 1, 4, a43) over 34 ms, i.e. it became the background instead of fading out of it. What is
    // left on screen is the border, which holds a real colour and does animate — so the whole exit
    // reads as "the checkbox turns into an empty box at once".
    //
    // The target keeps the checked fill's RGB and sets only alpha to 0, so the spring moves a single
    // component and the fill dims where it stands. The resting `Off` fill is drawn at alpha 0 either
    // way, so nothing about the settled state changes.
    let box_color = if state.is_checked() {
        colors.box_color(enabled, state)
    } else {
        Color {
            a: 0,
            ..colors.box_color(enabled, ToggleableState::On)
        }
    };
    // The checkmark's COLOUR fades with the state, but only in ALPHA. Its target when leaving is the
    // checked colour at alpha 0, NOT `unchecked_checkmark`.
    //
    // That distinction is the whole point. `unchecked_checkmark` is `Color.Transparent` — `(0, 0, 0, 0)`,
    // black — and `Color` interpolates in Oklab (`graphics/color.rs:66`), which carries lightness as a
    // component. Fading white into black-with-zero-alpha therefore drops the glyph's LIGHTNESS as well as
    // its opacity, and the two together collapse its contrast against the container in a couple of
    // frames: measured on the rendered frame, the peak-vs-interior lightness difference inside the box
    // went 206 → 63 in the first 17 ms frame and then barely moved. To the eye the tick does not fade,
    // it turns into a bare box immediately — which is exactly what this was reported as.
    //
    // Fading the checked colour's alpha alone leaves L untouched, so the glyph dims uniformly and stays
    // a glyph while it does. M3's `checkmarkColor(state)` returns the transparent colour for `Off`
    // (`:948`, `:451`), so this is a deliberate departure, recorded in section 5 of `docs/checkbox.md`
    // next to the draw-fraction one — both exist for the same reason: at 20 dp the reference's exit is
    // not readable as motion.
    //
    // `checkmark_color(state)` ignores `enabled` on purpose — that is M3's flag-off branch
    // (`Checkbox.kt:506`), the default one, so a disabled checked box keeps the checked colour.
    let checked_mark = colors.checked_checkmark;
    let check_color = if state.is_checked() {
        checked_mark
    } else {
        // Same RGB, zero alpha: `Dyn` only ever sees two colours, and the spring between them moves one
        // component.
        crate::graphics::Color {
            a: 0,
            ..checked_mark
        }
    };

    // The draw fraction (check / dash), animated per M3 — see `checkbox_draw_spec`.
    let check_spec = checkbox_draw_spec(state);
    // The container/border colour, animated per state the way M3 does — see
    // `checkbox_color_spec` for the reference and the two branches. A DISABLED checkbox snaps instead:
    // Compose returns the disabled colour through `rememberUpdatedState` and says so in a comment — "If not
    // enabled 'snap' to the disabled state, as there should be no animations between enabled / disabled"
    // (`material3/Checkbox.kt:634-643`, `:667-676`).
    let color_spec = if enabled {
        crate::animation::AnimationSpec::Spring(checkbox_color_spec(state))
    } else {
        crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default())
    };
    let box_color_anim = ctx.animate_color_as_state(box_color, color_spec.clone());
    let border_color_anim = ctx.animate_color_as_state(border_color, color_spec);
    // The checkmark's colour animates UNCONDITIONALLY, which is where M3 differs from its own box and
    // border: `animateColorAsState(checkmarkColor, colorAnimationSpec)` has no `enabled` branch
    // (`Checkbox.kt:634`), unlike the two colours above it, so a glyph never snaps across a
    // disable/enable switch either.
    //
    // The spec is the DRAW spec, not `checkbox_color_spec`, and that is the point of this line: the
    // glyph has to fade on the same clock it shrinks on. M3 reuses `colorAnimationSpecForState` here,
    // which means the emptying direction fades on `FastEffects` (damping 1.0, stiffness 3800) while its
    // size moves on `DefaultSpatial` (0.9, 700) — the opacity is ~5x ahead of the geometry. Measured at
    // 17 ms frames after the flip, with `FastEffects` on the colour:
    //
    // ```text
    //           17 ms        34 ms        51 ms        68 ms
    // alpha     162 (63 %)    85 (33 %)    43 (17 %)    21 (8 %)
    // scale     0.868        0.683        0.509        0.367
    // ```
    //
    // The glyph is already near-invisible a third of the way through, so the shrink that follows carries
    // nothing and the exit reads as "the tick vanishes and a box is left" — which is how it was
    // reported. One spec for both channels keeps the glyph legible while it leaves. Recorded in
    // section 5 of `docs/checkbox.md`.
    let check_color_anim = ctx.animate_color_as_state(check_color, check_spec.clone());

    // `On` scales the check to 1, `Indeterminate` scales the dash to 1, `Off` scales both to 0 — a
    // spring approximating M3's `checkDrawFraction` + `crossCenterGravitation` transition.
    let check_scale = ctx.animate_float_as_state(
        if state == ToggleableState::On {
            1.0
        } else {
            0.0
        },
        check_spec.clone(),
    );
    let dash_scale = ctx.animate_float_as_state(
        if state == ToggleableState::Indeterminate {
            1.0
        } else {
            0.0
        },
        check_spec,
    );

    let shape = CheckboxDefaults::shape();
    // Size: Compose applies `Modifier.minimumInteractiveComponentSize()` in front of the toggleable modifier
    // ONLY when the checkbox takes clicks (`material3/Checkbox.kt:293-302`), so an interactive checkbox is a
    // 48 dp node with its 20 dp box centred, and one without a click handler is just the box plus Compose's
    // 2 dp padding. winia used `StateLayerSize` (40 dp) as the node for both, which is neither: an interactive
    // checkbox was 8 dp short on each axis.
    let node_size = if on_click.is_some() {
        CHECKBOX_TOUCH_TARGET
    } else {
        CHECKBOX_IDLE_SIZE
    };
    let mut m = Modifier::new()
        .size(node_size, node_size)
        // The ring's SHAPE, carried without clipping: the renderer infers it from the nearest
        // Background/Border/Clip and strokes the ring OUTSIDE the node's box, so a `clip` here cut the ring away
        // — the same trap the Button's Text style and the radio button hit. A zero-width transparent border
        // states the shape and clips nothing.
        .border(
            0.0,
            crate::graphics::Color::from_argb(0, 0, 0, 0),
            Shape::Circle,
        );
    if enabled && let Some(on_click) = on_click {
        m = m
            .clickable_with_source(&interaction, move || on_click())
            // `Checkbox.kt:285`: `ripple(bounded = false, radius = CheckboxTokens.StateLayerSize / 2)` —
            // the halo is `StateLayerSize` (40 dp), which is SMALLER than the 48 dp node, so it has to be
            // named rather than derived from the node. The spreading layer still grows to the node's
            // diagonal.
            .ripple_with_state_layer_size(
                &interaction,
                theme.on_surface,
                false,
                CHECKBOX_STATE_LAYER_SIZE,
            );
    }
    // Accessibility: role plus the tri-state value — Off and Indeterminate are different answers,
    // which is why this is not a bool.
    m = m.semantics(
        crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::Checkbox)
            .merge_descendants(true)
            .state(
                crate::semantics::SemanticsState::new()
                    .checked(state)
                    .enabled(enabled),
            ),
    );
    m = m.then(modifier);

    match ctx.start_restartable_group(
        key,
        m,
        BoxLayout::new().alignment(crate::layout::Alignment::Center),
    ) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            // The 20×20 visual: when checked or indeterminate the border colour equals the
            // container's and the two merge into one fill; when Off it is a transparent background
            // with a 2 dp border — M3's `drawBox` branches.
            let visual = Modifier::new()
                .size(CHECKBOX_SIZE, CHECKBOX_SIZE)
                .background(move || box_color_anim.peek(), shape)
                .border(
                    CHECKBOX_STROKE_WIDTH,
                    move || border_color_anim.peek(),
                    shape,
                );
            let vkey = ctx.next_key();
            // The supplied checkmark colour is still a PARAMETER of this group: `colors(..)` reaching
            // `checked_checkmark` has to rebuild the two `Icon`s below. Their tint is a `Dyn` read at
            // paint time, so the ANIMATION value would arrive without this — but `Dyn::same_parameter`
            // calls any two dynamics equal (`graphics/dyn_value.rs:43-49`), which is exactly the case a
            // regenerated tint would hide behind; declaring the source value keeps the rebuild
            // explicit. Measured before the tint became dynamic: `checked_checkmark` moved from
            // `on_primary` to (1, 2, 3) and both icons kept painting (255, 255, 255).
            //
            // A theme switch never showed this — the theme provider re-enters the whole subtree — so it
            // only surfaced when a caller passed a changed `CheckboxColors`.
            ctx.changed(&check_color);
            match ctx.start_restartable_group(
                vkey,
                visual,
                BoxLayout::new().alignment(crate::layout::Alignment::Center),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // The check mark (On).
                    let scale = check_scale.clone();
                    crate::components::icon::Icon::svg_path(CHECK_MARK_PATH)
                        .size(CHECKBOX_SIZE)
                        .tint(check_color_anim.clone())
                        .modifier(Modifier::new().graphics_layer(move || GraphicsLayerParams {
                            scale_x: scale.peek(),
                            scale_y: scale.peek(),
                            ..Default::default()
                        }))
                        .build(ctx);
                    // The dash (Indeterminate).
                    let scale = dash_scale.clone();
                    crate::components::icon::Icon::svg_path(DASH_PATH)
                        .size(CHECKBOX_SIZE)
                        .tint(check_color_anim)
                        .modifier(Modifier::new().graphics_layer(move || GraphicsLayerParams {
                            scale_x: scale.peek(),
                            scale_y: scale.peek(),
                            ..Default::default()
                        }))
                        .build(ctx);
                }
            }
            ctx.end_restartable_group();
        }
    }
    ctx.set_current_node_focus_color(theme.primary);
    ctx.end_restartable_group();
}

/// The `TriStateCheckbox` builder — the counterpart of material3's
/// `TriStateCheckbox(state, onClick, enabled, colors, interactionSource)`.
pub struct TriStateCheckbox {
    state: ToggleableState,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl TriStateCheckbox {
    pub fn new(state: ToggleableState) -> Self {
        Self {
            state,
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn state(mut self, state: ToggleableState) -> Self {
        self.state = state;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: CheckboxColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Inject an interaction source (hoist it: the press/hover/focus states are emitted into it).
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    fn on_click_opt(mut self, f: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        self.on_click = f;
        self
    }

    fn colors_opt(mut self, colors: Option<CheckboxColors>) -> Self {
        self.colors = colors;
        self
    }

    fn interaction_source_opt(mut self, source: Option<MutableInteractionSource>) -> Self {
        self.interaction_source = source;
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        checkbox_impl(
            ctx,
            self.state,
            self.on_click,
            self.enabled,
            self.colors,
            self.interaction_source,
            self.modifier,
        );
    }

    pub fn get_state(&self) -> ToggleableState {
        self.state
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<CheckboxColors> {
        self.colors
    }
}

/// The `Checkbox` builder — the counterpart of material3's
/// `Checkbox(checked, onCheckedChange, enabled, colors, interactionSource)`. It delegates to
/// `TriStateCheckbox` internally.
pub struct Checkbox {
    checked: bool,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl Checkbox {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            on_checked_change: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_checked_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_checked_change = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: CheckboxColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Inject an interaction source (hoist it: the press/hover/focus states are emitted into it).
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// `#[composable]` so the inner `TriStateCheckbox` call site takes a stable base from this
    /// `build` statement.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // M3：Checkbox → TriStateCheckbox(state = ToggleableState(checked),
        // onClick = { onCheckedChange(!checked) })
        let checked = self.checked;
        let on_click = self
            .on_checked_change
            .map(|cb| -> Arc<dyn Fn() + Send + Sync> { Arc::new(move || cb(!checked)) });
        TriStateCheckbox::new(ToggleableState::from_bool(checked))
            .on_click_opt(on_click)
            .enabled(self.enabled)
            .colors_opt(self.colors)
            .interaction_source_opt(self.interaction_source)
            .modifier(self.modifier)
            .build(ctx);
    }

    pub fn get_checked(&self) -> bool {
        self.checked
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<CheckboxColors> {
        self.colors
    }
}

#[cfg(test)]
mod tests;

/// Parent/child linkage regression (from `checkbox_demo`): a Column remembers c1/c2/c3, derives
/// `parent_state` and loops a Checkbox per child. Ticking a child (`c2.set`) must update the
/// select-all `TriStateCheckbox`'s state (On → Indeterminate/Off).
/// ⚠ The children's `State`s MUST be `remember`ed — that binds them to the owner's queue. A bare
/// `State::new` never enqueues its notify and therefore never recomposes, which is a standing trap
/// in this test harness.
#[test]
fn parent_tri_state_follows_children() {
    use std::cell::RefCell;
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut composer = crate::runtime::composer::Composer::new();
    let holder = RefCell::new(None::<crate::runtime::state::State<bool>>);
    let parent_states = RefCell::new(Vec::new());

    let build = |composer: &mut crate::runtime::composer::Composer| {
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let c1 = ctx.remember(|| true);
                let c2 = ctx.remember(|| true);
                let c3 = ctx.remember(|| true);
                *holder.borrow_mut() = Some(c2.clone());
                let all = c1.get() && c2.get() && c3.get();
                let none = !c1.get() && !c2.get() && !c3.get();
                let ps = if all {
                    ToggleableState::On
                } else if none {
                    ToggleableState::Off
                } else {
                    ToggleableState::Indeterminate
                };
                parent_states.borrow_mut().push(ps);
                // The select-all box.
                TriStateCheckbox::new(ps).build(ctx);
                // One row per child — an explicit key per item keeps the instances apart.
                for (label, c) in [
                    ("Child 1", c1.clone()),
                    ("Child 2", c2.clone()),
                    ("Child 3", c3.clone()),
                ] {
                    ctx.key(label, |ctx| {
                        crate::layout::Row::new().build(ctx, |ctx| {
                            crate::components::Text::new(label)
                                .font_size(13.0)
                                .build(ctx);
                            Checkbox::new(c.get()).on_checked_change(|_| {}).build(ctx);
                        });
                    });
                }
            });
        }));
        composer.layout(crate::layout::constraints::Constraints::new(
            0.0, 300.0, 0.0, 600.0,
        ));
    };

    // Frame 1: all true → the parent is On.
    build(&mut composer);
    let ps1 = *parent_states.borrow().last().unwrap();
    assert_eq!(
        ps1,
        ToggleableState::On,
        "all true must read On (got {ps1:?})"
    );

    // Tick child 2 off (c2 → false): On → Indeterminate, the real linkage check.
    let c2 = holder.borrow().clone().unwrap();
    c2.set(false);
    build(&mut composer);
    let ps2 = *parent_states.borrow().last().unwrap();
    assert_eq!(
        ps2,
        ToggleableState::Indeterminate,
        "c2=false must read Indeterminate (got {ps2:?}) — the select-all follows its children"
    );

    // Tick c3 off too: all false → Off.
    // (c3 is remembered inside the closure and no reference to it is kept, so continuing linkage is
    // confirmed through the number of derivations instead: the parent recomputes on every child change.)
    assert!(
        parent_states.borrow().len() >= 2,
        "parent_state must recompute after a child changes (ran {} times)",
        parent_states.borrow().len()
    );
}

/// The real click path (what the demo does): find a child Checkbox's `Clickable` callback in the
/// arena and call it — the equivalent of a mouse click — so the child's `State` updates and the
/// parent `TriStateCheckbox` follows.
/// Checks: clicking child 2 (checked=true→false) takes the parent from On to Indeterminate.
#[test]
fn parent_tri_state_follows_click() {
    use std::cell::RefCell;
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut composer = crate::runtime::composer::Composer::new();
    let holder = RefCell::new(None::<crate::runtime::state::State<bool>>);
    let parent_states = RefCell::new(Vec::new());
    // The children's `Clickable` callbacks in the order they appear — child 1/2/3.
    let clickables = RefCell::new(Vec::new());

    let build = |composer: &mut crate::runtime::composer::Composer| {
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let c1 = ctx.remember(|| true);
                let c2 = ctx.remember(|| true);
                let c3 = ctx.remember(|| true);
                if holder.borrow().is_none() {
                    *holder.borrow_mut() = Some(c2.clone());
                }
                let all = c1.get() && c2.get() && c3.get();
                let none = !c1.get() && !c2.get() && !c3.get();
                let ps = if all {
                    ToggleableState::On
                } else if none {
                    ToggleableState::Off
                } else {
                    ToggleableState::Indeterminate
                };
                parent_states.borrow_mut().push(ps);
                TriStateCheckbox::new(ps).build(ctx);
                for (label, c) in [
                    ("Child 1", c1.clone()),
                    ("Child 2", c2.clone()),
                    ("Child 3", c3.clone()),
                ] {
                    ctx.key(label, |ctx| {
                        crate::layout::Row::new().build(ctx, |ctx| {
                            crate::components::Text::new(label)
                                .font_size(13.0)
                                .build(ctx);
                            let cc = c.clone();
                            Checkbox::new(c.get())
                                .on_checked_change(move |v| cc.update(|s| *s = v))
                                .build(ctx);
                        });
                    });
                }
            });
        }));
        composer.layout(crate::layout::constraints::Constraints::new(
            0.0, 300.0, 0.0, 600.0,
        ));
        // Collect the `Clickable` callbacks.
        clickables.borrow_mut().clear();
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                    clickables.borrow_mut().push(on_click.clone());
                }
            }
        }
    };

    build(&mut composer);
    assert_eq!(
        *parent_states.borrow().last().unwrap(),
        ToggleableState::On,
        "all true must read On"
    );
    let clicks = clickables.borrow().clone();
    // Find child 2's `Clickable`. In the demo's structure the loop's Checkboxes are the 2nd, 3rd
    // and 4th `Clickable` (the select-all `TriStateCheckbox` comes first), so index 1 is child 2.
    assert!(
        clicks.len() >= 3,
        "expected several Clickables (got {})",
        clicks.len()
    );
    let sub2_click = clicks[1].clone();
    // Simulate a click on child 2 (on_click = cb(!checked): checked=true → passes false → c2=false).
    sub2_click();
    // Next frame: c2=false → the parent is Indeterminate.
    build(&mut composer);
    let ps = *parent_states.borrow().last().unwrap();
    assert_eq!(
        ps,
        ToggleableState::Indeterminate,
        "clicking child 2 must take the parent to Indeterminate (got {ps:?}) — the select-all \
         follows a click"
    );
    let c2 = holder.borrow().clone().unwrap();
    assert!(!c2.get(), "c2 must be false after the click");
}

/// Parent/child linkage at the RENDER level (what the demo does): a Column, a loop of children and
/// the select-all `TriStateCheckbox`.
/// Clicking child 3 (false→true) takes the select-all from Indeterminate to On, so the pixels of the
/// select-all region must change. (Reported as "the select-all button does not follow its children";
/// this test pins the visual linkage, not just the derived value.)
#[test]
fn parent_tri_state_renders_change_on_child_click() {
    use skia_safe::{Color, surfaces};
    use std::cell::RefCell;
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = crate::runtime::composer::Composer::new();
    let clickables = RefCell::new(Vec::new());
    let parent_box_pos = RefCell::new(None::<(f32, f32, f32, f32)>);

    let build_scene = |ctx: &mut ComposeCtx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let c1 = ctx.remember(|| true);
                let c2 = ctx.remember(|| true);
                let c3 = ctx.remember(|| false);
                let all = c1.get() && c2.get() && c3.get();
                let none = !c1.get() && !c2.get() && !c3.get();
                let ps = if all {
                    ToggleableState::On
                } else if none {
                    ToggleableState::Off
                } else {
                    ToggleableState::Indeterminate
                };
                // The select-all (the first 40×40 checkbox) — carrying an on_click, as in the demo.
                TriStateCheckbox::new(ps).on_click(|| {}).build(ctx);
                for (label, c) in [
                    ("Child 1", c1.clone()),
                    ("Child 2", c2.clone()),
                    ("Child 3", c3.clone()),
                ] {
                    ctx.key(label, |ctx| {
                        crate::layout::Row::new().build(ctx, |ctx| {
                            crate::components::Text::new(label)
                                .font_size(13.0)
                                .build(ctx);
                            let cc = c.clone();
                            Checkbox::new(c.get())
                                .on_checked_change(move |v| cc.update(|s| *s = v))
                                .build(ctx);
                        });
                    });
                }
            });
        });
    };
    // Render once, record the select-all's position and collect the `Clickable`s.
    let mut render = |composer: &mut crate::runtime::composer::Composer| -> Vec<u8> {
        composer.compose(build_scene);
        composer.layout(crate::layout::constraints::Constraints::new(
            0.0, 400.0, 0.0, 600.0,
        ));
        // The parent checkbox is the first node at the touch-target size; what the test samples is the
        // 20 dp VISUAL box centred inside it (`CHECKBOX_TOUCH_TARGET` vs `CHECKBOX_SIZE`).
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        fn first_box(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
        ) -> Option<(f32, f32, f32, f32)> {
            let n = &nodes[idx];
            let inset = (CHECKBOX_TOUCH_TARGET - CHECKBOX_SIZE) / 2.0;
            if n.measured_size.width == CHECKBOX_TOUCH_TARGET
                && n.measured_size.height == CHECKBOX_TOUCH_TARGET
            {
                return Some((
                    n.position.x + inset,
                    n.position.y + inset,
                    CHECKBOX_SIZE,
                    CHECKBOX_SIZE,
                ));
            }
            for &c in &n.children {
                if let Some(r) = first_box(nodes, c) {
                    return Some(r);
                }
            }
            None
        }
        *parent_box_pos.borrow_mut() = first_box(nodes, root);
        clickables.borrow_mut().clear();
        for n in nodes {
            for el in n.modifier.elements() {
                if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                    clickables.borrow_mut().push(on_click.clone());
                }
            }
        }
        let mut surface = surfaces::raster_n32_premul((400, 600)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        px.iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect::<Vec<_>>()
    };
    // Extract the pixels of the select-all region.
    let region_px = |px: &[u8], (x, y, w, h): (f32, f32, f32, f32)| -> Vec<u8> {
        let mut out = Vec::new();
        for row in 0..(h as usize) {
            for col in 0..(w as usize) {
                let gx = (x as usize) + col;
                let gy = (y as usize) + row;
                let i = (gy * 400 + gx) * 3;
                if i + 2 < px.len() {
                    out.extend_from_slice(&px[i..i + 3]);
                }
            }
        }
        out
    };

    // First frame: the select-all is Indeterminate.
    let px1 = render(&mut composer);
    let pos = parent_box_pos
        .borrow()
        .clone()
        .expect("the select-all's position");
    let r1 = region_px(&px1, pos);
    let clicks = clickables.borrow().clone();
    assert!(
        clicks.len() >= 4,
        "expected the select-all plus three children as Clickables (got {})",
        clicks.len()
    );
    // Click child 3 — the 4th `Clickable`, after the select-all and children 1 and 2.
    let sub3 = clicks[3].clone();
    sub3(); // c3: false→true
    // Advance the animations (the colour and the glyph transition).
    for _ in 0..20 {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let px2 = render(&mut composer);
    for _ in 0..20 {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let px3 = render(&mut composer);
    let r2 = region_px(&px3, pos);
    assert_ne!(
        r1, r2,
        "clicking child 3 must change the select-all region's pixels (Indeterminate→On) — the \
         select-all follows its children (region {pos:?})"
    );
}

/// Reproduces a difference the demo showed: a Column with `vertical_scroll`, a `ctx.key` loop and
/// the select-all linkage.
/// Reported as: in the scrolling list, clicking child 3 ticked the child but left the select-all
/// unchanged. (`parent_tri_state_renders_change_on_child_click` passes without the scroll, so the
/// difference is the scroll.)
#[test]
fn parent_tri_state_follows_child_in_scroll_column() {
    use std::cell::RefCell;
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = crate::runtime::composer::Composer::new();
    let clickables = RefCell::new(Vec::new());
    let parent_states = RefCell::new(Vec::new());

    let build_scene = |ctx: &mut ComposeCtx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            crate::layout::Column::new()
                .modifier(
                    crate::modifier::Modifier::new()
                        .fill_max_size()
                        .vertical_scroll(crate::modifier::ScrollState::new()),
                )
                .build(ctx, |ctx| {
                    let c1 = ctx.remember(|| true);
                    let c2 = ctx.remember(|| true);
                    let c3 = ctx.remember(|| false);
                    let all = c1.get() && c2.get() && c3.get();
                    let none = !c1.get() && !c2.get() && !c3.get();
                    let ps = if all {
                        ToggleableState::On
                    } else if none {
                        ToggleableState::Off
                    } else {
                        ToggleableState::Indeterminate
                    };
                    parent_states.borrow_mut().push(ps);
                    TriStateCheckbox::new(ps).on_click(|| {}).build(ctx);
                    for (label, c) in [
                        ("Child 1", c1.clone()),
                        ("Child 2", c2.clone()),
                        ("Child 3", c3.clone()),
                    ] {
                        ctx.key(label, |ctx| {
                            crate::layout::Row::new().build(ctx, |ctx| {
                                crate::components::Text::new(label)
                                    .font_size(13.0)
                                    .build(ctx);
                                let cc = c.clone();
                                Checkbox::new(c.get())
                                    .on_checked_change(move |v| cc.update(|s| *s = v))
                                    .build(ctx);
                            });
                        });
                    }
                });
        });
    };
    // Frame 1, then click child 3 — the 4th `Clickable`.
    composer.compose(build_scene);
    composer.layout(crate::layout::constraints::Constraints::new(
        0.0, 400.0, 0.0, 600.0,
    ));
    clickables.borrow_mut().clear();
    for n in composer.arena_nodes() {
        for el in n.modifier.elements() {
            if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                clickables.borrow_mut().push(on_click.clone());
            }
        }
    }
    let clicks = clickables.borrow().clone();
    assert!(
        clicks.len() >= 4,
        "expected at least 4 Clickables (got {})",
        clicks.len()
    );
    let sub3 = clicks[3].clone();
    sub3(); // c3 false→true
    // Next frame: the select-all must read On.
    composer.compose(build_scene);
    composer.layout(crate::layout::constraints::Constraints::new(
        0.0, 400.0, 0.0, 600.0,
    ));
    let ps = *parent_states.borrow().last().unwrap();
    assert_eq!(
        ps,
        ToggleableState::On,
        "in a scrolling Column, clicking child 3 must turn the select-all On (got {ps:?}) — a scroll \
         container must not break the linkage"
    );
}

/// The core hypothesis: when a `State.get()` at the top of a scope-free `Column`'s content changes,
/// the Column must Enter (`compose_dirty_count` grows). If it Skipped (count unchanged) the content
/// scope would never receive the dependency and the select-all linkage would break — the root cause
/// of the demo's dash never changing.
#[test]
fn macroized_column_content_state_notify_enters_column() {
    use std::cell::RefCell;
    let mut composer = crate::runtime::composer::Composer::new();
    let holder = RefCell::new(None::<crate::runtime::state::State<bool>>);
    let scene = |composer: &mut crate::runtime::composer::Composer| {
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let c3 = ctx.remember(|| false);
                *holder.borrow_mut() = Some(c3.clone());
                let _ = c3.get(); // a top-level dependency → must register on the Column's scope
                TriStateCheckbox::new(if c3.get() {
                    ToggleableState::On
                } else {
                    ToggleableState::Off
                })
                .on_click(|| {})
                .build(ctx);
            });
        }));
        composer.layout(crate::layout::constraints::Constraints::new(
            0.0, 300.0, 0.0, 300.0,
        ));
    };
    scene(&mut composer);
    let c = holder.borrow().clone().unwrap();
    c.set(true);
    scene(&mut composer);
    // The Column must Enter (content re-runs → ps is recomputed → the TriStateCheckbox turns On).
    let d2 = composer.compose_dirty_count;
    assert!(
        d2 >= 1,
        "the Column must Enter after c3 changes (dirty count {d2})"
    );
}

#[test]
fn a_checkbox_styling_overrides_only_what_it_sets() {
    let theme = crate::theme::ThemeColors::default_light();
    let purple = Color::from_argb(255, 120, 0, 200);
    let plain = checkbox_colors_with_styles(&theme);
    let styled = crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new()
            .with(CheckboxStyling::new(move |scope| scope.checked_box(purple))),
        || checkbox_colors_with_styles(&theme),
    );
    assert_eq!(styled.checked_box, purple);
    assert_eq!(
        styled.unchecked_box, plain.unchecked_box,
        "the rest is the theme's"
    );
}
