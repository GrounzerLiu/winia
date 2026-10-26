use super::*;

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
pub(crate) struct Provider {
    /// The window the element belongs to; also the key its snapshot is published under.
    pub(crate) window_id: u64,
    /// The path from the window root to the element (see the type doc above).
    pub(crate) path: Vec<usize>,
}

impl Provider {
    pub(crate) fn root(window_id: u64) -> Self {
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
        // The nearest ANCESTOR that offers ScrollToIndex — a path prefix, so this is the collection
        // this element sits in. The element's own action is not an answer here: an item asks its
        // container to scroll it, it does not scroll itself.
        let scroll_owner = (0..self.path.len()).rev().find_map(|len| {
            let (ancestor, _) = snapshot.resolve(&self.path[..len])?;
            ancestor.scroll_to_index.as_ref().map(|_| ancestor.node_id)
        });
        Some(ElementData {
            node_id: node.node_id,
            role: node.role,
            name: node.effective_name(),
            state: node.state.clone(),
            clickable: node.clickable,
            focused: node.focused,
            live_region: node.live_region,
            has_children: !node.children.is_empty(),
            collection: node.collection.is_some(),
            collection_index: node.collection_item.map(|info| info.index),
            scroll_owner,
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
#[derive(Clone)]
pub(crate) struct ElementData {
    pub(crate) node_id: u64,
    pub(crate) role: Option<SemanticsRole>,
    pub(crate) name: Option<String>,
    pub(crate) state: crate::semantics::SemanticsState,
    pub(crate) clickable: bool,
    pub(crate) focused: bool,
    pub(crate) live_region: crate::semantics::LiveRegionMode,
    /// Whether this element has any children — what makes UIA's name fallback (join the children)
    /// apply, so a container with no name of its own answers an empty string instead.
    pub(crate) has_children: bool,
    /// Whether this element is a COLLECTION (Compose's `CollectionInfo`): a lazy list.
    pub(crate) collection: bool,
    /// This element's position in its enclosing collection, when it is an item of one.
    pub(crate) collection_index: Option<usize>,
    /// The element that offers ScrollToIndex for this item: its nearest ancestor collection. Queued
    /// with the action so the frame loop knows which list to move; `None` for the collection itself
    /// and for anything outside a collection.
    pub(crate) scroll_owner: Option<u64>,
}

pub(crate) fn control_type_for(element: &ElementData) -> UIA_CONTROLTYPE_ID {
    // A collection is a LIST and its items are LIST ITEMS, whatever role (or none) they carry: a
    // reader that hears "text" for every row never gets to say "row 3 of 200".
    if element.collection {
        return UIA_ListControlTypeId;
    }
    if element.collection_index.is_some() {
        return UIA_ListItemControlTypeId;
    }
    match element.role {
        Some(SemanticsRole::Button) => UIA_ButtonControlTypeId,
        // UIA has no switch control type; a switch IS a toggleable control, which CheckBox is here.
        Some(SemanticsRole::Checkbox) | Some(SemanticsRole::Switch) => UIA_CheckBoxControlTypeId,
        Some(SemanticsRole::RadioButton) => UIA_RadioButtonControlTypeId,
        Some(SemanticsRole::Tab) => UIA_TabItemControlTypeId,
        Some(SemanticsRole::Image) => UIA_ImageControlTypeId,
        Some(SemanticsRole::ProgressBar) => UIA_ProgressBarControlTypeId,
        Some(SemanticsRole::Dialog) => UIA_PaneControlTypeId,
        // `Role.DropdownList` — the exposed dropdown menu's anchor. UIA's closest control type is
        // List: what the anchor opens is a list of options.
        Some(SemanticsRole::DropdownList) => UIA_ListControlTypeId,
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
pub(crate) fn supports_pattern(element: &ElementData, pattern_id: UIA_PATTERN_ID) -> bool {
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
    // ScrollItem is what a client calls to bring an ITEM into view; it needs both the item's position
    // and the collection that offers the jump, so an item outside a collection (a stray declaration)
    // offers nothing rather than a pattern that would have nowhere to send the request.
    if pattern_id == UIA_ScrollItemPatternId {
        return element.collection_index.is_some() && element.scroll_owner.is_some();
    }
    false
}

/// The Toggle pattern: `Toggle()` presses a checkbox or switch, and `ToggleState()` reports it.
///
/// winia has one activation path — a click — so this queues the same `UiAction::Invoke` the Invoke
/// pattern does. The element's own handler decides what the new value is (a tri-state checkbox cycles
/// through Indeterminate), which is why the provider does not compute it.
#[implement(IToggleProvider)]
pub(crate) struct TogglePattern {
    pub(crate) window_id: u64,
    pub(crate) node_id: u64,
    pub(crate) state: ToggleableState,
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
pub(crate) struct SelectionItemPattern {
    pub(crate) window_id: u64,
    pub(crate) node_id: u64,
    pub(crate) selected: bool,
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
pub(crate) struct RangeValuePattern {
    pub(crate) value: f64,
    pub(crate) min: f64,
    pub(crate) max: f64,
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
pub(crate) struct InvokePattern {
    pub(crate) window_id: u64,
    pub(crate) node_id: u64,
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

/// The ScrollItem pattern: "bring this item into view" — Compose's `ScrollToIndex` seen from the
/// item's side.
///
/// The provider queues the jump for the frame loop (it must not run application code) and names the
/// COLLECTION that offers it, so a window with several lists moves the right one.
#[implement(IScrollItemProvider)]
pub(crate) struct ScrollItemPattern {
    pub(crate) window_id: u64,
    pub(crate) owner: u64,
    pub(crate) index: usize,
}

impl IScrollItemProvider_Impl for ScrollItemPattern_Impl {
    fn ScrollIntoView(&self) -> WinResult<()> {
        crate::semantics::request_action(
            self.window_id,
            crate::semantics::UiAction::ScrollToIndex {
                node: Some(self.owner),
                index: self.index,
            },
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
                UIA_ScrollItemPatternId => ScrollItemPattern {
                    window_id: self.window_id,
                    // `supports_pattern` already required both; the fallbacks keep the types total.
                    owner: element.scroll_owner.unwrap_or(element.node_id),
                    index: element.collection_index.unwrap_or(0),
                }
                .into(),
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
            UIA_ControlTypePropertyId => Ok(variant_i32(control_type_for(&node).0)),
            UIA_NamePropertyId => match node.name.as_deref() {
                Some(name) if !name.is_empty() => Ok(variant_bstr(name)),
                // A CONTAINER with no name answers an empty string, not "not supported": UIA's
                // fallback for an unnamed container is to concatenate its children's names, which
                // made a lazy list announce as "0 1 2 3 4 5 6 7 8". Compose leaves such a name to
                // the caller and never joins children, so an empty answer is the closer behaviour.
                _ if node.has_children => Ok(variant_bstr("")),
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
            UIA_IsScrollItemPatternAvailablePropertyId => Ok(variant_bool(
                supports_pattern(&node, UIA_ScrollItemPatternId),
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
            // LiveSetting: 0 off, 1 polite, 2 assertive — the levels UIA defines, which is what a
            // client reads to decide how urgent a `LiveRegionChanged` from this element is.
            UIA_LiveSettingPropertyId => Ok(variant_i32(match node.live_region {
                crate::semantics::LiveRegionMode::Off => 0,
                crate::semantics::LiveRegionMode::Polite => 1,
                crate::semantics::LiveRegionMode::Assertive => 2,
            })),
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

