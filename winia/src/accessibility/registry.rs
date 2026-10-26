use super::*;

// ── the HWND registry ──
//
// `HostRawElementProvider` needs the HWND to hand UIA the window's own default provider (which fills
// in everything this provider does not answer), and the provider tree is built from snapshot data
// that carries no handle.

static HWNDS: std::sync::Mutex<Option<std::collections::HashMap<u64, isize>>> =
    std::sync::Mutex::new(None);

pub(crate) fn register_hwnd(window_id: u64, hwnd: HWND) {
    let mut map = HWNDS.lock().unwrap();
    map.get_or_insert_with(Default::default)
        .insert(window_id, hwnd.0 as isize);
}

pub(crate) fn unregister_hwnd(window_id: u64) {
    if let Ok(mut map) = HWNDS.lock() {
        if let Some(map) = map.as_mut() {
            map.remove(&window_id);
        }
    }
}

/// Providers, keyed by `(window, index path)`.
///
/// UIA does not only walk a fragment tree, it also compares the providers it gets back: an element it
/// has already seen must come back as the SAME provider instance, or the walk stops after the first
/// element (measured — the whole tree collapsed to one child before this cache existed). Providers
/// therefore live here for as long as their window does; each resolves its path against the current
/// snapshot on every call, so a cached instance reports whatever the element at that path is now.
struct ProviderCache(std::collections::HashMap<(u64, Vec<usize>), IRawElementProviderFragment>);

// SAFETY: COM interface pointers are not `Send` by their type, but these are only ever touched from
// the UI thread — UIA marshals its calls onto the thread that answered `WM_GETOBJECT` — and the
// provider itself does nothing else with them. The mutex is what keeps the map itself sound.
unsafe impl Send for ProviderCache {}

static PROVIDERS: std::sync::Mutex<Option<ProviderCache>> = std::sync::Mutex::new(None);

pub(crate) fn cached_provider(
    window_id: u64,
    path: &[usize],
    make: impl FnOnce() -> Provider,
) -> IRawElementProviderFragment {
    let mut store = PROVIDERS.lock().unwrap();
    let cache = store.get_or_insert_with(|| ProviderCache(Default::default()));
    let key = (window_id, path.to_vec());
    if let Some(existing) = cache.0.get(&key) {
        return existing.clone();
    }
    let provider: IRawElementProviderFragment = make().into();
    cache.0.insert(key, provider.clone());
    provider
}

/// The root provider of a window — the one `WM_GETOBJECT` answers with, so it has to be the same
/// instance every time UIA asks.
pub(crate) fn root_provider(window_id: u64) -> IRawElementProviderSimple {
    let fragment = cached_provider(window_id, &[], || Provider::root(window_id));
    // The root is both: it is the fragment root and the simple provider UIA was asked for.
    // Every Provider implements all three interfaces, so this cast cannot fail in practice; a NULL
    // would be a better answer than a panic inside a window procedure, but there is no null interface
    // to build, so the cast is simply taken at its word.
    fragment.cast().expect("a Provider is always an IRawElementProviderSimple")
}

pub(crate) fn forget_providers(window_id: u64) {
    if let Ok(mut store) = PROVIDERS.lock() {
        if let Some(cache) = store.as_mut() {
            cache.0.retain(|(id, _), _| *id != window_id);
        }
    }
}

pub(crate) fn hwnd_for(window_id: u64) -> Option<HWND> {
    let map = HWNDS.lock().ok()?;
    let value = *map.as_ref()?.get(&window_id)?;
    Some(HWND(value as *mut core::ffi::c_void))
}

// ═══════════════════════════════════════════════════════════
// The window procedure hook
// ═══════════════════════════════════════════════════════════

/// Where the window's client area starts on screen, in physical pixels — the origin UIA's
/// BoundingRectangle is measured from, since it is in screen coordinates.
pub(crate) fn client_screen_origin(hwnd: HWND) -> (f64, f64) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    // SAFETY: the call takes the HWND we were handed and writes into a local.
    unsafe {
        let mut top_left = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut top_left).as_bool() {
            return (0.0, 0.0);
        }
        (top_left.x as f64, top_left.y as f64)
    }
}

pub(crate) unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    // WM_GETOBJECT asks for one of the window's accessibility objects. A UIA provider answers
    // `UiaRootObjectId` and nothing else — measured: answering `OBJID_CLIENT` as well (the older
    // MSAA convention) makes UIA take the provider but never walk past the first element, because
    // the client-area object is not where a raw element provider belongs. Everything else, including
    // OBJID_CLIENT, stays with DefWindowProc.
    if msg == WM_GETOBJECT && lparam.0 as i32 == UiaRootObjectId {
        // SAFETY: `ref_data` is the pointer `install` boxed for this window, and the subclass is
        // removed before it is freed.
        let context = unsafe { (ref_data as *const ProviderContext).as_ref() };
        let window_id = context.map(|context| context.window_id);
        if let Some(window_id) = window_id {
            if std::env::var_os("WINIA_A11Y_TRACE").is_some() {
                let snapshot = crate::semantics::published(window_id);
                eprintln!(
                    "[accessibility] answering WM_GETOBJECT for window {window_id}: snapshot={:?}",
                    snapshot.as_ref().map(|s| (s.main.len(), s.overlays.len()))
                );
            }
            let root = root_provider(window_id);
            // SAFETY: this is the documented answer to WM_GETOBJECT — UIA marshals the provider, and
            // the returned LRESULT is what the message must produce.
            return unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &root) };
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

