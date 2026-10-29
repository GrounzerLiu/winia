//! UI value types — Compose's `androidx.compose.ui.unit`.
//!
//! `Dp`, `Sp`, `Offset`, `Size`, and the `Density` abstraction that converts `Dp`/`Sp` to pixels.
//!
//! ## Design
//! - `Dp` and `Sp` are typed wrappers around `f32`, so px/dp/sp cannot be mixed by accident.
//! - `Offset` and `Size` are 2D vector types with arithmetic.
//! - `Density` converts `Dp`/`Sp` to pixels; the window's `scale_factor` supplies the density and
//!   the accessibility setting supplies the font scale.
//! - All of them implement `AnimatableValue`, so they animate directly.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

// ═══════════════════════════════════════════════════════════
// Dp — density-independent pixels (1 dp is about 1/160 inch).
// ═══════════════════════════════════════════════════════════

/// A density-independent pixel.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Dp(pub f32);

impl Dp {
    pub fn value(&self) -> f32 { self.0 }

    /// Convert to physical pixels through a [`Density`].
    pub fn to_px(&self, density: Density) -> f32 { self.0 * density.density }

    /// This dp as a **layout coordinate**.
    ///
    /// Layout coordinates in this framework are LOGICAL pixels and one dp IS one logical
    /// pixel — `Dimension::to_logical_px` returns `Dp::value()` unchanged, pinned by its own
    /// test at density 2.0 — so this is simply the number.
    ///
    /// It exists because the wrong choice is the inviting one: `to_px` reads like "give me
    /// the pixel value", but it returns a PHYSICAL value, which is wrong anywhere the result
    /// is compared with, assigned to, or measured against layout geometry — window sizes,
    /// constraint values, `Modifier::size`/`padding`/`offset`, anchor positions. Sizing a
    /// 360dp drawer with `to_px` at 1.5x density asks for 540 *logical* px, which silently
    /// fills a 520-wide window. Use this one in layout code; `to_px` is for a length that
    /// leaves the layout system (the platform, a canvas in device space).
    pub fn to_logical(&self) -> f32 { self.0 }
}

// ── Dp arithmetic ──

impl Add for Dp {
    type Output = Dp;
    fn add(self, rhs: Dp) -> Dp { Dp(self.0 + rhs.0) }
}
impl AddAssign for Dp {
    fn add_assign(&mut self, rhs: Dp) { self.0 += rhs.0; }
}
impl Sub for Dp {
    type Output = Dp;
    fn sub(self, rhs: Dp) -> Dp { Dp(self.0 - rhs.0) }
}
impl SubAssign for Dp {
    fn sub_assign(&mut self, rhs: Dp) { self.0 -= rhs.0; }
}
impl Mul<f32> for Dp {
    type Output = Dp;
    fn mul(self, rhs: f32) -> Dp { Dp(self.0 * rhs) }
}
impl Div<f32> for Dp {
    type Output = Dp;
    fn div(self, rhs: f32) -> Dp { Dp(self.0 / rhs) }
}
impl Neg for Dp {
    type Output = Dp;
    fn neg(self) -> Dp { Dp(-self.0) }
}
impl From<f32> for Dp {
    fn from(v: f32) -> Self { Dp(v) }
}
impl From<Dp> for f32 {
    fn from(d: Dp) -> Self { d.0 }
}

// ═══════════════════════════════════════════════════════════
// Sp — scale-independent pixels: font sizes, which follow the user's text-size setting.
// ═══════════════════════════════════════════════════════════

/// Scaled pixels — a font size, which follows the user's text-scale setting.
///
/// `Dp` does NOT scale with it: an accessibility text size makes the glyphs bigger, not the
/// buttons. `Density.font_scale` carries the factor and is installed by the window (see
/// `runtime::density::set_system_font_scale`); a `Density` built without one answers 1.0, which is
/// the old behaviour for every test and for any embedder that never sets it.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Sp(pub f32);

impl Sp {
    pub fn value(&self) -> f32 { self.0 }

    /// Convert to physical pixels, applying both the density and the font scale.
    pub fn to_px(&self, density: Density) -> f32 {
        self.0 * density.density * density.font_scale
    }
}

// ── Sp arithmetic ──

