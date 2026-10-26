use super::*;

// ═══════════════════════════════════════════════════════════
// Installing
// ═══════════════════════════════════════════════════════════

/// Hook a window's procedure so it answers `WM_GETOBJECT` with our provider.
///
/// Returns whether the hook was installed. Failure is not fatal — the window keeps drawing, it is
/// just not reachable through UIA — and the caller logs it.
pub fn install(window: &dyn winit::window::Window, window_id: u64) -> bool {
    let Some(hwnd) = hwnd_of(window) else {
        log::warn!("[accessibility] the window has no Win32 handle; UIA is unavailable for it");
        return false;
    };
    register_hwnd(window_id, hwnd);
    // SAFETY: `hwnd` belongs to the window being installed on. The ref data is an owned pointer that
    // every callback reads (the window cannot outlive the hook: `uninstall` removes it while the
    // HWND is still valid).
    let installed = unsafe {
        let data = Box::into_raw(Box::new(ProviderContext { window_id }));
        let result = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, data as usize);
        if !result.as_bool() {
            drop(Box::from_raw(data));
        }
        result.as_bool()
    };
    if !installed {
        log::warn!("[accessibility] SetWindowSubclass failed; UIA is unavailable for this window");
    }
    if std::env::var_os("WINIA_A11Y_TRACE").is_some() {
        eprintln!("[accessibility] install window_id={window_id} hwnd={:?} ok={installed}", hwnd.0);
    }
    installed
}

/// Remove the hook. Called as the window is destroyed.
pub fn uninstall(window: &dyn winit::window::Window, window_id: u64) {
    let Some(hwnd) = hwnd_of(window) else { return };
    // SAFETY: the HWND belongs to this window, and `subclass_proc` is the procedure installed with it.
    unsafe {
        let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
    }
    unregister_hwnd(window_id);
    forget_providers(window_id);
    crate::semantics::forget(window_id);
    if let Ok(mut store) = REPORTED.lock() {
        if let Some(store) = store.as_mut() {
            store.remove(&window_id);
        }
    }
}

/// What the bridge remembers about a window between frames, to tell what actually changed.
///
/// All of it is compared, not recomputed on demand: the frame loop calls [`notify`] once per rendered
/// frame, and a notification is only worth raising when the previous frame looked different.
#[derive(Default)]
struct LastReported {
    /// The tree's shape, from [`crate::semantics::WindowSemantics::structure_fingerprint`].
    structure: u64,
    /// The path of the focused element, or `None` when nothing is focused.
    focus: Option<Vec<usize>>,
    /// The FOCUSED element's announced properties, from [`announced_properties`]. Property changes are
    /// reported for that element only, which bounds them by construction: at most one element can be
    /// focused, so this can never flood a client. (Every element, every frame would: a progress bar
    /// advancing is a property change per frame, and a screen reader announcing each one is unusable —
    /// which is why Compose announces a bar's value when it is FOCUSED, not as it moves.)
    focused_properties: Vec<(UIA_PROPERTY_ID, u64)>,
    /// The live regions the tree had, as `(node id, announced text, mode)` — what the
    /// `LiveRegionChanged` notification compares against.
    live_regions: Vec<(u64, String)>,
    /// Whether a first frame has been reported: the first one RECORDS the state instead of announcing
    /// it. A client that attaches later asks `GetFocus`; announcing the startup focus would be noise,
    /// and there is nothing to compare it against.
    seen_a_frame: bool,
}

/// The properties of one element that a client is told about when they change, as
/// `(UIA property, a hash of the value)`.
///
/// The hash is what makes the comparison cheap and is deliberately lossy in the same way for every
/// kind: only equality matters here, because a difference means "re-read this property".
fn announced_properties(node: &crate::semantics::SemanticsNode) -> Vec<(UIA_PROPERTY_ID, u64)> {
    use std::hash::{Hash, Hasher};
    let mut hash = |value: &dyn std::fmt::Debug| -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{value:?}").hash(&mut hasher);
        hasher.finish()
    };

    let mut out = Vec::new();
    if let Some(name) = node.effective_name() {
        out.push((UIA_NamePropertyId, hash(&name)));
    }
    if let Some(enabled) = node.state.enabled_value() {
        out.push((UIA_IsEnabledPropertyId, hash(&enabled)));
    }
    if let Some(checked) = node.state.checked_value() {
        out.push((UIA_ToggleToggleStatePropertyId, hash(&checked)));
    }
    if let Some(selected) = node.state.selected_value() {
        out.push((UIA_SelectionItemIsSelectedPropertyId, hash(&selected)));
    }
    if let Some((current, min, max)) = node.state.progress_value() {
        out.push((UIA_RangeValueValuePropertyId, hash(&(current, min, max))));
    }
    out
}

