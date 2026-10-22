use super::*;

pub struct TimedAnimation<T: AnimatableValue> {
    from: T,
    /// `None` for a fling: there is no target, only the decay's own limit.
    to: Option<T>,
    spec: AnimationSpec,
    /// The starting velocity in the value's own units per second (a fling's `initial_velocity` included).
    initial_velocity: f32,
    /// The same per component, for a value that has several.
    initial_velocity_components: [f32; MAX_ANIMATION_COMPONENTS],
    components: usize,
}

impl<T: AnimatableValue + Clone + 'static> TimedAnimation<T> {
    /// A motion from `from` to `to` under any spec — Compose's `TargetBasedAnimation(animationSpec,
    /// converter, initialValue, targetValue, initialVelocity)`.
    pub fn target(from: T, to: T, spec: impl Into<AnimationSpec>) -> Self {
        Self::with_velocity(from, to, spec.into(), 0.0)
    }

    /// The same with an initial speed (units per second), which a hand-off needs: the remainder of a motion
    /// that was already moving.
    pub fn with_velocity(from: T, to: T, spec: AnimationSpec, initial_velocity: f32) -> Self {
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let components = from.write_components(&mut components);
        Self {
            from,
            to: Some(to),
            spec,
            initial_velocity,
            initial_velocity_components: [0.0; MAX_ANIMATION_COMPONENTS],
            components,
        }
    }

    /// A fling with no target — Compose's `DecayAnimation(decayAnimationSpec, converter, initialValue,
    /// initialVelocity)`. The velocity has the value's own type, so a vector flings on every component.
    pub fn fling(from: T, velocity: T, spec: DecaySpec) -> Self {
        let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
        let len = velocity.write_components(&mut components);
        Self {
            from,
            to: None,
            spec: AnimationSpec::Decay(spec),
            initial_velocity: AnimatableValue::to_f32(&velocity),
            initial_velocity_components: components,
            components: len,
        }
    }

    /// How long the motion lasts — Compose's `Animation.durationNanos`. A spring's answer is the estimate
    /// the engine itself now runs on, a fling's is the time its velocity takes to fall under the threshold,
    /// and a `Repeatable`'s is its iterations times its base.
    pub fn duration(&self) -> Duration {
        match &self.spec {
            AnimationSpec::Spring(spec) => Duration::from_secs_f32(
                estimate_spring_duration_seconds(
                    spec.stiffness,
                    spec.damping_ratio,
                    self.initial_velocity,
                    self.displacement(),
                    spec.threshold,
                )
                .max(0.0),
            ),
            AnimationSpec::Tween(spec) => spec.delay + spec.duration,
            AnimationSpec::Snap(spec) => spec.delay,
            AnimationSpec::Keyframes(spec) => spec.duration,
            AnimationSpec::Repeatable(spec) => {
                base_duration_of(&spec.base).saturating_mul(spec.iterations)
            }
            AnimationSpec::Decay(spec) => {
                let friction = spec.friction.max(0.001);
                let v0 = self.initial_velocity.abs().max(1e-6);
                // |v0·e^(-friction·t)| = threshold
                Duration::from_secs_f32((v0 / spec.threshold).ln() / friction)
            }
        }
    }

    /// The value at `elapsed` — Compose's `getValueFromNanos`.
    pub fn value_at(&self, elapsed: Duration) -> T {
        match &self.spec {
            AnimationSpec::Spring(spec) => {
                let target = self.target_of();
                if elapsed >= self.duration() {
                    // The clock the engine runs on: at the estimated end it writes the exact target.
                    return target;
                }
                if self.components > 1 {
                    let mut from_components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let mut to_components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let len = self.from.write_components(&mut from_components);
                    target.write_components(&mut to_components);
                    for (i, slot) in to_components.iter_mut().take(len).enumerate() {
                        let mut velocity = self.initial_velocity_components[i];
                        let displacement = compute_spring_displacement_with_step(
                            spec.stiffness,
                            spec.damping_ratio,
                            spec.mass,
                            from_components[i] - *slot,
                            &mut velocity,
                            elapsed,
                            spec.threshold,
                            1.0 / 480.0,
                        );
                        *slot += displacement;
                    }
                    return T::from_components(&to_components[..len]);
                }
                let mut velocity = self.initial_velocity;
                let displacement = compute_spring_displacement_with_step(
                    spec.stiffness,
                    spec.damping_ratio,
                    spec.mass,
                    self.displacement(),
                    &mut velocity,
                    elapsed,
                    spec.threshold,
                    1.0 / 480.0,
                );
                AnimatableValue::from_f32(AnimatableValue::to_f32(&target) + displacement)
            }
            AnimationSpec::Tween(spec) => {
                let target = self.target_of();
                if elapsed < spec.delay {
                    return self.from.clone();
                }
                let t = fraction_of(elapsed - spec.delay, spec.duration);
                if t >= 1.0 {
                    return target;
                }
                self.from.lerp(&target, spec.interpolator.interpolate(t))
            }
            AnimationSpec::Snap(spec) => {
                let target = self.target_of();
                if elapsed < spec.delay {
                    self.from.clone()
                } else {
                    target
                }
            }
            AnimationSpec::Keyframes(spec) => {
                let target = self.target_of();
                let t = fraction_of(elapsed, spec.duration);
                if t >= 1.0 {
                    return target;
                }
                self.from.lerp(&target, interpolate_keyframes(&spec.frames, t))
            }
            AnimationSpec::Repeatable(spec) => {
                let target = self.target_of();
                let base = base_duration_of(&spec.base);
                let total = base.saturating_mul(spec.iterations);
                if base.is_zero() || elapsed >= total {
                    return target;
                }
                let played = match spec.start_offset_type {
                    StartOffsetType::Delay => elapsed.saturating_sub(spec.start_offset),
                    StartOffsetType::FastForward => elapsed + spec.start_offset,
                };
                let cycle = (played.as_secs_f32() / base.as_secs_f32()) as u32;
                let phase = (played.as_secs_f32() % base.as_secs_f32()) / base.as_secs_f32();
                let eased = loop_factor(Some(&spec.base), spec.mode, cycle, phase);
                self.from.lerp(&target, eased)
            }
            AnimationSpec::Decay(spec) => {
                let friction = spec.friction.max(0.001);
                let t = elapsed.as_secs_f32();
                let decay = (-friction * t).exp();
                if self.components > 1 {
                    let mut components = [0.0f32; MAX_ANIMATION_COMPONENTS];
                    let len = self.from.write_components(&mut components);
                    for (i, slot) in components.iter_mut().take(len).enumerate() {
                        let v0 = self.initial_velocity_components[i];
                        *slot += if (v0 * decay).abs() < spec.threshold {
                            v0 / friction
                        } else {
                            v0 / friction * (1.0 - decay)
                        };
                    }
                    return T::from_components(&components[..len]);
                }
                let v0 = self.initial_velocity;
                let raw = if (v0 * decay).abs() < spec.threshold {
                    AnimatableValue::to_f32(&self.from) + v0 / friction
                } else {
                    AnimatableValue::to_f32(&self.from) + v0 / friction * (1.0 - decay)
                };
                AnimatableValue::from_f32(raw)
            }
        }
    }

    /// How fast the value moves at `elapsed`, in the value's own units per second. A spring and a fling
    /// answer from the physics; everything else answers by stepping 1 ms and measuring, which is what a
    /// caller holding an easing curve but no derivative can do. Compose returns a vector from
    /// `getVelocityVectorFromNanos`; this is its scalar projection.
    pub fn velocity_at(&self, elapsed: Duration) -> f32 {
        match &self.spec {
            AnimationSpec::Spring(spec) => {
                let mut velocity = self.initial_velocity;
                compute_spring_displacement_with_step(
                    spec.stiffness,
                    spec.damping_ratio,
                    spec.mass,
                    self.displacement(),
                    &mut velocity,
                    elapsed,
                    spec.threshold,
                    1.0 / 480.0,
                );
                velocity
            }
            AnimationSpec::Decay(spec) => {
                self.initial_velocity * (-spec.friction.max(0.001) * elapsed.as_secs_f32()).exp()
            }
            _ => {
                let step = Duration::from_millis(1);
                let a = self.value_at(elapsed);
                let b = self.value_at(elapsed + step);
                (AnimatableValue::to_f32(&b) - AnimatableValue::to_f32(&a)) / step.as_secs_f32()
            }
        }
    }

    /// The displacement the motion starts with (value minus target), which every spring calculation is
    /// expressed in.
    fn displacement(&self) -> f32 {
        match &self.to {
            Some(to) => AnimatableValue::to_f32(&self.from) - AnimatableValue::to_f32(to),
            None => 0.0,
        }
    }

    fn target_of(&self) -> T {
        self.to.clone().expect("this spec has a target")
    }
}

