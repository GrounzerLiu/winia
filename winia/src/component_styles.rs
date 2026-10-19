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
}
