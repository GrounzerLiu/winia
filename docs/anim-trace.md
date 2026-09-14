# anim-trace — verifiable animation

Per-frame geometry and opacity for everything that animates, emitted by the framework so a run can be
inspected and verified afterwards. No application code is involved: there is no API to call, only a
feature to build with and (optionally) an environment variable naming a file.

## Why it exists

Judging an animation from screenshots does not work: this project spent several rounds sampling painted
pixels at ~100 ms per screenshot, inserting temporary probes that had to be reverted, and still
mis-explaining what the eye reported. Two examples from that stretch, both caught only after a trace
existed:

- a flight whose target was resolved to `(96,96)` — the source's own size — so the cross-fade animated
  opacity without ever growing, while a same-screen morph supplied the visible growth;
- a flight cancelled at `p=0.20` by a re-slot inside a scene host, after which `begin_flight` bailed and
  a morph took over.

## Enabling it

```bash
# feature (OFF by default; without it every emission point compiles to a no-op)
cargo run -p winia --example nav_shared_element_demo --features "debug-server anim-trace"

# optional: write NDJSON to a file (one record per line)
WINIA_ANIM_TRACE=trace.ndjson cargo run --features anim-trace ...

# live view, no file needed (debug-server + anim-trace both on)
#   send `tr [n]` over the WebSocket or on stdin -> the last n records, one JSON object per line
```

With the feature on, the in-memory ring that serves `tr` is always fed (bounded, see Limits); a file is
written only when `WINIA_ANIM_TRACE` is set. With the feature off, nothing is recorded and nothing is
written.

## Record schema

One JSON object per line. `\"kind\"` is `flight`, `scene`, `event` or `node`.

| field | meaning |
|---|---|
| `frame`, `t_ms` | render frame counter and wall-clock stamp |
| `kind`, `subject` | what this is, and its identity (framework-derived) |
| `scope`, `key`, `role`, `flight` | for a flight end: the shared scope id, the shared key, `Source`/`Target`/`Morph`, the flight id |
| `scene` | scene id, for `kind: "scene"` |
| `phase`, `detail` | for `kind: "event"`: `start` / `cancel` / `candidate` / `rebind` / `resolve` / `morph_open`, plus a human-readable reason. For `kind: "node"`, `phase` is the paint disposition: `tree` / `layer` / `placeholder` |
| `progress` | animation progress 0..=1 (a spring may overshoot) |
| `layout` | the node's layout rect `{x,y,w,h}` |
| `painted` | the rect actually drawn, after flight scale and clip — what the eye sees |
| `alpha` | the end's OWN opacity (the flight's / transition's value) |
| `effective_alpha` | the opacity actually used for the draw: `alpha` composed with the scene layer's |
| `scene_visibility` | the visibility the scene host published for the scene this subject is in |
| `radii`, `clip` | corner radii and whether the draw is clipped to the bounds |

Opacity is recorded in three parts on purpose: `alpha` alone cannot answer "who faded this", and that
question decided several of the bugs above (a detached ghost painted from the transition layer, where
the scene's own fade does not apply; an in-tree end that the scene's layer does fade).

`layout` and `painted` are separate for the same reason: they diverge exactly when a flight is driving
a node, which is when a report is interesting.

## Identity, without app code

Subjects are named by the framework, in three schemes:

- `flight:<key>#<role>` — one end of a flight, `role` being `Source` / `Target` / `Morph`;
- `scene:<id>` — a scene published by a scene host; its `effective_alpha` is the visibility the host
  announced;
- `mark:<key>#<role|plain>` (`kind: "node"`) — EVERY marked node in the tree each frame, not just the
  ends of a flight. This is what answers "how many copies of one key are painted, and where": a copy that
  a flight already owns appears with `role: null` and `phase: placeholder` (measured: a leaving nav scene
  re-composed its hero, three records for one key in one frame, the third at its own 96x96 rect, which is
  how the second copy was found — and it is now suppressed).

