//! The registry for per-component-class stylings.
//!
//! This module holds the MECHANISM, not the stylings: a component owns its scope type and its
//! [`Styling`] alias in its own file, because that is where the colours it sets are defined. Keeping
//! every component's styling here would make one 50-component file, and would make this module - which
//! every component reads - depend on every component in turn.
//!
//! The registry is keyed by type, so it stays closed: `ComponentStyles::with(ButtonStyling::new(..))`
//! stores it under `TypeId` of the component's own styling type, and the component asks for it back
//! with `get::<ButtonStyling>()`. Adding a component touches that component's file and nothing else.
//!
//! Resolution order at a call site:
//!
//! 1. the value passed explicitly (`Button::colors(...)`) wins over everything;
//! 2. then the styling registered for that component class, if one is in scope;
//! 3. then the variant default derived from the theme.
//!
//! Stylings compose with [`Styling::then`]: the receiver runs first, the argument second, so a broad
//! styling can be layered with a narrow one and the later setters win.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::composition_local::CompositionLocal;

/// One component's styling: a function over that component's scope type.
///
/// A component defines its own alias, so the name at the call site is its own:
///
/// ```ignore
/// pub type ButtonStyling = crate::component_styles::Styling<ButtonStylingScope>;
/// ```
pub struct Styling<S: 'static>(Arc<dyn Fn(&mut S) + Send + Sync>);

impl<S: 'static> Clone for Styling<S> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<S: 'static> Styling<S> {
    pub fn new(f: impl Fn(&mut S) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    /// The broad styling first, this one second: afterwards wins, field by field.
    pub fn then(self, other: Styling<S>) -> Styling<S> {
        Styling::new(move |scope| {
            (self.0)(scope);
            (other.0)(scope);
        })
    }

    pub fn apply(&self, scope: &mut S) {
        (self.0)(scope);
    }
}

/// The stylings in scope, keyed by the component's own styling type.
///
/// An empty registry means "derive everything from the theme", which is the default outside any
/// [`with_component_styles`].
#[derive(Clone, Default)]
pub struct ComponentStyles {
    by_type: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ComponentStyles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `styling` for its component class, replacing any earlier one of the same type.
    pub fn with<T: Any + Send + Sync>(mut self, styling: T) -> Self {
        self.by_type.insert(TypeId::of::<T>(), Arc::new(styling));
        self
    }

    /// The styling registered for `T`, if any.
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.by_type.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    /// `self` with `other`'s entries on top: the same class from `other` replaces `self`'s, and
    /// everything else is kept. This is how an application layers a base set (brand) with a page set
    /// (one screen restyled) without copying the base.
    pub fn merge(mut self, other: ComponentStyles) -> Self {
        for (class, styling) in other.by_type {
            self.by_type.insert(class, styling);
        }
        self
    }

    /// How many component classes are styled (for tests and diagnostics).
    pub fn len(&self) -> usize {
        self.by_type.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }
}

/// The value every read gets outside a provider. Allocated once: every component build that has no
/// styling in scope reads this, so allocating an empty set per read would be a per-component,
/// per-frame cost for nothing.
fn empty_styles() -> Arc<ComponentStyles> {
    static EMPTY: std::sync::OnceLock<Arc<ComponentStyles>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Arc::new(ComponentStyles::default())).clone()
}

static LOCAL_STYLES: std::sync::LazyLock<CompositionLocal<Arc<ComponentStyles>>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(empty_styles));

/// Provide `styles` to everything composed inside `content`.
///
/// Nesting works the way other locals do: an inner block replaces the whole set for its subtree (not
/// a merge), which is what makes "drop the outer styling here" expressible.
///
/// A `CompositionLocal` does not invalidate readers that composed before it: a styling that changes at
/// runtime has to be driven by a `State` read inside the subtree.
pub fn with_component_styles<R>(styles: ComponentStyles, content: impl FnOnce() -> R) -> R {
    LOCAL_STYLES.provides(Arc::new(styles), content)
}

/// Provide `styles` and re-compose the subtree when the SET ITSELF changes.
///
/// The plain [`with_component_styles`] is a `CompositionLocal` provider, and a local does not
/// invalidate its readers: a set that changes at runtime (a brand switch, a screen selecting a
/// different look) only reaches components that recompose for some other reason. This form compares
/// the `Arc` it provided last time with the one it is given now and dirties the subtree when they
/// differ - the same remedy `WiniaTheme` applies to a theme switch.
///
/// Identity, not equality: hold the `Arc` and rebuild it only when the styling actually changes.
/// A set reconstructed every frame is a different set every frame here, and will re-compose the
/// subtree every frame.
#[crate::composable]
pub fn with_component_styles_when_changed<R>(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    styles: Arc<ComponentStyles>,
    content: impl FnOnce(&mut crate::runtime::composer::ComposeCtx) -> R,
) -> R {
    // Keyed by position, not by call site: this also runs from a per-frame wrapper where there is no
    // `#[composable]` statement to key off, and two providers in one composer must not share the
    // memory of what they last provided.
    const NAMESPACE: u64 = 0x7769_6e69_6173_7479; // "winia sty"
    let slot_key = ctx.position_key(NAMESPACE);
    // Seeded with the value being provided, so the first frame does not dirty itself for nothing.
    let previous = {
        let seed = Arc::clone(&styles);
        ctx.remember_backchannel_at_key(slot_key, move || {
            crate::runtime::state::Backchannel::new(Some(seed))
        })
    };
    let same = previous
        .get()
        .as_ref()
        .is_some_and(|provided| Arc::ptr_eq(provided, &styles));
    if !same {
        previous.set(Some(Arc::clone(&styles)));
        ctx.mark_subtree_dirty();
    }
    LOCAL_STYLES.provides(styles, move || content(ctx))
}