impl Add for Sp {
    type Output = Sp;
    fn add(self, rhs: Sp) -> Sp { Sp(self.0 + rhs.0) }
}
impl Sub for Sp {
    type Output = Sp;
    fn sub(self, rhs: Sp) -> Sp { Sp(self.0 - rhs.0) }
}
impl Mul<f32> for Sp {
    type Output = Sp;
    fn mul(self, rhs: f32) -> Sp { Sp(self.0 * rhs) }
}
impl Div<f32> for Sp {
    type Output = Sp;
    fn div(self, rhs: f32) -> Sp { Sp(self.0 / rhs) }
}
impl From<f32> for Sp {
    fn from(v: f32) -> Self { Sp(v) }
}
impl From<Sp> for f32 {
    fn from(s: Sp) -> Self { s.0 }
}

// ═══════════════════════════════════════════════════════════
// Density — the Dp/Sp <-> px conversion.
// ═══════════════════════════════════════════════════════════

/// Screen density plus the user's text-scale factor.
///
/// `density` converts physical pixels; `font_scale` multiplies `Sp` only, and comes from the
/// platform (`from_density_and_font_scale`). `from_density` leaves it at 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Density {
    /// Logical pixels per physical pixel (1.0 is mdpi).
    pub density: f32,
    /// The user's text scale (the accessibility setting; above 1.0 for larger text).
    pub font_scale: f32,
}

impl Density {
    pub const fn from_density(density: f32) -> Self {
        Self { density, font_scale: 1.0 }
    }

    /// A density that also carries the user's text scale (Compose's `Density(density, fontScale)`).
    pub const fn from_density_and_font_scale(density: f32, font_scale: f32) -> Self {
        Self { density, font_scale }
    }
    pub const fn standard() -> Self {
        Self { density: 1.0, font_scale: 1.0 }
    }

    pub fn to_px(&self, dp: Dp) -> f32 { dp.0 * self.density }
}

// ═══════════════════════════════════════════════════════════
// Offset — Compose's `Offset`.
// ═══════════════════════════════════════════════════════════

/// A 2D offset in `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const ZERO: Offset = Offset { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self { Offset { x, y } }
}

impl Add for Offset {
    type Output = Offset;
    fn add(self, rhs: Offset) -> Offset { Offset::new(self.x + rhs.x, self.y + rhs.y) }
}
impl Sub for Offset {
    type Output = Offset;
    fn sub(self, rhs: Offset) -> Offset { Offset::new(self.x - rhs.x, self.y - rhs.y) }
}
impl Neg for Offset {
    type Output = Offset;
    fn neg(self) -> Offset { Offset::new(-self.x, -self.y) }
}

// ═══════════════════════════════════════════════════════════
// Size — Compose's `Size`.
// ═══════════════════════════════════════════════════════════

/// A 2D size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };

    pub fn new(width: f32, height: f32) -> Self { Size { width, height } }

    /// Whether a point falls inside this size's rectangle.
    pub fn contains(&self, offset: Offset) -> bool {
        offset.x >= 0.0 && offset.x < self.width && offset.y >= 0.0 && offset.y < self.height
    }
}

impl Add for Size {
    type Output = Size;
    fn add(self, rhs: Size) -> Size { Size::new(self.width + rhs.width, self.height + rhs.height) }
}
impl Sub for Size {
    type Output = Size;
    fn sub(self, rhs: Size) -> Size { Size::new(self.width - rhs.width, self.height - rhs.height) }
}

// ═══════════════════════════════════════════════════════════
// `AnimatableValue` impls — the animation system's entry point.
// ═══════════════════════════════════════════════════════════

