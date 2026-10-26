use super::*;

#[derive(Clone, Debug)]
pub enum AnimationSpec {
    Spring(SpringSpec),
    Tween(TweenSpec),
    /// A keyframe sequence (cf. Compose `keyframes`).
    Keyframes(KeyframesSpec),
    /// Run a sub-animation repeatedly (cf. Compose `repeatable`).
    Repeatable(RepeatableSpec),
    /// Instant jump to the target (cf. Compose `snap`), optionally after a delay
    Snap(SnapSpec),
    /// Exponential decay (cf. Compose `exponentialDecay` — fling/inertial scrolling, no target value).
    Decay(DecaySpec),
}

impl From<TweenSpec> for AnimationSpec {
    fn from(s: TweenSpec) -> Self { AnimationSpec::Tween(s) }
}

impl From<SpringSpec> for AnimationSpec {
    fn from(s: SpringSpec) -> Self { AnimationSpec::Spring(s) }
}

impl From<DecaySpec> for AnimationSpec {
    fn from(s: DecaySpec) -> Self { AnimationSpec::Decay(s) }
}

impl AnimationSpec {
    /// Compose's `visibilityThreshold` argument — how close a spring has to get before it counts as
    /// arrived, in the value's own unit. In Compose the threshold is a construction parameter of the
    /// `Animatable` (and the default spec is built with it: `spring(visibilityThreshold = …)`); winia's
    /// springs carry theirs on the spec, so this is the per-call override.
    ///
    /// A no-op for specs with no threshold: a `Tween`/`Keyframes` ends on its clock, and a `Decay`'s
    /// stop threshold is its own (`DecaySpec::threshold`).
    pub fn with_visibility_threshold(self, threshold: f32) -> Self {
        match self {
            AnimationSpec::Spring(spec) => AnimationSpec::Spring(SpringSpec { threshold, ..spec }),
            // A repeat's base is a `DurationSpec` (tween/keyframes/snap): none of them has a
            // visibility threshold, so there is nothing to carry here.
            AnimationSpec::Repeatable(spec) => AnimationSpec::Repeatable(spec),
            other => other,
        }
    }
}

/// The physical constants of a spring animation (`AnimationSpec::Spring`). `PartialEq` so a component
/// can assert the spec it hands the engine is the motion-scheme token it claims to be.
#[derive(Clone, Debug, PartialEq)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    pub mass: f32,
    pub threshold: f32,
}

impl Default for SpringSpec {
    fn default() -> Self {
        Self {
            damping_ratio: 1.0,
            // Compose StiffnessLow (200): settles in ~300-400ms with a smooth, pronounced transition
            stiffness: 200.0,
            mass: 1.0,
            threshold: 0.01,
        }
    }
}

impl SpringSpec {
    // -- Compose Spring constants (cf. androidx.compose.animation.core.Spring) --
    /// Damping ratio: no bounce (1.0 — critically damped).
    pub const DAMPING_RATIO_NO_BOUNCY: f32 = 1.0;
    /// Damping ratio: low bounce (0.75).
    pub const DAMPING_RATIO_LOW_BOUNCY: f32 = 0.75;
    /// Damping ratio: medium bounce (0.5).
    pub const DAMPING_RATIO_MEDIUM_BOUNCY: f32 = 0.5;
    /// Damping ratio: high bounce (0.4).
    pub const DAMPING_RATIO_HIGH_BOUNCY: f32 = 0.4;
    /// Stiffness: very low (50 — slow and soft).
    pub const STIFFNESS_VERY_LOW: f32 = 50.0;
    /// Stiffness: low (200).
    pub const STIFFNESS_LOW: f32 = 200.0;
    /// Stiffness: medium (400).
    pub const STIFFNESS_MEDIUM: f32 = 400.0;
    /// Stiffness: high (1000 — fast and crisp).
    pub const STIFFNESS_HIGH: f32 = 1000.0;
    /// M3's `StandardMotionTokens.SpringDefaultEffectsStiffness` (1600) — the spring behind
    /// `MotionSchemeKeyTokens.DefaultEffects`.
    pub const STIFFNESS_EFFECTS_DEFAULT: f32 = 1600.0;
    /// M3's `StandardMotionTokens.SpringFastEffectsStiffness` (3800) — the spring behind
    /// `MotionSchemeKeyTokens.FastEffects`.
    pub const STIFFNESS_EFFECTS_FAST: f32 = 3800.0;
    /// Every M3 spatial spring shares this damping ratio
    /// (`StandardMotionTokens.Spring{Default,Fast,Slow}SpatialDamping = 0.9f`): a spatial move carries a
    /// slight overshoot, where an effect spring (damping 1.0) settles without one.
    pub const DAMPING_RATIO_SPATIAL: f32 = 0.9;
    /// `SpringSlowSpatialStiffness` (300) — a large move (a sheet, a full-screen expansion).
    pub const STIFFNESS_SPATIAL_SLOW: f32 = 300.0;
    /// `SpringDefaultSpatialStiffness` (700) — the ordinary move (an indicator sliding, a check drawing).
    pub const STIFFNESS_SPATIAL_DEFAULT: f32 = 700.0;
    /// `SpringFastSpatialStiffness` (1400) — a small move (a dot growing, a thumb sliding).
    pub const STIFFNESS_SPATIAL_FAST: f32 = 1400.0;

