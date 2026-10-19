//! Per-component-class style overrides, provided once for a subtree.
//!
//! The theme answers "what does this application look like" through role colours: `primary`,
//! `secondary_container`, `surface`. A component derives its own colours from those roles, which is
//! why seeding the palette green turns every filled button green and leaves a tonal button tonal.
//!
//! That is the right default, and it is not always enough: an application sometimes wants THIS control
//! class restyled without moving the palette - every button green while the rest of the scheme stays
//! blue. [`ComponentStyles`] is that hook: one optional styling per component class, resolved where
//! the component would otherwise derive its defaults from the theme.
//!
//! Resolution order at a call site:
//!
//! 1. the value passed explicitly (`Button::colors(...)`) wins over everything;
//! 2. then the global styling for that component class, if one is in scope;
//! 3. then the variant default derived from the theme (`ButtonColors::from_theme`).
//!
//! Stylings compose with [`ButtonStyling::then`]: the receiver runs first, the argument second, so a
//! broad styling can be layered with a narrow one and the later setters win.

use std::sync::Arc;

use crate::graphics::Color;
use crate::runtime::composition_local::CompositionLocal;
use crate::theme::ThemeColors;

use crate::components::button::{ButtonColors, ButtonStyle as ButtonVariant};
use crate::components::checkbox::CheckboxColors;

/// The mutable view a [`ButtonStyling`] writes into.
///
/// It starts from what the component would have used anyway (`colors` is the variant default for the
/// current theme), so a styling that only sets one field leaves the rest exactly as the theme left
/// them. `theme` is exposed so a styling can derive from roles instead of hard-coding.
pub struct ButtonStylingScope {
    pub theme: ThemeColors,
    pub colors: ButtonColors,
}

impl ButtonStylingScope {
    /// The container colour for the resting, enabled state.
    pub fn container(&mut self, color: Color) {
        self.colors.container = color;
    }

    /// The label/icon colour for the resting, enabled state.
    pub fn content(&mut self, color: Color) {
        self.colors.content = color;
    }

    /// The disabled pair, which M3 keeps at 12%/38% of `on_surface` roles.
    pub fn disabled(&mut self, container: Color, content: Color) {
        self.colors.disabled_container = container;
        self.colors.disabled_content = content;
    }

    /// Replace the whole set at once.
    pub fn colors(&mut self, colors: ButtonColors) {
        self.colors = colors;
    }
}

/// One button styling. A function over the scope, so it can read the theme and set any subset.
#[derive(Clone)]
pub struct ButtonStyling(Arc<dyn Fn(&mut ButtonStylingScope) + Send + Sync>);

