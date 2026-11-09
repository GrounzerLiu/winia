//! `keyframesWithSpline` - Compose's `KeyframesWithSplineSpec` and the `MonoSpline` it evaluates.
//!
//! Ported from `animation-core/MonoSpline.kt` and `VectorizedMonoSplineKeyframesSpec.kt`: a
//! piecewise cubic Hermite spline whose tangents are limited so a monotone band stays monotone
//! (Fritsch-Carlson: when `hypot(a, b) > 9` both tangents are scaled by `3 / hypot`). The spline
//! runs over the keyframe FRACTIONS of the from->to band, the same representation `KeyframesSpec`
//! uses, so one spec still drives a `Color` or a `Size`.

use super::*;

/// A 1-D monotone cubic spline through `(time, value)` knots.
#[derive(Clone, Debug)]
pub(crate) struct MonoSpline {
    times: Vec<f32>,
    values: Vec<f32>,
    tangents: Vec<f32>,
}

impl MonoSpline {
    /// `times` must be strictly increasing and `values.len() == times.len() >= 2`.
    pub(crate) fn new(times: Vec<f32>, values: Vec<f32>) -> Self {
        let n = times.len();
        debug_assert!(n >= 2 && values.len() == n);
        let mut slopes = vec![0.0f32; n - 1];
        let mut tangents = vec![0.0f32; n];
        for i in 0..n - 1 {
            let dt = times[i + 1] - times[i];
            slopes[i] = (values[i + 1] - values[i]) / dt;
            if i == 0 {
                tangents[i] = slopes[i];
            } else {
                tangents[i] = (slopes[i - 1] + slopes[i]) * 0.5;
            }
        }
        tangents[n - 1] = slopes[n - 2];
        // Monotone limiting, exactly as MonoSpline.kt:69-86.
        for i in 0..n - 1 {
            if slopes[i] == 0.0 {
                tangents[i] = 0.0;
                tangents[i + 1] = 0.0;
            } else {
                let a = tangents[i] / slopes[i];
                let b = tangents[i + 1] / slopes[i];
                let h = a.hypot(b);
                if h > 9.0 {
                    let t = 3.0 / h;
                    tangents[i] = t * a * slopes[i];
                    tangents[i + 1] = t * b * slopes[i];
                }
            }
        }
        Self {
            times,
            values,
            tangents,
        }
    }

    /// `getPos`: extrapolates past the ends with the end slope (MonoSplineIsExtrapolate = true).
    pub(crate) fn value_at(&self, t: f32) -> f32 {
        let n = self.times.len();
        if t <= self.times[0] {
            return self.values[0] + (t - self.times[0]) * self.slope_at(self.times[0]);
        }
        if t >= self.times[n - 1] {
            return self.values[n - 1] + (t - self.times[n - 1]) * self.slope_at(self.times[n - 1]);
        }
        for i in 0..n - 1 {
            if t == self.times[i] {
                return self.values[i];
            }
            if t < self.times[i + 1] {
                let h = self.times[i + 1] - self.times[i];
                let x = (t - self.times[i]) / h;
                return hermite_interpolate(
                    h,
                    x,
                    self.values[i],
                    self.values[i + 1],
                    self.tangents[i],
                    self.tangents[i + 1],
                );
            }
        }
        self.values[n - 1]
    }

    /// `getSlope`: the derivative, used by the frame engine's velocity report.
    pub(crate) fn slope_at(&self, t: f32) -> f32 {
        let n = self.times.len();
        let t = t.clamp(self.times[0], self.times[n - 1]);
        for i in 0..n - 1 {
            if t <= self.times[i + 1] {
                let h = self.times[i + 1] - self.times[i];
                let x = (t - self.times[i]) / h;
                return hermite_differential(
                    h,
                    x,
                    self.values[i],
                    self.values[i + 1],
                    self.tangents[i],
                    self.tangents[i + 1],
                ) / h;
            }
        }
        0.0
    }
}

/// Cubic Hermite spline (`hermiteInterpolate` in MonoSpline.kt).
pub(crate) fn hermite_interpolate(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    let x3 = x2 * x;
    h * t1 * (x - 2.0 * x2 + x3) + h * t2 * (x3 - x2) + y1 - (3.0 * x2 - 2.0 * x3) * (y1 - y2)
}

/// The derivative of [`hermite_interpolate`] with respect to `x`, over `h`.
pub(crate) fn hermite_differential(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    h * (t1 - 2.0 * x * (2.0 * t1 + t2) + 3.0 * (t1 + t2) * x2) - 6.0 * (x - x2) * (y1 - y2)
}

/// `keyframesWithSpline { }` — keyframe values connected by a monotone cubic spline.
#[derive(Clone, Debug)]
pub struct KeyframesWithSplineSpec {
    pub duration: Duration,
    /// `(progress 0..1, fraction, segment easing)`, sorted, with anchors at 0 and 1.
    pub frames: Vec<(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)>,
}