    pub fn bouncy() -> Self {
        Self { damping_ratio: 0.6, threshold: 0.1, ..Self::default() }
    }

    /// The theme's `DefaultEffects` token — `WiniaTheme::motion_scheme().default_effects`, which in the
    /// standard scheme is `spring(dampingRatio = 1.0, stiffness = 1600)`
    /// (`material3/MotionScheme.kt:152-156`, `tokens/StandardMotionTokens.kt:22-23`). Compose's
    /// `MotionSchemeKeyTokens.DefaultEffects.value()` resolves to exactly this spring, which is what
    /// M3 uses for a state arriving: a checkbox filling, a radio button dot appearing, a tab's
    /// content colour fading IN.
    ///
    /// The threshold is the default 0.01 — for a colour that is one eight-bit step of Oklab
    /// lightness, and for a length one hundredth of a dp.
    pub fn effects_default() -> Self {
        crate::theme::WiniaTheme::motion_scheme().default_effects
    }

    /// The theme's `FastEffects` token — `spring(dampingRatio = 1.0, stiffness = 3800)` in the standard
    /// scheme (the schemes share their effect springs)
    /// (`material3/MotionScheme.kt:158-162`). M3 uses it for a state LEAVING — a checkbox emptying, a
    /// tab's colour fading out — so the exit is quicker than the arrival.
    pub fn effects_fast() -> Self {
        crate::theme::WiniaTheme::motion_scheme().fast_effects
    }

    /// The theme's `SlowEffects` token — the effect spring for a change that should read as slow; damping 1.0
    /// and stiffness 800 in either scheme.
    pub fn effects_slow() -> Self {
        crate::theme::WiniaTheme::motion_scheme().slow_effects
    }

    /// The theme's `DefaultSpatial` token — `spring(dampingRatio = 0.9, stiffness = 700)` in the standard
    /// scheme, `spring(0.8, 380)` in the expressive one
    /// (`material3/MotionScheme.kt:134-139`, `tokens/StandardMotionTokens.kt:20-21`). This is what M3
    /// moves a POSITION or a size with — a tab indicator travelling, a checkmark drawing, a segmented
    /// button's item resizing — and the 0.9 damping is deliberate: these moves overshoot slightly,
    /// unlike the effect springs above. winia used to approximate it with a no-bouncy spring.
    pub fn spatial_default() -> Self {
        crate::theme::WiniaTheme::motion_scheme().default_spatial
    }

    /// The theme's `FastSpatial` token — `spring(dampingRatio = 0.9, stiffness = 1400)` in the standard
    /// scheme, `spring(0.6, 800)` in the expressive one (`MotionScheme.kt:140-145`)
    /// — a small spatial change: a dot growing, a switch thumb sliding, an icon scaling
    /// (`NavigationBar.kt:270`, `RadioButton.kt:86`, `Switch.kt:162`, `SegmentedButton.kt:391`).
    pub fn spatial_fast() -> Self {
        crate::theme::WiniaTheme::motion_scheme().fast_spatial
    }

    /// The theme's `SlowSpatial` token — `spring(dampingRatio = 0.9, stiffness = 300)` in the standard
    /// scheme, `spring(0.8, 200)` in the expressive one (`MotionScheme.kt:146-151`)
    /// — a large spatial change, such as a sheet or an expanded container.
    pub fn spatial_slow() -> Self {
        crate::theme::WiniaTheme::motion_scheme().slow_spatial
    }
}

