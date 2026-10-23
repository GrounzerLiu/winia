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
| High | `animate_float_as_state`, `animate_value_as_state`, `animate_int/dp/offset/size/color_as_state`, and the `_with_done` variants (`finishedListener`) | `ComposeCtx` (`runtime/composer.rs`) |
| High | `animate_content_size(spec)` — Compose `Modifier.animateContentSize` as a composition-owned modifier helper; the box springs toward the content size | `ComposeCtx`; see `docs/animate-content-size.md` |
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
- Colour motions pushed by `animate_color_as_state` live in a second registry
  (`ACTIVE_COLOR_ANIMATIONS`) so they need no type erasure. A `Color` pushed through the generic
  `push_animatable` (or `Animatable<Color>`) lives in the main one, like any other value.

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
| `Repeatable` | caller's iterations/mode | `iterations × base.duration`; `initialStartOffset` in Delay or FastForward flavour (a Delay offset adds to the wall-clock length, a FastForward one is skipped into the first cycle) |
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

## 8. Where a value goes: composition, layout, or paint

An animation is a `State<T>` the engine writes every frame. What decides its cost is WHERE that
value is read — the same `State<f32>` can drive any of the three.

| Read at | Hand it to | Per frame | Cost |
| --- | --- | --- | --- |
| **Composition** — a component parameter, a structural branch | `state.get()` | the reading composable re-runs | highest |
| **Layout** — size, padding, offset, min/max | `size`/`width`/`height` take `impl Into<SizeValue>`; `offset`/`padding`/`min`/`max` take `impl Into<Dimension>` — either accepts `State<f32>`, `Animating<f32>`, `DerivedValue<f32>` or a closure | the node is measured again (its subtree with it) | medium |
| **Paint** — alpha, scale, translation, colour, shape, icon tint, image alpha, blur radius, border width | `graphics_layer(closure)` / `background(closure)` / `Dyn<Shape>` on background/border/clip / `Dyn<Color>` on icon tint and ripple / `Dyn<f32>` on image alpha and blur / a `Dimension` on `border`, all read at draw time (`peek()` for a closure) | the node is painted again; nothing is measured or composed | lowest |

Every read in the paint row uses `peek()`. Paint runs with the dependency tracker off
(`DEP_MODE::None`), so `get()` would do the same thing today — and would attach the dependency to
whatever slot is active the moment a paint pass ran inside a compose/layout transaction (a snapshot,
a preview). The composition row is the opposite: there `get()` IS the dependency that drives the
recomposition, and the layout row's `Dimension` read registers the layout dependency.

The middle row is what `Dimension::Dynamic` is FOR. A layout length is a `Dimension`, and its
`Dynamic` form holds a closure that runs **during measure**; a `State::get()` there registers a
LAYOUT dependency on the node being measured, so a change marks that node layout-dirty and measures
it again — no recomposition, and no bypass through the composition (see `docs/state-handles.md` §3
for which handle the framework issues, and `docs/architecture-audit.md` for the two-phase
`layout_deps` → `layout_dirty_keys` machinery).

The same animated offset, written three ways:

```rust
// Layout: the node MOVES in the layout, so anything that depends on the position follows.
let dy = ctx.animate_float_as_state(target_y, spring);
Modifier::new().offset(0.0, dy);          // State<f32> -> Dimension::Dynamic

// Paint: a visual shift only - re-painted, never re-measured.
let dx = ctx.animate_float_as_state(target_x, spring);
Modifier::new().graphics_layer(move || GraphicsLayerParams {
    translation_x: dx.peek(), ..Default::default()
});

// Composition: the value takes part in the structure, so something has to re-run.
let alpha = ctx.animate_float_as_state(target_alpha, spring);
if alpha.get() > 0.0 { /* ... */ }
```

