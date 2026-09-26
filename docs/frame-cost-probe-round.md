# Frame cost, measured outside the benchmark (probe round)

> Status: **measurement round, no framework change.** Everything here was taken with throwaway
> probes that were reverted before the round ended: `git status` is empty at `9d404e5`, and the two
> probes are saved as patches under `target/probe/` (`compose_sections_probe.patch`,
> `frame_timing_probe.patch`).
>
> Why it exists: `docs/benchmarks.md` prices the frame with one synthetic scene (800 rows, boxes and
> text). This round asked two questions that document cannot answer — **how much of the tail is
> actually skippable** (ablations), and **what a real app's frame costs** (frame timing on a running
> demo, in a debug AND a release build). Both answers moved the next round's target; the release
> figures reversed the conclusion the debug figures first suggested, and that reversal is §3's
> point.

## 1. The compose tail, section by section (instrumented)

`WINIA_SECT_PROF=1 cargo bench -p winia` with a warmup gate (`WINIA_SECT_WARMUP`), boxes, 800 rows,
42 843 frames recorded. Instrumented figures (a clock pair per section), so read them as an upper
bound on each section and a reliable shape of the tail:

| section | ns/frame | note |
|---|---|---|
| truncate | 0 | |
| `collect_live_keys` | 47 707 | whole slot-tree walk per frame |
| `materialize` | 130 608 | descriptor walk + in-place claim (its verify half was priced at ~17 µs by round 17) |
| `retain_shared_sources` | 72 891 | too large for an early-return check — timer-floor artifact, see below |
| `prune_stale_child_links` | 13 593 | whole-arena walk per frame |
| `register_modifier_deps` | 57 105 | whole-arena walk per frame |
| `take_deps` | 30 | |
| `reconcile_compose_deps` | 57 284 | reverse-graph rebuild + cleanup, ~48 µs of it the rebuild |
| prev drain | 103 | ~0 on an idle frame (nothing unreused to free) |
| **tail total** | **379 321** | vs a frame-level compose-only idle of ~425 µs on the same machine |

The tail is ~90 % of an idle compose, so this is the right surface. The shape also says what kind of
change helps: three of these are whole-tree walks (`collect_live_keys`, `prune`, `materialize`) and
two are graph/traversal logic (`register_modifier_deps`, `reconcile`) that is proportional to graph
size, not to work done.

## 2. Ablations: what a gate would actually buy

Two gates were applied to one build each and measured with the same bench invocation (frame-level
figures are noisy at ±10 %; the earlier, cleaner runs gave an idle frame of 541–546 µs before and
538.7 µs with the prune gate, i.e. **within noise**):

| ablated | idle frame 800 rows | verdict |
|---|---|---|
| `prune_stale_child_links` skipped | 538.7 µs (baseline 541–546) | cost is real but small; **gating it is not worth its own invariant risk** |
| `collect_live_keys` replaced by a frozen set | **344.8 µs** | the largest single number measured this round — but see below |

**The frozen-set number is not an optimization.** Freezing the set also stops the read graph from
growing (every read is recorded against a slot the set does not contain), so the reverse-graph
rebuild stops doing its per-state work: the 200 µs is mostly *the graph*, not the walk. It remains
useful evidence — it says the walk + graph pair is where an idle frame's compose goes, in that
order — but nothing can be shipped from it as-is.

Two things follow for the next round:

1. **Gates are the wrong shape.** `prune_stale_child_links` measured flat when disabled partly
   because the operation is already only ~13.5 µs, and gating it needs its invariant to be provable
   per frame (an existing test deliberately exercises a 0-Entry compose with a stale listing to keep
   the repair unconditional). Same for `collect_live_keys`: cutting it is a behaviour change, not a
   skip.
2. **`reconcile_compose_deps` is the honest target.** It is ~57 µs of an idle frame — ~48 µs the
   reverse-graph rebuild plus ~9 µs the union/cleanup — and unlike the walks it is *logic* whose cost
   is proportional to the read graph, which a real screen's graph (many slots × several reads each)
   makes bigger than the bench's single slot × 800 reads. The nineteenth fix already made the walk
   skip the rebuild when the forward graph did not move; what is left is the case where it *does*
   move — a one-row update re-records the same 800 reads, marks `changed`, and rebuilds everything.
   A per-slot diff that rewrites only the slots that moved (with the union cleanup still per frame)
   is the round to measure next.

## 3. A real app's frame

Probe: `WINIA_FRAME_PROF=1` makes each frame cost one `[fp] <µs>` line, measured from the frame
handler's entry to the end of overlay composition + draw, with the per-frame debug tree suppressed
(its own walk would be inside the figure). Driven over the debug WebSocket with a fixed script: 120
wheel steps, one jump-to-item-500, then 60 steps back — 181 commands, 128 rendered frames,
`lazy_column_demo` at 420x620, 15 list items composed of 1000, 59-node tree.

The same script was run against a **debug** build and a **release** build, and the probe was then
extended from one total to the frame's phases, because the first measurement (debug only) turned out
to be misleading about the production split:

| | debug | release |
|---|---|---|
| p50 (total) | 2 908 µs | **1 253 µs** → 1 170 µs with the split probe |
| p90 | 3 177 µs | 1 654 µs |
| p99 | 6 119 µs | 5 778 µs |
| max (startup) | 10 164 µs | 8 460 µs |
| startup frames 1–5 | 2 241–2 385 µs | 712–821 µs |

