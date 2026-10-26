//! The sizing vocabulary the layout and the modifier chain share.
//!
//! Two types, two jobs:
//! - [`Dimension`] is a LENGTH. It always resolves to logical pixels: `Fixed`, `Dp`, `Px`, or
//!   `Dynamic` (a closure evaluated during layout — how a layout-property animation drives size,
//!   offset or padding without recomposing). Anything that needs "a number of logical pixels" takes
//!   one, so `Fill` and `Auto` cannot appear where they would mean nothing.
//! - [`SizeValue`] is HOW a node is sized: a length, or `Fill` / `Auto` / one of the content's own
//!   intrinsic measurements.

use std::sync::Arc;

/// A length — always resolvable to logical pixels.
///
/// `Dynamic` is what makes layout-property animation work: the closure is called during
/// measure/layout, so a `State<f32>` read there registers a LAYOUT dependency — the node re-measures
/// every frame without the composition running again.
#[derive(Clone)]
pub enum Dimension {
    /// Fixed logical pixels.
    Fixed(f32),
    /// Density-independent pixels (1dp == 1 logical pixel in this project, no conversion).
    Dp(crate::unit::Dp),
    /// Physical pixels, converted through the ambient density.
    Px(crate::unit::Px),
    /// Evaluated during layout; the read registers a layout dependency on the enclosing node.
    Dynamic(Arc<dyn Fn() -> f32 + Send + Sync>),
}

impl Dimension {
    pub fn is_fixed(&self) -> bool {
        !matches!(self, Dimension::Dynamic(_))
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Dimension::Dynamic(_))
    }

    /// Resolve to logical pixels. `Px` reads the ambient density; `Dynamic` evaluates its closure
    /// (during layout that read is what subscribes the node to the value).
    pub fn to_logical_px(&self) -> f32 {
        match self {
            Dimension::Fixed(v) => *v,
            Dimension::Dp(d) => d.value(),
            Dimension::Px(p) => p.to_logical(crate::runtime::density::current_density()),
            Dimension::Dynamic(f) => f(),
        }
    }
}

/// Values compare structurally for the static forms and by IDENTITY for a closure: two closures are
/// the same length only when they are the same closure object.
impl std::fmt::Debug for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dimension::Fixed(v) => write!(f, "fixed({v})"),
            Dimension::Dp(d) => write!(f, "dp({})", d.value()),
            Dimension::Px(p) => write!(f, "px({})", p.0),
            Dimension::Dynamic(_) => write!(f, "<dynamic>"),
        }
    }
}

impl Dimension {
    /// Whether two lengths are the same PARAMETER, for the element comparison: the static forms must
    /// be equal, and two dynamic forms are the same request — the number is read during layout and
    /// carried by the layout dependency. A static turning into a dynamic (or back) is a CHANGE.
    ///
    /// A named method rather than `PartialEq` on purpose: for a dynamic length there is no honest
    /// `==` (two closures cannot be compared by value), and a pointer comparison would make a
    /// rebuilt closure look like a new parameter every frame.
    pub fn same_parameter(&self, other: &Dimension) -> bool {
        match (self, other) {
            (Dimension::Fixed(a), Dimension::Fixed(b)) => a == b,
            (Dimension::Dp(a), Dimension::Dp(b)) => a == b,
            (Dimension::Px(a), Dimension::Px(b)) => a == b,
            (Dimension::Dynamic(_), Dimension::Dynamic(_)) => true,
            _ => false,
        }
    }
}

impl From<f32> for Dimension {
    fn from(v: f32) -> Self { Dimension::Fixed(v) }
}

impl From<crate::unit::Dp> for Dimension {
    fn from(d: crate::unit::Dp) -> Self { Dimension::Dp(d) }
}

impl From<crate::unit::Px> for Dimension {
    fn from(p: crate::unit::Px) -> Self { Dimension::Px(p) }
}

