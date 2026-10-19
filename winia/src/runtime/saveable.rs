//! Saveable state: values that outlive the window that composed them.
//!
//! remember lives in a slot, and a slot dies with its composition - close the window and the slot
//! table goes with it. remember_saveable registers the value with a SaveableStateRegistry that lives
//! OUTSIDE the composition, so a snapshot can be taken before the window goes away and handed to the
//! next one.
//!
//! The registry is provided through a CompositionLocal, so restoration is explicit: wrap the new
//! composition in with_saveable_registry and every remember_saveable under it restores by key.
//!
//! What can be saved is deliberately narrow - the same reason Compose restricts it to Bundle types:
//! a value that has to be reconstructable is data, not a handle. Saveable covers the primitives
//! winia needs today (bool, i32, i64, f64) plus String.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use crate::composable;
use crate::runtime::composition_local::CompositionLocal;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;

/// One saved scalar. Deliberately a closed set: anything richer is the caller's to flatten.
#[derive(Clone, Debug, PartialEq)]
pub enum SavedValue {
    Bool(bool),
    I64(i64),
    F64(f64),
    Text(String),
}

/// A type that can round-trip through SavedValue.
///
/// Implement it for your own type when it flattens cleanly (an enum tag plus payload, a few fields
/// joined); the registry does not care how, only that the same value comes back.
pub trait Saveable: Clone + PartialEq + Send + Sync + 'static {
    fn to_saved(&self) -> SavedValue;
    fn from_saved(value: &SavedValue) -> Option<Self>;
}

macro_rules! saveable_int {
    ($ty:ty) => {
        impl Saveable for $ty {
            fn to_saved(&self) -> SavedValue {
                SavedValue::I64(*self as i64)
            }
            fn from_saved(value: &SavedValue) -> Option<Self> {
                match value {
                    SavedValue::I64(v) => <$ty>::try_from(*v).ok(),
                    _ => None,
                }
            }
        }
    };
}

saveable_int!(i32);
saveable_int!(i64);

impl Saveable for f64 {
    fn to_saved(&self) -> SavedValue { SavedValue::F64(*self) }
    fn from_saved(value: &SavedValue) -> Option<Self> {
        match value { SavedValue::F64(v) => Some(*v), _ => None }
    }
}

impl Saveable for bool {
    fn to_saved(&self) -> SavedValue { SavedValue::Bool(*self) }
    fn from_saved(value: &SavedValue) -> Option<Self> {
        match value { SavedValue::Bool(v) => Some(*v), _ => None }
    }
}

impl Saveable for String {
    fn to_saved(&self) -> SavedValue { SavedValue::Text(self.clone()) }
    fn from_saved(value: &SavedValue) -> Option<Self> {
        match value { SavedValue::Text(v) => Some(v.clone()), _ => None }
    }
}

type Provider = Arc<dyn Fn() -> Option<SavedValue> + Send + Sync>;

struct RegistryInner {
    /// Values handed over from a previous window, consumed once each.
    restored: HashMap<String, SavedValue>,
    providers: Vec<(u64, String, Provider)>,
    next_id: u64,
}

/// Where saveable values live between windows.
pub struct SaveableStateRegistry {
    inner: std::sync::Mutex<RegistryInner>,
}

impl SaveableStateRegistry {
    /// A registry that will restore `restored` (empty for a first window).
    pub fn new(restored: HashMap<String, SavedValue>) -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(RegistryInner {
                restored,
                providers: Vec::new(),
                next_id: 1,
            }),
        })
    }

    /// Take the value saved under `key`, if a previous window left one.
    ///
    /// Consumed on read: two callers with the same key cannot both restore it, which is what makes a
    /// duplicated key a visible mistake instead of a silent share.
    pub fn consume_restored(&self, key: &str) -> Option<SavedValue> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).restored.remove(key)
    }

    /// Snapshot every registered value. Call it while the composition is still alive.
    pub fn perform_save(&self) -> HashMap<String, SavedValue> {
        let providers: Vec<(u64, String, Provider)> = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.providers.clone()
        };
        let mut out = HashMap::new();
        for (_id, key, provider) in providers {
            if let Some(value) = provider() {
                out.insert(key, value);
            }
        }
        out
    }

    fn register(self: &Arc<Self>, key: &str, provider: Provider) -> SaveableRegistration {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let id = inner.next_id;
        inner.next_id += 1;
        inner.providers.push((id, key.to_string(), provider));
        SaveableRegistration { registry: Arc::downgrade(self), id }
    }

    fn unregister(&self, id: u64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .providers
            .retain(|(provider_id, _, _)| *provider_id != id);
    }
}

/// Keeps a provider registered; dropping it (the slot going away) removes the provider.
pub struct SaveableRegistration {
    registry: Weak<SaveableStateRegistry>,
    id: u64,
}

impl Drop for SaveableRegistration {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.unregister(self.id);
        }
    }
}

static SAVEABLE_REGISTRY: std::sync::LazyLock<CompositionLocal<Arc<SaveableStateRegistry>>> =
    std::sync::LazyLock::new(|| {
        CompositionLocal::new(|| panic!("no saveable registry: wrap the composition in with_saveable_registry"))
    });

