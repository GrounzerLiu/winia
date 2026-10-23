//! The ambient layout direction — Compose's `LocalLayoutDirection`.
//!
//! It lives here, not on the theme, because it is not a Material 3 idea: `Row`/`Column` read it to
//! mirror their placements and the runtime reads it while materializing a node, neither of which may
//! depend on the design system. `WiniaTheme` PROVIDES it (a theme is a natural place to set it), and
//! that direction of dependency — theme provides, layout reads — is the one that holds.

use crate::runtime::composition_local::CompositionLocal;
use crate::layout::LayoutDirection;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

static LOCAL_DIRECTION: LazyLock<CompositionLocal<LayoutDirection>> =
    LazyLock::new(|| CompositionLocal::new(|| LayoutDirection::Ltr));

// Test/debug override: 0 = use the composition local, 1 = Ltr, 2 = Rtl. A UI test needs to
// exercise RTL without depending on an example's settings sheet; the debug-server `dir` command
// sets this and marks the tree dirty.
static DEBUG_DIRECTION: AtomicU8 = AtomicU8::new(0);

/// The ambient layout direction, or `Ltr` when nothing provided one.
pub fn current() -> LayoutDirection {
    match DEBUG_DIRECTION.load(Ordering::Relaxed) {
        1 => LayoutDirection::Ltr,
        2 => LayoutDirection::Rtl,
        _ => LOCAL_DIRECTION.current(),
    }
}

/// Set (or clear, with `None`) the debug-only direction override.
pub(crate) fn set_debug_direction(direction: Option<LayoutDirection>) {
    let value = match direction {
        Some(LayoutDirection::Ltr) => 1,
        Some(LayoutDirection::Rtl) => 2,
        None => 0,
    };
    DEBUG_DIRECTION.store(value, Ordering::Relaxed);
}

/// Provide `direction` to the subtree while `f` composes.
pub fn provides<T>(direction: LayoutDirection, f: impl FnOnce() -> T) -> T {
    LOCAL_DIRECTION.provides(direction, f)
}
