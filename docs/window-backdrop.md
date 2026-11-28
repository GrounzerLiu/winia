# Window system backdrops

winia exposes the Windows system backdrop material (Mica, Desktop Acrylic, Mica Alt) as a single
window request. The API is deliberately thin: it writes one DWM attribute and reports what the
system kept. Everything that decides whether the material is *visible* stays in the application's
hands.

```rust
Window::new()
    .backdrop(WindowBackdrop::Mica)
    .transparent(true)
    .decorations(false)
    .build(ctx, |ctx| { /* content */ });
```

```rust
id.state().set_backdrop(WindowBackdrop::Acrylic);
let status = id.state().backdrop_status();
```

## What winia does, and does not do

Does:

- maps [`WindowBackdrop`](../winia/src/app/window_backdrop.rs) onto `DWMWA_SYSTEMBACKDROP_TYPE` and
  calls `DwmSetWindowAttribute`, then reads the value back with `DwmGetWindowAttribute`;
- reports the outcome through `WindowState::backdrop_status()` and
  `WindowEvent::BackdropStatusChanged`;
- keeps the requested value on platforms that cannot honour it, so the declaration stays
  inspectable.

Does not:

- set `transparent`, change `decorations`, or otherwise rewrite the window's other axes;
- call `DwmExtendFrameIntoClientArea`; the community reference implementation warns that it puts
  the opaque frame material behind the client area instead of making the client transparent
  ([jeweg/win32-window-transparency](https://github.com/jeweg/win32-window-transparency));
- touch the wgpu surface or swapchain;
- probe the OS build with undocumented APIs, or retry the call.

## Mapping

| `WindowBackdrop` | `DWM_SYSTEMBACKDROP_TYPE` | Documented placement |
| --- | --- | --- |
| `None` | `DWMSBT_NONE` (1) | nowhere |
| `Auto` | `DWMSBT_AUTO` (0) | just behind the default Win32 title bar |
| `Mica` | `DWMSBT_MAINWINDOW` (2) | behind the entire window bounds |
| `Acrylic` | `DWMSBT_TRANSIENTWINDOW` (3) | behind the entire window bounds |
| `MicaAlt` | `DWMSBT_TABBEDWINDOW` (4) | behind the entire window bounds |

Source: [DWM_SYSTEMBACKDROP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type).

## Where the material is actually visible

The system draws the material *behind* the window, so it only shows through pixels the window does
not paint opaquely. winia never decides that for you:

| Request | Decorations | Client area | What shows |
| --- | --- | --- | --- |
| `Auto` | system title bar | opaque | material in the title bar |
| `Mica` / `Acrylic` / `MicaAlt` | system title bar | opaque, always | the title bar only; Windows does not compose a decorated client per pixel |
| `Mica` / `Acrylic` / `MicaAlt` | none | transparent surface, no opaque fill | the material shows through the client |

The Mica guidance says the same from the other side: to show Mica in the title bar you extend into
the non-client area and use a transparent custom title bar
([Mica material](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)).

## Windows composites the client per pixel only without decorations

This is the one Windows rule that decides whether a whole-window material can ever be visible, and it
is a platform behaviour rather than something winia chooses.

Measured on Windows 11 build 26300 with `CompositeAlphaMode::PreMultiplied` and `Bgra8Unorm`:

| Window | Client area |
| --- | --- |
| created undecorated, stays undecorated | transparent, material visible |
| created undecorated, decorations enabled afterwards | stays transparent |
| created decorated | opaque (white); turning decorations off afterwards does not change it |

The composition mode is fixed when the window is created, so nothing at runtime can restore a
transparent client on a window that was created with decorations. Each of these was tried on the
same build and left the client opaque:

- calling `DwmEnableBlurBehindWindow` again with the empty-region blur behind;
- `DwmExtendFrameIntoClientArea` with `-1` margins;
- adding `WS_EX_LAYERED` at runtime, with and without `SWP_FRAMECHANGED`. The fill turned from white
to black and the frame fell back to the classic (non-DWM) title bar, so the material stopped being
drawn entirely.

Upstream records the same asymmetry: winit's [PR #1011](https://github.com/rust-windowing/winit/pull/1011)
added `WS_EX_LAYERED` for transparent decorated windows with the note that "borderless (or
un-decorated) windows were not affected", and the Tauri maintainers answer the same class of report
by recommending transparency only with decorations off
([tauri#8632](https://github.com/tauri-apps/tauri/issues/8632)).

Consequences for applications:

- whole-window material: `decorations(false)` and `transparent(true)`;
- material in the title bar of a decorated window: `Auto`;
- decorated window with an opaque client: any request, but the client itself never shows material.

## Status semantics

| Status | Meaning |
| --- | --- |
| `NotRequested` | no material requested, or a previous request was cleared with `None` |
| `Pending` | queued: the window does not exist yet, or the write has not run yet |
| `Applied` | DWM accepted the call and reads back the requested value |
| `Unverified(hresult)` | DWM accepted the call, but the read-back failed; the value is unconfirmed |
| `Unsupported` | the platform implements no system backdrop; the request is kept |
| `Rejected(hresult)` | DWM refused the call; Windows 10 and Windows 11 before build 22621 return `E_INVALIDARG` |
| `Mismatch { requested, reported }` | the call succeeded but DWM reports a different value |

`Applied` confirms the attribute, not the pixels. The read-back itself has only been verified on
Windows 11 build 26300: where `DwmGetWindowAttribute` refuses the attribute, the write is reported
as `Unverified` rather than as `Rejected`, because the set succeeded even though the confirmation is
missing. Avalonia reports a case on Windows 11 build 26300
where the framework believes the material is applied while DWM never receives it
([AvaloniaUI/Avalonia#22867](https://github.com/AvaloniaUI/Avalonia/issues/22867)); winia's status is
only as truthful as the DWM read-back.

## Known rendering prerequisite

Client-area transparency needs a surface that can carry alpha. The same community reference lists
the rule that a flip-model swapchain from `CreateSwapChainForHwnd` is always opaque, and that a
composition swapchain with `DXGI_ALPHA_MODE_PREMULTIPLIED` is required instead. winia's current
wgpu surface path is unchanged by this feature, so a whole-window material may still not appear in
the client area even with `transparent(true)` and `decorations(false)`. That is a separate rendering
task, not a backdrop switch.

## Switching at runtime

- `set_backdrop` queues the attribute write; the status moves to `Pending` and then to the result.
- Changing `decorations` or `transparent` later does not re-apply the material: DWM keeps the
  attribute it accepted.
- `set_backdrop(WindowBackdrop::None)` clears the attribute (`DWMSBT_NONE`).

## Platform support

`WindowCapabilities::backdrop` is true on every Windows desktop window and false elsewhere. It is a
platform claim, not a build guarantee: Windows 10 and Windows 11 before build 22621 expose the
attribute and reject the write with `E_INVALIDARG`, which surfaces as `BackdropStatus::Rejected`.
Other platforms keep the request for inspection and report `BackdropStatus::Unsupported`; no
winit-side vibrancy (macOS `NSVisualEffectView`, Wayland blur protocols) is wired to this request.

Backdrop requests are ignored on Windows app-bar windows, which the shell owns — both the
builder option and `set_backdrop`.
