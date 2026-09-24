//! Windows UI Automation bridge: the semantics tree, published to the OS.
//!
//! A UIA client (Narrator, Inspect.exe, `AutomationElement` in a test) reaches an application through
//! its HWND: the client sends `WM_GETOBJECT`, the window procedure answers with
//! `UiaReturnRawElementProvider`, and everything after that is COM calls into the returned provider.
//! winit does not forward `WM_GETOBJECT`, so the window procedure is hooked with `SetWindowSubclass`.
//! The hook does exactly one thing: hand UIA the provider.
//!
//! The provider reads `crate::semantics`' published snapshot, which the frame loop fills once per
//! frame — including the window's screen origin, its size and the scale it draws at, so the provider
//! never has to ask the window anything. It never touches the composer: UIA marshals provider calls
//! onto the UI thread (they come through the window's message queue), so building a tree or waiting
//! for a frame inside a provider call would deadlock the render loop. A query answers with the last
//! frame's declarations, and the next query sees the next frame.
//!
//! Actions go the other way, through a queue the frame loop drains
//! (`semantics::request_action` / `take_actions`): a provider never runs application code at a moment
//! the application does not control.
//!
//! Role mapping — see `docs/semantics.md` for the reasoning:
//!
//! | semantics | UIA control type | pattern |
//! |---|---|---|
//! | Button | Button | Invoke |
//! | Checkbox, Switch | CheckBox (UIA has no switch control type) | Toggle |
//! | RadioButton | RadioButton | SelectionItem |
//! | Tab | TabItem | SelectionItem |
//! | Image | Image | — |
//! | ProgressBar | ProgressBar | — |
//! | Dialog | Pane | — |
//! | *(no role, but named)* | Text | — |
//! | the window itself | Window | — |

use crate::semantics::{SemanticsNode, SemanticsRole, WindowSemantics};
use crate::ui::checkbox::ToggleableState;
use std::sync::Arc;
use windows::core::{implement, Interface, IUnknown, Result as WinResult};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::{
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VARIANT, VT_BOOL, VT_BSTR, VT_I4,
};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IInvokeProvider_Impl, IRawElementProviderFragment,
    IRawElementProviderFragment_Impl, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderSimple, IRawElementProviderSimple_Impl,
    NavigateDirection, NavigateDirection_FirstChild, NavigateDirection_LastChild,
    NavigateDirection_NextSibling, NavigateDirection_Parent, NavigateDirection_PreviousSibling,
    ProviderOptions, ProviderOptions_ServerSideProvider, UIA_AutomationIdPropertyId,
    UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
    UIA_ClassNamePropertyId, UIA_CONTROLTYPE_ID, UIA_ControlTypePropertyId, UIA_FrameworkIdPropertyId,
    UIA_HasKeyboardFocusPropertyId, UIA_ImageControlTypeId, UIA_InvokePatternId,
    UIA_IsContentElementPropertyId, UIA_IsControlElementPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_PATTERN_ID,
    UIA_PaneControlTypeId, UIA_ProgressBarControlTypeId, UIA_PROPERTY_ID,
    IRangeValueProvider, IRangeValueProvider_Impl, ISelectionItemProvider,
    ISelectionItemProvider_Impl, IToggleProvider, IToggleProvider_Impl,
    ToggleState_Indeterminate, ToggleState_Off, ToggleState_On,
    UIA_IsRangeValuePatternAvailablePropertyId, UIA_RangeValueMaximumPropertyId,
    UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId, UIA_RangeValueValuePropertyId,
    UIA_RadioButtonControlTypeId, UIA_SelectionItemIsSelectedPropertyId, UIA_SelectionItemPatternId,
    UIA_TabItemControlTypeId, UIA_TextControlTypeId, UIA_TogglePatternId,
    UIA_ToggleToggleStatePropertyId, UIA_WindowControlTypeId, UiaGetReservedNotSupportedValue,
    UiaRootObjectId,
    UiaHostProviderFromHwnd, UiaRect, UiaReturnRawElementProvider,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::WM_GETOBJECT;

/// Our subclass id — arbitrary, but it must be unique within the window procedure.
const SUBCLASS_ID: usize = 0x776E_6961; // "wnia"

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
}

fn hwnd_of(window: &dyn winit::window::Window) -> Option<HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

/// Per-window data handed to the subclass procedure as `dwRefData`.
struct ProviderContext {
    window_id: u64,
}

// ── the HWND registry ──
//
// `HostRawElementProvider` needs the HWND to hand UIA the window's own default provider (which fills
// in everything this provider does not answer), and the provider tree is built from snapshot data
// that carries no handle.

static HWNDS: std::sync::Mutex<Option<std::collections::HashMap<u64, isize>>> =
    std::sync::Mutex::new(None);

fn register_hwnd(window_id: u64, hwnd: HWND) {
    let mut map = HWNDS.lock().unwrap();
    map.get_or_insert_with(Default::default)
        .insert(window_id, hwnd.0 as isize);
}

