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

/// 读取当前子树 Density（默认 standard=1.0）
pub fn current_density() -> Density {
    LOCAL_DENSITY.current()
}

/// 在子树中提供 Density
pub fn with_density<R>(density: Density, content: impl FnOnce() -> R) -> R {
    LOCAL_DENSITY.provides(density, content)
}
