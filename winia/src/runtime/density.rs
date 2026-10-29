//! `LocalDensity` — the density a subtree resolves `Dp`/`Sp` against.
//!
//! Compose keeps `Density` in `ui.unit` and the *local* in `ui.platform`; winia had both in
//! `unit.rs`, which made the lowest layer name `runtime` for `CompositionLocal`. The value lives in
//! [`crate::unit`], the way it is provided lives here.
//!
//! `runtime` and not `theme` because a `CompositionLocal` is runtime machinery: the static is the
//! same shape as any other local, and `theme.rs` provides its own on top of this one.

use crate::runtime::composition_local::CompositionLocal;
use crate::unit::Density;
use std::sync::LazyLock;

static LOCAL_DENSITY: LazyLock<CompositionLocal<Density>> = LazyLock::new(|| {
    CompositionLocal::new(|| Density::standard())
});

/// The process-wide text scale — the "make text bigger" accessibility setting.
///
/// Compose reads `LocalDensity.fontScale` from the platform; winia keeps one process-wide value
/// (`platform::font_scale()`) and folds it into every `Density` a window builds, so a change shows up
/// in the next frame without any component knowing about it.
static SYSTEM_FONT_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The factor a window should put in its `Density` (1.0 until the platform answers).
pub fn system_font_scale() -> f32 {
    let bits = SYSTEM_FONT_SCALE.load(std::sync::atomic::Ordering::Relaxed);
    if bits == 0 { 1.0 } else { f32::from_bits(bits) }
}

/// Install the text scale the platform reports. A value outside 0.5..=4.0 is rejected rather than
/// folded in: a nonsense factor would resize every glyph in the app and there is no way back.
pub fn set_system_font_scale(scale: f32) -> bool {
    if !(0.5..=4.0).contains(&scale) {
        return false;
    }
    SYSTEM_FONT_SCALE.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
    true
}

/// When the platform was last asked for the text scale — the frame path calls
/// [`refresh_system_font_scale`], which throttles the registry read to this interval.
static LAST_QUERY: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// How often the frame path re-reads the platform value. A change is applied on the next frame;
/// a frame that arrives sooner than this skips the read, which is what keeps a 60 Hz app from doing
/// 60 registry reads a second.
const QUERY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Re-read the platform's text scale, install it, and say whether it CHANGED.
///
/// The caller (`AppState`'s frame path) requests a recomposition when this answers true: `Sp` is
/// resolved during composition, so a factor that arrived after the compose would only show up a
/// frame late. winit raises `WM_SETTINGCHANGE` as `ThemeChanged` only when the THEME changed, so a
/// pure text-size change never reaches the event loop — polling here is what makes the setting live.
pub fn refresh_system_font_scale() -> bool {
    let mut last = LAST_QUERY.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    if let Some(previous) = *last {
        if now.duration_since(previous) < QUERY_INTERVAL {
            return false;
        }
    }
    *last = Some(now);
    drop(last);
    install_measured_text_scale(crate::runtime::platform::font_scale())
}

/// Install a MEASURED factor and say whether it differs from the current one.
///
/// Split out of [`refresh_system_font_scale`] so the decision is testable without a registry: a
/// nonsense value is rejected (and answers false, because nothing changed).
pub fn install_measured_text_scale(measured: f32) -> bool {
    let previous = system_font_scale();
    if (measured - previous).abs() < 0.001 {
        return false;
    }
    set_system_font_scale(measured)
}

/// A `Density` for a window at `density`, carrying the process-wide text scale.
pub fn density_with_system_font_scale(density: f32) -> Density {
    Density::from_density_and_font_scale(density, system_font_scale())
}

/// 读取当前子树 Density（默认 standard=1.0）
pub fn current_density() -> Density {
    LOCAL_DENSITY.current()
}

/// 在子树中提供 Density
pub fn with_density<R>(density: Density, content: impl FnOnce() -> R) -> R {
    LOCAL_DENSITY.provides(density, content)
}