/// Provide `registry` to everything composed inside `f`.
pub fn with_saveable_registry<R>(registry: Arc<SaveableStateRegistry>, f: impl FnOnce() -> R) -> R {
    SAVEABLE_REGISTRY.provides(registry, f)
}

/// The composer-side form: provide an already-held registry without cloning the Arc twice.
pub(crate) fn with_registry_of<R>(registry: &Arc<SaveableStateRegistry>, f: impl FnOnce() -> R) -> R {
    SAVEABLE_REGISTRY.provides(Arc::clone(registry), f)
}

/// A State that a later window can pick up again, keyed by `key`.
///
/// Without a registry in scope this is plain remember: the value still lives in the slot, it just
/// never leaves it. That keeps the call site identical in a window that does not care about
/// restoration (tests, previews).
#[composable]
pub fn remember_saveable<T: Saveable>(
    ctx: &mut ComposeCtx,
    key: &str,
    init: impl FnOnce() -> T,
) -> State<T> {
    let registry = SAVEABLE_REGISTRY.try_current();
    let restored = registry
        .as_ref()
        .and_then(|registry| registry.consume_restored(key))
        .and_then(|value| T::from_saved(&value));
    // `remember` already wraps the value in a `State`: building one inside would nest two.
    let state: State<T> = ctx.remember(move || restored.unwrap_or_else(init));
    if let Some(registry) = registry {
        let sink = state.clone();
        let provider: Provider = Arc::new(move || Some(T::to_saved(&sink.peek())));
        let registration = registry.register(key, provider);
        // The registration lives in the slot: dropping it (the composition point going away) removes
        // the provider, so a dead composition cannot keep feeding the next window.
        ctx.remember_handle(move || Arc::new(registration));
    }
    state
}
#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point: a value saved by one composition comes back in the next one.
    ///
    /// The composer is kept alive across the save: a slot that is dropped takes its registration
    /// with it, which is the behaviour the third test pins.
    #[test]
    fn a_saved_value_comes_back_in_the_next_composition() {
        let first = SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen = None;
        with_saveable_registry(first.clone(), || {
            composer.compose(|ctx| seen = Some(remember_saveable(ctx, "count", || 0i32)));
        });
        let state: State<i32> = seen.expect("composed");
        assert_eq!(state.peek(), 0, "nothing to restore yet, so the init runs");

        state.set(42);
        let saved = first.perform_save();
        assert_eq!(saved.get("count"), Some(&SavedValue::I64(42)));

        let second = SaveableStateRegistry::new(saved);
        let mut next = crate::runtime::composer::Composer::new();
        let mut restored = None;
        with_saveable_registry(second, || {
            next.compose(|ctx| restored = Some(remember_saveable(ctx, "count", || 0i32)));
        });
        assert_eq!(
            restored.expect("composed").peek(),
            42,
            "the next composition starts from what the last one saved"
        );
    }

    /// A restored value is consumed once: a duplicated key is a mistake, not a silent share.
    #[test]
    fn a_restored_value_is_consumed_once() {
        let mut restored = HashMap::new();
        restored.insert("dup".to_string(), SavedValue::I64(7));
        let registry = SaveableStateRegistry::new(restored);
        assert_eq!(registry.consume_restored("dup"), Some(SavedValue::I64(7)));
        assert_eq!(registry.consume_restored("dup"), None);
    }

    /// With no registry in scope it degrades to remember rather than panicking.
    #[test]
    fn without_a_registry_it_is_plain_remember() {
        let mut composer = crate::runtime::composer::Composer::new();
        let mut out = None;
        composer.compose(|ctx| out = Some(remember_saveable(ctx, "count", || 5i32)));
        let state: State<i32> = out.unwrap();
        assert_eq!(state.peek(), 5);
        state.set(6);
        assert_eq!(state.peek(), 6);
    }

    /// A registry attached to a composer is in scope on every pass, not just the first.
    #[test]
    fn a_composers_registry_is_provided_on_every_pass() {
        let registry = SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        composer.set_saveable_registry(registry.clone());

        let mut seen = None;
        composer.compose(|ctx| seen = Some(remember_saveable(ctx, "k", || 3i32)));
        let state: State<i32> = seen.expect("composed");
        state.set(9);

        // A later pass re-enters the same scope with the registry still in scope.
        let mut again = None;
        composer.compose(|ctx| again = Some(remember_saveable(ctx, "k", || 0i32)));
        assert_eq!(again.expect("composed").peek(), 9, "the same slot, registry or not");

        assert_eq!(registry.perform_save().get("k"), Some(&SavedValue::I64(9)));
    }

    /// A composition point that goes away takes its provider with it.
    #[test]
    fn a_removed_composition_point_stops_saving() {
        let registry = SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        with_saveable_registry(registry.clone(), || {
            composer.compose(|ctx| {
                let _ = remember_saveable(ctx, "gone", || 1i32);
            });
        });
        assert_eq!(registry.perform_save().len(), 1);

        with_saveable_registry(registry.clone(), || {
            composer.compose(|_ctx| {});
        });
        assert!(
            registry.perform_save().is_empty(),
            "the slot took the registration with it"
        );
    }
}