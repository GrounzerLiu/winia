# Window materials across platforms

Status: implemented. Windows is verified locally. The winia dispatch and winit call site are
compiled on Windows; winit's AppKit/Wayland backend implementations and their runtime behaviour are
not built or run here.

## Public API

```rust
pub enum WindowBackdrop {
    None,
    System,
    Mica,
    Acrylic,
    MicaAlt,
}
```

`System` is the cross-platform default. The named materials are explicit Windows DWM requests.
There is no `standard()`, no platform wrapper enum, and no duplicated macOS material enum:
`System` already means "use this platform's default", while macOS keeps winit's default
`FullScreenUI` material.

```rust
Window::new().backdrop(WindowBackdrop::System);
Window::new().backdrop(WindowBackdrop::Acrylic); // Windows only
```

## Mapping

| Request | Windows 11 22621+ | macOS | Wayland | X11 / Android / iOS |
| --- | --- | --- | --- | --- |
| `None` | `DWMSBT_NONE` | `set_blur(false)` | `set_blur(false)` | `NotRequested` |
| `System` | `DWMSBT_MAINWINDOW` (Mica) | `set_blur(true)` | `set_blur(true)` | `Unsupported` |
| `Mica` | `DWMSBT_MAINWINDOW` | `Unsupported` | `Unsupported` | `Unsupported` |
| `Acrylic` | `DWMSBT_TRANSIENTWINDOW` | `Unsupported` | `Unsupported` | `Unsupported` |
| `MicaAlt` | `DWMSBT_TABBEDWINDOW` | `Unsupported` | `Unsupported` | `Unsupported` |

## Backends

**Windows.** winit's `set_blur` is an empty function on this platform, so winia calls
`DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` and reads the value back with
`DwmGetWindowAttribute`. `System` and `Mica` both map to `DWMSBT_MAINWINDOW`.

**macOS.** `System` calls `Window::set_blur(true)`, which makes winit place its default
`NSVisualEffectView` material behind the window contents. `None` calls `set_blur(false)`. Named
Windows materials are not converted to a different macOS material; they report `Unsupported`.

**Wayland.** `System` calls `Window::set_blur(true)`. winit uses
`ext_background_effect_manager_v1` or `org_kde_kwin_blur_manager` when the compositor advertises
one. The compositor owns the radius and tint. If it has no blur protocol, winit ignores the call;
the status still means the request was delivered, not that pixels were confirmed.

**X11, Android and iOS.** No system-material backend is wired, so material requests report
`Unsupported` and `None` reports `NotRequested`.

## Status

`Applied` means the request was delivered to the platform API. Windows additionally reads the
DWM value back and can report `Mismatch` or `Unverified`. macOS and Wayland have no read-back through
winit, and a Wayland compositor without a blur manager may ignore the request, so `Applied` does not
confirm a visible effect there.

`Unsupported` means either the platform has no backend or the request does not match that
platform. The request remains visible through `WindowState::backdrop()`.

## Capabilities

`WindowCapabilities::backdrop` is true on Windows, macOS and the Wayland backend. It is false on
X11, Android and iOS. On Wayland it describes the backend, not a guarantee that the running
compositor implements a blur protocol.

## Independence and popups

The material request never changes `transparent`, `decorations`, the frame, or the rendering
surface. A whole-window material is visible only where the client does not paint an opaque pixel;
on Windows that requires a window created undecorated. See [window-backdrop.md](window-backdrop.md).

Popup windows do not expose this API. Measured on Windows 11 build 26300, DWM accepts
`DWMWA_SYSTEMBACKDROP_TYPE` on a `WS_POPUP` window but paints a flat fill and drops per-pixel alpha.
The in-window substitute is [popup-blur.md](popup-blur.md).

## Implementation shape

`window_backdrop::apply` dispatches on `RawWindowHandle`:

- `Win32` calls `window_backdrop::windows`;
- `AppKit` and `Wayland` call `Window::set_blur`;
- every other handle reports `NotRequested` for `None` and `Unsupported` for a material.

The DWM module and the Win32 dispatch arm are `#[cfg(target_os = "windows")]`. The AppKit and
Wayland arms call the cross-platform `Window::set_blur` trait method and are type-checked here. Public
call sites need no `cfg`.

## Verification

| Platform | Verification | Local status |
| --- | --- | --- |
| Windows | fixture status, pixel probes, screenshots | verified |
| macOS | build and run over light and dark desktops | winia call site compiled; winit backend/runtime not built here |
| Wayland | run under KDE or an `ext-background-effect` compositor | winia call site compiled; winit backend/runtime not built here |

The repository has no cross-platform CI. No target-specific winia branch remains uncompiled; what
remains unverified is winit's platform backend implementation and the resulting runtime behaviour.