#[derive(Clone, Debug)]
pub struct TweenSpec {
    pub duration: Duration,
    /// Wait before the tween starts — during it the value stays at `from` (cf. Compose
    /// `tween(delayMillis)`). `Duration::ZERO` is the plain tween, so a spec that never sets it
    /// behaves exactly as before.
    pub delay: Duration,
    /// Table-driven interpolator (a v1 pre-sampled table, so no heavy computation at runtime; `Linear::new()` is the identity).
    pub interpolator: std::sync::Arc<dyn interpolator::Interpolator>,
}

impl TweenSpec {
    /// Convenience constructor: `TweenSpec::new(duration, interpolator)` — the interpolator is wrapped in
    /// `Arc<dyn Interpolator>` automatically (the 29 table-driven interpolators, or `Linear` passed straight in).
    pub fn new(
        duration: Duration,
        interpolator: impl Into<std::sync::Arc<dyn interpolator::Interpolator>>,
    ) -> Self {
        Self { duration, delay: Duration::ZERO, interpolator: interpolator.into() }
    }

    /// The same tween, waiting `delay` before it starts (Compose's `tween(delayMillis)`).
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl Default for TweenSpec {
    /// Compose's `tween()` with no arguments: `Duration = DefaultDurationMillis` (300 ms) and
    /// `easing = FastOutSlowInEasing` (`animation/core/AnimationSpec.kt:816-820`,
    /// `core/Easing.kt:50`). Linear interpolation is NOT the default there — a tween that eases in and
    /// out is — so `TweenSpec::new(duration, interpolator::Linear::new())` is what an explicitly linear
    /// ramp looks like.
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(300),
            delay: Duration::ZERO,
            interpolator: std::sync::Arc::new(interpolator::fast_out_slow_in()),
        }
    }
}

/// A keyframe sequence: `(progress 0..1, value 0..1, segment interpolator)`, evaluated with the value
/// applied as a FRACTION of the from→to displacement.
///
/// That fraction is a deliberate difference from Compose: `keyframes { durationMillis = 400; 1.2f at
/// 0.6f }` carries a value in the ANIMATED TYPE's units (1.2 px, say) plus an optional `Easing` per
/// keyframe, while winia's needs to stay type-generic — the same spec drives a `Color` or a `Size`,
/// where "the value in the target's units" does not exist. Per-segment easing IS available (the third
/// element of a frame, see `interpolator`); `new` fills it with `Linear`.
#[derive(Clone, Debug)]
pub struct KeyframesSpec {
    pub duration: Duration,
    pub frames: Vec<(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)>,
}

impl KeyframesSpec {
    /// Simplified constructor: just (progress, value), linear between segments.
    pub fn new(duration: Duration, frames: Vec<(f32, f32)>) -> Self {
        let linear: std::sync::Arc<dyn interpolator::Interpolator> =
            std::sync::Arc::new(interpolator::Linear::new());
        let frames = frames.into_iter().map(|(p, v)| (p, v, linear.clone())).collect();
        Self { duration, frames }
    }
}

/// Compose-shaped keyframes: absolute millisecond times and values in the ANIMATED TYPE's units, with an
/// optional easing per keyframe —
///
/// ```ignore
/// // keyframes { durationMillis = 400; 0f at 0; 340f at 200; 300f at 400 } on a 100 → 300 flight
/// let spec = KeyframesOf::between(100.0, 300.0).at(0, 100.0).at(200, 340.0).at(400, 300.0).build();
/// ```
///
/// `build` normalises the band's endpoints into the fraction form the engine evaluates (see
/// `KeyframesSpec`), so the shape reads the way Compose's does while one spec still drives a `Color` or a
/// `Size`. A keyframe with no easing interpolates linearly. When the caller leaves the 0 ms keyframe out,
/// one is supplied at the `from` value — Compose's "the animation starts at the initial value".
pub struct KeyframesOf {
    from: f32,
    to: f32,
    frames: Vec<(u64, f32, Option<std::sync::Arc<dyn interpolator::Interpolator>>)>,
}

impl KeyframesOf {
    /// The band the values are measured in — the value the animation starts at and the one it targets.
    pub fn between(from: f32, to: f32) -> Self {
        Self { from, to, frames: Vec::new() }
    }

    /// `value at millis`, interpolated linearly from the previous keyframe.
    pub fn at(mut self, millis: u64, value: f32) -> Self {
        self.frames.push((millis, value, None));
        self
    }

    /// `value at millis with easing` — Compose's `value at millis with Easing`.
    pub fn at_with(
        mut self,
        millis: u64,
        value: f32,
        easing: std::sync::Arc<dyn interpolator::Interpolator>,
    ) -> Self {
        self.frames.push((millis, value, Some(easing)));
        self
    }