/// Every live region in the tree, as `(node id, the text to announce)` — the presence and the text,
/// which is what decides whether a client should be told "say this".
fn live_regions(snapshot: &crate::semantics::WindowSemantics) -> Vec<(u64, String)> {
    snapshot
        .flatten()
        .into_iter()
        .filter_map(|(node, _)| {
            if node.live_region == crate::semantics::LiveRegionMode::Off {
                return None;
            }
            Some((node.node_id, node.effective_name().unwrap_or_default()))
        })
        .collect()
}

static REPORTED: std::sync::Mutex<Option<std::collections::HashMap<u64, LastReported>>> =
    std::sync::Mutex::new(None);

/// Report this frame's semantics to UIA: raise `StructureChanged` when the tree's shape changed, and
/// `AutomationFocusChanged` when the focused element moved.
///
/// Called once per rendered frame by the frame loop, right after the snapshot is published. Nothing is
/// raised unless a client is listening: `UiaClientsAreListening` is the platform's own answer to "is
/// anyone there", and without a listener the events would be built for nobody.
pub fn notify(window_id: u64, snapshot: &crate::semantics::WindowSemantics) {
    let trace = std::env::var_os("WINIA_A11Y_TRACE").is_some();
    // SAFETY: a plain query.
    let listening = unsafe { UiaClientsAreListening() }.as_bool();
    if !listening {
        // Nobody is listening: forget what was last reported so that a client attaching later sees
        // the next change as the first one (it reads the current state itself on attach).
        if let Ok(mut store) = REPORTED.lock() {
            if let Some(store) = store.as_mut() {
                store.remove(&window_id);
            }
        }
        if trace {
            eprintln!("[accessibility] notify: no client is listening");
        }
        return;
    }

    let structure = snapshot.structure_fingerprint();
    let focus = snapshot.focused_path();
    let live = live_regions(snapshot);
    // The focused element's properties, and — for the comparison below — the same list from the
    // previous frame. Only the focused element's are tracked, so this is at most a handful of entries.
    let focused_now = focus
        .as_ref()
        .and_then(|path| snapshot.resolve(path))
        .map(|(node, _)| node);
    let focused_properties = focused_now.map(announced_properties).unwrap_or_default();

    let (structure_changed, focus_moved, property_changes, live_changed) = {
        let mut store = REPORTED.lock().unwrap();
        let last = store
            .get_or_insert_with(Default::default)
            .entry(window_id)
            .or_default();
        let structure_changed = last.seen_a_frame && last.structure != structure;
        let focus_moved = last.seen_a_frame && last.focus != focus;
        // Property changes only make sense for the element that KEPT the focus: when focus moves, the
        // focus-changed event is the notification, and the properties come with a re-read.
        let property_changes: Vec<UIA_PROPERTY_ID> = if last.seen_a_frame && !focus_moved {
            last.focused_properties
                .iter()
                .filter(|(property, hash)| {
                    focused_properties
                        .iter()
                        .any(|(now_property, now_hash)| now_property == property && now_hash != hash)
                })
                .map(|(property, _)| *property)
                .collect()
        } else {
            Vec::new()
        };
        // A live region is announced when it APPEARS or when its text changes — the two cases a
        // screen reader should speak about without being asked.
        let live_changed: Vec<u64> = if !last.seen_a_frame {
            Vec::new()
        } else {
            live.iter()
                .filter(|(id, text)| {
                    match last.live_regions.iter().find(|(old_id, _)| old_id == id) {
                        None => true,
                        Some((_, old_text)) => old_text != text,
                    }
                })
                .map(|(id, _)| *id)
                .collect()
        };
        last.structure = structure;
        last.focus = focus.clone();
        last.focused_properties = focused_properties;
        last.live_regions = live.clone();
        last.seen_a_frame = true;
        (structure_changed, focus_moved, property_changes, live_changed)
    };

    if trace {
        eprintln!(
            "[accessibility] notify: listening={listening} structure_changed={structure_changed} \
             focus_moved={focus_moved} properties_changed={} live_changed={live_changed:?} focus={focus:?}",
            property_changes.len()
        );
    }

    if structure_changed {
        // The whole subtree is invalidated rather than a specific child, because this is the answer
        // for "I do not know which element moved": a client re-reads the tree.
        let root = root_provider(window_id);
        // SAFETY: UIA marshals the call; a NULL runtime id with `ChildrenInvalidated` is the documented
        // "the whole subtree is new" form.
        let _ = unsafe {
            UiaRaiseStructureChangedEvent(
                &root,
                StructureChangeType_ChildrenInvalidated,
                std::ptr::null_mut(),
                0,
            )
        };
    }

    if focus_moved {
        if let Some(path) = &focus {
            // The element itself is the source of the event: a client that receives it calls
            // `GetFocus` (or reads the element from the sender) to learn where focus went.
            let provider = cached_provider(window_id, path, || Provider {
                window_id,
                path: path.clone(),
            });
            if let Ok(simple) = provider.cast::<IRawElementProviderSimple>() {
                // SAFETY: as above.
                let result = unsafe { UiaRaiseAutomationEvent(&simple, UIA_AutomationFocusChangedEventId) };
                if trace {
                    eprintln!("[accessibility] raised focus-changed for {path:?}: {result:?}");
                }
            } else if trace {
                eprintln!("[accessibility] could not cast the provider for {path:?}");
            }
        } else {
            // Focus left every element: there is no element to raise from, so the window's root
            // answers instead — a client that asks `GetFocus` then gets nothing, which is the truth.
            let root = root_provider(window_id);
            // SAFETY: as above.
            let result = unsafe { UiaRaiseAutomationEvent(&root, UIA_AutomationFocusChangedEventId) };
            if trace {
                eprintln!("[accessibility] raised focus-changed (left everything): {result:?}");
            }
        }
    }

    // Property changes on the focused element. The old and new values are read BACK FROM THE PROVIDER,
    // not taken from the compare: that way the event never disagrees with what a client would get if
    // it asked for the property right now, and there is one place that decides what a property means.
    if let (Some(path), false) = (&focus, property_changes.is_empty()) {
        let provider = cached_provider(window_id, path, || Provider {
            window_id,
            path: path.clone(),
        });
        if let Ok(simple) = provider.cast::<IRawElementProviderSimple>() {
            for property in property_changes {
                // The old value is no longer available (the frame it belonged to is gone), so it is
                // reported as empty: UIA's own convention for "changed, previous value unknown", and
                // honest — a client re-reads the property, which is what it does anyway.
                let old = VARIANT::default();
                // Read through the Simple interface, which is what owns `GetPropertyValue`.
                // SAFETY: a COM call into this same provider; the VARIANT it returns is owned here.
                let new = unsafe { simple.GetPropertyValue(property) }.unwrap_or_default();
                // SAFETY: UIA marshals the call; both VARIANTs are owned here and copied into it.
                let result = unsafe {
                    UiaRaiseAutomationPropertyChangedEvent(&simple, property, &old, &new)
                };
                if trace {
                    eprintln!(
                        "[accessibility] raised property-changed {:?} on {path:?}: {result:?}",
                        property.0
                    );
                }
            }
        }

    }

    // Live regions: a snackbar appearing, or its text changing, is spoken without the user doing
    // anything — which is the whole point of the mode.
    for id in live_changed {
        let Some(path) = path_of_node(snapshot, id) else { continue };
        let provider = cached_provider(window_id, &path, || Provider {
            window_id,
            path: path.clone(),
        });
        if let Ok(simple) = provider.cast::<IRawElementProviderSimple>() {
            // SAFETY: as above.
            let result = unsafe { UiaRaiseAutomationEvent(&simple, UIA_LiveRegionChangedEventId) };
            if trace {
                eprintln!("[accessibility] raised live-region-changed for {path:?}: {result:?}");
            }
        }
    }
}

/// The index path of an element, addressed the way the provider tree is — what turns a node id (the
/// stable identity the notifications compare on) into the path a provider is built from.
fn path_of_node(snapshot: &crate::semantics::WindowSemantics, node_id: u64) -> Option<Vec<usize>> {
    let mut found = None;
    let mut visit = |node: &crate::semantics::SemanticsNode, path: Vec<usize>| {
        if found.is_none() && node.node_id == node_id {
            found = Some(path);
        }
    };
    for (index, (node, _)) in snapshot.top_level().into_iter().enumerate() {
        walk_paths_semantics(node, vec![index], &mut visit);
    }
    found
}

/// Depth-first walk of a semantics subtree handing each element its index path.
fn walk_paths_semantics(
    node: &crate::semantics::SemanticsNode,
    path: Vec<usize>,
    f: &mut impl FnMut(&crate::semantics::SemanticsNode, Vec<usize>),
) {
    f(node, path.clone());
    for (index, child) in node.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        walk_paths_semantics(child, child_path, f);
    }
}

fn hwnd_of(window: &dyn winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

/// Per-window data handed to the subclass procedure as `dwRefData`.
pub(crate) struct ProviderContext {
    pub(crate) window_id: u64,
}

