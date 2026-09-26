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

(Two runs later this table was superseded by the finer one below — 1649 µs a frame, with the draw
phase split into Skia recording and present. Both are real; the difference is machine load, and the
SHARES are the stable part. The finer table is the one to read.)

> ## Phase 1: the frame's phases, and one plan refuted by them
>
> The probe was extended to the frame's phases and run on the same scripted interaction
> (`WINIA_FRAME_PROF`; release build; 120 steady frames of 128):
>
> | phase | p50 | share |
> |---|---|---|
> | compose loop | 263 µs | 15.9 % |
> | main layout | 296 µs | 17.1 % |
> | overlay pass | 6 µs | 0.4 % |
> | predraw (flight poll, focus sync, draw setup) | 28 µs | 1.7 % |
> | draw — of which Skia recording | 520 µs | 32.5 % |
> | ↳ `skia` (tree walk + transition layer + overlays) | 155 µs | 9.2 % |
> | ↳ `flush` (the surface's submit/present) | 361 µs | 23.2 % |
> | after (screenshot readback, IME sync) | 0 µs | 0 % |
> | **frame** | **1649 µs** | |
>
> Two controls, because the present figure invites a wrong reading:
>
> - **`WINIA_RENDER_BACKEND=cpu`**, same script: frame 2051 µs, `draw` 728 µs, `skia` 419 µs,
>   `flush` 303 µs. So `flush` is 300-400 µs on BOTH backends — with the CPU path it is softbuffer's
>   blit, with Vulkan the fence/present path — and is not a Vulkan defect. It is the Windows present
>   floor, outside the framework.
> - **The compose tail on this same tree**, from the section probe (`psect`, 125 frames): 56.5 µs a
>   frame — `materialize` 31.3, `reconcile` 9.1, `retain_shared` 5.5, `modifier_deps` 3.0,
>   `prev_drain` 2.7, `prune` 2.5, `live_keys` 2.3. **That is 3.4 % of the frame**, and the single
>   item phase 2 of this round planned to rewrite (`reconcile`'s reverse-graph rebuild, ~48 µs of an
>   800-row bench frame) is **9 µs — 0.55 % — on a real tree.**
>
> What this establishes:
>
> 1. **The plan to optimize `reconcile` next is refuted, not deferred.** The bench's figures scale
>    with its 800-row tree; a real screen's tail is 23 % of compose and 3.4 % of the frame, so a
>    correct, careful rewrite of its largest item buys 0.55 % and would have to be justified by
>    something other than frame time. Recommendation: do not run it for performance.
> 2. **The framework's own CPU work is ~750 µs of a 1649 µs frame** (compose 263 + layout 296 +
>    skia 155 + overlay/predraw 34), the rest being the present floor and the frame's bookkeeping.
>    At 60 Hz that is ~11 % of the budget for a 59-node screen.
> 3. **Layout (296 µs) is compose-sized, and that is the compose-side surprise**: with nothing
>    dirty, what shows through is the fixed per-frame work — `register_modifier_deps`'s arena walk,
>    the reuse-index rebuild, the transaction, the dirty sweep — not per-node work.
> 4. **Skia recording is already cheap on Vulkan (155 µs).** On the CPU backend the same recording
>    costs 419 µs, which is the honest place a renderer round would aim — but that backend is the
>    fallback, not the default.
>
> Reproduce with: `git apply target/probe/frame_phase_probe.patch` (the phase split plus `ptree`),
> `target/probe/compose_sections_probe.patch` (the tail split plus `psect`), then
> `tmp/drive_lazy_demo_quit.py` against a demo built with `--features debug-server`, with
> `WINIA_FRAME_PROF=1` and/or `WINIA_SECT_PROF=1`. The raw logs are
> `target/probe/frame_phase_split_{debug,release}.log` and `target/probe/frame_drawsplit_release.log`.

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