    pub fn build(mut self) -> KeyframesSpec {
        // Last keyframe wins for a repeated millisecond value; the order is otherwise the caller's.
        self.frames.sort_by_key(|(millis, _, _)| *millis);
        self.frames.dedup_by_key(|(millis, _, _)| *millis);
        let linear: std::sync::Arc<dyn interpolator::Interpolator> =
            std::sync::Arc::new(interpolator::Linear::new());
        let duration_ms = self.frames.last().map(|(millis, _, _)| *millis).unwrap_or(0);
        if self.frames.first().map(|(millis, _, _)| *millis).unwrap_or(0) != 0 {
            // Compose starts from the initial value when there is no keyframe at 0.
            self.frames.insert(0, (0, self.from, None));
        }
        let span = self.to - self.from;
        let frames = self
            .frames
            .into_iter()
            .map(|(millis, value, easing)| {
                let progress = if duration_ms == 0 {
                    0.0
                } else {
                    millis as f32 / duration_ms as f32
                };
                // A band with no width (from == to) has nothing to interpolate: every keyframe value
                // collapses to it, which is what the engine's `from.lerp(&to, …)` produces anyway.
                let fraction = if span.abs() <= f32::EPSILON {
                    0.0
                } else {
                    (value - self.from) / span
                };
                (progress, fraction, easing.unwrap_or_else(|| linear.clone()))
            })
            .collect();
        KeyframesSpec { duration: Duration::from_millis(duration_ms), frames }
    }
}

/// Repeat: completes after `iterations` runs.
#[derive(Clone, Debug)]
pub struct RepeatableSpec {
    pub iterations: u32,
    pub mode: RepeatMode,
    /// The cycle's curve. A `DurationSpec` rather than any `AnimationSpec`: a spring has no cycle
    /// length, so the invalid combinations are unrepresentable instead of runtime-checked.
    pub base: DurationSpec,
    /// Compose's `initialStartOffset`: `Delay` holds the first value for this long before the first
    /// cycle, `FastForward` starts that far into it. The offset applies to the FIRST cycle only.
    pub(crate) start_offset: Duration,
    pub(crate) start_offset_type: StartOffsetType,
}

/// `snap(delayMillis = …)` — Compose's `SnapSpec`: hold the value where it is for the delay, then jump
/// straight to the target. A zero delay (the default) is the plain instant jump the engine always had.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapSpec {
    pub delay: Duration,
}

impl SnapSpec {
    pub fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

impl Default for SnapSpec {
    fn default() -> Self {
        Self { delay: Duration::ZERO }
    }
}

impl RepeatableSpec {
    pub fn new(iterations: u32, mode: RepeatMode, base: impl Into<DurationSpec>) -> Self {
        Self {
            iterations,
            mode,
            base: base.into(),
            start_offset: Duration::ZERO,
            start_offset_type: StartOffsetType::Delay,
        }
    }

