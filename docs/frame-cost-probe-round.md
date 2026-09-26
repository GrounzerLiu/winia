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

The same script was run against a **debug** build and a **release** build, because the first
measurement (debug only) turned out to be misleading about the production split:

| | debug | release |
|---|---|---|
| p50 | 2 908 µs | **1 253 µs** |
| p90 | 3 177 µs | 1 654 µs |
| p99 | 6 119 µs | 5 778 µs |
| max (startup) | 10 164 µs | 8 460 µs |
| startup frames 1–5 | 2 241–2 385 µs | 712–821 µs |

Readings:

- **A production frame is ~1.25 ms, and that is the same figure the benchmark prices for its own
  800-row frame** (compose+layout, text, 1 271 µs). So the benchmark is not marginal to the frame:
  with a real screen of this size it is at the same order as the whole frame, and every microsecond
  it removes is worth about a microsecond of frame budget — the opposite of what the debug figures
  suggested (a debug build hides the framework's real cost under unoptimized render/draw code, and
  it hides it precisely *because* the rest of the frame is ~3x more expensive there).
  The comparison is across two different trees (the bench composes 800 rows; this demo composes 15),
  so it is a coincidence of orders rather than a controlled result — but it is the reason the next
  instrument should split the frame's own phases instead of trusting either number alone.
- **No warm-up effect, in either build:** the first rendered frames are the cheapest of the run
  (712–821 µs release against a 1 253 µs p50). The framework's first-frame cost lives in the first
  *compose of each screen*, not in the first rendered frame.
- **The structural-change tail is real work, not debug overhead:** p99 (6.1 ms debug / 5.8 ms
  release) and max (10.2 / 8.5 ms) barely move between builds, where the median moves by 2.3x. The
  jump-to-item-500 frame is dominated by something that survives optimization — the cold-frame
  compose path the bench also measures (22066 µs for 800 text rows there, i.e. the same shape at
  scale).
- Side observation, useful to the UI suite: with the per-frame debug tree on (the non-profiled
  build), the same interaction rendered far fewer frames over the same wall-clock time — the tree
  walk is a large per-frame tax on every debug-server run, which is why `fixture_all` timings are
  not frame timings.

## 4. What this round recommends

1. **The compose tail is worth a formal round after all, and its target is named.**
   `reconcile_compose_deps`'s reverse-graph rebuild is ~48 µs of an idle 800-row frame (~57 µs with
   the cleanup), it is the one large item in §1 whose cost scales with the *read graph* rather than
   with the tree, and a real screen's graph (many slots × several reads each) is much larger than
   the bench's single slot × 800 reads. The nineteenth fix already skips the rebuild when the
   forward graph did not move; what remains is the case where it moved — a one-row update re-records
   the same 800 reads, marks `changed`, and rebuilds the whole reverse index. A per-slot diff that
   rewrites only the slots that changed is the round to measure.
2. **Do not gate the whole-tree walks.** `prune_stale_child_links` measured flat when skipped
   (538.7 µs against a 541–546 µs baseline, ±10 % noise) and its unconditional call is pinned by a
   test written for exactly that shape. `collect_live_keys`'s frozen-set ablation (344.8 µs) looks
   large but buys it by not growing the read graph — a behaviour change, not a skip.
3. **Split the real frame's phases before touching the render side.** The release figure says a frame
   and the benchmark are the same order, but it does not say what share of the 1.25 ms is compose,
   layout, render or present. Extending this probe (draw/submit brackets) is one round of work and
   would decide whether the next optimization round belongs on the compose side at all.


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