`Auto` and `Intrinsic` are NOT lengths — they are SIZE MODES, a decision about how a node uses the
space it was offered. There is no "0.37 of an Auto", so they are never evaluated per frame; a mode
that changes over time is a recomposition decision (chosen in the composition from a `State<bool>`).
`Fill` is not a `SizeValue` at all: it is `fill_max_width`/`fill_max_height`/`fill_max_size`, because
filling has to reach the measure step that tightens the axis to the incoming maximum (with a
scroll-viewport fallback when the max is infinite) — which a plain size value cannot.

## 9. Who owns a motion

A motion started from a composable belongs to the SLOT it was started in: `animate_*_as_state`,
`remember_animatable` and `remember_infinite_transition` register an `on_forget` cleanup, so a
composable or container branch that leaves the tree takes its motion with it instead of leaving it
ticking until the whole `Composer` is dropped. `Composer::drop` is the backstop.

What this does NOT cover: a plain `if` that skips an `animate_*_as_state` call inside a component
that keeps composing. winia does not emit a slot group for a Rust `if` (Compose's compiler does), so
nothing is dropped, the callback never fires, and the motion lives until that component itself goes.
The call sites that do this today — a conditional button/chip elevation and the FAB's — are harmless
(that same component still reads the value), but a new conditional call site that must die with its
branch needs a group of its own (`ctx.start_scope`, or a child composable).

## 10. Ambient API

| API | What it gives |
| --- | --- |
| `remember_animatable(initial)` | `Animatable<T>`: `value/target/is_running/velocity/snap_to/animate_to/animate_decay/animate_to_with_result/animate_sequence/set_bounds/clear_bounds/bounds/stop` |
| `update_transition(target, spec, label)` | a state machine whose pairs animate; `animate_float/color/dp/size/offset/value`, `create_child_transition` |
| `remember_infinite_transition()` | `animate_float/animate_color/animate_float_preserving`; auto-disposed with its slot |
| `AnimatedVisibility` / `AnimatedContent` / `Crossfade` / `AnimatedSize` | the container family, each in `components/` with its own tests |
| `TimedAnimation` | a spec evaluated at an arbitrary time, with nothing registered — the hand-off / "what would 137 ms look like" query |

## 11. The frame clock

`effect::with_frame_nanos()` waits for the next frame of **any** window; `ctx.frame_clock()` gives a
per-window clock. Both are broadcast from the render loop (`frame_tick`), and a window that is not
rendering does not tick them — use tokio timers for wall-clock work.

## 12. Observability and tests

- `active_animations_report()` — one line per live motion (`spec=`, `from`, `target`, `elapsed`,
  `v=`, component counts). The only way to answer "who is moving this state".
- `animation_target` / `animation_velocity` / `is_animating_state` — per-state introspection.
- `docs/anim-trace.md` + `debug/animation.rs` — with the `anim-trace` feature and `WINIA_ANIM_TRACE=<file>`,
  one NDJSON line per subject per frame (layout rect, painted rect, alpha in three parts).
- `trace_flag(name)` — a cached env-var switch for per-component traces.

**Time is injectable.** Every motion reads the module's `now()`: `Instant::now()` in production, and a
frozen thread-local clock in a test. `animation::tests::serial_clock()` gives the test both the shared
suites lock and that clock; advance it explicitly and the whole engine follows that timeline:

```rust
let (_guard, clock) = animation::tests::serial_clock();
clear_all_animations();
push_animatable(state.clone(), 100.0, spring);

for _ in 0..200 {
    clock.advance(Duration::from_millis(8));   // one 120 Hz frame of ENGINE time
    update_animations();
    // assert the value at exactly this timestamp
}
```

Nothing sleeps, nothing depends on how fast the machine is, and a test can assert a trajectory frame by
frame instead of sampling one instant. The animation module's suite has no `std::thread::sleep` left.

Tests share the global registries, so: call `clear_all_animations()` at both ends, and note the suite
must run single-threaded — `RUST_TEST_THREADS=1` is set in the local (untracked) `.cargo/config.toml`,
so anything running these tests elsewhere (CI included) has to set it too, or a parallel
`clear_all_animations` wipes a motion mid-test.

## 13. Known limitations

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
