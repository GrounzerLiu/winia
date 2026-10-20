# The animation system

> Rewritten from the code on 2026-10-20 (branch `v2`). The previous version of this file described a
> tree that no longer exists: it claimed a colour spring was downgraded to a tween, a `dt` over 166 ms
> was truncated, `AnimatedVisibility`/`Crossfade`/`animateContentSize` were missing, and
> `SpringSpec::default()` was stiffness 1500. All four are wrong today. `animation-gap-analysis.md`
> is a dated snapshot of the 2026-08-05 pass and is kept only as a historical record.

## 1. Shape

Three layers, Compose-shaped, but push-based: there are no coroutines and no suspend point.

| Layer | What it is | Where |
| --- | --- | --- |
| High | `animate_float_as_state`, `animate_value_as_state`, `animate_int/dp/offset/size/color_as_state` | `ComposeCtx` (`runtime/composer.rs`) |
| Middle | `update_transition`, `remember_infinite_transition`, `remember_animatable` | `animation.rs` |
| Engine | `Motion<T>` + the global registry; the public handle is `Animatable<T>` | `animation.rs` |

A component never drives frames itself: it hands a target and a spec to a `push_*` function, the
engine registers a motion against a state, and the render loop advances every registered motion once
per frame with the real elapsed time. Reading the state is what makes anything recompose or repaint.

## 2. The frame

```
push_* -> ACTIVE_ANIMATIONS (Vec<Box<dyn AnimationInstance>>)
                    |
event loop: new_events -> update_animations() -> Motion::update -> Animating::set
                    |                                   (notify dependents, no wake)
                    +-- animating? ControlFlow::WaitUntil(now + frame_interval), throttled request_redraw
                    +-- just stopped? force one more frame (the last write's request may have been skipped)
```

- `new_events` advances the registry and, while anything is running, switches the loop to
  `WaitUntil` so the next frame arrives even with no input.
- `RedrawRequested` advances it **again**: a Windows resize runs a modal loop that starves
  `AboutToWait`, and WM_PAINT is the only event that keeps arriving, so a resize must tick there to
  stay animated. Two ticks per frame are harmless — `Motion` integrates real time, not ticks.
- Colour motions live in a second registry (`ACTIVE_COLOR_ANIMATIONS`) to stay type-erased-free.

## 3. Values

`AnimatableValue` (`animation.rs`) is what a spec can move: `lerp`, `to_f32`/`from_f32` (a scalar
projection), component read/write, and `supports_spring`.

| Type | Components | Spring |
| --- | --- | --- |
| `f32`, `i32`, `Dp`, `Sp` | 1 | one spring |
| `Size`, `Offset` | 2 | one spring per axis |
| `Color` | 4 (Oklab alpha, L, a, b) | one spring per component |

`to_f32`/`from_f32` exist only for the scalar paths (decay, a one-value spring); a vector's
projection is a placeholder and nothing should write it back. Multi-component springs run
`SpringLane`s, decays run `DecayLane`s.

## 4. Specs

`AnimationSpec`: `Spring`, `Tween`, `Keyframes`, `Repeatable`, `Snap`, `Decay`.
`DurationSpec` (`Tween`/`Keyframes`/`Snap`) is the base a repeat cycles over — the type system, not a
runtime check, is what rules out a spring with no cycle length.

| Spec | Defaults | Ends when |
| --- | --- | --- |
| `Spring` | damping 1.0, stiffness 200, mass 1.0, threshold 0.01 | the analytic rest check says so, **or** the Compose-style estimated duration elapses — whichever comes first; writes the exact target |
| `Tween` | 300 ms, fast-out-slow-in | its clock; `delay` holds `from` first |
| `Keyframes` | caller's frames | its clock; the last frame writes the exact target |
| `Repeatable` | caller's iterations/mode | `iterations × base.duration`; `initialStartOffset` in Delay or FastForward flavour |
| `Snap` | no delay | immediately after `delay` |
| `Decay` | friction 4.2, threshold 0.1 | `|v0·e^(-f·t)| < threshold`; analytic, parks on the exact limit |

Spring integration: semi-implicit Euler with a step capped at `min(1/60 s, 1/(4·ω₀))` (a stiff spring
gets a shorter step to stay stable) and at most 2000 steps per call — ~33 s of a soft spring. A frame
is allowed to be long; the cap only bounds pathological work.

`Decay` is the one spec with no target: it coasts from a velocity. Reaching a *target* entry point
(`push_animatable*`, which has no velocity) it is replaced by a 300 ms tween — the old behaviour ran
it at velocity zero, which reported itself finished on the first frame, so a scalar never moved and a
vector was rebuilt through the placeholder projection.

## 5. Registration, dedup and retarget

Two families, deliberately different:

