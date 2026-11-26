# Rendering backend: Skia Graphite on wgpu

winia renders through rust-skia's experimental Graphite `wgpu` backend. The window and swapchain
are owned by an internal `winia::render::WgpuWindow`; the old `skiwin` crate and its Vulkan/GL/CPU
selection layer are no longer part of the workspace. The `vulkan`/`gl` feature flags and the
`WINIA_RENDER_BACKEND` environment variable have been removed.

## Versions and build requirements

- Rust 1.88 or newer.
- `skia-safe` 0.158.0 with the `wgpu` feature (Graphite + Dawn backend).
- The matching `wgpu` 30 types are re-exported as `skia_safe::wgpu`; winia does not declare a second
  `wgpu` dependency.
- This Skia variant has no published prebuilt archive. The first build downloads Skia and Dawn's
  WebGPU headers and compiles from source, so it requires network access, Python, Ninja, LLVM
  (`clang-cl`) and the platform C++ toolchain.
- No winia-specific environment variables are required. Cargo uses its normal defaults, and rust-skia
  detects LLVM in the standard installation locations. The following variables are optional fallbacks:
  - `LLVM_HOME`: set it only when LLVM auto-detection fails.
  - `CC` / `CXX`: set them to `clang-cl.exe` only when the default C++ compiler is incompatible.
  - `CARGO_HOME`: point it at another writable directory only when the default registry cache is not
    writable; rust-skia unpacks and patches Skia inside that source directory on Windows.
  - `CARGO_TARGET_DIR`: optionally shorten it on Windows when the default target path makes Skia's
    dependency tree exceed the legacy path limit.
  - `HTTP_PROXY` / `HTTPS_PROXY`: needed only when the network requires a proxy.

### Windows path length

`clang-cl` does not declare `longPathAware`, so the `LongPathsEnabled` registry switch does not lift the
260-character limit for it. Skia's build passes source files to `clang-cl` as paths relative to the
build directory, and the resolved length is `target-dir` + `../` prefix + `CARGO_HOME` registry path +
file name. With the default `%USERPROFILE%\.cargo` and a project below `%USERPROFILE%\Documents`, that
sum lands just above 260 and the build fails with
`GetFullPathNameA(...): The filename or extension is too long`.

Keep the target directory short. The `.cargo/` directory is gitignored, so each checkout can pin its
own value in `.cargo/config.toml` without affecting anyone else:

```toml
[build]
target-dir = "D:\\t"
```

With a two-character target root the resolved length drops to roughly 235 characters, which compiles.
Setting `CARGO_TARGET_DIR` as a user-wide environment variable also works but applies to every project
on the machine; prefer the per-checkout config.

## Frame lifecycle

The wgpu renderer is created lazily on the first draw, so a window destroyed before its first frame does
not allocate a GPU device or Graphite context. Resume recreates a destroyed renderer; resize before the
first draw is folded into the initial surface size.

1. Acquire the current `wgpu::SurfaceTexture`.
2. Wrap its texture with `graphite::wgpu::surfaces::wrap_texture`.
3. Run the existing renderer against the Skia `Surface`.
4. Snap the recorder, insert the recording and submit it while the surface is still alive.
5. Optionally read pixels back through `graphite::Context::read_pixels` for debug capture.
6. Drop the per-frame Skia surface and present the frame through `wgpu::Queue::present`.

The recorder must outlive every per-frame Skia surface, and both Graphite objects must be dropped
before the wgpu device and queue. `WgpuRenderer` declares fields in the required drop order. A bounded
`ImageProvider` uploads raster and other non-Graphite images; without one, Graphite silently drops those
draws. The cache uses LRU eviction so per-frame offscreen images do not accumulate indefinitely.

## Surface configuration

A non-sRGB surface format is preferred when available, because Skia writes sRGB-encoded values and
the swapchain must not encode them a second time. Transparent windows prefer `PreMultiplied` alpha,
then `PostMultiplied`; if the surface only supports opaque composition, winia logs the fallback.

Resize reconfigures the surface at the new physical size. `Outdated` and `Lost` surface textures
trigger reconfiguration and skip the frame. `Timeout` and `Occluded` skip presentation. Zero-sized
surfaces are ignored until the window becomes visible again.

## Debug capture

The debug server's `request_capture` / `take_capture` path now reads the submitted Graphite surface.
This is deliberately after `Context::submit`, so the capture represents the frame submitted for
presentation rather than a partially recorded frame. The surface requests `COPY_SRC` when its
capabilities allow it; platforms without that usage cannot provide pixel capture.

## Validation

```bash
cargo check -p winia --lib
cargo test -p winia --lib render::wgpu
cargo test -p winia --test wgpu_offscreen_test -- --ignored
cargo test -p winia --lib render::capture -- --ignored
cargo test -p winia --test window_lifecycle_test
```

The offscreen test requires a wgpu adapter and is ignored by default so headless test runs do not
fail for lack of a GPU. Window lifecycle and UI fixture tests exercise the real swapchain path.

Validated on Windows with an RTX 5060: offscreen pixels 1/1, Graphite capture 1/1, render tests 29 passed
plus 1 ignored GPU test, font tests 2/2, `app::window` 45/45, render snapshots 42/42, window lifecycle
25/25, and the debug-server UI suite 6/6. The all-targets check, changed-file rustfmt and
`git diff --check` also pass.

## Current limits

- The upstream backend is experimental.
- Each window currently creates its own wgpu adapter/device and Graphite context.
- A machine without a usable wgpu adapter cannot open a window; there is no CPU/GL fallback in this
  migration.
- Native validation outside Windows is still pending.