fn unregister_hwnd(window_id: u64) {
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

fn cached_provider(
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
fn root_provider(window_id: u64) -> IRawElementProviderSimple {
    let fragment = cached_provider(window_id, &[], || Provider::root(window_id));
    // The root is both: it is the fragment root and the simple provider UIA was asked for.
    // Every Provider implements all three interfaces, so this cast cannot fail in practice; a NULL
    // would be a better answer than a panic inside a window procedure, but there is no null interface
    // to build, so the cast is simply taken at its word.
    fragment.cast().expect("a Provider is always an IRawElementProviderSimple")
}

fn forget_providers(window_id: u64) {
    if let Ok(mut store) = PROVIDERS.lock() {
        if let Some(cache) = store.as_mut() {
            cache.0.retain(|(id, _), _| *id != window_id);
        }
    }
}

fn hwnd_for(window_id: u64) -> Option<HWND> {
    let map = HWNDS.lock().ok()?;
    let value = *map.as_ref()?.get(&window_id)?;
    Some(HWND(value as *mut core::ffi::c_void))
}

// ═══════════════════════════════════════════════════════════
// The window procedure hook
// ═══════════════════════════════════════════════════════════

/// Where the window's client area starts on screen, in physical pixels — the origin UIA's
/// BoundingRectangle is measured from, since it is in screen coordinates.
fn client_screen_origin(hwnd: HWND) -> (f64, f64) {
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

unsafe extern "system" fn subclass_proc(
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

// ═══════════════════════════════════════════════════════════
// The provider
// ═══════════════════════════════════════════════════════════

/// The window is the fragment root; `path` addresses an element inside it: `[]` is the window, `[2]`
/// the third top-level element, `[2, 0]` that element's first child (`WindowSemantics::resolve`).
///
/// Providers are cheap and are created per UIA call — navigation returns a fresh instance rather than
/// maintaining an object graph. Identity is the RuntimeId, derived from the element's own `node_id`,
/// so a client sees a stable element even though the COM pointer differs.
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct Provider {
    window_id: u64,
    path: Vec<usize>,
}

impl Provider {
    fn root(window_id: u64) -> Self {
        Self {
            window_id,
            path: Vec::new(),
        }
    }



    /// The snapshot this provider reads — `None` before the window's first frame, which a client has
    /// to tolerate (it can attach while the app is still starting up).
    fn snapshot(&self) -> Option<Arc<WindowSemantics>> {
        crate::semantics::published(self.window_id)
    }

    /// The element this provider stands for; `None` for the window root.
    fn element(&self) -> Option<ElementData> {
        let snapshot = self.snapshot()?;
        let (node, _) = snapshot.resolve(&self.path)?;
        Some(ElementData {
            node_id: node.node_id,
            role: node.role,
            name: node.effective_name(),
            state: node.state,
            clickable: node.clickable,
            focused: node.focused,
        })
    }

    /// Where the window's client area starts on screen, in physical pixels — asked of the OS each
    /// time, because the user can move the window between one query and the next.
    fn screen_origin(&self) -> (f64, f64) {
        match hwnd_for(self.window_id) {
            Some(hwnd) => client_screen_origin(hwnd),
            None => (0.0, 0.0),
        }
    }

    /// This element's children (the window's top-level elements when it is the root), as the
    /// providers UIA gets back — cached, so the same element keeps the same instance.
    fn child_providers(&self) -> Vec<IRawElementProviderFragment> {
        let Some(snapshot) = self.snapshot() else {
            return Vec::new();
        };
        let count = snapshot.children_at(&self.path).len();
        (0..count)
            .map(|index| {
                let mut path = self.path.clone();
                path.push(index);
                let window_id = self.window_id;
                let make_path = path.clone();
                cached_provider(window_id, &path, move || Provider {
                    window_id,
                    path: make_path,
                })
            })
            .collect()
    }

    /// `(x, y, width, height)` in physical screen pixels: the element's logical bounds, scaled and
    /// offset by its overlay's origin and then by the window's own screen origin — all of which the
    /// snapshot carries, so nothing here asks the OS.
    fn bounds(&self) -> (f64, f64, f64, f64) {
        let Some(snapshot) = self.snapshot() else {
            return (0.0, 0.0, 0.0, 0.0);
        };
        let scale = snapshot.scale_factor as f64;
        let origin = self.screen_origin();
        match snapshot.resolve(&self.path) {
            Some((node, overlay)) => {
                let (x, y, w, h) = node.bounds;
                (
                    origin.0 as f64 + (overlay.0 + x) as f64 * scale,
                    origin.1 as f64 + (overlay.1 + y) as f64 * scale,
                    w as f64 * scale,
                    h as f64 * scale,
                )
            }
            // The window's own rectangle is its client area.
            None => (
                origin.0 as f64,
                origin.1 as f64,
                snapshot.window_size.0 as f64 * scale,
                snapshot.window_size.1 as f64 * scale,
            ),
        }
    }
}

/// One element of the snapshot, copied out of it.
///
/// Copied rather than borrowed because the tree lives behind an `Arc` that a method cannot hand back
/// alongside a reference into it — and the values are small: an id, a name, and a `Copy` state.
struct ElementData {
    node_id: u64,
    role: Option<SemanticsRole>,
    name: Option<String>,
    state: crate::semantics::SemanticsState,
    clickable: bool,
    focused: bool,
}

fn control_type_for(role: Option<SemanticsRole>) -> UIA_CONTROLTYPE_ID {
    match role {
        Some(SemanticsRole::Button) => UIA_ButtonControlTypeId,
        // UIA has no switch control type; a switch IS a toggleable control, which CheckBox is here.
        Some(SemanticsRole::Checkbox) | Some(SemanticsRole::Switch) => UIA_CheckBoxControlTypeId,
        Some(SemanticsRole::RadioButton) => UIA_RadioButtonControlTypeId,
        Some(SemanticsRole::Tab) => UIA_TabItemControlTypeId,
        Some(SemanticsRole::Image) => UIA_ImageControlTypeId,
        Some(SemanticsRole::ProgressBar) => UIA_ProgressBarControlTypeId,
        Some(SemanticsRole::Dialog) => UIA_PaneControlTypeId,
        // A named element with no role is text — the shape `Text` composes.
        None => UIA_TextControlTypeId,
    }
}

/// Whether an element offers a pattern.
///
/// An element can offer several at once, and the ones it offers follow from what it can do: a click
/// target offers Invoke, a checked control offers Toggle, a selected one offers SelectionItem. A
/// checkbox is therefore Invoke AND Toggle — `IsInvokePatternAvailable` and the Toggle state are both
/// true of it, and a client may use either. (An earlier version returned a single pattern, which made
/// an enabled checkbox offer only Invoke and left a client looking for Toggle with nothing, even
/// though the state was right there.)
fn supports_pattern(element: &ElementData, pattern_id: UIA_PATTERN_ID) -> bool {
    if pattern_id == UIA_InvokePatternId {
        return element.clickable;
    }
    if pattern_id == UIA_TogglePatternId {
        return element.state.checked_value().is_some();
    }
    if pattern_id == UIA_SelectionItemPatternId {
        return element.state.selected_value().is_some();
    }
    if pattern_id == UIA_RangeValuePatternId {
        return element.state.progress_value().is_some();
    }
    false
}

/// The Toggle pattern: `Toggle()` presses a checkbox or switch, and `ToggleState()` reports it.
///
/// winia has one activation path — a click — so this queues the same `UiAction::Invoke` the Invoke
/// pattern does. The element's own handler decides what the new value is (a tri-state checkbox cycles
/// through Indeterminate), which is why the provider does not compute it.
#[implement(IToggleProvider)]
struct TogglePattern {
    window_id: u64,
    node_id: u64,
    state: ToggleableState,
}

impl IToggleProvider_Impl for TogglePattern_Impl {
    fn Toggle(&self) -> WinResult<()> {
        crate::semantics::request_action(
            self.window_id,
            crate::semantics::UiAction::Invoke(self.node_id),
        );
        Ok(())
    }

    fn ToggleState(&self) -> WinResult<windows::Win32::UI::Accessibility::ToggleState> {
        Ok(match self.state {
            ToggleableState::On => ToggleState_On,
            ToggleableState::Off => ToggleState_Off,
            ToggleableState::Indeterminate => ToggleState_Indeterminate,
        })
    }
}

/// The SelectionItem pattern: `Select()` picks a radio, tab or segment; `IsSelected()` reports it.
///
/// `AddToSelection` and `RemoveFromSelection` are refused: winia's selectable controls are
/// single-choice (a radio group, a tab row), so "add to the selection" has no meaning for them —
/// reporting that honestly is better than pretending the click did something it did not.
#[implement(ISelectionItemProvider)]
struct SelectionItemPattern {
    window_id: u64,
    node_id: u64,
    selected: bool,
}

impl ISelectionItemProvider_Impl for SelectionItemPattern_Impl {
    fn Select(&self) -> WinResult<()> {
        crate::semantics::request_action(
            self.window_id,
            crate::semantics::UiAction::Invoke(self.node_id),
        );
        Ok(())
    }

    fn AddToSelection(&self) -> WinResult<()> {
        // Single-choice controls have no "add to the selection"; saying so is the honest answer, and it
        // must be an ERROR — S_OK here would tell the client the click happened.
        Err(not_implemented())
    }

    fn RemoveFromSelection(&self) -> WinResult<()> {
        Err(not_implemented())
    }

    fn IsSelected(&self) -> WinResult<windows::core::BOOL> {
        Ok(windows::core::BOOL::from(self.selected))
    }

    fn SelectionContainer(&self) -> WinResult<IRawElementProviderSimple> {
        // The container (a radio group, a tab row) is not modelled as an element of its own, so there is
        // nothing to name; a client falls back to walking up the fragment tree.
        Err(not_implemented())
    }
}

/// The RangeValue pattern: what makes a progress bar useful to a screen reader ("50 percent")
/// instead of merely present ("progress bar").
///
/// Read-only on purpose: a progress bar reports progress, it does not accept a value. UIA expresses
/// that with `IsReadOnly`, and `SetValue` answers `UIA_E_NOTSUPPORTED` rather than pretending to
/// accept one — the same rule as every other refusal in this file.
#[implement(IRangeValueProvider)]
struct RangeValuePattern {
    value: f64,
    min: f64,
    max: f64,
}

impl IRangeValueProvider_Impl for RangeValuePattern_Impl {
    fn SetValue(&self, _val: f64) -> WinResult<()> {
        Err(not_implemented())
    }

    fn Value(&self) -> WinResult<f64> {
        Ok(self.value)
    }

    fn IsReadOnly(&self) -> WinResult<windows::core::BOOL> {
        Ok(windows::core::BOOL::from(true))
    }

    fn Maximum(&self) -> WinResult<f64> {
        Ok(self.max)
    }

    fn Minimum(&self) -> WinResult<f64> {
        Ok(self.min)
    }

    fn LargeChange(&self) -> WinResult<f64> {
        // UIA wants the step sizes; a read-only bar has none. Zero is the documented "no meaningful
        // change" answer, and clients that care check `IsReadOnly` first.
        Ok(0.0)
    }

    fn SmallChange(&self) -> WinResult<f64> {
        Ok(0.0)
    }
}

/// The Invoke pattern: what a client calls to "press" an element.
///
/// The action is queued for the frame loop rather than run here (module header), and it names the same
/// node the snapshot does, so it reaches the same callback a real click does.
#[implement(IInvokeProvider)]
struct InvokePattern {
    window_id: u64,
    node_id: u64,
}

impl IInvokeProvider_Impl for InvokePattern_Impl {
    fn Invoke(&self) -> WinResult<()> {
        crate::semantics::request_action(
            self.window_id,
            crate::semantics::UiAction::Invoke(self.node_id),
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// VARIANT helpers
//
// windows-rs 0.62 generates VARIANT as the raw `#[repr(C)]` union, so scalar variants are built by
// hand; the propsys helpers cover the SAFEARRAY cases (BoundingRectangle, RuntimeId).
// ═══════════════════════════════════════════════════════════

/// The two union levels inside a `VARIANT`, as windows-rs spells them.
type VariantInner = windows::Win32::System::Variant::VARIANT_0_0;
type VariantUnion = windows::Win32::System::Variant::VARIANT_0;

/// Build a VARIANT around one scalar.
///
/// The inner struct is assembled as a value and then moved into the VARIANT, rather than written
/// through the union in place: the compiler rejects the in-place form, and rightly so — writing
/// through a `ManuallyDrop` union field would run a destructor on whatever was there before.
fn variant_with(build: impl FnOnce(&mut VariantInner)) -> VARIANT {
    let mut inner = core::mem::ManuallyDrop::new(VariantInner::default());
    build(&mut inner);
    // The union literal is the whole construction: `VARIANT.Anonymous` is the `VARIANT_0` union and
    // `Anonymous` is the arm holding the freshly built `ManuallyDrop<VARIANT_0_0>`.
    VARIANT {
        Anonymous: VariantUnion { Anonymous: inner },
    }
}

fn variant_i32(value: i32) -> VARIANT {
    variant_with(|inner| {
        inner.vt = VT_I4;
        // Writing a union arm is safe here: `Anonymous` is a `Copy` scalar field and the struct is a
        // fresh local, so nothing is being overwritten.
        inner.Anonymous.lVal = value;
    })
}

fn variant_bool(value: bool) -> VARIANT {
    variant_with(|inner| {
        inner.vt = VT_BOOL;
        // As above; -1 is VARIANT_TRUE.
        inner.Anonymous.boolVal =
            windows::Win32::Foundation::VARIANT_BOOL(if value { -1 } else { 0 });
    })
}

fn variant_bstr(value: &str) -> VARIANT {
    variant_with(|inner| {
        inner.vt = VT_BSTR;
        // The VARIANT takes ownership of the BSTR and UIA frees it with VariantClear — which is
        // exactly why the union field is `ManuallyDrop<BSTR>`.
        inner.Anonymous.bstrVal = core::mem::ManuallyDrop::new(windows::core::BSTR::from(value));
    })
}

/// The protocol's own "this property is not supported": a VT_UNKNOWN VARIANT holding UIA's reserved
/// sentinel object. It is not the same as an empty value, which is why an unsupported property
/// answers with it rather than with an error.
fn variant_not_supported() -> VARIANT {
    match unsafe { UiaGetReservedNotSupportedValue() } {
        Ok(value) => variant_with(|inner| {
            inner.vt = windows::Win32::System::Variant::VT_UNKNOWN;
            // The VARIANT takes ownership of the interface reference.
            inner.Anonymous.punkVal = core::mem::ManuallyDrop::new(Some(value));
        }),
        // The sentinel cannot be fetched at all (uiautomationcore missing): an empty VARIANT is the
        // least wrong answer, where an error would fail the whole property read.
        Err(_) => VARIANT::default(),
    }
}

fn variant_double(value: f64) -> VARIANT {
    variant_with(|inner| {
        inner.vt = windows::Win32::System::Variant::VT_R8;
        // SAFETY: `dblVal` is a plain f64.
        unsafe { inner.Anonymous.dblVal = value };
    })
}

fn variant_double_array(values: &[f64]) -> VARIANT {
    // SAFETY: propsys allocates and fills a SAFEARRAY of R8 from the slice, which outlives the call.
    unsafe { InitVariantFromDoubleArray(values).unwrap_or_default() }
}

fn variant_i32_array(values: &[i32]) -> VARIANT {
    // SAFETY: as above, for a SAFEARRAY of I4.
    unsafe { InitVariantFromInt32Array(values).unwrap_or_default() }
}

/// The `Result` error the generated interfaces expect, for a request that cannot be answered at all
/// (a point outside every element, a focus query with nothing focused).
fn not_supported() -> windows::core::Error {
    windows::core::HRESULT(windows::Win32::UI::Accessibility::UIA_E_ELEMENTNOTAVAILABLE as i32).into()
}

/// "This provider does not implement that pattern", in the shape the protocol asks for it: S_OK with
/// a NULL out parameter — which is what `GetPatternProvider` must return for a pattern an element
/// does not have. An empty `Error` produces exactly that (windows-rs remaps S_OK to its own sentinel
/// so it can live in a `Result`; COM treats any non-negative HRESULT as success).
fn no_pattern() -> windows::core::Error {
    windows::core::Error::empty()
}

/// "This method is not supported" — for a method the provider implements but refuses, where S_OK
/// would be a lie: it tells the client the action happened. Measured: returning `no_pattern` (S_OK)
/// from `AddToSelection` made the client see success, which is worse than seeing a failure.
fn not_implemented() -> windows::core::Error {
    windows::core::HRESULT(windows::Win32::UI::Accessibility::UIA_E_NOTSUPPORTED as i32).into()
}

/// Depth-first walk of a subtree, handing each element its index path and origin.
fn walk_paths(
    node: &SemanticsNode,
    path: Vec<usize>,
    origin: (f32, f32),
    f: &mut impl FnMut(&SemanticsNode, (f32, f32), Vec<usize>),
) {
    f(node, origin, path.clone());
    for (index, child) in node.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        walk_paths(child, child_path, origin, f);
    }
}

// ═══════════════════════════════════════════════════════════
// IRawElementProviderSimple
// ═══════════════════════════════════════════════════════════

// The `UIA_*` constants are upper-case by the SDK's naming and are matched on as if they were enum
// variants, which is what the generated bindings model them as.
#[allow(non_upper_case_globals)]
impl IRawElementProviderSimple_Impl for Provider_Impl {
    fn ProviderOptions(&self) -> WinResult<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, pattern_id: UIA_PATTERN_ID) -> WinResult<IUnknown> {
        let Some(element) = self.element() else {
            return Err(no_pattern());
        };
        // Every pattern `supports_pattern` advertises is one this hands out — a client that reads the
        // `IsXxxPatternAvailable` property and then asks for the pattern must not come up empty, which
        // is what happened while Toggle and SelectionItem were advertised but unimplemented.
        if supports_pattern(&element, pattern_id) {
            let unknown: IUnknown = match pattern_id {
                UIA_InvokePatternId => InvokePattern {
                    window_id: self.window_id,
                    node_id: element.node_id,
                }
                .into(),
                UIA_TogglePatternId => TogglePattern {
                    window_id: self.window_id,
                    node_id: element.node_id,
                    state: element.state.checked_value().unwrap_or(ToggleableState::Off),
                }
                .into(),
                UIA_SelectionItemPatternId => SelectionItemPattern {
                    window_id: self.window_id,
                    node_id: element.node_id,
                    selected: element.state.selected_value().unwrap_or(false),
                }
                .into(),
                UIA_RangeValuePatternId => {
                    let (current, min, max) = element
                        .state
                        .progress_value()
                        .unwrap_or((0.0, 0.0, 1.0));
                    RangeValuePattern {
                        value: current as f64,
                        min: min as f64,
                        max: max as f64,
                    }
                    .into()
                }
                // `supports_pattern` answers false for anything else, so reaching here is a bug — and
                // saying so beats handing back a pattern that is not the one that was asked for.
                _ => Err(not_implemented())?,
            };
            return Ok(unknown);
        }
        // A client asking for a pattern an element does not have is routine — the window root, every
        // label, every checkbox asked for Invoke — so this is the protocol's "none", not a failure.
        Err(no_pattern())
    }

    fn GetPropertyValue(&self, property_id: UIA_PROPERTY_ID) -> WinResult<VARIANT> {
        let element = self.element();
        let (x, y, w, h) = self.bounds();

        // The window root answers a smaller set: the rest belongs to the elements inside it.
        let Some(element) = element else {
            return match property_id {
                UIA_ControlTypePropertyId => Ok(variant_i32(UIA_WindowControlTypeId.0)),
                UIA_BoundingRectanglePropertyId => Ok(variant_double_array(&[x, y, w, h])),
                UIA_NativeWindowHandlePropertyId => Ok(variant_i32(self.window_id as i32)),
                UIA_IsContentElementPropertyId | UIA_IsControlElementPropertyId
                | UIA_IsEnabledPropertyId => Ok(variant_bool(true)),
                UIA_IsKeyboardFocusablePropertyId
                | UIA_HasKeyboardFocusPropertyId
                | UIA_IsInvokePatternAvailablePropertyId
                | UIA_IsRangeValuePatternAvailablePropertyId
                | UIA_IsOffscreenPropertyId => Ok(variant_bool(false)),
                UIA_ClassNamePropertyId => Ok(variant_bstr("WiniaWindow")),
                UIA_FrameworkIdPropertyId => Ok(variant_bstr("winia")),
                UIA_AutomationIdPropertyId => Ok(variant_bstr("winia-window")),
                // The name is deliberately not answered: with none of its own, UIA falls back to the
                // native window provider, which knows the title.
                _ => Ok(variant_not_supported()),
            };
        };
        let node = element;

        match property_id {
            UIA_ControlTypePropertyId => Ok(variant_i32(control_type_for(node.role).0)),
            UIA_NamePropertyId => match node.name.as_deref() {
                Some(name) if !name.is_empty() => Ok(variant_bstr(name)),
                _ => Ok(variant_not_supported()),
            },
            UIA_IsEnabledPropertyId => Ok(variant_bool(node.state.enabled_value().unwrap_or(true))),
            UIA_IsKeyboardFocusablePropertyId => Ok(variant_bool(node.clickable)),
            UIA_HasKeyboardFocusPropertyId => Ok(variant_bool(node.focused)),
            UIA_IsContentElementPropertyId | UIA_IsControlElementPropertyId => Ok(variant_bool(true)),
            UIA_IsOffscreenPropertyId => Ok(variant_bool(false)),
            UIA_IsInvokePatternAvailablePropertyId => {
                Ok(variant_bool(supports_pattern(&node, UIA_InvokePatternId)))
            }
            UIA_IsRangeValuePatternAvailablePropertyId => Ok(variant_bool(
                supports_pattern(&node, UIA_RangeValuePatternId),
            )),
            UIA_RangeValueValuePropertyId => match node.state.progress_value() {
                Some((current, _, _)) => Ok(variant_double(current as f64)),
                None => Ok(variant_not_supported()),
            },
            UIA_RangeValueMinimumPropertyId => match node.state.progress_value() {
                Some((_, min, _)) => Ok(variant_double(min as f64)),
                None => Ok(variant_not_supported()),
            },
            UIA_RangeValueMaximumPropertyId => match node.state.progress_value() {
                Some((_, _, max)) => Ok(variant_double(max as f64)),
                None => Ok(variant_not_supported()),
            },
            UIA_ToggleToggleStatePropertyId => match node.state.checked_value() {
                // ToggleState: 0 off, 1 on, 2 indeterminate — what a tri-state checkbox reports.
                Some(ToggleableState::On) => Ok(variant_i32(1)),
                Some(ToggleableState::Off) => Ok(variant_i32(0)),
                Some(ToggleableState::Indeterminate) => Ok(variant_i32(2)),
                None => Ok(variant_not_supported()),
            },
            UIA_SelectionItemIsSelectedPropertyId => match node.state.selected_value() {
                Some(selected) => Ok(variant_bool(selected)),
                None => Ok(variant_not_supported()),
            },
            UIA_BoundingRectanglePropertyId => Ok(variant_double_array(&[x, y, w, h])),
            UIA_AutomationIdPropertyId => Ok(variant_bstr(&format!("winia-{}", node.node_id))),
            UIA_ClassNamePropertyId => {
                Ok(variant_bstr(node.role.map(|role| role.name()).unwrap_or("text")))
            }
            UIA_FrameworkIdPropertyId => Ok(variant_bstr("winia")),
            _ => Ok(variant_not_supported()),
        }
    }

    fn HostRawElementProvider(&self) -> WinResult<IRawElementProviderSimple> {
        // Only the ROOT has a host: the window's own default provider, which UIA consults for
        // everything this one does not answer (the native frame, and the title it knows). A child
        // must return nothing here — measured: answering with the host for every element made each
        // one advertise the window's WindowPattern and TransformPattern, which it does not have.
        if !self.path.is_empty() {
            return Err(no_pattern());
        }
        let Some(hwnd) = hwnd_for(self.window_id) else {
            return Err(not_supported());
        };
        // SAFETY: a plain query against an HWND we registered ourselves.
        unsafe { UiaHostProviderFromHwnd(hwnd).map_err(|_| not_supported()) }
    }
}

// ═══════════════════════════════════════════════════════════
// IRawElementProviderFragment
// ═══════════════════════════════════════════════════════════


// The `NavigateDirection_*` names are globals by the SDK's naming, matched on as if they were enum
// variants — which is how the generated bindings model them.
#[allow(non_upper_case_globals)]
impl Provider_Impl {
    fn navigate_inner(&self, direction: NavigateDirection) -> WinResult<IRawElementProviderFragment> {
        match direction {
            NavigateDirection_Parent => {
                if self.path.is_empty() {
                    // A fragment root has no parent.
                    return Err(not_supported());
                }
                let mut path = self.path.clone();
                path.pop();
                Ok(Provider {
                    window_id: self.window_id,
                    path,
                }
                .into())
            }
            NavigateDirection_FirstChild | NavigateDirection_LastChild => {
                let children = self.child_providers();
                let child = if direction == NavigateDirection_FirstChild {
                    children.into_iter().next()
                } else {
                    children.into_iter().last()
                };
                match child {
                    Some(child) => Ok(child.into()),
                    None => Err(not_supported()),
                }
            }
            NavigateDirection_NextSibling | NavigateDirection_PreviousSibling => {
                // A sibling is the neighbour of this element among its parent's children; the root
                // has none.
                let Some((&index, parent)) = self.path.split_last() else {
                    return Err(not_supported());
                };
                let siblings = Provider {
                    window_id: self.window_id,
                    path: parent.to_vec(),
                }
                .child_providers();
                let neighbour = if direction == NavigateDirection_PreviousSibling {
                    index.checked_sub(1)
                } else {
                    Some(index + 1)
                };
                match neighbour.and_then(|i| siblings.into_iter().nth(i)) {
                    Some(sibling) => Ok(sibling.into()),
                    None => Err(not_supported()),
                }
            }
            _ => Err(not_supported()),
        }
    }
}

impl IRawElementProviderFragment_Impl for Provider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> WinResult<IRawElementProviderFragment> {
        let trace = std::env::var_os("WINIA_A11Y_TRACE").is_some();
        if trace {
            eprintln!(
                "[accessibility] Navigate dir={} path={:?} children_of_parent={}",
                direction.0,
                self.path,
                self.snapshot().map(|s| s.children_at(&self.path).len()).unwrap_or(0)
            );
        }
        let result = self.navigate_inner(direction);
        if trace {
            eprintln!("[accessibility]   -> {}", if result.is_ok() { "ok" } else { "err" });
        }
        result
    }

    fn GetRuntimeId(&self) -> WinResult<*mut SAFEARRAY> {
        // UIA's convention: the first element is UiaAppendRuntimeId (3), then values unique within the
        // app. Deriving the rest from `node_id` keeps it stable across frames for the same element,
        // which is what a client diffs on.
        let element_part = self
            .element()
            .map(|element| element.node_id as i32)
            .unwrap_or(0);
        let ids = [3i32, self.window_id as i32, element_part];
        let variant = variant_i32_array(&ids);
        // SAFETY: the VARIANT owns the SAFEARRAY. The pointer is taken out so ownership travels to
        // UIA, which frees it — which is why the VARIANT must not run its destructor.
        let array = unsafe { variant.Anonymous.Anonymous.Anonymous.parray };
        if array.is_null() {
            return Err(not_supported());
        }
        core::mem::forget(variant);
        Ok(array)
    }

    fn BoundingRectangle(&self) -> WinResult<UiaRect> {
        let (left, top, width, height) = self.bounds();
        Ok(UiaRect {
            left,
            top,
            width,
            height,
        })
    }

    fn GetEmbeddedFragmentRoots(&self) -> WinResult<*mut SAFEARRAY> {
        // Nothing embedded: everything this provider describes belongs to the window it was made for.
        Err(not_supported())
    }

    fn SetFocus(&self) -> WinResult<()> {
        let Some(element) = self.element() else {
            return Err(not_supported());
        };
        crate::semantics::request_action(
            self.window_id,
            crate::semantics::UiAction::Focus(element.node_id),
        );
        Ok(())
    }

    fn FragmentRoot(&self) -> WinResult<IRawElementProviderFragmentRoot> {
        // The same cached instance `WM_GETOBJECT` answers with: UIA compares them.
        let root = cached_provider(self.window_id, &[], || Provider::root(self.window_id));
        root.cast().map_err(|_| not_supported())
    }
}

// ═══════════════════════════════════════════════════════════
// IRawElementProviderFragmentRoot
// ═══════════════════════════════════════════════════════════

impl IRawElementProviderFragmentRoot_Impl for Provider_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> WinResult<IRawElementProviderFragment> {
        let Some(snapshot) = self.snapshot() else {
            return Err(not_supported());
        };
        let scale = snapshot.scale_factor as f64;
        let window_origin = self.screen_origin();
        // Deepest match wins: parents are visited before their children and the last hit is kept,
        // which is the innermost element containing the point — the rule hit testing uses.
        let mut hit: Option<Vec<usize>> = None;
        let mut visit = |node: &SemanticsNode, origin: (f32, f32), path: Vec<usize>| {
            let (bx, by, bw, bh) = node.bounds;
            let sx = window_origin.0 + (origin.0 + bx) as f64 * scale;
            let sy = window_origin.1 + (origin.1 + by) as f64 * scale;
            if x >= sx && x <= sx + bw as f64 * scale && y >= sy && y <= sy + bh as f64 * scale {
                hit = Some(path);
            }
        };
        for (index, (node, origin)) in snapshot.top_level().into_iter().enumerate() {
            walk_paths(node, vec![index], origin, &mut visit);
        }
        match hit {
            Some(path) => Ok(Provider {
                window_id: self.window_id,
                path,
            }
            .into()),
            None => Err(not_supported()),
        }
    }

    fn GetFocus(&self) -> WinResult<IRawElementProviderFragment> {
        let Some(snapshot) = self.snapshot() else {
            return Err(not_supported());
        };
        let mut found: Option<Vec<usize>> = None;
        let mut visit = |node: &SemanticsNode, _origin: (f32, f32), path: Vec<usize>| {
            if node.focused && found.is_none() {
                found = Some(path);
            }
        };
        for (index, (node, origin)) in snapshot.top_level().into_iter().enumerate() {
            walk_paths(node, vec![index], origin, &mut visit);
        }
        match found {
            Some(path) => Ok(Provider {
                window_id: self.window_id,
                path,
            }
            .into()),
            None => Err(not_supported()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::SemanticsState;

    fn element(
        role: Option<SemanticsRole>,
        clickable: bool,
        state: SemanticsState,
    ) -> ElementData {
        ElementData {
            node_id: 1,
            role,
            name: Some("x".to_string()),
            state,
            clickable,
            focused: false,
        }
    }

    #[test]
    fn every_role_maps_to_a_control_type() {
        // The mapping is the contract a client reads: getting a role wrong makes a screen reader
        // announce the wrong kind of control, which is worse than announcing nothing.
        let cases = [
            (SemanticsRole::Button, UIA_ButtonControlTypeId),
            (SemanticsRole::Checkbox, UIA_CheckBoxControlTypeId),
            // UIA has no switch type; a switch is a toggleable control, so CheckBox it is.
            (SemanticsRole::Switch, UIA_CheckBoxControlTypeId),
            (SemanticsRole::RadioButton, UIA_RadioButtonControlTypeId),
            (SemanticsRole::Tab, UIA_TabItemControlTypeId),
            (SemanticsRole::Image, UIA_ImageControlTypeId),
            (SemanticsRole::ProgressBar, UIA_ProgressBarControlTypeId),
            (SemanticsRole::Dialog, UIA_PaneControlTypeId),
        ];
        for (role, expected) in cases {
            assert_eq!(control_type_for(Some(role)).0, expected.0, "{role:?}");
        }
        // A named element with no role is text — `Text` and `RichText` compose exactly that.
        assert_eq!(control_type_for(None).0, UIA_TextControlTypeId.0);
    }

    #[test]
    fn a_click_target_offers_invoke() {
        let button = element(Some(SemanticsRole::Button), true, SemanticsState::new());
        assert!(supports_pattern(&button, UIA_InvokePatternId));
        // A plain button has no toggle or selection state, so it offers neither.
        assert!(!supports_pattern(&button, UIA_TogglePatternId));
        assert!(!supports_pattern(&button, UIA_SelectionItemPatternId));
    }

    #[test]
    fn a_clickable_checkbox_offers_invoke_and_toggle() {
        // The case that made "one pattern per element" wrong: an ENABLED checkbox is clickable AND
        // checked, and a client may address it with either pattern.
        let checkbox = element(
            Some(SemanticsRole::Checkbox),
            true,
            SemanticsState::new().checked_bool(true),
        );
        assert!(supports_pattern(&checkbox, UIA_InvokePatternId));
        assert!(supports_pattern(&checkbox, UIA_TogglePatternId));
        assert!(!supports_pattern(&checkbox, UIA_SelectionItemPatternId));
    }

    #[test]
    fn a_disabled_checkbox_still_offers_toggle() {
        // Its state is readable and a screen reader must be able to announce it; `IsEnabled` is what
        // tells the client not to act on it.
        let disabled = element(
            Some(SemanticsRole::Checkbox),
            false,
            SemanticsState::new().checked(crate::ui::checkbox::ToggleableState::Indeterminate),
        );
        assert!(!supports_pattern(&disabled, UIA_InvokePatternId));
        assert!(supports_pattern(&disabled, UIA_TogglePatternId));
    }

    #[test]
    fn a_selected_control_offers_selection_item() {
        let radio = element(
            Some(SemanticsRole::RadioButton),
            true,
            SemanticsState::new().selected(true),
        );
        assert!(supports_pattern(&radio, UIA_SelectionItemPatternId));
        assert!(supports_pattern(&radio, UIA_InvokePatternId));
        assert!(!supports_pattern(&radio, UIA_TogglePatternId));
    }

    #[test]
    fn a_progress_bar_offers_range_value_and_its_number() {
        let bar = element(
            Some(SemanticsRole::ProgressBar),
            false,
            SemanticsState::new().progress(30.0, 0.0, 100.0),
        );
        assert!(supports_pattern(&bar, UIA_RangeValuePatternId), "a value to report means the pattern");
        assert!(!supports_pattern(&bar, UIA_InvokePatternId), "a progress bar is not clickable");

        // And the number itself reads back through the interface a client uses.
        let provider = RangeValuePattern { value: 30.0, min: 0.0, max: 100.0 };
        let provider: IRangeValueProvider = provider.into();
        // SAFETY: COM calls into the implementation above; the provider outlives them.
        unsafe {
            assert_eq!(provider.Value().expect("a value"), 30.0);
            assert_eq!(provider.Minimum().expect("a min"), 0.0);
            assert_eq!(provider.Maximum().expect("a max"), 100.0);
            assert!(provider.IsReadOnly().expect("a flag").as_bool(), "progress is not settable");
            assert!(provider.SetValue(50.0).is_err(), "and refuses rather than pretending");
        }
    }

    #[test]
    fn an_indeterminate_progress_bar_reports_no_value() {
        // A spinner has no value; a reader is told it is a progress bar and nothing more. Reporting
        // zero would be a lie — the difference between "0 percent" and "in progress".
        let spinner = element(Some(SemanticsRole::ProgressBar), false, SemanticsState::new());
        assert!(!supports_pattern(&spinner, UIA_RangeValuePatternId));
        assert_eq!(spinner.state.progress_value(), None);
    }

    #[test]
    fn an_element_with_no_action_offers_no_pattern() {
        // A label: name and role, neither clickable nor toggleable. Advertising a pattern here is what
        // made clients ask for one and get nothing back.
        let label = element(None, false, SemanticsState::new());
        assert!(!supports_pattern(&label, UIA_InvokePatternId));
        assert!(!supports_pattern(&label, UIA_TogglePatternId));
        assert!(!supports_pattern(&label, UIA_SelectionItemPatternId));
    }

    #[test]
    fn toggle_state_carries_all_three_values() {
        // The tri-state checkbox depends on this: "indeterminate" must not collapse into "off".
        let state_of = |state| {
            let pattern = TogglePattern {
                window_id: 0,
                node_id: 0,
                state,
            };
            let provider: IToggleProvider = pattern.into();
            // SAFETY: the call goes through COM into the implementation above; the provider is alive for
            // the call and owns nothing the caller has to free.
            unsafe { provider.ToggleState().expect("a state").0 }
        };
        assert_eq!(state_of(ToggleableState::On), ToggleState_On.0);
        assert_eq!(state_of(ToggleableState::Off), ToggleState_Off.0);
        assert_eq!(
            state_of(ToggleableState::Indeterminate),
            ToggleState_Indeterminate.0
        );
    }

    #[test]
    fn selection_item_reports_its_selection_and_refuses_multi_select() {
        let selected = SelectionItemPattern {
            window_id: 0,
            node_id: 0,
            selected: true,
        };
        let provider: ISelectionItemProvider = selected.into();
        // SAFETY: COM calls into the implementation above; the provider outlives each call.
        unsafe {
            assert_eq!(provider.IsSelected().expect("a value").as_bool(), true);
            // Single-choice controls: "add to the selection" is refused rather than faked.
            assert!(provider.AddToSelection().is_err());
            assert!(provider.RemoveFromSelection().is_err());
        }
    }

    #[test]
    fn an_action_from_a_bridge_is_queued_for_this_window_only() {
        // The provider must not run application code, so what it does is queue — and the queue has to
        // keep windows apart, or a dialog's action would land on the page behind it.
        crate::semantics::request_action(4242, crate::semantics::UiAction::Invoke(7));
        crate::semantics::request_action(4243, crate::semantics::UiAction::Focus(9));
        assert!(crate::semantics::has_actions());
        assert_eq!(
            crate::semantics::take_actions(4242),
            vec![crate::semantics::UiAction::Invoke(7)]
        );
        assert_eq!(
            crate::semantics::take_actions(4243),
            vec![crate::semantics::UiAction::Focus(9)]
        );
        assert!(!crate::semantics::has_actions(), "the queue is drained");
        assert!(crate::semantics::take_actions(4242).is_empty());
    }
}