impl ButtonStyling {
    pub fn new(f: impl Fn(&mut ButtonStylingScope) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// The broad styling first, this one second: afterwards wins, field by field.
    pub fn then(self, other: ButtonStyling) -> ButtonStyling {
        ButtonStyling::new(move |scope| {
            (self.0)(scope);
            (other.0)(scope);
        })
    }

    fn apply(&self, scope: &mut ButtonStylingScope) {
        (self.0)(scope);
    }
}

/// The mutable view a [`CheckboxStyling`] writes into; same contract as the button one.
pub struct CheckboxStylingScope {
    pub theme: ThemeColors,
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
    pub fn unchecked_checkmark(&mut self, color: Color) {
        self.colors.unchecked_checkmark = color;
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

/// One checkbox styling.
#[derive(Clone)]
pub struct CheckboxStyling(Arc<dyn Fn(&mut CheckboxStylingScope) + Send + Sync>);

impl CheckboxStyling {
    pub fn new(f: impl Fn(&mut CheckboxStylingScope) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn then(self, other: CheckboxStyling) -> CheckboxStyling {
        CheckboxStyling::new(move |scope| {
            (self.0)(scope);
            (other.0)(scope);
        })
    }

    fn apply(&self, scope: &mut CheckboxStylingScope) {
        (self.0)(scope);
    }
}

/// The component-class stylings in scope. Every field is optional: `None` means "derive from the
/// theme as before", which is also the default outside any [`with_component_styles`].
#[derive(Clone, Default)]
pub struct ComponentStyles {
    button: Option<ButtonStyling>,
    checkbox: Option<CheckboxStyling>,
}

impl ComponentStyles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn button(mut self, styling: ButtonStyling) -> Self {
        self.button = Some(styling);
        self
    }

    pub fn checkbox(mut self, styling: CheckboxStyling) -> Self {
        self.checkbox = Some(styling);
        self
    }

    pub fn button_styling(&self) -> Option<&ButtonStyling> {
        self.button.as_ref()
    }

    pub fn checkbox_styling(&self) -> Option<&CheckboxStyling> {
        self.checkbox.as_ref()
    }
}

/// The value every read gets outside a provider. Allocated once: every component build that has no
/// styling in scope reads this, so allocating an empty set per read would be a per-button, per-frame
/// cost for nothing.
fn empty_styles() -> Arc<ComponentStyles> {
    static EMPTY: std::sync::OnceLock<Arc<ComponentStyles>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Arc::new(ComponentStyles::default())).clone()
}

static LOCAL_STYLES: std::sync::LazyLock<CompositionLocal<Arc<ComponentStyles>>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(empty_styles));

/// Provide `styles` to everything composed inside `content`.
///
/// Nesting works the way other locals do: an inner block can restyle one class and wrapper layers
/// keep the outer value. Nothing is invalidated by the call itself - a component reads the local when
/// it composes, so a subtree re-composes inside the new value (see the `CompositionLocal::provides`
/// caveat in `runtime/composition_local.rs`).
pub fn with_component_styles<R>(styles: ComponentStyles, content: impl FnOnce() -> R) -> R {
    LOCAL_STYLES.provides(Arc::new(styles), content)
}

/// The stylings in scope (an empty set when none were provided).
pub fn current_component_styles() -> Arc<ComponentStyles> {
    LOCAL_STYLES.current()
}

/// What a button should look like: the variant default for `theme`, with the styling in scope applied
/// on top when there is one.
///
/// Public so a wrapper component or a test can ask the same question the button asks.
pub fn button_colors_with_styles(theme: &ThemeColors, variant: ButtonVariant) -> ButtonColors {
    let mut colors = ButtonColors::from_theme(theme, variant);
    if let Some(styling) = current_component_styles().button_styling() {
        let mut scope = ButtonStylingScope { theme: theme.clone(), colors };
        styling.apply(&mut scope);
        colors = scope.colors;
    }
    colors
}

/// The checkbox equivalent of [`button_colors_with_styles`].
pub fn checkbox_colors_with_styles(theme: &ThemeColors) -> CheckboxColors {
    let mut colors = crate::components::checkbox::CheckboxDefaults::checkbox_colors(theme);
    if let Some(styling) = current_component_styles().checkbox_styling() {
        let mut scope = CheckboxStylingScope { theme: theme.clone(), colors };
        styling.apply(&mut scope);
        colors = scope.colors;
    }
    colors
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Color;

    fn green() -> Color { Color::from_argb(255, 0, 128, 0) }
    fn red() -> Color { Color::from_argb(255, 200, 0, 0) }
    fn purple() -> Color { Color::from_argb(255, 120, 0, 200) }

    fn theme() -> ThemeColors {
        ThemeColors::default_light()
    }

    /// The point of the feature: one styling, every call site, no per-call change.
    #[test]
    fn a_styling_reaches_the_resolution_every_button_uses() {
        let theme = theme();
        let plain = button_colors_with_styles(&theme, ButtonVariant::Filled);
        assert_eq!(plain.container, theme.primary, "the default still comes from the role colour");

        let styled = with_component_styles(
            ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(green()))),
            || button_colors_with_styles(&theme, ButtonVariant::Filled),
        );
        assert_eq!(styled.container, green());
        assert_eq!(styled.content, plain.content, "only what the styling set changed");
    }

    #[test]
    fn then_lets_the_later_styling_win() {
        let theme = theme();
        let broad = ButtonStyling::new(|scope| scope.container(red()));
        let narrow = ButtonStyling::new(|scope| scope.container(green()));

        let broad_then_narrow = with_component_styles(
            ComponentStyles::new().button(broad.clone().then(narrow.clone())),
            || button_colors_with_styles(&theme, ButtonVariant::Filled),
        );
        assert_eq!(broad_then_narrow.container, green(), "the argument runs after the receiver");

        let narrow_then_broad = with_component_styles(
            ComponentStyles::new().button(narrow.then(broad)),
            || button_colors_with_styles(&theme, ButtonVariant::Filled),
        );
        assert_eq!(narrow_then_broad.container, red());
    }

    #[test]
    fn a_styling_can_derive_from_the_theme_instead_of_a_constant() {
        let theme = theme();
        let resolved = with_component_styles(
            ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(scope.theme.tertiary))),
            || button_colors_with_styles(&theme, ButtonVariant::Filled),
        );
        assert_eq!(resolved.container, theme.tertiary);
    }

    #[test]
    fn the_styling_ends_with_its_block() {
        let theme = theme();
        with_component_styles(
            ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(green()))),
            || assert_eq!(button_colors_with_styles(&theme, ButtonVariant::Filled).container, green()),
        );
        assert_eq!(
            button_colors_with_styles(&theme, ButtonVariant::Filled).container,
            theme.primary,
            "outside the block the theme derivation is back"
        );
    }

    /// With no styling in scope the resolution has to be byte-identical to the old derivation.
    #[test]
    fn no_styling_means_the_old_derivation() {
        let theme = theme();
        for variant in [
            ButtonVariant::Filled,
            ButtonVariant::Elevated,
            ButtonVariant::Tonal,
            ButtonVariant::Outlined,
            ButtonVariant::Text,
        ] {
            let resolved = button_colors_with_styles(&theme, variant);
            let old = ButtonColors::from_theme(&theme, variant);
            assert_eq!(resolved.container, old.container);
            assert_eq!(resolved.content, old.content);
            assert_eq!(resolved.disabled_container, old.disabled_container);
            assert_eq!(resolved.disabled_content, old.disabled_content);
        }
    }

    #[test]
    fn a_checkbox_styling_overrides_only_what_it_sets() {
        let theme = theme();
        let plain = checkbox_colors_with_styles(&theme);
        let styled = with_component_styles(
            ComponentStyles::new().checkbox(CheckboxStyling::new(|scope| scope.checked_box(purple()))),
            || checkbox_colors_with_styles(&theme),
        );
        assert_eq!(styled.checked_box, purple());
        assert_eq!(styled.unchecked_box, plain.unchecked_box);
    }

    /// A nested provider replaces the whole set for its block (it does not merge with the outer one):
    /// the styling is already resolved per component, and merging two sets would make "remove the outer
    /// styling" impossible.
    #[test]
    fn an_inner_provider_replaces_the_set_for_its_block() {
        let theme = theme();
        let outer = ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(red())));
        let inner = ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(green())));
        with_component_styles(outer, || {
            assert_eq!(button_colors_with_styles(&theme, ButtonVariant::Filled).container, red());
            with_component_styles(inner, || {
                assert_eq!(button_colors_with_styles(&theme, ButtonVariant::Filled).container, green());
            });
            assert_eq!(
                button_colors_with_styles(&theme, ButtonVariant::Filled).container,
                red(),
                "and the outer one is back afterwards"
            );
        });
    }

    /// The stylings are visible from inside a composition, which is where a component resolves them.
    #[test]
    fn the_styling_is_in_scope_while_a_button_composes() {
        let theme = theme();
        let seen: Arc<std::sync::Mutex<Option<Color>>> = Arc::new(std::sync::Mutex::new(None));
        let out = seen.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        with_component_styles(
            ComponentStyles::new().button(ButtonStyling::new(|scope| scope.container(green()))),
            || {
                composer.compose(|ctx| {
                    *out.lock().unwrap() =
                        Some(button_colors_with_styles(&theme, ButtonVariant::Filled).container);
                    // And the real component builds through the same path without panicking.
                    crate::components::button::Button::filled()
                        .build(ctx, |ctx| crate::components::text::Text::new("ok").build(ctx));
                });
            },
        );
        assert_eq!(
            *seen.lock().unwrap(),
            Some(green()),
            "a component resolving its colours during composition sees the styling"
        );
    }
}