> ## Phase 2: the split against tree size, and layout's real shape
>
> The phase figures above come from a 59-node screen. A real screen can show several times that, so
> the same probe was run over a size sweep: the same rows and the same script, only the window height
> changed (`winia/examples/probe_scaling.rs`, a probe example that was deleted after the run).
> Release, p50 per arm:
>
> | window height | nodes | frame | compose | layout | draw | overlay+predraw |
> |---|---|---|---|---|---|---|
> | 620 | 46 | 985 µs | 296 | 246 | 389 | 34 |
> | 1200 | 72 | 1188 µs | 325 | 365 | 436 | 43 |
> | 2400 | 128 | 1533 µs | 392 | 549 | 510 | 43 |
> | 4000 | 202 | 1887 µs | 504 | 724 | 620 | 36 |
>
> Least-squares over the four arms:
>
> | phase | slope | intercept |
> |---|---|---|
> | layout | **2.95 µs/node** | −46 µs |
> | compose | **1.29 µs/node** | 240 µs |
> | draw | 1.40 µs/node | 330 µs |
> | frame | 5.08 µs/node | 730 µs |
>
> What this changes:
>
> 1. **Layout is the most expensive per-node phase, and it is 3x compose's slope.** The earlier claim
>    of this document ("layout is compose-sized because nothing is dirty, so what shows through is
>    fixed work") is right for a 59-node tree and **wrong as a generalisation**: at 128 nodes layout
>    overtakes compose (549 vs 392 µs) and at 202 it is 1.44x compose. Compose's own slope (1.29 µs a
>    node) is the row machinery the bench measures; layout's (2.95 µs) is `measure_node` plus the two
>    whole-arena walks — `register_modifier_deps` and `collect_layout_index`/its reuse index — and it
>    is paid on every frame for every node, dirty or not.
> 2. **The frame budget, stated in the unit that matters.** At ~5.1 µs a node a frame at 60 Hz
>    supports on the order of 3 000 visible nodes before overrun (16.7 ms / 5.1 µs) — minus the
>    ~730 µs of fixed cost. That is the honest answer to "is winia fast enough": yes for screens of
>    hundreds of nodes, and the per-node price is what decides the ceiling.
> 3. **The compose-side target is therefore layout, not `reconcile`.** Layout's slope is where a real
>    optimization round belongs: `register_modifier_deps` walks the arena to re-register modifier
>    dependencies every frame regardless of whether any modifier changed, and layout rebuilds the
>    reuse index over the whole tree every frame. Both are candidates whose cost scales exactly like
>    the measured slope, and §1 prices them (~57 µs and ~52 µs at 3201 nodes, i.e. ~0.017 and ~0.016 µs
>    a node — the two together are only ~1 % of the slope, so measuring `measure_node` itself is the
>    first step of such a round, not implementing a fix).
>
> The earlier recommendation stands unchanged for the write side: the `reconcile` rewrite stays
> unrefuted as a *correctness*-neutral change and refuted as a *frame-time* one (9 µs on a 128-node
> tree, 0.55 %).
>
> ### Layout's interior, by ablation
>
> The slope above says layout is where the per-node cost is; the §1 section table says its two
> whole-arena walks are only ~0.033 µs/node between them, i.e. ~1 % of the slope. To find the rest,
> the layout body was ablated at two sizes (`WINIA_ABLATE_MEASURE` skips `measure_node` itself,
> `WINIA_ABLATE_INDEX` the reuse-index walk, `WINIA_ABLATE_MODDEPS` the compose tail's modifier-deps
> walk):
>
> | node count | baseline frame / layout | index off | mod-deps off | measure off |
> |---|---|---|---|---|
> | 128 | 1563 / 561 µs | 1651 / 559 (no win) | 1601 / 576 (no win) | cannot render |
> | 202 | 2067 / 842 µs | **1919 / 759 (−148 µs, −7 % of frame)** | 2037 / 803 (−30 µs, within noise) | cannot render |
>
> `measure_node` cannot be skipped outright — with no measurement the tree has no sizes and the frame
> stops drawing frames at all — so its cost is what remains: at 202 nodes, layout minus the two walks
> is still ~700 µs, which is 3.5 µs a node inside measurement and its dependants.
>
> Conclusion for the next round: **the target is `measure_node`, not the walks.** The reuse-index walk
> is worth ~7 % of the frame at 202 nodes (a real but secondary number; note its ablation also
> degrades reuse on the following frames, so −148 µs is a lower bound on its cost, not an upper), the
> modifier-deps walk is not measurable at these sizes, and the rest of layout is the per-node
> measurement pass itself.
>
> ### Inside `measure_node`
>
> The measurement pass was then split four ways (`WINIA_MEASURE_PROF`; the calls RECURSE through the
> policies, so the buckets nest: `policy` covers the subtree). ns **per measured call**:
>
> | bucket | 128-node tree (399 calls) | 202-node tree (613 calls) |
> |---|---|---|
> | fold check (clean → reuse cached size) | 38 | 42 |
> | pre-policy: constraint math + the modifier queries (`resolved_size`, `min/max/required_size`, `fixed_size`, padding, fill-max, scroll) | **177** | **160** |
> | policy (includes the whole child recursion and text shaping) | 88 732 | 65 343 |
> | post-policy (aspect ratio, scroll clamping, placement bookkeeping) | 36 | 37 |
>
> Two readings, both of which retire a hypothesis:
>
> 1. **The modifier queries are not the cost.** They are ~5 % of a measured call (177 ns of ~3.5 µs),
>    i.e. ~100 µs of an 800 µs layout frame in total — worth knowing, not worth a rewrite, and the
>    idea that "10 linear scans a node" would explain the slope is dead.
> 2. **Almost all of the cost is inside the measure policies** — `measure_flex`'s two-phase loop
>    (fixed children, then weighted children with their allocation), `LazyColumn`'s own traversal, and
>    the text shaping a measured `Text` performs. The nested accounting cannot separate those three
>    without instrumenting the policies themselves, which is where a further round would start.
>
> One structural fact worth carrying forward: **613 measured calls for a 202-node tree** — nodes are
> measured about three times per layout frame (the recursive descent, the two-phase flex loop for
> weighted children, the lazy container's own pass). Whether those repeats are all load-bearing is
> exactly what policy-level instrumentation would answer, and it is the largest single lever the
> numbers here point at (halving the calls halves the phase).
>
> ### The repeats, counted per node
>
> A second probe pass counted measured calls per node and per layout pass (sampled passes; the tree is
> the probe example: a `Column` holding a `Row` of two buttons plus a `LazyColumn` of ~100 visible
> rows, 195 nodes):
>
> ```
> [measures] pass #3: 390 measured calls over 195 distinct nodes
>   containers 198 / leaves 192
>   calls-per-node (calls: nodes): {2: 195}
>   calls per layout pass, in order (pass, calls): [(0, 0), (1, 44), (2, 223), (3, 195), (4, 390)]
> ```
>
> Established by this, and no more:
>
> 1. **The repeats are real and uniform in the sampled pass**: every one of the 195 nodes was measured
>    exactly twice in that pass (`{2: 195}`, no node once, none three times), while the *whole pass*
>    recorded 390 calls. So the doubling is a property of the pass, not of a subset of nodes.
> 2. **It is not one call per frame**: the app's own counter recorded **one `Composer::layout()` call
>    per frame** while the measure module counted a pass per call, so the two counters agree — the
>    doubling happens *inside* a single layout call.
> 3. **The background level is visible in the pass sequence**: `44, 223, 195, 390` calls for the same
>    tree across consecutive passes. The two low figures are passes where most nodes folded (nothing
>    dirty → the constant-fold arm returns before any counter), and the two high ones are passes that
>    measured the tree. The doubling appears in the high figure.
>
> NOT established, and deliberately not claimed: **which caller performs the second measurement.**
> The candidates the code shows are the flex policy's two-phase child loop (`flex.rs:188` and `:219` —
> phase 2 re-measures WEIGHTED children only, and this tree has none), the lazy container's own child
> measurement, and `Composer::layout`'s guarded second pass for a flight's layout override
> (`app.rs:722`, taken only when the poll attached an override for the first time). Telling them apart
> needs the caller's own counter inside the policy, which is the next probe, not the next guess.