And the phase split itself (steady frames, 123 debug / 125 release):

| phase | debug p50 | debug share | release p50 | release share |
|---|---|---|---|---|
| compose loop | 1 366 µs | 48.8 % | **252 µs** | 22.4 % |
| main layout | 682 µs | 24.4 % | **317 µs** | 25.4 % |
| overlay pass | 39 µs | 1.4 % | 6 µs | 0.6 % |
| predraw (flight poll, focus sync, draw setup) | 54 µs | 1.8 % | 33 µs | 2.6 % |
| draw (`sw.draw` closure + submit/present) | 647 µs | 23.6 % | **545 µs** | **49.0 %** |
| after (screenshot readback, IME sync) | 0 µs | 0 % | 0 µs | 0 % |
| **frame** | **2 839 µs** | | **1 170 µs** | |

> **Correction, same day.** The first version of §3 compared the release frame (1253 µs) with the
> benchmark's 800-row compose+layout figure (1271 µs) and called the benchmark "not a corner of the
> frame". The phase split added below shows that was a coincidence of two numbers, not a share: this
> app's compose+layout is **556 µs** of the 1170 µs frame, and the benchmark reaches 1271 µs by
> composing 53x more content. The comparison is corrected in place; the phase split is what the round
> should have measured first.

Readings:

- **In a release build the draw path is the largest single phase (545 µs, 49 %)**, with layout (317 µs,
  25 %) slightly ahead of compose (252 µs, 22 %). Overlays are free (6 µs).
- **Layout being compose-sized is the surprise.** The bench reports layout at ~1/3 of compose because
  its container re-runs and every row skips, making compose huge; here nothing is dirty, so layout's
  *fixed* per-frame work shows through — the same content the §1 table prices: `register_modifier_deps`'s
  arena walk (~57 µs at 3201 nodes, mostly fixed), `LayoutTransaction` (15 µs), the dirty-mark sweep,
  and the reuse-index rebuild (~52 µs at 3201 nodes).
- **compose's real-app figure is far below the bench's per-row machinery.** 252 µs for a 59-node tree
  is mostly per-frame fixed cost (§1's tail), not per-node work: the bench's 800-row compose-only idle
  frame is 425 µs for 13.5x the nodes.
- **The debug build inflates compose 5.4x** (1366 vs 252) while the draw path moves only 1.2x
  (647 vs 545). A debug build therefore exaggerates the framework's compose share and hides the draw
  path — the opposite error of the one §3's first version made.
- **No warm-up effect:** the first rendered frame is the cheapest of the run in both builds
  (712–821 µs release against a 1170 µs p50).
- **The structural-change tail is real work:** p99 5.8 ms release against a 1.17 ms p50, and the jump
  frame is where it lands — the cold-frame shape the bench also prices.

## 4. What this round recommends

1. **Split the draw phase before choosing a side.** It is 49 % of a production frame and it is one
   number: Skia recording (`render::render` + layer + overlays), present, and the per-frame setup are
   all inside it. Until that is broken out, "optimize the renderer" is as unsupported as "optimize
   compose" was.
2. **compose is close to done; layout is the compose-side target that is left.** Layout's 317 µs is
   dominated by fixed per-frame work over both trees. Of the items in §1, the ones a real app pays in
   full are `register_modifier_deps` (arena walk) and layout's reuse-index walk (`collect_layout_index`,
   ~52 µs at 3201 nodes), plus `reconcile`'s reverse-graph rebuild (~48 µs of compose). The bench's
   row loop, by contrast, is heavy only because the bench is heavy.
3. **Do not gate the whole-tree walks.** `prune_stale_child_links` measured flat when skipped (538.7 µs
   against a 541–546 µs baseline), and `collect_live_keys`'s frozen-set ablation (344.8 µs) buys its
   win by no longer growing the read graph.



## 5. Reproducing

```bash
# 1. the section split
git apply target/probe/compose_sections_probe.patch
WINIA_SECT_PROF=1 WINIA_SECT_WARMUP=10000 cargo bench -p winia -- --nocapture   # ends with the [sect] table
git checkout -- winia/src/core/composer.rs winia/benches/recompose.rs

# 2. the real-app frame timing — run BOTH builds; the debug one alone misleads about the split
git apply target/probe/frame_timing_probe.patch
cargo build            -p winia --example lazy_column_demo --features debug-server
cargo build --release  -p winia --example lazy_column_demo --features debug-server
WINIA_FRAME_PROF=1 WINIA_DEBUG_PORT=9989 ./target/release/examples/lazy_column_demo.exe > frame.log 2>&1 &
python tmp/drive_lazy_demo.py        # 181 commands; aborts if TREE output stops
grep '^\[fp\]' frame.log             # one line per rendered frame: <µs>
taskkill //F //IM lazy_column_demo.exe   # the exe stays locked until it exits (link fails with 1104)
git checkout -- winia/src/app.rs winia/src/debug.rs
```

Both probes are self-declared throwaway in their own comments (`THROWAWAY PROBE`), gated so a normal
build is untouched, and were reverted at the end of the round. The drive script lives at
`tmp/drive_lazy_demo.py`.