Event records carry the key and the slots involved. Scene membership is read from a `SceneTag` on the
ancestor chain rather than stored on each marker — a marker's modifier element is built once and reused,
so a captured id freezes its first value (measured: one end `scene=None`, the other the LEAVING scene).

## Report tool

```bash
cargo run -p winia --example anim_trace_report -- trace.ndjson [--subject hero] [--stall-ms 120] \
    [--jump-px-per-s 4000]
```

Prints each end's trajectory (painted rect, alpha, progress) and checks the failure modes this project
actually hit. It exits 1 when any check fails and 2 when the input cannot be checked at all, so a
scripted run can gate on it:

- `STALL` — the painted rect stops changing while the animation is unfinished (the byte-identical
  frames a screen capture had shown). A flight whose start and end rects are equal is exempt: an
  opacity-only morph moves nothing on purpose.
- `JUMP` — the rect moves faster than `--jump-px-per-s` (default 4000 px/s, computed from the record
  timestamps, so one long frame is not a jump). It used to be a per-record threshold, which fired on
  healthy animations during a hitch.
- `OPACITY EARLY` (leaving end) — alpha reaches zero long before the geometry settles.
- `OPACITY MISSING` (entering end) — it never fades in at all (the morph case).
- `TARGET MISMATCH` — the `end` rect the flight's own `resolve` event announced is not where it settles.
  The event is matched by key and flight id; the first version looked for a subject string the emitter
  never writes, so the check could never fire.
- `SETTLE MISMATCH` — the two ends of one flight settle more than 200 ms apart.
- `INCOMPLETE` — a flight stops before progress 1 with no `cancel` event: a truncated trace used to be
  reported as clean, which made the tool useless as a gate.

Records are grouped by subject AND flight id: one subject is reused across navigations (the same hero
flies out and back), and treating those as one trajectory invents anomalies. Unparseable lines are
counted and reported (and are fatal above a quarter of the file); `--subject` matching nothing, an
unknown option, or a trace with no flight records exit 2 instead of silently passing.

## Tests

```rust
crate::anim_trace::capture_start();
// … drive the composer …
let records = crate::anim_trace::capture_take();
crate::anim_trace::capture_stop();
```

`anim_trace_records_a_headless_flight` (gated on the feature) uses this to pin a flight's trajectory:
both ends traced, both drawing the same lerped rect at the start (the source's size), the entering end
finishing at the target's size, opacity cross-fading, and every record carrying the composited alpha and
the layout rect. Disabling the emission in `write_flight_visuals` makes it fail immediately.

## Comparison with Compose

Compose has no equivalent first-class output. Verifying an animation there means writing per-test code —
`ComposeTestRule.mainClock` to drive the clock, `onNodeWithTag(...).getBoundsInRoot()` for geometry, and
`captureToImage()` for pixels — for each assertion, and nothing is recorded automatically for a run that
a human is watching. The Layout Inspector shows a live tree, but not a per-frame trajectory with
opacity.

winia emits the trajectory itself, keyed by framework identity, so the same data serves a live `tr`
query, a post-run report, and a headless assertion, with no application code in any of the three.

## Limits

- The live ring keeps the most recent 4000 records (a few seconds of a busy transition); the file sink
  is unbounded until the process ends.
- With the feature on, every emission point runs even when no file is configured (that is what makes
  `tr` work). The cost is per MARKED NODE per frame, not per flight: a node record is written for every
  marked node, idle ones included — that is what makes a suppressed duplicate visible — plus one record
  per scene per frame and the flight-end records. Reproducible from a captured file:
  `tmp/a3.ndjson` (three navigations, frames 1..605) holds 6336 records — node 3412, flight 2416,
  scene 502, event 6 — of which the `hero` marker contributes layer 1206 and placeholder 195.
- Only flights, scenes, marked-node geometry and lifecycle events are emitted so far. Named `animate_*`
  values (`nav` progress, `AnimatedVisibility`, `animate_*_as_state`) are not recorded yet.
