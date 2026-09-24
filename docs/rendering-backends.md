# Rendering backends

winia draws through one of three Skia backends, all of them inside `skiwin`:

| Backend | What it needs | Feature | Notes |
|---|---|---|---|
| `vulkan` | A Vulkan loader + a driver ICD that can present to the window | `vulkan` | Default choice. Triple-buffered, `Fifo` present mode. |
| `gl` | A driver that can give a WGL/EGL/GLES context for the window | `gl` | GPU fallback for machines with no Vulkan ICD (older drivers, some VMs). |
| `cpu` | Nothing but a window | always on | Skia rasterises into a softbuffer surface. Slower, but it cannot fail for lack of a GPU. |

## Selection

`skiwin::SkiaWindow::new` picks the backend when a window is created:

1. If `WINIA_RENDER_BACKEND` names a backend (`vulkan`, `gl`, `cpu`, plus `opengl` and `softbuffer`
   as aliases), **only** that one is tried, and a failure is fatal — a pinned backend that silently
   fell back would hide the very thing the pin was there to test.
2. Otherwise the backends compiled into the build are tried most-capable-first: Vulkan, then GL, then
   CPU. The first one that opens the window wins.
3. A backend that fails is reported once per process, to `log::warn` and to stderr — the app runs
   anyway, but a user whose machine dropped to software rendering needs to hear about it. Without a
   logger installed (winia installs none) the stderr line is the only report, which is why it is
   there: falling back is not silent.

If every backend fails, `SkiaWindow::new` panics naming each failure. There is nothing left to draw
with, and a clear panic beats a window that never appears.

`SkiaWindow::backend()` reports which one a window actually opened on.

```bash
cargo run --example alert_dialog_demo                          # automatic
WINIA_RENDER_BACKEND=cpu cargo run --example alert_dialog_demo # pin one
```

## Feature flags

`winia`'s default features are `["vulkan", "gl"]`, so a default build carries the whole fallback
chain. `cpu` needs no feature. To build a variant deliberately:

```bash
cargo check -p winia --no-default-features                          # cpu only
cargo check -p winia --no-default-features --features gl            # gl only
cargo check -p winia --no-default-features --features vulkan        # vulkan only
```

Note that `skia-bindings` resolves its prebuilt archive by an **exact** feature key, and only some
combinations are published. `winia`'s manifest pins the full Windows Skia feature set
(`vulkan, textlayout, svg, gl, d3d`) regardless of which backends are enabled, so every configuration
above resolves to the published archive. Checking `skiwin` on its own with a GPU feature enabled does
not: the build finds no archive for that key and falls back to compiling Skia from source. Check the
backend features through `winia`.

## Debug capture

The debug server's screenshot path (`request_capture` / `take_capture`, and through it the `px`
command and the raw frame stream) works on all three backends. The capture is backend-neutral because
each draw path reads back at the point where its pixels are final:

- Vulkan and GL capture after `flush_and_submit`, so the capture is the frame that was actually
  submitted to the presentation engine. (Reading inside the draw closure happens before the flush and
  sees partial content.)
- CPU captures inside the draw closure: the pixels being drawn *are* the presented frame.

Row order needs no correction: `read_pixels` returns rows top-down whatever the surface's origin is,
even though GL framebuffers are created `BottomLeft` and Vulkan render targets `TopLeft`. This was
measured rather than assumed — the GL capture was checked against a desktop screenshot of the same
window, because a flip that looks necessary for a `BottomLeft` surface puts an upright readback
upside down.

## Known limits

- The `d3d` backend is a 116-line stub in `skiwin/src/d3d.rs` and is not compiled
  (`// mod d3d;` in `skiwin/src/lib.rs`).
- Backend *initialisation* returns errors and falls back. Per-frame recovery does not: Vulkan
  swapchain recreation and device-loss handling still panic in debug builds
  (`skiwin/src/vulkan/renderer.rs`) and recover only in release. A `SkiaWindowTrait::draw` that
  returned `Result` would be needed to lift those, which is a change to the trait every backend and
  call site shares.
- GL framebuffers use the default framebuffer (`fboid: 0`) with an `RGBA8` format, and the GL
  surface's sRGB handling follows the driver's default framebuffer. Colour differences against
  Vulkan are possible on drivers that gamma-correct the default framebuffer.