    /// Compose's `repeatable(..., initialStartOffset = StartOffset(…))`: `StartOffsetType::Delay` holds
    /// the first value for `offset` before the first cycle, `FastForward` starts that far into it. The
    /// offset applies to the first cycle only; later cycles follow normally.
    pub fn with_start_offset(mut self, offset: Duration, kind: StartOffsetType) -> Self {
        self.start_offset = offset;
        self.start_offset_type = kind;
        self
    }
}

/// How many independent spring components one value can be split into (`Size`/`Offset` have 2,
/// `Color`'s Oklab `alpha, L, a, b` is exactly 4). The fixed array is what keeps a frame's step free
/// of allocations.
pub const MAX_ANIMATION_COMPONENTS: usize = 4;

/// A value that can be animated.
pub trait AnimatableValue: Clone + PartialEq {
    /// Mix towards `to` by `t` — the interpolation a Tween/Keyframes spec uses.
    fn lerp(&self, to: &Self, t: f32) -> Self;
    /// The scalar projection (spring de-duplication; exact only for scalar types).
    fn to_f32(&self) -> f32;
    /// Rebuild from that projection (exact only for scalar types; a placeholder for vectors).
    fn from_f32(v: f32) -> Self;
    /// Exact target comparison (the default `PartialEq`).
    fn same_target(&self, other: &Self) -> bool { self == other }
    /// Whether a Spring can run on this type. A scalar is one spring; a value that overrides
    /// [`Self::write_components`] with several components runs one spring per component, which is
    /// Compose's `VectorConverter` + `VectorizedSpringSpec`.
    fn supports_spring() -> bool { false }
    /// Write the components a Spring advances independently, returning how many: each gets its own
    /// spring and its own visibility threshold, which is how `Size` lands BOTH axes on target and how
    /// `Color` mixes in Oklab.
    ///
    /// The default does not split (a scalar needs no override). A vector must override this and
    /// [`Self::from_components`] together — its `from_f32` is not injective.
    fn write_components(&self, out: &mut [f32; MAX_ANIMATION_COMPONENTS]) -> usize {
        out[0] = self.to_f32();
        1
    }
    /// The inverse of [`Self::write_components`]: rebuild a value from its components.
    fn from_components(components: &[f32]) -> Self {
        Self::from_f32(components[0])
    }
}

impl AnimatableValue for f32 {
    fn lerp(&self, to: &f32, t: f32) -> Self { self + (to - self) * t }
    fn to_f32(&self) -> f32 { *self }
    fn from_f32(v: f32) -> Self { v }
    fn supports_spring() -> bool { true }
}

impl AnimatableValue for crate::graphics::Color {
    /// Oklab interpolation, which is what Compose's colour animations interpolate: the vector is
    /// `(alpha, L, a, b)` converted to Oklab (`ColorVectorConverter.kt:33-47`) and the four components
    /// are mixed independently, then clamped on the way back. A Tween's fraction lands here; a Spring
    /// runs one spring per component instead (see `SpringLane`).
    ///
    /// This replaces a CAM16-UCS blend (`material_colors::blend::cam16_ucs`) whose result is close but
    /// not the same curve, and whose alpha had to be interpolated by hand: Oklab is one space for all
    /// four components, and it is the one Compose uses.
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let from = self.to_oklab();
        let target = to.to_oklab();
        let mut components = [0.0f32; 4];
        for (i, slot) in components.iter_mut().enumerate() {
            *slot = from[i] + (target[i] - from[i]) * t;
        }
        Self::from_oklab(components)
    }

    /// The lightness component. Only the scalar fallbacks use it (Decay, and a Spring on a value that
    /// does not declare four components) — a colour animation goes through the lanes.
    fn to_f32(&self) -> f32 { self.to_oklab()[1] }

    /// A grey at that lightness: the scalar form is a placeholder, as the trait documents.
    fn from_f32(v: f32) -> Self { Self::from_oklab([1.0, v, 0.0, 0.0]) }

    /// Compose's four components: alpha, L, a, b.
    fn write_components(&self, out: &mut [f32; MAX_ANIMATION_COMPONENTS]) -> usize {
        out[..4].copy_from_slice(&self.to_oklab());
        4
    }

    fn from_components(components: &[f32]) -> Self {
        let mut vector = [0.0f32; 4];
        for (slot, value) in vector.iter_mut().zip(components.iter()) {
            *slot = *value;
        }
        Self::from_oklab(vector)
    }

    /// A colour is a four-component value like `Size`/`Offset`, so a Spring is a spring per component
    /// rather than a replacement motion.
    fn supports_spring() -> bool { true }
}


// ── Value equality ───────────────────────────────────────────────────────────────────────────────
//
// `AnimationSpec` cannot derive `PartialEq`: a tween and a keyframes spec hold their interpolator as
// `Arc<dyn Interpolator>`, and an `Arc` compares by identity. Parameter comparison needs a VALUE
// answer instead (a caller may rebuild the spec, with fresh `Arc`s, on every frame), which is what
// `Interpolator::same_parameter` provides.

impl TweenSpec {
    /// Same duration, same delay, and an interpolator that draws the same curve.
    pub fn same_parameter(&self, other: &Self) -> bool {
        self.duration == other.duration
            && self.delay == other.delay
            && self.interpolator.same_parameter(other.interpolator.as_ref())
    }
}

impl KeyframesSpec {
    /// Same duration and the same frame list, compared by value.
    pub fn same_parameter(&self, other: &Self) -> bool {
        self.duration == other.duration
            && self.frames.len() == other.frames.len()
            && self.frames.iter().zip(other.frames.iter()).all(|(a, b)| {
                a.0 == b.0 && a.1 == b.1 && a.2.same_parameter(b.2.as_ref())
            })
    }
}