/// A `Repeatable`'s cycle length. The type system already rules out a base without one
/// (`DurationSpec`), so there is no fallback arm left to get wrong in release builds.
fn base_duration_of(base: &DurationSpec) -> Duration {
    base.duration()
}

/// The fraction of a duration that `elapsed` represents, clamped: 1.0 for a zero-length spec (a value that
/// arrives at once) and for anything past the end.
fn fraction_of(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0) as f32
    }
}

/// Whether a spring has come to rest — the ONE place this is decided, because the integrator and
/// `Motion::update` both need it and must never disagree.
///
/// **The two sides are not in the same unit, and that is deliberate.** The displacement is in the
/// threshold's unit; the velocity is in that unit per SECOND, so `velocity.abs() < threshold` reads as
/// "slower than a threshold per second" rather than "within a threshold". The second half is what makes
/// the animation end at a TURNING POINT of the oscillation — the first moment the amplitude has decayed
/// under the threshold — and that is what lets a bouncy spring overshoot before it settles: through most
/// of the flight, passing through the threshold band means the spring is still moving fast.
///
/// Scaling the velocity into frames (`velocity * FRAME < threshold`) reads better and is WRONG: it fires
/// on that first passage, so the bounce is cut. Measured — `components::shared_transition`'s
/// `tier0_bouncy_spring_overshoot_renders_then_settles` (damping 0.6, threshold 0.1 on a 0..1 progress)
/// went from "overshoots past 1.0 and then settles" to "settles at 0.886", i.e. the flight ended while
/// the spring was still ~11 % short of its target. Scaling it the other way (`velocity * FRAME <
/// threshold * FRAME`) is exactly the code below.
///
/// Compose's own clock is now the primary rule and this test is the backstop. Compose sizes a spring with
/// `estimateAnimationDurationMillis` — how long the analytic solution needs to come within one
/// `visibilityThreshold` of the target and stay there (`animation-core/SpringEstimation.kt:36-89`) — and runs
/// the spec for exactly that long (`FloatSpringSpec.getDurationNanos`,
/// `animation-core/FloatAnimationSpec.kt:172-190`). `Motion` computes the same estimate when a spring starts
/// and ends the motion on it, so a spring here stops when Compose's does. This test still ends a spring the
/// integrator has already brought to rest, which keeps a pathological parameter (an estimate that never
/// converges) from ticking forever.
///
/// One measured difference remains, and it is in the VALUE rather than the time: this engine integrates the
/// spring in 1/60 s steps instead of evaluating the analytic solution, so at the estimated end the integrator
/// is a fraction of a pixel short, and writing the exact target there is a step of that size. Measured on a
/// 0 → 100 move with stiffness 400 and a 1 px threshold: the clock ends it at 0.335 s against an estimate of
/// 0.3319 s with the value at 98.42, i.e. a 1.6 px step, where the analytic value at that moment would sit
/// inside the 1 px band. With ONLY this velocity test — the rule before the clock arrived — the same spring
/// ran 0.556 s, i.e. 224 ms past Compose's finish.
#[inline]
pub(super) fn spring_at_rest(displacement: f32, velocity: f32, threshold: f32) -> bool {
    displacement.abs() < threshold && velocity.abs() < threshold
}