impl crate::animation::AnimatableValue for Dp {
    fn lerp(&self, to: &Dp, t: f32) -> Dp { Dp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Dp { Dp(v) }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for i32 {
    fn lerp(&self, to: &i32, t: f32) -> i32 {
        let from = *self as f32;
        (from + (*to as f32 - from) * t).round() as i32
    }
    fn to_f32(&self) -> f32 { *self as f32 }
    fn from_f32(v: f32) -> i32 { v.round() as i32 }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for Sp {
    fn lerp(&self, to: &Sp, t: f32) -> Sp { Sp(self.0 + (to.0 - self.0) * t) }
    fn to_f32(&self) -> f32 { self.0 }
    fn from_f32(v: f32) -> Sp { Sp(v) }
    fn supports_spring() -> bool { true }
}

impl crate::animation::AnimatableValue for Offset {
    fn lerp(&self, to: &Offset, t: f32) -> Offset {
        Offset::new(self.x + (to.x - self.x) * t, self.y + (to.y - self.y) * t)
    }
    fn to_f32(&self) -> f32 { (self.x * self.x + self.y * self.y).sqrt() }
    fn from_f32(v: f32) -> Offset { Offset::new(v, v) }
    // One spring per axis, exactly as Compose's `Offset.VectorConverter` expands it, so a Spring
    // never has to fall back to a Tween.
    fn supports_spring() -> bool { true }
    fn write_components(
        &self,
        out: &mut [f32; crate::animation::MAX_ANIMATION_COMPONENTS],
    ) -> usize {
        out[0] = self.x;
        out[1] = self.y;
        2
    }
    fn from_components(components: &[f32]) -> Offset {
        Offset::new(components[0], components[1])
    }
}

impl crate::animation::AnimatableValue for Size {
    fn lerp(&self, to: &Size, t: f32) -> Size {
        Size::new(self.width + (to.width - self.width) * t, self.height + (to.height - self.height) * t)
    }
    fn to_f32(&self) -> f32 { (self.width * self.width + self.height * self.height).sqrt() }
    fn from_f32(v: f32) -> Size { Size::new(v, v) }
    // One spring per dimension, matching Compose's `IntSize.VectorConverter`: both axes reach
    // their targets instead of sharing one displacement along the diagonal.
    fn supports_spring() -> bool { true }
    fn write_components(
        &self,
        out: &mut [f32; crate::animation::MAX_ANIMATION_COMPONENTS],
    ) -> usize {
        out[0] = self.width;
        out[1] = self.height;
        2
    }
    fn from_components(components: &[f32]) -> Size {
        Size::new(components[0], components[1])
    }
}

// ═══════════════════════════════════════════════════════════
// The Dp/Sp extension traits (Compose's `100.dp` / `14.sp`).
// ═══════════════════════════════════════════════════════════

/// A number as `Dp` (`100.dp()`, Compose's `100.dp`).
pub trait DpExt {
    fn dp(self) -> Dp;
}

impl DpExt for f32 {
    fn dp(self) -> Dp { Dp(self) }
}
impl DpExt for i32 {
    fn dp(self) -> Dp { Dp(self as f32) }
}
impl DpExt for u32 {
    fn dp(self) -> Dp { Dp(self as f32) }
}

/// A number as `Sp` (`14.sp()`, Compose's `14.sp`).
pub trait SpExt {
    fn sp(self) -> Sp;
}

impl SpExt for f32 {
    fn sp(self) -> Sp { Sp(self) }
}
impl SpExt for i32 {
    fn sp(self) -> Sp { Sp(self as f32) }
}
impl SpExt for u32 {
    fn sp(self) -> Sp { Sp(self as f32) }
}

// ═══════════════════════════════════════════════════════════
// Px — physical pixels.
// ═══════════════════════════════════════════════════════════

/// A physical pixel. Layout works in logical pixels, so a `Px` needs a [`Density`] to get there.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Px(pub f32);

impl Px {
    /// Convert to logical pixels through a [`Density`].
    pub fn to_logical(&self, density: Density) -> f32 {
        self.0 / density.density.max(f32::EPSILON)
    }
}

/// A number as `Px` (`20.px()`).
pub trait PxExt {
    fn px(self) -> Px;
}

impl PxExt for f32 {
    fn px(self) -> Px { Px(self) }
}
impl PxExt for i32 {
    fn px(self) -> Px { Px(self as f32) }
}
impl PxExt for u32 {
    fn px(self) -> Px { Px(self as f32) }
}

// ═══════════════════════════════════════════════════════════
// TextUnit — Compose's `TextUnit`: one type for `Sp` and `Px` text sizes.
// ═══════════════════════════════════════════════════════════

/// A text size unit: `Sp` (scaled pixels) or `Px` (physical pixels).
///
/// A bare `f32` is an `Sp` (font scale 1 makes it a logical pixel, which keeps older call sites
/// unchanged).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextUnit {
    /// Scaled pixels — multiplied by `Density.font_scale` on the way to logical pixels.
    Sp(Sp),
    /// Physical pixels; a [`Density`] turns them into logical ones.
    Px(Px),
    /// A multiple of the ENCLOSING font size (Compose's `TextUnitType.Em`).
    ///
    /// It has no absolute value on its own — `to_logical_px` has no base to multiply — so the
    /// style merge resolves it against the inherited size before a leaf sees it (`Text::build`,
    /// RichText's `resolve_base`). `to_logical_px` still answers something for one that slipped
    /// through (the raw multiple) because a panic in the measure path would be worse than a size.
    Em(f32),
}

impl TextUnit {
    /// Resolve to logical pixels: `Sp` scales by `Density.font_scale`, `Px` by `Density.density`.
    ///
    /// The density is a parameter and not an ambient lookup: asking for it would make this module —
    /// the lowest one — name the composition runtime. Compose does the same
    /// (`TextUnit.toPx(density)`), and the caller that has a `Density` in hand is the one that
    /// knows which subtree it is resolving for.
    pub fn to_logical_px(&self, density: Density) -> f32 {
        match self {
            // Compose's `TextUnit.toPx(density)`: an sp value is multiplied by the font scale.
            TextUnit::Sp(s) => s.value() * density.font_scale,
            TextUnit::Px(p) => p.to_logical(density),
            // Unresolved `Em`: see the variant's note. The style merge is supposed to have
            // replaced it with an `Sp` before a leaf sees it.
            TextUnit::Em(em) => *em,
        }
    }
}

impl Default for TextUnit {
    /// Zero `sp` — the neutral value a style means when it says nothing (Compose uses
    /// `TextUnit.Unspecified`, which would need a third state in every comparison).
    fn default() -> Self { TextUnit::Sp(Sp(0.0)) }
}

impl From<f32> for TextUnit {
    fn from(v: f32) -> Self { TextUnit::Sp(Sp(v)) }
}
impl From<Sp> for TextUnit {
    fn from(s: Sp) -> Self { TextUnit::Sp(s) }
}
impl From<Px> for TextUnit {
    fn from(p: Px) -> Self { TextUnit::Px(p) }
}

// ═══════════════════════════════════════════════════════════
// Unit tests.
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_px_conversion() {
        let d = Density::from_density(2.0);
        let dp = Dp(10.0);
        assert_eq!(dp.to_px(d), 20.0);
        // The two are NOT interchangeable: `to_px` is a device-space length, `to_logical` is
        // the layout coordinate (one dp is one logical px — see `Dimension::to_logical_px`).
        // Reaching for `to_px` in layout code is what sized a 640dp sheet to the whole window
        // on a HiDPI display.
        assert_eq!(dp.to_logical(), 10.0);
        assert_ne!(dp.to_logical(), dp.to_px(d));
    }

    /// A `Density` with a text scale multiplies `Sp` and leaves `Dp` alone; 1.0 is the old
    /// behaviour exactly.
    #[test]
    fn font_scale_multiplies_sp_only() {
        let plain = Density::from_density(2.0);
        let scaled = Density::from_density_and_font_scale(2.0, 1.5);
        assert_eq!(plain.font_scale, 1.0, "from_density leaves the text scale alone");
        assert_eq!(TextUnit::Sp(Sp(10.0)).to_logical_px(plain), 10.0);
        assert_eq!(TextUnit::Sp(Sp(10.0)).to_logical_px(scaled), 15.0);
        assert_eq!(Dp(10.0).to_px(scaled), 20.0, "a dp ignores the text scale");
        assert_eq!(TextUnit::Sp(Sp(10.0)).to_logical_px(scaled), 15.0);
    }

    /// A factor is only installed when it is usable: a nonsense value (a corrupted registry entry)
    /// must not resize every glyph in the app.
    #[test]
    fn a_nonsense_text_scale_is_rejected() {
        assert!(!crate::runtime::density::set_system_font_scale(0.0));
        assert!(!crate::runtime::density::set_system_font_scale(-1.0));
        assert!(!crate::runtime::density::set_system_font_scale(50.0));
        let previous = crate::runtime::density::system_font_scale();
        assert!(crate::runtime::density::set_system_font_scale(1.25));
        assert_eq!(crate::runtime::density::system_font_scale(), 1.25);
        let _ = crate::runtime::density::set_system_font_scale(previous);
    }
    #[test]
    fn sp_px_conversion_with_font_scale() {
        let d = Density::from_density(2.0);
        let sp = Sp(12.0);
        assert_eq!(sp.to_px(d), 24.0);
    }

    #[test]
    fn dp_arithmetic() {
        let a = Dp(5.0);
        let b = Dp(3.0);
        assert_eq!(a + b, Dp(8.0));
        assert_eq!(a - b, Dp(2.0));
        assert_eq!(a * 2.0, Dp(10.0));
        assert_eq!(a / 2.0, Dp(2.5));
        assert_eq!(-a, Dp(-5.0));
    }

    #[test]
    fn offset_size_ops() {
        let o = Offset::new(1.0, 2.0);
        let o2 = Offset::new(3.0, 4.0);
        assert_eq!(o + o2, Offset::new(4.0, 6.0));
        assert_eq!(o2 - o, Offset::new(2.0, 2.0));

        let s = Size::new(100.0, 50.0);
        assert!(s.contains(Offset::new(50.0, 25.0)));
        assert!(!s.contains(Offset::new(101.0, 25.0)));
    }

    #[test]
    fn animatable_value_impls() {
        use crate::animation::AnimatableValue;
        // Dp animation.
        let a = Dp(0.0);
        let b = Dp(100.0);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid, Dp(50.0));
        // Offset animation.
        let o = Offset::new(0.0, 0.0).lerp(&Offset::new(10.0, 20.0), 0.5);
        assert_eq!(o, Offset::new(5.0, 10.0));
        // Size animation.
        let s = Size::new(0.0, 0.0).lerp(&Size::new(8.0, 6.0), 0.25);
        assert_eq!(s, Size::new(2.0, 1.5));
    }