impl RepeatableSpec {
    /// Same iteration count, mode, cycle curve and initial start offset.
    pub fn same_parameter(&self, other: &Self) -> bool {
        self.iterations == other.iterations
            && self.mode == other.mode
            && self.base.same_parameter(&other.base)
            && self.start_offset == other.start_offset
            && self.start_offset_type == other.start_offset_type
    }
}

impl AnimationSpec {
    /// Value equality for parameter comparison (see the note above).
    pub fn same_parameter(&self, other: &Self) -> bool {
        match (self, other) {
            (AnimationSpec::Spring(a), AnimationSpec::Spring(b)) => a == b,
            (AnimationSpec::Tween(a), AnimationSpec::Tween(b)) => a.same_parameter(b),
            (AnimationSpec::Keyframes(a), AnimationSpec::Keyframes(b)) => a.same_parameter(b),
            (AnimationSpec::Repeatable(a), AnimationSpec::Repeatable(b)) => a.same_parameter(b),
            (AnimationSpec::Snap(a), AnimationSpec::Snap(b)) => a == b,
            (AnimationSpec::Decay(a), AnimationSpec::Decay(b)) => {
                a.friction == b.friction && a.threshold == b.threshold
            }
            _ => false,
        }
    }
}
#[cfg(test)]
mod same_parameter_tests {
    use super::*;
    use crate::animation::interpolator::{EaseInSine, EaseOutSine, Linear};
    use std::time::Duration;

    /// A spec rebuilt every frame holds fresh `Arc`s; equality has to be by VALUE or a container can
    /// never Skip past an unchanged spec.
    #[test]
    fn tween_specs_compare_by_value_not_by_arc_identity() {
        let a = TweenSpec::new(Duration::from_millis(300), Linear::new());
        let b = TweenSpec::new(Duration::from_millis(300), Linear::new());
        assert!(a.same_parameter(&b));
        assert!(!a.same_parameter(&TweenSpec::new(Duration::from_millis(301), Linear::new())));
        assert!(!a.same_parameter(&TweenSpec::new(Duration::from_millis(300), EaseInSine::new())));
        let delayed = TweenSpec::new(Duration::from_millis(300), Linear::new()).delay(Duration::from_millis(50));
        assert!(!a.same_parameter(&delayed), "a delay is part of the spec");
    }

    #[test]
    fn keyframes_compare_by_their_frames() {
        let a = KeyframesSpec::new(Duration::from_millis(400), vec![(0.0, 0.0), (0.5, 2.0), (1.0, 1.0)]);
        let b = KeyframesSpec::new(Duration::from_millis(400), vec![(0.0, 0.0), (0.5, 2.0), (1.0, 1.0)]);
        assert!(a.same_parameter(&b));
        let c = KeyframesSpec::new(Duration::from_millis(400), vec![(0.0, 0.0), (0.5, 2.5), (1.0, 1.0)]);
        assert!(!a.same_parameter(&c), "a moved keyframe is a different curve");
        let d = KeyframesSpec::new(Duration::from_millis(400), vec![(0.0, 0.0), (1.0, 1.0)]);
        assert!(!a.same_parameter(&d), "a shorter frame list is a different curve");
    }

    #[test]
    fn a_repeatable_compares_its_cycle_curve_and_offset() {
        let base = DurationSpec::Tween(TweenSpec::new(Duration::from_millis(200), Linear::new()));
        let a = RepeatableSpec::new(3, RepeatMode::Restart, base.clone());
        let b = RepeatableSpec::new(3, RepeatMode::Restart, base.clone());
        assert!(a.same_parameter(&b));
        let more = RepeatableSpec::new(4, RepeatMode::Restart, base.clone());
        assert!(!a.same_parameter(&more));
        let reversed = RepeatableSpec::new(3, RepeatMode::Reverse, base);
        assert!(!a.same_parameter(&reversed), "the mode decides where the last iteration ends");
    }

    #[test]
    fn animation_spec_variants_do_not_mix() {
        let spring = AnimationSpec::Spring(SpringSpec::default());
        let tween = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), Linear::new()));
        assert!(!spring.same_parameter(&tween));
        assert!(spring.same_parameter(&AnimationSpec::Spring(SpringSpec::default())));
        let stiffer = SpringSpec { stiffness: 300.0, ..SpringSpec::default() };
        assert!(!spring.same_parameter(&AnimationSpec::Spring(stiffer)), "a stiffer spring is a different curve");
        let a = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), EaseInSine::new()));
        let b = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), EaseOutSine::new()));
        assert!(!a.same_parameter(&b));
    }
}
