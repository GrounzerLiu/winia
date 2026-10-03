//! The ambient layout direction — Compose's `LocalLayoutDirection`.
//!
//! It lives here, not on the theme, because it is not a Material 3 idea: `Row`/`Column` read it to
//! mirror their placements and the runtime reads it while materializing a node, neither of which may
//! depend on the design system. `WiniaTheme` PROVIDES it (a theme is a natural place to set it), and
//! that direction of dependency — theme provides, layout reads — is the one that holds.

use crate::core::composition_local::CompositionLocal;
use crate::layout::LayoutDirection;
use std::sync::LazyLock;

static LOCAL_DIRECTION: LazyLock<CompositionLocal<LayoutDirection>> =
    LazyLock::new(|| CompositionLocal::new(|| LayoutDirection::Ltr));

/// The ambient layout direction, or `Ltr` when nothing provided one.
pub fn current() -> LayoutDirection {
    LOCAL_DIRECTION.current()
}

/// Provide `direction` to the subtree while `f` composes.
pub fn provides<T>(direction: LayoutDirection, f: impl FnOnce() -> T) -> T {
    LOCAL_DIRECTION.provides(direction, f)
}