| Entry point | Same target, motion running |
| --- | --- |
| `push_animatable` / `push_animatable_handle` (what `animate*AsState` calls every recomposition) | ignored — that is what makes the per-frame call idempotent |
| `push_animatable_restart` / `push_animatable_restart_with_result` (what `Animatable::animate_to` calls) | replaced — Compose's `animateTo` always runs what it is given |

A motion replaced mid-flight inherits the old one's velocity, **per component** (`RetargetVelocity`),
and only `Spring`/`Decay` have a velocity to pass on: a `Tween` interrupted by a spring restarts from
rest. `cancel_animation`/`cancel_animation_by_id` only unregister; they do not write a value or fire a
callback.

## 6. Ends, bounds, callbacks

`MotionEnd { reason, velocity }` is what a completion callback receives: `Finished`, `BoundReached`
(a value range stopped it) or `Forced` (the engine's 5 s overrun guard, which only applies to
`Spring`/`Decay` — they can be asymptotic; a tween's own duration is never cut).
`Animatable::set_bounds` (`set_animation_bounds` underneath) writes the bound exactly and ends the motion, and
a fling reports the velocity it could not use through `on_boundary`.

## 7. Scheduling semantics

`State` is the default handle: writes notify dependents **and** wake the loop. The engine writes
through `Animating<T>`: notify the dependents (recompose) but do not wake — the loop is already
awake while anything animates. Draw-layer properties (`alpha`, scale, translation) use `Visual<T>`:
the value is written with no notification at all and read with `peek()` at paint time, which is how
they animate without recomposing. `Backchannel<T>` is the measure/layout write-back channel. See
`docs/state-handles.md`.

A value is only written when it changes (`Animating::set` dedups), so an idle or landed motion does not
wake the tree.

## 8. Who owns a motion

A motion started from a composable belongs to that slot: `animate_*_as_state`, `remember_animatable`
and `remember_infinite_transition` register an `on_forget` cleanup, so a branch that changes shape (a
conditionally composed animated value) takes its motion with it instead of leaving it ticking until
the whole `Composer` is dropped. `Composer::drop` still clears everything it owns as a backstop.

## 9. Ambient API

| API | What it gives |
| --- | --- |
| `remember_animatable(initial)` | `Animatable<T>`: `value/target/is_running/velocity/snap_to/animate_to/animate_decay/animate_to_with_result/animate_sequence/set_bounds/clear_bounds/bounds/stop` |
| `update_transition(target, spec, label)` | a state machine whose pairs animate; `animate_float/color/dp/size/offset/value`, `create_child_transition` |
| `remember_infinite_transition()` | `animate_float/animate_color/animate_float_preserving`; auto-disposed with its slot |
| `AnimatedVisibility` / `AnimatedContent` / `Crossfade` / `AnimatedSize` | the container family, each in `components/` with its own tests |
| `TimedAnimation` | a spec evaluated at an arbitrary time, with nothing registered — the hand-off / "what would 137 ms look like" query |

## 10. The frame clock

`effect::with_frame_nanos()` waits for the next frame of **any** window; `ctx.frame_clock()` gives a
per-window clock. Both are broadcast from the render loop (`frame_tick`), and a window that is not
rendering does not tick them — use tokio timers for wall-clock work.

## 11. Observability and tests

- `active_animations_report()` — one line per live motion (`spec=`, `from`, `target`, `elapsed`,
  `v=`, component counts). The only way to answer "who is moving this state".
- `animation_target` / `animation_velocity` / `is_animating_state` — per-state introspection.
- `docs/anim-trace.md` + `anim_trace.rs` — with the `anim-trace` feature and `WINIA_ANIM_TRACE=<file>`,
  one NDJSON line per subject per frame (layout rect, painted rect, alpha in three parts).
- `trace_flag(name)` — a cached env-var switch for per-component traces.

Tests share the global registries, so: take `animation::tests::TEST_SERIAL`, call
`clear_all_animations()`, drive the engine by hand (`update_animations()` in a loop with short sleeps),
and clear again at the end. `RUST_TEST_THREADS=1` in `.cargo/config.toml` covers the rest.

## 12. Known limitations

1. `Repeatable` needs a `DurationSpec` base. A spring has no cycle length, so this is a type-level
   rule, not a gap — Compose's `DurationBasedAnimationSpec` says the same.
2. The overrun guard ends a `Spring`/`Decay` at 5 s. It only fires for pathological parameters
   (stiffness ~0, friction ~0); a spring normally ends on its estimated clock.
3. `Decay` through a target-based push is a 300 ms tween (see §4). A real fling takes a velocity of
   the value's own type: `push_decay`/`push_decay_vector`, or `Animatable::animate_decay`.
4. Bounds are scalar: `set_animation_bounds` compares the scalar projection, so a multi-component
   value cannot bound one axis.
5. The registries are process-global. A `Composer` clears what it owns on drop and its slots clean up
   after themselves, but two live composers share one table (that is also why tests are serial).
6. `label` on `animate*AsState` is not carried (only `Transition` has one); it is debug-only in
   Compose.