## 4. What this round recommends

1. **Do not run the `reconcile` rewrite as a performance round.** It was the named next step on the
   strength of the bench's 800-row figures (~48 µs of an idle compose frame). On a real tree it is
   9 µs, 0.55 % of the frame, and the whole compose tail is 3.4 % (2.5 % at 128 nodes). A correct
   incremental rewrite is a graph-invariant change whose failure mode is stale content; 0.55 % does
   not pay for it.
2. **Layout's 2.95 µs/node is the compose-side target** (phase 2 above). It is the largest per-node
   slope in the frame, it is paid on every node of every frame, and its two whole-arena walks are only
   ~1 % of it. The measurement pass has since been split (phase 3): the modifier queries are ~5 % of a
   measured call and the fold check is free, so what remains is the measure policies — and
   **613 measured calls for 202 nodes**, i.e. ~3 measurements a node per layout frame. Whether every
   repeat is load-bearing is the concrete next question, and the largest lever visible from here.
3. **The present path (300–400 µs on both backends) is half the frame and outside the framework's
   control** — softbuffer's blit on CPU, the fence/present path on Vulkan. It is worth understanding
   (a GPU-bound frame is not reducible by CPU work) but it is not a framework optimization.
4. **Skia recording is cheap on the default backend** (155 µs at 59 nodes, slope 1.40 µs/node) and 2.7x
   more expensive on the CPU fallback (419 µs) — the fallback is where a renderer round would aim.
5. **Do not gate the whole-tree walks.** `prune_stale_child_links` measured flat when skipped (538.7 µs
   against a 541–546 µs baseline), and `collect_live_keys`'s frozen-set ablation (344.8 µs) buys its
   win by no longer growing the read graph.
6. **The structural-change tail is still unsplit** (p99 5.8 ms against a 1.6 ms p50): the jump frame
   composes content that has never been composed, which is the cold-frame path the bench prices at
   22 ms for 800 text rows. It is the last target in this frame whose cost is milliseconds rather than
   microseconds.



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

# 3. measure_node's interior — needs a tree whose size can be varied, so it uses a probe example
#    (two files: the example plus the probe). The example is `lazy_column_demo`'s content with the
#    window height from PROBE_H; recreate it under `winia/examples/` and build with
#    `--features debug-server`, then:
git apply target/probe/measure_interior_probe.patch   # app.rs + composer.rs + debug.rs + node.rs, +232 lines
PROBE_H=4000 WINIA_DEBUG_PORT=9994 WINIA_MEASURE_PROF=1 ./target/release/examples/<example>.exe > m.log 2>&1 &
#   connect over the WebSocket, drive the same script, then send `pmeasure` and read `[measure]` from m.log
git checkout -- winia/src
```

Both probes are self-declared throwaway in their own comments (`THROWAWAY PROBE`), gated so a normal
build is untouched, and were reverted at the end of the round. The drive script lives at
`tmp/drive_lazy_demo.py`.