impl KeyframesWithSplineSpec {
    /// The fraction (0..1 of the from->to band) at progress `t` (0..1).
    pub(crate) fn fraction_at(&self, t: f32) -> f32 {
        if self.frames.len() < 2 {
            return t;
        }
        let n = self.frames.len();
        if t <= self.frames[0].0 {
            return self.frames[0].1;
        }
        if t >= self.frames[n - 1].0 {
            return self.frames[n - 1].1;
        }
        // Segment search, then the per-segment easing (VectorizedMonoSplineKeyframesSpec.getEasedTimeFromIndex).
        let mut i = 0;
        while i + 1 < n && t > self.frames[i + 1].0 {
            i += 1;
        }
        let (p0, _, easing) = &self.frames[i];
        let p1 = self.frames[i + 1].0;
        let raw = if p1 > *p0 { (t - p0) / (p1 - p0) } else { 0.0 };
        let eased = easing.interpolate(raw.clamp(0.0, 1.0));
        let time = p0 + (p1 - p0) * eased;
        let spline = self.spline();
        spline.value_at(time)
    }

    /// Value equality for parameter comparison (`AnimationSpec::same_parameter`): the duration and
    /// every `(progress, fraction)` pair, with each segment easing compared by its own rule.
    pub fn same_parameter(&self, other: &Self) -> bool {
        self.duration == other.duration
            && self.frames.len() == other.frames.len()
            && self
                .frames
                .iter()
                .zip(&other.frames)
                .all(|(a, b)| a.0 == b.0 && a.1 == b.1 && a.2.same_parameter(&*b.2))
    }

    fn spline(&self) -> MonoSpline {
        MonoSpline::new(
            self.frames.iter().map(|(p, _, _)| *p).collect(),
            self.frames.iter().map(|(_, f, _)| *f).collect(),
        )
    }
}

/// The Compose-shaped builder: absolute milliseconds and values in the animated type's units.
pub struct KeyframesWithSplineOf {
    from: f32,
    to: f32,
    frames: Vec<(
        u64,
        f32,
        Option<std::sync::Arc<dyn interpolator::Interpolator>>,
    )>,
}

impl KeyframesWithSplineOf {
    pub fn between(from: f32, to: f32) -> Self {
        Self {
            from,
            to,
            frames: Vec::new(),
        }
    }

    pub fn at(mut self, millis: u64, value: f32) -> Self {
        self.frames.push((millis, value, None));
        self
    }

    pub fn at_with(
        mut self,
        millis: u64,
        value: f32,
        easing: std::sync::Arc<dyn interpolator::Interpolator>,
    ) -> Self {
        self.frames.push((millis, value, Some(easing)));
        self
    }

    pub fn build(mut self) -> KeyframesWithSplineSpec {
        self.frames.sort_by_key(|(millis, _, _)| *millis);
        let mut deduped: Vec<(
            u64,
            f32,
            Option<std::sync::Arc<dyn interpolator::Interpolator>>,
        )> = Vec::new();
        for frame in self.frames.drain(..) {
            match deduped.last_mut() {
                Some(last) if last.0 == frame.0 => *last = frame,
                _ => deduped.push(frame),
            }
        }
        let duration_ms = deduped.last().map(|(millis, _, _)| *millis).unwrap_or(0);
        if deduped.first().map(|(millis, _, _)| *millis).unwrap_or(0) != 0 {
            deduped.insert(0, (0, self.from, None));
        }
        if deduped.last().map(|(millis, _, _)| *millis).unwrap_or(0) != duration_ms {
            // (unreachable: the last frame defines the duration)
        }
        let span = self.to - self.from;
        let linear: std::sync::Arc<dyn interpolator::Interpolator> =
            std::sync::Arc::new(interpolator::Linear::new());
        let mut frames: Vec<(f32, f32, std::sync::Arc<dyn interpolator::Interpolator>)> = deduped
            .into_iter()
            .map(|(millis, value, easing)| {
                let progress = if duration_ms == 0 {
                    0.0
                } else {
                    millis as f32 / duration_ms as f32
                };
                let fraction = if span.abs() <= f32::EPSILON {
                    0.0
                } else {
                    (value - self.from) / span
                };
                (progress, fraction, easing.unwrap_or_else(|| linear.clone()))
            })
            .collect();
        // The implicit end anchor: a spline needs a knot at 1. Compose fills a missing last
        // timestamp with the TARGET value (VectorizedMonoSplineKeyframesSpec.init).
        if frames.last().map(|(p, _, _)| *p).unwrap_or(0.0) < 1.0 {
            frames.push((1.0, 1.0, linear));
        }
        KeyframesWithSplineSpec {
            duration: Duration::from_millis(duration_ms.max(1)),
            frames,
        }
    }
}
