# Animation review (animation-core 1.13.0-beta01)

Scope: `winia/src/animation/` (12 files, ~8.8k lines) against
`reference/compose/animation-core-1.13.0-beta01`. Every claim below was measured with a temporary
probe inside the crate (the probes were deleted); the numbers are from those runs.

## Status at a glance

| Kind | Count |
| --- | --- |
| Defects fixed | 4 (A-1..A-4) |
| Open findings | 0 |
| Deliberate deviations | 3 (D-1..D-3) |
| Missing API | 3 (M-1..M-3); M-4 done |

## Defects

### A-1 `initialStartOffset` cut the last repeat cycle short — FIXED

`TimedAnimation::duration()` computed `base * iterations` and `value_at` finished when
`elapsed >= total`, ignoring `RepeatableSpec::start_offset`. With 2 x 100 ms and a 50 ms
`StartOffsetType::Delay` the motion reported 200 ms while the value at 200 ms was 0.4, then
snapped to the target; the frame engine (`Motion`, which subtracts the offset from `played`) ran
the correct 250 ms, so the two evaluators disagreed.

Fixed: `duration()` adds the delay / subtracts a fast-forward, and `value_at` ends on
`played >= total` like the frame engine. Test:
`animation::tests::a_repeat_start_offset_stretches_the_timeline` (duration 250 ms, value 0.5 at
200 ms).

### A-2 A repeated keyframe time kept the FIRST value — FIXED

`KeyframesOf::build` sorted then `dedup_by_key`, which keeps the first of a run, while its comment
(and Compose's builder, a map) says the LAST one wins. Measured:
`between(0,1).at(100, 0.2).at(100, 0.8)` produced frames `[(0.0, 0.0), (1.0, 0.2)]`.

Fixed with an explicit last-wins pass. Test:
`animation::tests::the_last_keyframe_wins_at_a_repeated_millisecond`.

### A-3 Spring constants did not match Compose's - FIXED

| winia | value | Compose | value |
| --- | --- | --- | --- |
| `STIFFNESS_MEDIUM` | 400 | `Spring.StiffnessMediumLow` | 400 |
| `STIFFNESS_HIGH` | 1000 | `Spring.StiffnessHigh` | 10_000 |
| `DAMPING_RATIO_HIGH_BOUNCY` | 0.4 | `Spring.DampingRatioHighBouncy` | 0.2 |
| (missing) | - | `Spring.StiffnessMedium` | 1500 |

`VectorizedAnimationSpec.kt:794-834` is the reference. Fixed to the Compose values: `STIFFNESS_MEDIUM`
is 1500, `STIFFNESS_HIGH` 10_000, `DAMPING_RATIO_HIGH_BOUNCY` 0.2, and `STIFFNESS_MEDIUM_LOW` (400) is
a new constant. The ten call sites that meant medium-low (`item`, `animated_size`, `visibility`,
`shared_transition`) now say `STIFFNESS_MEDIUM_LOW`.

### A-4 `SpringSpec::default()` was ~5x slower than Compose's `spring()` - FIXED

Compose's `spring()` defaults to `StiffnessMedium` (1500) with the type's visibility threshold
(px: 1.0). winia's default is stiffness 200 (`StiffnessLow`) with `threshold = 0.01`.
Measured for a 100 px move, damping 1.0:

| spec | duration |
| --- | --- |
| winia default (200 / 0.01) | 831 ms |
| winia stiffness with Compose's px threshold (200 / 1.0) | 469 ms |
| Compose default (1500 / 1.0) | 171 ms |
| 1500 / 0.01 | 304 ms |

`SpringSpec::default()` is used at 53 sites. It now uses `STIFFNESS_MEDIUM` (1500), Compose's
`spring()` default: the same 100 px move estimates ~304 ms instead of 831 ms (the remaining gap to
Compose's 171 ms is the threshold, D-2). Components that need medium-low pass it explicitly.

## Deviations (deliberate, recorded)

- **D-1 Keyframes store fractions.** `KeyframesSpec` takes `(progress, value 0..1)` fractions of the
  from→to displacement so one spec drives a `Color` or a `Size`; `KeyframesOf` converts from
  Compose-shaped absolute values and supplies the implicit 0 ms anchor. Consequence: the low-level
  `KeyframesSpec::new` with a first frame after 0 starts AT that frame's value (measured: frames
  `[(0.5, 0.25), (1.0, 1.0)]` give 0.25 at t=0 instead of starting from 0). Use `KeyframesOf` for
  Compose semantics.
- **D-2 One visibility threshold, not per type.** Compose picks a threshold per type (px 1.0, dp 0.4,
  ...); winia has a single `SpringSpec::threshold` (default 0.01). See A-4 for the measured effect.
- **D-3 No public `AnimationState` / `TargetBasedAnimation`.** `TimedAnimation` is the manual
  evaluator (value/velocity at an arbitrary time); there is no play/pause/seek object.

## Missing API

- **M-1 `keyframesWithSpline`** (and its `ArcSpline` / `MonoSpline` machinery) - no spline keyframes.
- **M-2 `DeferredTargetAnimation`** - Compose's lazily-created animation state.
- **M-3 Vector converters** - winia has `AnimatableValue` for f32, i32, Dp, Sp, Offset, Size, Color
  (and two internal types); Compose also converts `Rect`, `IntOffset`, `IntSize`, `DpOffset`.
- **M-4 Public duration estimation** - DONE: `animation::estimate_animation_duration(spec, from, to)`
  returns the duration of any finite spec (a spring's depends on the distance, hence the arguments).

## Checked and ruled out

- Tween `delay`: `duration()` = delay + duration and the value holds during the delay (measured
  100 ms tween + 50 ms delay: 0 at 40 ms, 0.1 at 60 ms, 1.0 at 160 ms).
- `snap(delay)`: holds the start value for the delay, jumps at the delay, reports that as its
  duration (measured 0/50/99 ms -> 0, 100 ms -> 1).
- `repeatable(iterations)`: plays exactly `iterations` cycles (3 x 100 ms finished at 300 ms with
  0.5 at 50/150/250).
- Reverse repeat: the odd cycle mirrors the iteration TIME, not the value (`loop_factor`), which is
  Compose's `VectorizedInfiniteRepeatableSpec` rule.
- `exponentialDecay`: friction 4.2 and threshold 0.1 match `ExponentialDecayFriction = -4.2f` and
  `absVelocityThreshold = 0.1f`.
- Retarget velocity is per component (`RetargetVelocity`), matching `AnimationState.velocity` with a
  2-D converter; bounds end with `MotionEndReason::BoundReached`.

## How to re-check

```bash
cargo test -p winia --lib animation::
```

The probes that produced the numbers above were `TimedAnimation::target(...)` with the spec under
test, sampling `value_at` at fixed times; `estimate` numbers came from the same `duration()`.