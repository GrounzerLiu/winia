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
| `Auto` | system title bar | opaque | material in the title bar only |
| `Mica` / `Acrylic` / `MicaAlt` | system title bar | opaque | the client area covers the material |
| `Mica` / `Acrylic` / `MicaAlt` | none | transparent surface, no opaque fill | the material can show through |

The Mica guidance says the same from the other side: to show Mica in the title bar you extend into
the non-client area and use a transparent custom title bar
([Mica material](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)).

## Status semantics

| Status | Meaning |
| --- | --- |
| `NotRequested` | no material requested, or a previous request was cleared with `None` |
| `Pending` | queued; the window does not exist on the platform yet |
| `Applied` | DWM accepted the call and reads back the requested value |
| `Unsupported` | the platform implements no system backdrop; the request is kept |
| `Rejected(hresult)` | DWM refused the call; Windows 10 and Windows 11 before build 22621 return `E_INVALIDARG` |
| `Mismatch { requested, reported }` | the call succeeded but DWM reports a different value |

`Applied` confirms the attribute, not the pixels. Avalonia reports a case on Windows 11 build 26300
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

`WindowCapabilities::backdrop` is true on Windows and false elsewhere. Other platforms keep the
request for inspection and report `BackdropStatus::Unsupported`; no winit-side vibrancy (macOS
`NSVisualEffectView`, Wayland blur protocols) is wired to this request.
