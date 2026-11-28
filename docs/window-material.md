# Window materials across platforms

Status: design. Windows is implemented; macOS and Wayland are specified but not implemented, because
neither can be compiled or run in this environment (see *Verification*).

## Goal

One window option that asks each platform for its **own** material, with three properties the
Windows-only `WindowBackdrop` already keeps:

1. one request maps onto one platform fact, with no implicit coupling to `transparent` or
   `decorations`;
2. the outcome is reported as a status that can be wrong only in the direction the platform allows;
3. a request that the running platform cannot honour is kept and reported, never silently rewritten.

## API

```rust
pub enum WindowBackdrop {
    None,
    Windows(WindowsBackdrop),
    MacOs(MacOsBackdrop),
    Wayland,
}

pub enum WindowsBackdrop { Auto, Mica, Acrylic, MicaAlt }
pub enum MacOsBackdrop { FullScreenUi, HudWindow, Menu, Popover, Sidebar, Selection, Titlebar,
                         HeaderView, ToolTip, UnderWindowBackground }
```

Why per-platform sub-enums instead of one flat list: a Windows-only material cannot be named on a
Mac by accident. The request either names the running platform's enum or it does not, and the
mismatch is reported as `Unsupported` instead of quietly doing nothing.

`WindowBackdrop::Wayland` carries no payload on purpose: the protocol decides the radius and the
tint, and `Window::set_blur(true)` is the whole request.

## Mapping

| Request | Windows 11 22621+ | macOS | Wayland |
| --- | --- | --- | --- |
| `None` | `DWMSBT_NONE` | `set_blur(false)` | `set_blur(false)` |
| `Windows(..)` | `DWMWA_SYSTEMBACKDROP_TYPE` | `Unsupported` | `Unsupported` |
| `MacOs(..)` | `Unsupported` | `NSVisualEffectView` material | `Unsupported` |
| `Wayland` | `Unsupported` | `Unsupported` | `set_blur(true)` |

Windows keeps the measured behaviour documented in [window-backdrop.md](window-backdrop.md): DWM
paints the material behind the window, the client shows it only for a window created undecorated,
and the value is read back through `DwmGetWindowAttribute`.

`MacOsBackdrop` mirrors `winit_appkit::BlurMaterial` one-to-one, so the macOS backend is a
conversion instead of a second taxonomy; that enum is `#[non_exhaustive]`, and a variant winia does
not know reports `Unsupported` rather than substituting a different material.

macOS uses winit's own surface rather than Objective-C in winia:

- at creation, `WindowAttributes::with_blur(true)` together with
  `WindowAttributesMacOS::with_blur_material(material)`;
- at runtime, `Window::set_blur(bool)` together with `WindowExtMacOS::set_blur_material(material)`.

winit renders the material as a sibling behind the window's `contentView`, tinted and following the
window appearance. The `private-apple-apis` Cargo feature switches it to `CGSSetWindowBackgroundBlurRadius`,
which is untinted, fixed radius, and can cause App Store rejection — winia does not enable it.

Wayland uses `Window::set_blur(bool)`, which winit implements only when the compositor advertises
`org_kde_kwin_blur_manager` or `ext_background_effect_manager_v1`. There is no radius or tint to
carry.

## Status

`BackdropStatus` keeps its variants and gains one distinction: `Mismatch` is Windows-only, because
only Windows has a documented read-back. macOS and Wayland report `Applied` when the call returned
without error; there is no way to confirm pixels there, and the doc says so instead of pretending
otherwise.

`Unsupported` covers both "this platform has no such mechanism" and "the platform has it but winia
has no backend yet". In both cases the request is kept and `WindowState::backdrop()` still reports
what was asked for; which platforms are in which group is documented rather than encoded in the
status.

## Capabilities and events

`WindowCapabilities::backdrop` means *the running platform has an implemented backend*. It is false on
macOS and Wayland until their backends land, even though `WindowBackdrop` can name them; the doc
keeps the request visible through `WindowState::backdrop()` either way.

`WindowEvent::BackdropStatusChanged` is unchanged.

## Popups

Not offered. Measured on Windows 11 build 26300: `DWMWA_SYSTEMBACKDROP_TYPE` is accepted on a
`WS_POPUP` window, but DWM paints a flat fill instead of the material and drops the window's
per-pixel alpha, so a transparent popup becomes an opaque block. The in-window substitute is
[popup-blur.md](popup-blur.md).

## Verification

| Platform | How it can be verified | Available here |
| --- | --- | --- |
| Windows | fixture scenario reads the status, screenshots and pixel probes | yes |
| macOS | build and run the example, screenshot a window over a light and a dark desktop | no |
| Wayland | `wayland-info` for the protocol, then run under KDE or a compositor with `ext-background-effect` | no |

Backend code that cannot be compiled here is not landed: a macOS-only branch that never type-checks
is not a feature, it is a guess. The API shape above is landable now because every variant compiles
on every platform and reports `Unsupported` where no backend exists.

## Migration

`WindowBackdrop::Mica` becomes `WindowBackdrop::Windows(WindowsBackdrop::Mica)`. The enum and the
status type keep their names, so `Window::backdrop`, `WindowState::backdrop`, `set_backdrop`,
`backdrop_status` and `WindowEvent::BackdropStatusChanged` are unchanged.
