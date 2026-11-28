# Window system backdrops

winia exposes a system-drawn window material as one request. `WindowBackdrop::System` selects the
platform default; `Mica`, `Acrylic` and `MicaAlt` are explicit Windows DWM materials. The API never
changes transparency, decorations, the frame, or the rendering surface.

```rust
Window::new()
    .backdrop(WindowBackdrop::System)
    .transparent(true)
    .decorations(false)
    .build(ctx, |ctx| { /* content */ });
```

```rust
id.state().set_backdrop(WindowBackdrop::Acrylic); // Windows only
let status = id.state().backdrop_status();
```

See [window-material.md](window-material.md) for the cross-platform mapping. On Windows:

## What winia does

- maps `System` and `Mica` to `DWMSBT_MAINWINDOW`, `Acrylic` to `DWMSBT_TRANSIENTWINDOW`, and
  `MicaAlt` to `DWMSBT_TABBEDWINDOW`;
- calls `DwmSetWindowAttribute`, then reads the value back with `DwmGetWindowAttribute`;
- reports the outcome through `WindowState::backdrop_status()` and
  `WindowEvent::BackdropStatusChanged`;
- keeps the requested value on platforms that cannot honour it.

`None` writes `DWMSBT_NONE`.

## Mapping

| `WindowBackdrop` | `DWM_SYSTEMBACKDROP_TYPE` | Documented placement |
| --- | --- | --- |
| `None` | `DWMSBT_NONE` (1) | nowhere |
| `System` / `Mica` | `DWMSBT_MAINWINDOW` (2) | behind the entire window bounds |
| `Acrylic` | `DWMSBT_TRANSIENTWINDOW` (3) | behind the entire window bounds |
| `MicaAlt` | `DWMSBT_TABBEDWINDOW` (4) | behind the entire window bounds |

Source: [DWM_SYSTEMBACKDROP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type).

## Where the material is visible

The system draws the material *behind* the window, so it shows only through pixels the client does
not paint opaquely:

| Request | Decorations | Client area | What shows |
| --- | --- | --- | --- |
| `System` / `Mica` / `Acrylic` / `MicaAlt` | system title bar | opaque | title bar only |
| `System` / `Mica` / `Acrylic` / `MicaAlt` | none | transparent surface, no opaque fill | material through the client |

## Windows fixes the client compositing mode at creation

Measured on Windows 11 build 26300 with `CompositeAlphaMode::PreMultiplied` and `Bgra8Unorm`:

| Window | Client area |
| --- | --- |
| created undecorated, stays undecorated | transparent, material visible |
| created undecorated, decorations enabled afterwards | stays transparent |
| created decorated | opaque (white); turning decorations off later does not change it |

Runtime retries did not restore a decorated window's client alpha:

- calling `DwmEnableBlurBehindWindow` again;
- `DwmExtendFrameIntoClientArea` with `-1` margins;
- adding `WS_EX_LAYERED`, with and without `SWP_FRAMECHANGED`. The fill turned black and the frame
  fell back to the classic non-DWM title bar, so the material stopped being drawn.

Upstream records the same asymmetry: winit's [PR #1011](https://github.com/rust-windowing/winit/pull/1011)
added `WS_EX_LAYERED` for transparent decorated windows and noted that borderless windows were not
affected. Tauri maintainers recommend transparency only with decorations off
([tauri#8632](https://github.com/tauri-apps/tauri/issues/8632)).

Consequences:

- whole-window material in the client: create with `decorations(false)` and `transparent(true)`;
- material in a decorated window's title bar: any material request, but the client stays opaque;
- a window created with decorations cannot be repaired at runtime by this API.

## Status semantics

| Status | Meaning |
| --- | --- |
| `NotRequested` | no material requested, or cleared with `None` |
| `Pending` | queued: the window does not exist yet, or the write has not run yet |
| `Applied` | Windows accepted the call and read back the requested value |
| `Unverified(hresult)` | Windows accepted the call, but the read-back failed |
| `Unsupported` | no backend, or a named Windows material on another platform |
| `Rejected(hresult)` | Windows refused the call; pre-22621 builds return `E_INVALIDARG` |
| `Mismatch { requested, reported }` | Windows read back a different value |

`Applied` confirms the attribute, not the pixels. The read-back has been verified on Windows 11
build 26300. Avalonia reports a case where a framework believes the material is applied while DWM
never receives it ([AvaloniaUI/Avalonia#22867](https://github.com/AvaloniaUI/Avalonia/issues/22867));
winia's Windows status is only as truthful as the DWM read-back.

## Measured client alpha

Although a flip-model `CreateSwapChainForHwnd` swapchain is documented as opaque, this checkout's
wgpu surface path carried alpha on Windows 11 build 26300:

| Sample point | Window shown | Window hidden |
| --- | --- | --- |
| background 1 | 35, 30, 34 | 23, 23, 35 |
| background 2 | 40, 29, 29 | 17, 17, 27 |

`Acrylic` sampled a uniform 84, 84, 84 while the background varied (42, 27, 33 / 32, 32, 32 /
38, 28, 33). The remaining limitation is creation-time decorations, not the swapchain.

## Runtime switching and platform support

- On a window with a backdrop backend that is not shell-owned, `set_backdrop` queues the write
  and status moves through `Pending` to the result. Without a backend it reports `Unsupported`
  directly; a shell-owned app-bar keeps the request and reports `Unsupported` without queuing.
- Changing decorations or transparency later does not re-apply the material.
- `set_backdrop(WindowBackdrop::None)` clears it.
- `WindowCapabilities::backdrop` is true on Windows, macOS and Wayland. Named Windows materials
  report `Unsupported` on macOS and Wayland; `System` uses winit's blur there.
- A supported Windows app-bar is shell-owned: builder and runtime backdrop requests are kept
  for inspection but report `Unsupported`. An app-bar request on another platform is an ordinary
  window, so `System` applies normally.