    #[test]
    fn dp_sp_extensions() {
        // Compose's `100.dp` / `14.sp`.
        let w: Dp = 100.dp();
        assert_eq!(w, Dp(100.0));
        let f: Dp = 2.5f32.dp();
        assert_eq!(f, Dp(2.5));
        let u: Dp = 30u32.dp();
        assert_eq!(u, Dp(30.0));

        let s: Sp = 14.sp();
        assert_eq!(s, Sp(14.0));
        let sf: Sp = 1.5f32.sp();
        assert_eq!(sf, Sp(1.5));
    }

    #[test]
    fn px_extension_and_dimension_conversion() {
        // 20.px() → Px
        let px: Px = 20.px();
        assert_eq!(px, Px(20.0));
        let pxf: Px = 3.5f32.px();
        assert_eq!(pxf, Px(3.5));

        // Dp/Px -> Dimension, which `.size()` consumes.
        use crate::layout::Dimension;
        let d: Dimension = 10.dp().into();
        assert!(d.same_parameter(&Dimension::Dp(Dp(10.0))));
        let p: Dimension = 20.px().into();
        assert!(p.same_parameter(&Dimension::Px(Px(20.0))));

        // `Dimension::to_logical_px` needs the ambient density, so its assertion lives beside it
        // in `layout/sizing.rs` rather than here.
    }
    /// Guard for the class of bug this module's doc comments describe.
    ///
    /// `to_px` reads like "the pixel value", but it returns a PHYSICAL length while every
    /// length in layout code is LOGICAL (one dp is one logical px). It was wrong at six sites
    /// across three rounds — the navigation drawer's width, both sheets' width and three peek
    /// sites in the scaffold (all found one at a time, all with the tests green), and then the
    /// fling velocity threshold, which only a codebase-wide sweep turned up. Every one of them
    /// compiled and passed its tests, and shipped as a HiDPI rendering bug.
    ///
    /// There are no callers left outside this file, so this test keeps it that way. If you are
    /// adding one deliberately — a length that LEAVES the layout system, for the platform or a
    /// canvas in device space — extend this test and say why in the commit. Anything that is
    /// compared with or assigned to layout geometry wants `to_logical()`.
    #[test]
    fn no_code_outside_this_module_converts_a_dp_to_physical_px() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        scan_dir(&root, &mut offenders);
        assert!(
            offenders.is_empty(),
            "`to_px` is for lengths leaving the layout system; layout lengths are logical              (`to_logical`). Callers found: {offenders:?}"
        );
    }

    fn scan_dir(dir: &std::path::Path, offenders: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_dir(&path, offenders);
                continue;
            }
            let is_rust = path.extension().is_some_and(|e| e == "rs");
            let is_this_file = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy() == "unit.rs");
            if !is_rust || is_this_file {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for (i, line) in text.lines().enumerate() {
                // Comments are skipped: the doc comments that explain this trap quote the
                // wrong call on purpose.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains(".to_px(") {
                    offenders.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
    }
}
