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

    /// Decode the value saved under `key` and consume it only if it decodes.
    ///
    /// A value whose type does not match this call site is LEFT in place: consuming it first and
    /// decoding afterwards would destroy data on a mismatch, and the mismatch is exactly what a
    /// changed type at that key looks like.
    fn restore_typed<T: Saveable>(&self, key: &str) -> Option<T> {
        let raw = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.restored.get(key).cloned()
        };
        let value = T::from_saved(&raw?)?;
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).restored.remove(key);
        Some(value)
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
                // First registration wins, the same rule restore uses (the first composed call site
                // owns the key). Last-wins on save would swap two call sites' values across windows.
                out.entry(key).or_insert(value);
            }
        }
        out
    }

    fn register(self: &Arc<Self>, key: &str, provider: Provider) -> SaveableRegistration {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        debug_assert!(
            !inner.providers.iter().any(|(_, existing, _)| existing == key),
            "two composition points registered the same saveable key: {key}"
        );
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

/// Whether a registry is currently in scope (test helper: the local has no public probe).
#[cfg(test)]
pub(crate) fn registry_in_scope() -> bool {
    SAVEABLE_REGISTRY.try_current().is_some()
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
    // Decode first, consume second: a value that does not fit this call site stays for a later one.
    let restored: Option<T> = registry.as_ref().and_then(|registry| registry.restore_typed(key));
    // `remember` already wraps the value in a `State`: building one inside would nest two.
    let state: State<T> = ctx.remember(move || restored.unwrap_or_else(init));
    if let Some(registry) = registry {
        // The registration is kept in a slot so a CHANGED key can replace it. Registering on every
        // pass would churn (remember_handle only runs its init once, so the extra registration is
        // dropped again) and, worse, a key that changed after the first pass would never be the one
        // saved: the old key would keep writing the old state forever.
        type RegistrationCell = Arc<std::sync::Mutex<Option<(String, Arc<SaveableRegistration>)>>>;
        let cell: RegistrationCell = ctx.remember_handle(|| Arc::new(std::sync::Mutex::new(None)));
        let mut slot = cell.lock().unwrap_or_else(|e| e.into_inner());
        let stale = slot.as_ref().map(|(registered, _)| registered != key).unwrap_or(true);
        if stale {
            // Drop the old registration first: it holds the same key while we register the new one,
            // and a duplicate key is a debug assertion.
            *slot = None;
            let sink = state.clone();
            let provider: Provider = Arc::new(move || Some(T::to_saved(&sink.peek())));
            let registration = registry.register(key, provider);
            *slot = Some((key.to_string(), Arc::new(registration)));
        }
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
    ///
    /// The second call site does not exist on the first pass, so it can only register if the
    /// registry is provided again when it appears. (Asserting a value that survived from pass 1 would
    /// prove nothing: the slot would return it with or without a registry.)
    #[test]
    fn a_composers_registry_is_provided_on_every_pass() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        composer.set_saveable_registry(registry.clone());
        let later = Arc::new(AtomicBool::new(false));

        let flag = later.clone();
        composer.compose(move |ctx| {
            let _ = remember_saveable(ctx, "first", || 1i32);
            if flag.load(Ordering::SeqCst) {
                let _ = remember_saveable(ctx, "second", || 2i32);
            }
        });
        assert!(!registry.perform_save().contains_key("second"));

        later.store(true, Ordering::SeqCst);
        let flag = later.clone();
        composer.compose(move |ctx| {
            let _ = remember_saveable(ctx, "first", || 1i32);
            if flag.load(Ordering::SeqCst) {
                let _ = remember_saveable(ctx, "second", || 2i32);
            }
        });
        assert!(
            registry.perform_save().contains_key("second"),
            "the call site that appeared on pass 2 registered, so the registry was in scope"
        );
    }

    /// A key that changes re-registers: the old key stops saving and the new one starts.
    #[test]
    fn a_changed_key_reregisters() {
        let registry = SaveableStateRegistry::new(HashMap::new());
        let mut composer = crate::runtime::composer::Composer::new();
        composer.set_saveable_registry(registry.clone());

        composer.compose(|ctx| {
            let _ = remember_saveable(ctx, "tab.a", || 1i32);
        });
        assert!(registry.perform_save().contains_key("tab.a"));

        composer.compose(|ctx| {
            let _ = remember_saveable(ctx, "tab.b", || 1i32);
        });
        let saved = registry.perform_save();
        assert!(saved.contains_key("tab.b"), "the new key saves");
        assert!(!saved.contains_key("tab.a"), "and the old one no longer does");
    }

    /// Every scalar round-trips, and a mismatched variant is refused rather than coerced.
    #[test]
    fn every_scalar_round_trips_and_mismatches_are_refused() {
        assert_eq!(bool::from_saved(&true.to_saved()), Some(true));
        assert_eq!(f64::from_saved(&1.5f64.to_saved()), Some(1.5));
        assert_eq!(String::from_saved(&"hi".to_string().to_saved()), Some("hi".to_string()));
        assert_eq!(i32::from_saved(&(-7i32).to_saved()), Some(-7));
        assert_eq!(i64::from_saved(&(9i64).to_saved()), Some(9));
        assert_eq!(i32::from_saved(&SavedValue::Text("x".into())), None);
        assert_eq!(bool::from_saved(&SavedValue::I64(1)), None);
    }

    /// A value that does not decode is left for a call site that can use it, not destroyed.
    #[test]
    fn an_undecodable_value_is_not_consumed() {
        let mut restored = HashMap::new();
        restored.insert("k".to_string(), SavedValue::Text("not a number".into()));
        let registry = SaveableStateRegistry::new(restored);
        assert!(registry.restore_typed::<i32>("k").is_none());
        assert_eq!(
            registry.consume_restored("k"),
            Some(SavedValue::Text("not a number".into())),
            "the value is still there for a call site that can read it"
        );
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
// ── Persistence ──

/// On-disk form of the registry, for state that has to survive the process.
///
/// Everything here is defensive on purpose: a corrupted, truncated, oversized or foreign file must
/// never panic and never take the UI down. Any problem is logged and treated as "nothing saved" - the
/// application starts fresh, which is the only sane degradation for a cache-like file.
///
/// Format (JSON, `version` first so a future change can reject cleanly):
///
/// ```json
/// {"version":1,"values":{"count":{"i":42},"name":{"t":"hi"},"ratio":{"f":"400921fb54442d18"}}}
/// ```
///
/// `f64` is stored as the hex of its bits, not as a JSON number: JSON has no NaN or infinity, and
/// rounding a value the user saved would be a silent corruption. The tags also keep the format
/// self-describing, so an unknown one is skipped instead of poisoning the whole file.
pub mod persist {
    use crate::debug_log;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::SavedValue;

    /// Refuse anything larger: a saved-state file is a few kilobytes, so a multi-megabyte one is
    /// either foreign or hostile, and reading it would be a slow way to run out of memory.
    const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
    const FORMAT_VERSION: u64 = 1;

    /// Where snapshots live between runs.
    #[derive(Clone, Debug)]
    pub struct PersistStore {
        dir: PathBuf,
    }

    impl PersistStore {
        /// Use `dir` for snapshots. The directory is created on first save, not here.
        ///
        /// `dir` is the whole namespace: one per application. Window keys are derived from call
        /// sites inside that application, so two applications sharing a directory would look up
        /// each other's keys.
        pub fn open(dir: impl Into<PathBuf>) -> Self {
            Self { dir: dir.into() }
        }

        /// The platform's per-user data directory for `app`.
        ///
        /// `%LOCALAPPDATA%\<app>` on Windows, `$HOME/Library/Application Support/<app>` on macOS,
        /// `$XDG_DATA_HOME/<app>` or `$HOME/.local/share/<app>` elsewhere. The directory is NOT
        /// created here and nothing is read or written until a window closes.
        ///
        /// Returns `None` when the environment names no data directory, and for a name that is not a
        /// plain directory component (empty, a separator, `.`/`..`): a value that escaped the app's
        /// own directory would write outside it, which is not this type's decision to make. There is
        /// deliberately no temp-directory fallback: persisting into a directory that the OS clears is
        /// worse than not persisting, because it looks like it worked.
        pub fn for_app(app: &str) -> Option<Self> {
            let app = valid_app_name(app)?;
            Some(Self::open(data_root()?.join(app)))
        }

        /// Where snapshots for `app` would go, without creating or touching anything.
        pub fn for_app_path(app: &str) -> Option<PathBuf> {
            Self::for_app(app).map(|store| store.dir)
        }

        fn path_for(&self, key: u64) -> PathBuf {
            self.dir.join(format!("window-{key:016x}.json"))
        }

        /// Read the snapshot for `key`. Any failure - missing file, wrong version, unreadable, too
        /// large, corrupt - yields an empty map rather than an error: the caller cannot do anything
        /// useful with the distinction, and a fresh start is always valid.
        pub fn load(&self, key: u64) -> HashMap<String, SavedValue> {
            let path = self.path_for(key);
            let meta = match std::fs::metadata(&path) {
                Ok(meta) => meta,
                Err(_) => return HashMap::new(),
            };
            if !meta.is_file() {
                return HashMap::new();
            }
            if meta.len() > MAX_FILE_BYTES {
                debug_log!("[saveable] refusing to read {}: {} bytes", path.display(), meta.len());
                return HashMap::new();
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) => {
                    debug_log!("[saveable] unreadable {}: {e}", path.display());
                    return HashMap::new();
                }
            };
            match decode(&text) {
                Some(values) => values,
                None => {
                    debug_log!("[saveable] ignoring corrupt state in {}", path.display());
                    HashMap::new()
                }
            }
        }

        /// Write the snapshot for `key`. Errors are logged, never propagated: losing a restore point is
        /// not worth taking a window down.
        pub fn save(&self, key: u64, values: &HashMap<String, SavedValue>) {
            if let Err(e) = std::fs::create_dir_all(&self.dir) {
                debug_log!("[saveable] cannot create {}: {e}", self.dir.display());
                return;
            }
            let path = self.path_for(key);
            let body = encode(values);
            // Refuse to WRITE something the reader would refuse to read: an oversized file that is
            // silently ignored on the next run is worse than a logged skip.
            if body.len() as u64 > MAX_FILE_BYTES {
                debug_log!(
                    "[saveable] not saving {}: {} bytes exceeds the {} byte cap",
                    path.display(),
                    body.len(),
                    MAX_FILE_BYTES
                );
                return;
            }
            // Atomic replace: a crash mid-write must not leave a half file that the next run then
            // refuses - the previous snapshot stays valid instead.
            let tmp = path.with_extension("json.tmp");
            if let Err(e) = std::fs::write(&tmp, body) {
                debug_log!("[saveable] cannot write {}: {e}", tmp.display());
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                // Windows refuses to rename onto an existing file; drop the old one and retry once.
                let _ = std::fs::remove_file(&path);
                if let Err(e2) = std::fs::rename(&tmp, &path) {
                    debug_log!("[saveable] cannot replace {}: {e} / {e2}", path.display());
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }

        /// Remove a snapshot, e.g. once its window has restored it and no longer needs the file.
        pub fn forget(&self, key: u64) {
            let _ = std::fs::remove_file(self.path_for(key));
        }
    }

    /// A directory component that cannot escape its parent.
    fn valid_app_name(app: &str) -> Option<&str> {
        let trimmed = app.trim();
        if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
            return None;
        }
        if trimmed.contains(['/', '\\', '\0']) || trimmed.contains("..") {
            return None;
        }
        // Windows reserves these even as a plain component.
        const RESERVED: [&str; 6] = ["con", "prn", "aux", "nul", "com1", "lpt1"];
        let lower = trimmed.to_ascii_lowercase();
        if RESERVED.contains(&lower.as_str()) {
            return None;
        }
        Some(trimmed)
    }

    /// The platform root that per-application directories live in.
    fn data_root() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
                return Some(PathBuf::from(dir));
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join("Library").join("Application Support"));
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
                let dir = PathBuf::from(dir);
                // The spec says relative values are invalid and must be ignored.
                if dir.is_absolute() {
                    return Some(dir);
                }
            }
            if let Some(home) = std::env::var_os("HOME") {
                return Some(PathBuf::from(home).join(".local").join("share"));
            }
        }
        None
    }

    fn encode(values: &HashMap<String, SavedValue>) -> String {
        let mut map = serde_json::Map::with_capacity(values.len());
        for (key, value) in values {
            let encoded = match value {
                SavedValue::Bool(v) => Some(serde_json::json!({ "b": v })),
                SavedValue::I64(v) => Some(serde_json::json!({ "i": v })),
                SavedValue::F64(v) => Some(serde_json::json!({ "f": format!("{:016x}", v.to_bits()) })),
                SavedValue::Text(v) => Some(serde_json::json!({ "t": v })),
            };
            if let Some(encoded) = encoded {
                map.insert(key.clone(), encoded);
            }
        }
        serde_json::json!({ "version": FORMAT_VERSION, "values": map }).to_string()
    }

    fn decode(text: &str) -> Option<HashMap<String, SavedValue>> {
        let root: serde_json::Value = serde_json::from_str(text).ok()?;
        let version = root.get("version")?.as_u64()?;
        if version != FORMAT_VERSION {
            debug_log!("[saveable] snapshot version {version} is not {FORMAT_VERSION}; ignoring");
            return None;
        }
        let values = root.get("values")?.as_object()?;
        let mut out = HashMap::with_capacity(values.len());
        for (key, value) in values {
            // A single malformed entry is skipped, not fatal: the rest of the snapshot is still good.
            if let Some(decoded) = decode_value(value) {
                out.insert(key.clone(), decoded);
            }
        }
        Some(out)
    }

    fn decode_value(value: &serde_json::Value) -> Option<SavedValue> {
        let object = value.as_object()?;
        if let Some(v) = object.get("b").and_then(|v| v.as_bool()) {
            return Some(SavedValue::Bool(v));
        }
        if let Some(v) = object.get("i").and_then(|v| v.as_i64()) {
            return Some(SavedValue::I64(v));
        }
        if let Some(text) = object.get("f").and_then(|v| v.as_str()) {
            let bits = u64::from_str_radix(text, 16).ok()?;
            return Some(SavedValue::F64(f64::from_bits(bits)));
        }
        if let Some(text) = object.get("t").and_then(|v| v.as_str()) {
            return Some(SavedValue::Text(text.to_string()));
        }
        None
    }

    static STORE: std::sync::Mutex<Option<Arc<PersistStore>>> = std::sync::Mutex::new(None);

    /// Send snapshots to disk. Call before the first window opens; without it state only lives in
    /// memory for the process lifetime (the default, and the right one for tests).
    pub fn set_store(store: Option<PersistStore>) {
        *STORE.lock().unwrap_or_else(|e| e.into_inner()) = store.map(Arc::new);
    }

    /// The configured store, if any.
    pub(crate) fn store() -> Option<Arc<PersistStore>> {
        STORE.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Load a snapshot from the configured store (empty when there is none).
    pub(crate) fn load_saved(key: u64) -> Option<HashMap<String, SavedValue>> {
        store().map(|store| store.load(key))
    }

    /// Persist a snapshot to the configured store, if any.
    pub(crate) fn save_saved(key: u64, values: &HashMap<String, SavedValue>) {
        if let Some(store) = store() {
            store.save(key, values);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A scratch directory INSIDE the workspace: the test runner may be sandboxed away from the
        /// system temp directory, and a test that silently cannot write would assert on nothing.
        fn temp_dir(tag: &str) -> PathBuf {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            // Under the WORKSPACE target directory (the one cargo itself writes to), not the process
            // temp dir and not the crate dir: a test binary may be sandboxed away from both.
            let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            let dir = workspace
                .join("target")
                .join("test-tmp")
                .join(format!("saveable-{tag}-{unique}"));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        }

        /// Save and fail loudly if the file did not appear - otherwise a sandboxed or read-only
        /// environment turns every assertion below into "left: 0" with no clue why.
        fn save_or_fail(store: &PersistStore, key: u64, values: &HashMap<String, SavedValue>) {
            store.save(key, values);
            assert!(
                store.path_for(key).is_file(),
                "the snapshot was not written to {} (permissions?)",
                store.path_for(key).display()
            );
        }

        fn sample() -> HashMap<String, SavedValue> {
            let mut values = HashMap::new();
            values.insert("count".to_string(), SavedValue::I64(42));
            values.insert("name".to_string(), SavedValue::Text("hi".to_string()));
            values.insert("ratio".to_string(), SavedValue::F64(0.5));
            values.insert("flag".to_string(), SavedValue::Bool(true));
            values
        }

        #[test]
        fn a_snapshot_round_trips_through_the_file() {
            let dir = temp_dir("roundtrip");
            let store = PersistStore::open(&dir);
            save_or_fail(&store, 7, &sample());
            let mut loaded = store.load(7);
            loaded.remove("ratio");
            assert_eq!(loaded.len(), 3);
            assert_eq!(store.load(7).get("ratio"), Some(&SavedValue::F64(0.5)));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn nan_survives_exactly() {
            let dir = temp_dir("nan");
            let store = PersistStore::open(&dir);
            let mut values = HashMap::new();
            values.insert("nan".to_string(), SavedValue::F64(f64::NAN));
            values.insert("inf".to_string(), SavedValue::F64(f64::INFINITY));
            save_or_fail(&store, 1, &values);
            let loaded = store.load(1);
            match loaded.get("nan") {
                Some(SavedValue::F64(v)) => assert!(v.is_nan(), "NaN has to come back as NaN"),
                other => panic!("expected a NaN, got {other:?}"),
            }
            match loaded.get("inf") {
                Some(SavedValue::F64(v)) => assert_eq!(*v, f64::INFINITY),
                other => panic!("expected infinity, got {other:?}"),
            }
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn missing_directory_loads_empty() {
            let dir = temp_dir("missing");
            let store = PersistStore::open(&dir);
            assert!(store.load(3).is_empty());
        }

        #[test]
        fn corrupt_file_loads_empty_instead_of_panicking() {
            let dir = temp_dir("corrupt");
            let store = PersistStore::open(&dir);
            store.save(5, &sample());
            std::fs::write(store.path_for(5), "{ this is not json").unwrap();
            assert!(store.load(5).is_empty(), "a corrupt file is ignored, not fatal");

            std::fs::write(store.path_for(5), "").unwrap();
            assert!(store.load(5).is_empty(), "an empty file is ignored too");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_truncated_snapshot_is_ignored() {
            let dir = temp_dir("truncated");
            let store = PersistStore::open(&dir);
            store.save(9, &sample());
            let body = std::fs::read_to_string(store.path_for(9)).unwrap();
            std::fs::write(store.path_for(9), &body[..body.len() / 2]).unwrap();
            assert!(store.load(9).is_empty());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn an_oversized_snapshot_is_refused() {
            let dir = temp_dir("oversized");
            let store = PersistStore::open(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(store.path_for(9), vec![b'x'; (MAX_FILE_BYTES + 1) as usize]).unwrap();
            assert!(store.load(9).is_empty(), "a multi-megabyte file is not worth reading");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn an_unknown_version_is_ignored() {
            let dir = temp_dir("version");
            let store = PersistStore::open(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(store.path_for(1), r#"{"version":99,"values":{"a":{"i":1}}}"#).unwrap();
            assert!(store.load(1).is_empty(), "a future format must not be guessed at");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_malformed_entry_is_skipped_and_the_rest_survives() {
            let dir = temp_dir("partial");
            let store = PersistStore::open(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                store.path_for(1),
                r#"{"version":1,"values":{"good":{"i":3},"bad":{"zzz":1},"also":{"t":"ok"}}}"#,
            )
            .unwrap();
            let loaded = store.load(1);
            assert_eq!(loaded.get("good"), Some(&SavedValue::I64(3)));
            assert_eq!(loaded.get("also"), Some(&SavedValue::Text("ok".to_string())));
            assert!(!loaded.contains_key("bad"));
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// A value too large for the reader is not written in the first place.
        #[test]
        fn an_oversized_value_is_not_written() {
            let dir = temp_dir("bigvalue");
            let store = PersistStore::open(&dir);
            let mut values = HashMap::new();
            values.insert("huge".to_string(), SavedValue::Text("x".repeat((MAX_FILE_BYTES + 1) as usize)));
            store.save(1, &values);
            assert!(
                !store.path_for(1).exists(),
                "a snapshot the reader would refuse must not be written"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn saving_twice_replaces_the_snapshot() {
            let dir = temp_dir("overwrite");
            let store = PersistStore::open(&dir);
            let mut first = HashMap::new();
            first.insert("a".to_string(), SavedValue::I64(1));
            save_or_fail(&store, 2, &first);
            let mut second = HashMap::new();
            second.insert("b".to_string(), SavedValue::I64(2));
            save_or_fail(&store, 2, &second);
            let loaded = store.load(2);
            assert_eq!(loaded.get("b"), Some(&SavedValue::I64(2)));
            assert!(!loaded.contains_key("a"));
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// An application name is one directory component, or it is refused.
        #[test]
        fn an_app_name_cannot_escape_its_directory() {
            assert_eq!(valid_app_name("my-app"), Some("my-app"));
            assert_eq!(valid_app_name("  spaced  "), Some("spaced"));
            for bad in ["", "   ", ".", "..", "a/b", "a\\b", "../evil", "x..y", "nul"] {
                assert_eq!(valid_app_name(bad), None, "{bad:?} must be refused");
            }
        }

        /// The resolved directory sits under the platform root and under the app name.
        #[test]
        fn for_app_puts_the_app_under_the_platform_root() {
            let Some(path) = PersistStore::for_app_path("winia-demo") else {
                // No data directory in this environment: nothing to assert, and nothing to create.
                return;
            };
            assert!(path.ends_with("winia-demo"), "{}", path.display());
            if let Some(root) = data_root() {
                assert!(path.starts_with(&root), "{}", path.display());
            }
            assert!(!path.exists(), "resolving a path must not create it");
        }

        #[test]
        fn for_app_refuses_a_bad_name_without_touching_the_filesystem() {
            assert!(PersistStore::for_app("../escape").is_none());
            assert!(PersistStore::for_app("").is_none());
        }

        /// The configured store is optional: no store means no disk access at all.
        /// The configured store is process-global, so tests that change it take turns.
        static STORE_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn the_global_store_is_optional_and_round_trips() {
            let _serial = STORE_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let dir = temp_dir("global");
            set_store(Some(PersistStore::open(&dir)));
            let mut values = HashMap::new();
            values.insert("k".to_string(), SavedValue::I64(1));
            save_saved(5, &values);
            assert_eq!(
                load_saved(5).and_then(|v| v.get("k").cloned()),
                Some(SavedValue::I64(1))
            );

            set_store(None);
            assert!(
                load_saved(5).is_none(),
                "with no store configured nothing touches the disk"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_read_only_directory_does_not_panic_or_lose_the_process() {
            let dir = temp_dir("readonly");
            let store = PersistStore::open(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            // Simulate an unwritable target by replacing the destination with a directory: renaming
            // onto it fails, and the store must swallow that rather than propagate.
            std::fs::create_dir_all(store.path_for(4)).unwrap();
            store.save(4, &sample());
            assert!(store.load(4).is_empty());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}