/// The spring integrator: fixed steps inside one frame's time (accumulator style).
pub(super) fn compute_spring_displacement(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    elapsed: Duration, threshold: f32,
) -> f32 {
    compute_spring_displacement_with_step(
        stiffness,
        damping_ratio,
        mass,
        initial_displacement,
        velocity,
        elapsed,
        threshold,
        1.0 / 60.0,
    )
}

/// The integrator with its ordinary-frame step spelled out. [`TimedAnimation`] asks with a shorter one: a
/// query has no frame to keep up with, and a coarser step than a running animation uses shows up as a gap
/// between the two. Measured on a 100-unit stiffness-200 spring, worst gap against a running motion: 7.7 px
/// at the 1/60 s frame step, 1.7 px at 1/240 s, under 1.5 px at 1/480 s — the query's own integration error,
/// not a difference in what is being computed.
pub(super) fn compute_spring_displacement_with_step(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    elapsed: Duration, threshold: f32,
    frame_cap: f32,
) -> f32 {
    /// The longest step an ordinary frame uses (1/60 s).
    const FRAME_CAP: f32 = 1.0 / 60.0;
    /// Semi-implicit Euler is only stable while the step stays well under the spring's own period, so the
    /// step is bounded by the spring (`1/(4·omega0)` = a sixteenth of a period) instead of a fixed 1/60 s:
    /// a soft spring keeps the frame cap, a very stiff one gets a proportionally shorter step.
    /// A frame can be arbitrarily long (a stalled loop, a resize, a suspended app). Integrating all of it
    /// keeps a spring on the same clock as a Tween, which is what Compose's time-based springs do — the cap
    /// only stops a pathological gap from costing unbounded work in one frame. `MAX_STEPS` at the bound
    /// covers ~33 s of a soft spring and ~5 s of a very stiff one; beyond that the remainders are dropped,
    /// which is the one case where a spring can still fall behind.
    const MAX_STEPS: u32 = 2000;
    let omega0 = (stiffness / mass.max(f32::EPSILON)).sqrt();
    let step_cap = if omega0 > 0.0 {
        (1.0 / (4.0 * omega0)).min(frame_cap)
    } else {
        frame_cap
    };
    let mut total_dt = elapsed.as_secs_f32().min(step_cap * MAX_STEPS as f32);
    let mut displacement = initial_displacement;
    let damping_coeff = damping_ratio * 2.0 * omega0 * mass;
    while total_dt > 0.0 {
        let step = total_dt.min(step_cap);
        let force = -stiffness * displacement - damping_coeff * *velocity;
        *velocity += force / mass.max(f32::EPSILON) * step;
        displacement += *velocity * step;
        total_dt -= step;
    }
    if spring_at_rest(displacement, *velocity, threshold) {
        *velocity = 0.0;
        return 0.0;
    }
    displacement
}