impl From<crate::runtime::state::State<f32>> for Dimension {
    fn from(s: crate::runtime::state::State<f32>) -> Self {
        Dimension::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<crate::runtime::state::Animating<f32>> for Dimension {
    fn from(s: crate::runtime::state::Animating<f32>) -> Self {
        Dimension::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<crate::runtime::state::DerivedValue<f32>> for Dimension {
    fn from(d: crate::runtime::state::DerivedValue<f32>) -> Self {
        Dimension::Dynamic(Arc::new(move || d.get()))
    }
}

impl From<&crate::runtime::state::State<f32>> for Dimension {
    fn from(s: &crate::runtime::state::State<f32>) -> Self {
        let s = s.clone();
        Dimension::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::runtime::state::Animating<f32>> for Dimension {
    fn from(s: &crate::runtime::state::Animating<f32>) -> Self {
        let s = s.clone();
        Dimension::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::runtime::state::DerivedValue<f32>> for Dimension {
    fn from(d: &crate::runtime::state::DerivedValue<f32>) -> Self {
        let d = d.clone();
        Dimension::Dynamic(Arc::new(move || d.get()))
    }
}

impl From<&crate::runtime::state::State<crate::unit::Dp>> for Dimension {
    fn from(s: &crate::runtime::state::State<crate::unit::Dp>) -> Self {
        let s = s.clone();
        Dimension::Dynamic(Arc::new(move || s.get().value()))
    }
}

impl From<&crate::runtime::state::Animating<crate::unit::Dp>> for Dimension {
    fn from(s: &crate::runtime::state::Animating<crate::unit::Dp>) -> Self {
        let s = s.clone();
        Dimension::Dynamic(Arc::new(move || s.get().value()))
    }
}

impl<F: Fn() -> f32 + Send + Sync + 'static> From<F> for Dimension {
    fn from(f: F) -> Self { Dimension::Dynamic(Arc::new(f)) }
}

/// Which of the content's intrinsic measurements a size modifier asks for — Compose's
/// `androidx.compose.foundation.layout.IntrinsicSize`.
///
/// An intrinsic measurement is what the content would be with NO incoming space to fill: `Min` is
/// the smallest it can be laid out at (for text, the longest unbreakable run), `Max` is the size it
/// takes with nothing wrapped. `Modifier::width(IntrinsicSize::Max)` therefore sizes a node to its
/// own content instead of to its parent, which is how Compose makes a menu exactly as wide as its
/// widest item (`material3/Menu.kt` uses `Column(width(IntrinsicSize.Max))`).
///
/// The incoming constraints still win afterwards: Compose documents the modifier as "the incoming
/// measurement constraints may override this value", and `requiredWidth/requiredHeight` are the
/// variant that ignores them (`enforceIncoming = false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSize {
    /// The smallest size the content can be laid out at (Compose `IntrinsicSize.Min`).
    Min,
    /// The size the content takes when nothing is wrapped or compressed (Compose `IntrinsicSize.Max`).
    Max,
}

/// How a node is sized: a [`Dimension`], or a layout mode that only a SIZE can have.
///
/// The modes live here rather than in `Dimension` on purpose: an offset or a padding has no "size to
/// the content", and putting them in the length type made those positions accept a value that
/// silently resolved to 0.
///
/// `Fill` is deliberately NOT a variant: a fill has to reach the measure step that tightens the axis
/// to the incoming maximum (and the scroll-viewport fallback for an infinite max), which
/// `fill_max_width`/`fill_max_height`/`fill_max_size` do and a plain size value cannot. A
/// `SizeValue::Fill` used to exist and did nothing - `size(Fill, 24.0)` measured to the content width.
pub enum SizeValue {
    Length(Dimension),
    /// Size to the content.
    Auto,
    /// Size to one of the content's own intrinsic measurements.
    Intrinsic(IntrinsicSize),
}

impl SizeValue {
    /// A per-frame length, from a closure someone already boxed.
    pub fn dynamic(f: Arc<dyn Fn() -> f32 + Send + Sync>) -> Self {
        SizeValue::Length(Dimension::Dynamic(f))
    }

    /// The length this value carries, if it is one.
    pub fn as_length(&self) -> Option<&Dimension> {
        match self {
            SizeValue::Length(d) => Some(d),
            _ => None,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, SizeValue::Length(d) if d.is_dynamic())
    }

    /// Resolve a length; `None` for the layout modes, which only a measure pass can resolve.
    pub fn to_logical_px(&self) -> Option<f32> {
        self.as_length().map(|d| d.to_logical_px())
    }
}

impl From<Dimension> for SizeValue {
    fn from(d: Dimension) -> Self { SizeValue::Length(d) }
}

impl From<f32> for SizeValue {
    fn from(v: f32) -> Self { SizeValue::Length(Dimension::Fixed(v)) }
}

impl From<crate::unit::Dp> for SizeValue {
    fn from(v: crate::unit::Dp) -> Self { SizeValue::Length(Dimension::Dp(v)) }
}

impl From<crate::unit::Px> for SizeValue {
    fn from(v: crate::unit::Px) -> Self { SizeValue::Length(Dimension::Px(v)) }
}

impl From<crate::runtime::state::State<f32>> for SizeValue {
    fn from(s: crate::runtime::state::State<f32>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl From<crate::runtime::state::Animating<f32>> for SizeValue {
    fn from(s: crate::runtime::state::Animating<f32>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl From<crate::runtime::state::DerivedValue<f32>> for SizeValue {
    fn from(d: crate::runtime::state::DerivedValue<f32>) -> Self {
        SizeValue::Length(Dimension::from(d))
    }
}

impl From<&crate::runtime::state::State<f32>> for SizeValue {
    fn from(s: &crate::runtime::state::State<f32>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl From<&crate::runtime::state::Animating<f32>> for SizeValue {
    fn from(s: &crate::runtime::state::Animating<f32>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl From<&crate::runtime::state::DerivedValue<f32>> for SizeValue {
    fn from(d: &crate::runtime::state::DerivedValue<f32>) -> Self {
        SizeValue::Length(Dimension::from(d))
    }
}

impl From<&crate::runtime::state::State<crate::unit::Dp>> for SizeValue {
    fn from(s: &crate::runtime::state::State<crate::unit::Dp>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl From<&crate::runtime::state::Animating<crate::unit::Dp>> for SizeValue {
    fn from(s: &crate::runtime::state::Animating<crate::unit::Dp>) -> Self {
        SizeValue::Length(Dimension::from(s))
    }
}

impl<F: Fn() -> f32 + Send + Sync + 'static> From<F> for SizeValue {
    fn from(f: F) -> Self { SizeValue::Length(Dimension::Dynamic(Arc::new(f))) }
}

impl From<IntrinsicSize> for SizeValue {
    fn from(s: IntrinsicSize) -> Self { SizeValue::Intrinsic(s) }
}

impl Clone for SizeValue {
    fn clone(&self) -> Self {
        match self {
            SizeValue::Length(d) => SizeValue::Length(d.clone()),
            SizeValue::Auto => SizeValue::Auto,
            SizeValue::Intrinsic(s) => SizeValue::Intrinsic(*s),
        }
    }
}

impl std::fmt::Debug for SizeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SizeValue::Length(d) => write!(f, "{d:?}"),
            SizeValue::Auto => write!(f, "auto"),
            SizeValue::Intrinsic(s) => write!(f, "intrinsic({s:?})"),
        }
    }
}

impl SizeValue {
    /// The same rule as [`Dimension::same_parameter`], one level up: the same mode twice is the same
    /// request, two lengths defer to the length, and a mode against a length is a change.
    pub fn same_parameter(&self, other: &SizeValue) -> bool {
        match (self, other) {
            (SizeValue::Length(a), SizeValue::Length(b)) => a.same_parameter(b),
            (SizeValue::Auto, SizeValue::Auto) => true,
            (SizeValue::Intrinsic(a), SizeValue::Intrinsic(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::density::with_density;

    /// Dp -> logical pixels is the dp value itself; Px -> logical pixels is px / density. The `Px` arm reads the ambient density,
    /// so this is the module that supplies it — the assertion used to sit in `unit.rs`, which must
    /// not name the runtime layer for it.
    #[test]
    fn dimension_to_logical_px_resolves_against_the_ambient_density() {
        with_density(crate::unit::Density::from_density(2.0), || {
            assert_eq!(Dimension::Dp(crate::unit::Dp(10.0)).to_logical_px(), 10.0);
            assert_eq!(Dimension::Px(crate::unit::Px(20.0)).to_logical_px(), 10.0); // 20px / 2.0
            assert_eq!(Dimension::Fixed(8.0).to_logical_px(), 8.0);
        });
    }

    /// A dynamic length is evaluated at resolve time, and it is a LENGTH — not a size mode.
    #[test]
    fn a_dynamic_dimension_resolves_and_is_a_length() {
        let v = crate::runtime::state::State::new(12.5f32);
        let d = Dimension::from(&v);
        assert!(d.is_dynamic());
        assert_eq!(d.to_logical_px(), 12.5);
        let s = SizeValue::from(d);
        assert_eq!(s.to_logical_px(), Some(12.5));
    }
}