/// The same provider for the immutable case, without a ctx: kept for callers that build the set once
/// (an application root) and never change it.
pub fn with_component_styles_arc<R>(
    styles: Arc<ComponentStyles>,
    content: impl FnOnce() -> R,
) -> R {
    LOCAL_STYLES.provides(styles, content)
}

/// Whether the palette in scope is a DARK one, decided by the lightness of its `surface` in Oklab.
///
/// It answers "what does this palette look like", which is the question a styling has: a subtree
/// pinned to `WiniaTheme::light()` under a dark system must style as light, and the platform's
/// `isSystemInDarkTheme` would say the opposite. Deciding it from the palette also keeps working for
/// a custom scheme produced by a seed, where nothing records a mode.
pub fn palette_is_dark(colors: &crate::theme::ThemeColors) -> bool {
    // `to_oklab` returns [alpha, L, a, b] - index 1 is the lightness.
    colors.surface.to_oklab()[1] < 0.5
}

/// The stylings in scope (an empty set when none were provided).
pub fn current_component_styles() -> Arc<ComponentStyles> {
    LOCAL_STYLES.current()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u32);
    type CounterStyling = Styling<Counter>;

    #[test]
    fn a_styling_is_found_by_its_own_type_and_by_no_other() {
        struct Other;
        let styles = ComponentStyles::new().with(CounterStyling::new(|c| c.0 += 1));
        assert!(styles.get::<CounterStyling>().is_some());
        assert!(styles.get::<Other>().is_none());
        assert_eq!(styles.len(), 1);
    }

    #[test]
    fn registering_the_same_class_twice_keeps_the_later_one() {
        let styles = ComponentStyles::new()
            .with(CounterStyling::new(|c| c.0 = 1))
            .with(CounterStyling::new(|c| c.0 = 2));
        assert_eq!(styles.len(), 1);
        let mut counter = Counter(0);
        styles.get::<CounterStyling>().unwrap().apply(&mut counter);
        assert_eq!(counter.0, 2);
    }

    #[test]
    fn then_runs_the_receiver_first() {
        let mut counter = Counter(0);
        CounterStyling::new(|c| c.0 += 1)
            .then(CounterStyling::new(|c| c.0 *= 10))
            .apply(&mut counter);
        assert_eq!(counter.0, 10, "(0 + 1) * 10, not 0 * 10 + 1");
    }

    #[test]
    fn the_set_is_empty_outside_a_provider() {
        assert!(current_component_styles().is_empty());
    }

    #[test]
    fn an_inner_provider_replaces_the_set_for_its_block() {
        with_component_styles(ComponentStyles::new().with(CounterStyling::new(|c| c.0 = 1)), || {
            assert_eq!(current_component_styles().len(), 1);
            with_component_styles(ComponentStyles::new(), || {
                assert!(current_component_styles().is_empty(), "replace, not merge");
            });
            assert_eq!(current_component_styles().len(), 1, "and the outer one is back");
        });
    }

    /// `merge` keeps classes the argument does not mention, and lets it win for the ones it does.
    #[test]
    fn merge_overrides_per_class_and_keeps_the_rest() {
        struct A(u8);
        struct B(u8);
        type AStyling = Styling<A>;
        type BStyling = Styling<B>;

        let base = ComponentStyles::new()
            .with(AStyling::new(|a| a.0 = 1))
            .with(BStyling::new(|b| b.0 = 1));
        let page = ComponentStyles::new().with(AStyling::new(|a| a.0 = 2));
        let merged = base.merge(page);
        assert_eq!(merged.len(), 2, "the class the page did not mention survives");

        let mut a = A(0);
        let mut b = B(0);
        merged.get::<AStyling>().unwrap().apply(&mut a);
        merged.get::<BStyling>().unwrap().apply(&mut b);
        assert_eq!(a.0, 2, "the page wins for its class");
        assert_eq!(b.0, 1, "and the base still applies to the others");
    }

    /// The mode is decided from the palette, not from the platform: a theme pinned light under a dark
    /// system has to style as light.
    #[test]
    fn the_palette_decides_whether_it_is_dark() {
        use crate::graphics::Color;
        use crate::theme::ThemeColors;
        assert!(!palette_is_dark(&ThemeColors::default_light()));
        assert!(palette_is_dark(&ThemeColors::default_dark()));

        // Only `surface` decides: a palette whose surface is light is a light palette even if other
        // roles are dark, and the other way round.
        let light_surface = ThemeColors {
            surface: Color::from_argb(255, 255, 255, 255),
            ..ThemeColors::default_dark()
        };
        assert!(!palette_is_dark(&light_surface), "surface alone decides");
        let dark_surface = ThemeColors {
            surface: Color::from_argb(255, 10, 10, 10),
            ..ThemeColors::default_light()
        };
        assert!(palette_is_dark(&dark_surface));

        // Around the Oklab threshold (L = 0.5 is #636363): just below is dark, just above is light.
        let below = ThemeColors {
            surface: Color::from_argb(255, 0x5A, 0x5A, 0x5A),
            ..ThemeColors::default_light()
        };
        let above = ThemeColors {
            surface: Color::from_argb(255, 0x70, 0x70, 0x70),
            ..ThemeColors::default_light()
        };
        assert!(palette_is_dark(&below), "#5A5A5A is below L=0.5");
        assert!(!palette_is_dark(&above), "#707070 is above L=0.5");
    }
}
