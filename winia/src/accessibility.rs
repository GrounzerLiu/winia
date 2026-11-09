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

use crate::selection::ToggleableState;
use crate::semantics::{SemanticsNode, SemanticsRole, WindowSemantics};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Variant::{
    InitVariantFromDoubleArray, InitVariantFromInt32Array, VARIANT, VT_BOOL, VT_BSTR, VT_I4,
};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IInvokeProvider_Impl, IRangeValueProvider, IRangeValueProvider_Impl,
    IRawElementProviderFragment, IRawElementProviderFragment_Impl, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderSimple,
    IRawElementProviderSimple_Impl, IScrollItemProvider, IScrollItemProvider_Impl,
    ISelectionItemProvider, ISelectionItemProvider_Impl, IToggleProvider, IToggleProvider_Impl,
    NavigateDirection, NavigateDirection_FirstChild, NavigateDirection_LastChild,
    NavigateDirection_NextSibling, NavigateDirection_Parent, NavigateDirection_PreviousSibling,
    ProviderOptions, ProviderOptions_ServerSideProvider, StructureChangeType_ChildrenInvalidated,
    ToggleState_Indeterminate, ToggleState_Off, ToggleState_On, UIA_AutomationFocusChangedEventId,
    UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId,
    UIA_CONTROLTYPE_ID, UIA_CheckBoxControlTypeId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_FrameworkIdPropertyId, UIA_HasKeyboardFocusPropertyId,
    UIA_ImageControlTypeId, UIA_InvokePatternId, UIA_IsContentElementPropertyId,
    UIA_IsControlElementPropertyId, UIA_IsEnabledPropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsOffscreenPropertyId, UIA_IsRangeValuePatternAvailablePropertyId,
    UIA_IsScrollItemPatternAvailablePropertyId, UIA_ListControlTypeId, UIA_ListItemControlTypeId,
    UIA_LiveRegionChangedEventId, UIA_LiveSettingPropertyId, UIA_NamePropertyId,
    UIA_NativeWindowHandlePropertyId, UIA_PATTERN_ID, UIA_PROPERTY_ID, UIA_PaneControlTypeId,
    UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_RangeValueMaximumPropertyId,
    UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId, UIA_RangeValueValuePropertyId,
    UIA_ScrollItemPatternId, UIA_SelectionItemIsSelectedPropertyId, UIA_SelectionItemPatternId,
    UIA_StructureChangedEventId, UIA_TabItemControlTypeId, UIA_TextControlTypeId,
    UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId, UIA_WindowControlTypeId,
    UiaClientsAreListening, UiaGetReservedNotSupportedValue, UiaHostProviderFromHwnd,
    UiaRaiseAutomationEvent, UiaRaiseAutomationPropertyChangedEvent, UiaRaiseStructureChangedEvent,
    UiaRect, UiaReturnRawElementProvider, UiaRootObjectId,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::WM_GETOBJECT;
use windows::core::{IUnknown, Interface, Result as WinResult, implement};

/// Our subclass id — arbitrary, but it must be unique within the window procedure.
const SUBCLASS_ID: usize = 0x776E_6961; // "wnia"

mod install;
mod provider;
mod registry;

pub(crate) use install::*;
pub(crate) use provider::*;
pub(crate) use registry::*;

pub use install::{install, notify, uninstall};

#[cfg(test)]
mod tests;
