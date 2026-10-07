//! `Crossfade` — cross-fading content switch (mirrors Compose `Crossfade`)
//!
//! ```ignore
//! Crossfade::new(page)                            // State<Page>
//!     .animation(TweenSpec::default())
//!     .build(ctx, |ctx, p| {                      // the content closure gets that generation's state
//!         Text::new(format!("Page: {:?}", p)).build(ctx);
//!     });
//! ```
//!
//! Mechanism (BOTH generations on screen, overlapping inside one tween — Compose's semantics):
//! - State: `current` (the incoming generation) + `previous: Option<T>` (the outgoing one)
//!   + `enter` (incoming 0→1) and `exit` (outgoing 1→0) progresses
//! - Switch: a target change moves the old target into `previous` and the new one into `current`
//!   IMMEDIATELY; both compose into their own container slot, the incoming composed last so it
//!   draws on top, and the outgoing is dropped when its own `exit` bottoms out
//! - Draw layers: each generation's `graphics_layer` reads its own progress (render-time `peek` —
//!   no remeasure, no recompose)
//! - Layout: `BoxLayout` stack semantics, the container is the content's size — there is NO size
//!   animation. That is the only thing separating this from `AnimatedContent`, which is how
//!   Compose draws the line too.
//!
//! Compose's `Crossfade` keeps a `currentlyVisible` list (`Crossfade.kt:104`) and gives every
//! visible state its own `animateFloat` alpha; `previous`/`current` are that list here.
//!
//! Known differences from Compose:
//! - **One outgoing generation only.** Compose's list can hold three at once (a switch made
//!   mid-transition); `previous` is a single `Option`, so a rapid re-switch hard-cuts the middle
//!   generation.
//! - No `label` (the engine's parameter is `_label`, unused).

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{interpolator, push_animatable, AnimationSpec, TweenSpec};
use crate::layout::box_layout::BoxLayout;
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use std::time::Duration;

/// Stable key bases for the two generations (`ctx.key` scopes). Positional keys do not work: once
/// the outgoing generation leaves, the incoming one's statement key shifts up a slot and inherits
/// it (same shape as `animated_content.rs`).
const GENERATION_ENTRY: u64 = 0xC205_0000_0000_0001;

/// Compose `tween()`'s default easing, `FastOutSlowInEasing` = `CubicBezierEasing(0.2, 0, 0, 1)`.
fn fast_out_slow_in() -> interpolator::CubicBezier {
    interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0)
}

/// Compose `Crossfade`'s default `animationSpec = tween()`: 300 ms, `FastOutSlowInEasing`.
fn default_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(300), fast_out_slow_in()))
}

/// Content switch transition (both generations on screen, cross-fading).
pub struct Crossfade<T> {
    target: State<T>,
    spec: AnimationSpec,
    modifier: Modifier,
    /// Content identity (Compose's `contentKey`, `Crossfade.kt:101`): two states with the same key
    /// are the SAME content — the new value is swapped in with NO transition at all. Without one,
    /// every value change animates. The caller maps its state to a `u64`.
    content_key: Option<Box<dyn Fn(&T) -> u64 + Send + Sync>>,
}

impl<T: Clone + PartialEq + 'static> Crossfade<T> {
    /// Compose's defaults: `animationSpec = tween()` (300 ms, `FastOutSlowIn`).
    pub fn new(target: State<T>) -> Self {
        Self {
            target,
            spec: default_spec(),
            modifier: Modifier::new(),
            content_key: None,
        }
    }

    /// The switch animation spec (Compose's `animationSpec` parameter).
    pub fn animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.spec = spec.into();
        self
    }

    /// A modifier for the container itself (Compose's `modifier` parameter).
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = m;
        self
    }

    /// The same key means the same content: the state changes but nothing animates (Compose's
    /// `contentKey` semantics).
    pub fn content_key(mut self, f: impl Fn(&T) -> u64 + Send + Sync + 'static) -> Self {
        self.content_key = Some(Box::new(f));
        self
    }

    /// One generation's composition key (Compose's `key(contentKey(it))`, `Crossfade.kt:136`).
    ///
    /// With a `content_key` the caller's key is used, so a state that comes back REUSES its slots.
    /// Without one it falls back to a counter: a fresh slot per generation. Either way it has to
    /// vary per generation, but not for the reason it is tempting to write down: with the
    /// `ctx.changed(&value)` declaration in `build` the content closure re-runs on a value change
    /// anyway (measured: making the outgoing key constant alone still produced the right pair). What
    /// a constant key does is reuse the slots UNDER the closure, so what a generation `remember`s is
    /// the previous generation's, and a switch made mid-transition can show the old pair for a
    /// frame — measured with both keys constant: `[50, 60]` one frame after the target moved to the
    /// third value, where `[60, 70]` belongs.
    fn generation_key(&self, value: &T, counter: u64) -> u64 {
        match &self.content_key {
            Some(k) => k(value),
            None => counter,
        }
    }

    /// Build the cross-fading container.
    ///
    /// ⚠ Do not wrap this in a macro: the `target` and `exit` dependencies have to be read by THIS
    /// function's own scope. A macro would close them into an inner scope, the parent container
    /// would no longer see the progress move, and the completion check would freeze (the same
    /// regression `animated_visibility` hit).
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx, T)) {
        // Dependency: a target change recomposes the caller's scope, so this `build` re-runs and
        // the container's slot goes dirty with it.
        let target = self.target.get();
        // Everything on screen, plus one alpha per entry. Compose holds the same two things: a
        // `currentlyVisible` list and, for each entry, an `animateFloat` toward 1 if that entry IS
        // the target and 0 otherwise (`Crossfade.kt:104-136`). The list lives in a `remember`ed
        // struct because the alphas have to be indexed by entry — a `ctx.remember` call site is a
        // statement position, so it cannot hand out one slot per entry.
        let core = ctx.remember_backchannel(|| VisibleSet::new(target.clone()));
        let fade = VisibilityTransition::fade_in(self.spec.clone());

        let key = ctx.next_key();
        match ctx.start_restartable_group(key, self.modifier.clone(), BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Per-frame work happens INSIDE this branch: a write made before the container
                // starts would mark its slot dirty during the pass already consuming it. See the
                // same comment in `animated_content.rs` for the measurement.
                let target_now = self.target.get();
                // Subscribe to the entries, and to every entry's alpha: that second half is what
                // keeps the container entering each frame while a fade runs, which is where the next
                // frame's `push_animatable` and the drop rule below live. Without the read the
                // composition is skipped after the first frame and nothing ever leaves the screen
                // (measured: the outgoing entry stayed composed and
                // `crossfade_switches_content_on_target_change` read `[50, 200]` where `[200]` is
                // right).
                let mut set = core.peek();
                for entry in set.entries.iter() {
                    let _ = set.alpha(entry.era).get();
                }

                // ── The target is not on screen yet, or is on screen already ──
                // Compose asks two different questions here (`Crossfade.kt:112-124`): whether the
                // target VALUE is the state currently shown, and — separately — whether any visible
                // state shares its `contentKey`. A value that is new but matches a visible state's
                // key REPLACES that entry in place, which is what makes a state that comes back
                // keep its alpha (and its slots) and reverse its fade instead of restarting from
                // nothing.
                let newest_is_the_target = set
                    .entries
                    .last()
                    .map(|e| e.value == target_now)
                    .unwrap_or(false);
                if !newest_is_the_target {
                    match set.position_by_key(&target_now, &self.content_key) {
                        Some(i) => {
                            // In place: the entry keeps its era, so its alpha keeps running and its
                            // slots survive. A `content_key` match therefore updates the content
                            // with no animation at all, which is the point of the key.
                            //
                            // It deliberately does NOT move to the end. Compose's
                            // `currentlyVisible[replacementId] = targetState` leaves the position too
                            // and draws the target on top regardless, but winia matches a child's key
                            // at its POSITION (or by its path-hash prefix there), so a reorder
                            // truncates from that index and rebuilds both subtrees — losing exactly
                            // the `remember`ed state this branch exists to preserve. Measured in a
                            // minimal probe: `ctx.key` around a restartable group, two entries, swap
                            // the order, both slots created again. The cost of not moving is z-order
                            // only: a state that comes back while another is leaving draws under it
                            // for the length of that fade.
                            set.entries[i].value = target_now.clone();
                        }
                        None => set.push(target_now.clone()),
                    }
                    core.set(set.clone());
                }

                // ── One alpha per entry: toward 1 if it is the target, 0 otherwise ──
                for entry in set.entries.iter() {
                    let goal = if self.matches(&entry.value, &target_now) { 1.0 } else { 0.0 };
                    push_animatable(set.alpha(entry.era), goal, self.spec.clone());
                }

                // ── Drop what has finished leaving. The target itself never drops: a brand new
                // entry starts at 0 and has to be allowed to rise.
                let survivors: Vec<Entry<T>> = set
                    .entries
                    .iter()
                    .filter(|e| self.matches(&e.value, &target_now) || set.alpha(e.era).peek() > 0.001)
                    .cloned()
                    .collect();
                if survivors.len() != set.entries.len() {
                    set.retain(survivors);
                    core.set(set.clone());
                }

                // ── Compose what is left, oldest first so the newest is drawn on top ──
                for entry in set.entries.iter() {
                    let cfg = fade.clone();
                    let g = set.alpha(entry.era);
                    let layer =
                        Modifier::new().graphics_layer(move || cfg.layer_params(g.peek(), (0.0, 0.0)));
                    let value = entry.value.clone();
                    let gen_id = self.generation_key(&value, entry.era);
                    ctx.key((GENERATION_ENTRY, gen_id), |ctx| {
                        // Declare this entry's value as the slot parameter: a value change re-runs
                        // the content without needing the key to change.
                        ctx.changed(&value);
                        let mkey = ctx.next_key();
                        if let GroupStatus::Enter =
                            ctx.start_restartable_group(mkey, layer, BoxLayout::new())
                        {
                            content(ctx, value.clone());
                        }
                        ctx.end_restartable_group();
                    });
                }
            }
        }
        ctx.end_restartable_group();
    }

    /// Whether an entry's value counts as the same content as the target — Compose's `contentKey`
    /// comparison, which is value equality when the caller supplied no key.
    fn matches(&self, a: &T, b: &T) -> bool {
        match &self.content_key {
            Some(k) => k(a) == k(b),
            None => a == b,
        }
    }
}

/// One state on screen, with the composition key its slots live under.
#[derive(Debug, Clone)]
struct Entry<T> {
    value: T,
    era: u64,
}

/// Every state on screen plus one alpha per entry — Compose's `currentlyVisible` and its
/// `contentMap`'s `animateFloat`s, in one `remember`ed value.
///
/// The alphas are `State<f32>` handles kept in a map keyed by entry, not by position: an entry keeps
/// its own alpha across the switches of the entries around it, which is what lets a state that comes
/// back reverse mid-fade instead of restarting.
#[derive(Debug)]
struct VisibleSet<T> {
    entries: Vec<Entry<T>>,
    alphas: std::collections::HashMap<u64, State<f32>>,
    next_gen: u64,
}

impl<T: Clone> Clone for VisibleSet<T> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            alphas: self.alphas.clone(),
            next_gen: self.next_gen,
        }
    }
}

/// Entries compare by their value (the key an entry is identified by) and their era spans the set:
/// `State::set` requires `PartialEq`, and two sets with the same states on screen in the same order
/// are the same set to the composition, alphas aside (the alphas live in `State`s the container
/// subscribes to separately).
impl<T: PartialEq> PartialEq for VisibleSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.next_gen == other.next_gen
            && self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .zip(other.entries.iter())
                .all(|(a, b)| a.era == b.era && a.value == b.value)
    }
}

impl<T: Clone + PartialEq + 'static> VisibleSet<T> {
    fn new(first: T) -> Self {
        let mut alphas = std::collections::HashMap::new();
        alphas.insert(0, State::new(1.0));
        Self {
            entries: vec![Entry { value: first, era: 0 }],
            alphas,
            next_gen: 1,
        }
    }

    /// This entry's alpha. An entry that is on screen always has one — `push` creates it — so the
    /// fallback only covers a value that was never pushed.
    fn alpha(&self, era: u64) -> State<f32> {
        self.alphas.get(&era).cloned().unwrap_or_else(|| State::new(0.0))
    }

    /// Where the entry matching `value` by content key sits, if it is on screen.
    fn position_by_key(&self, value: &T, key: &Option<Box<dyn Fn(&T) -> u64 + Send + Sync>>) -> Option<usize> {
        self.entries.iter().position(|e| same(&e.value, value, key))
    }

    fn push(&mut self, value: T) {
        let era = self.next_gen;
        self.next_gen += 1;
        self.alphas.insert(era, State::new(0.0));
        self.entries.push(Entry { value, era });
    }

    /// Keep only `survivors`, dropping the alphas of everything else.
    fn retain(&mut self, survivors: Vec<Entry<T>>) {
        let keep: std::collections::HashSet<u64> = survivors.iter().map(|e| e.era).collect();
        self.alphas.retain(|era, _| keep.contains(era));
        self.entries = survivors;
    }
}

fn same<T: PartialEq>(a: &T, b: &T, key: &Option<Box<dyn Fn(&T) -> u64 + Send + Sync>>) -> bool {
    match key {
        Some(k) => k(a) == k(b),
        None => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::constraints::Constraints;
    use crate::layout::node::LayoutNode;
    use crate::runtime::composer::Composer;
    use crate::runtime::state::State;

    /// Test leaf: its width reflects the state (page=0 → 50, anything else → 200).
    struct SizedLeaf {
        w: f32,
    }
    impl SizedLeaf {
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(self.w, 30.0),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    /// Every leaf's width, ascending — two generations on screen show up as two entries.
    fn leaf_widths(composer: &Composer) -> Vec<f32> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize, out: &mut Vec<f32>) {
            if nodes[idx].children.is_empty() {
                out.push(nodes[idx].measured_size.width);
                return;
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, out);
            }
        }
        let mut out = Vec::new();
        walk(nodes, root, &mut out);
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    /// Integration: a target change cross-fades both generations and settles on the new content.
    #[test]
    fn crossfade_switches_content_on_target_change() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, page| {
                    let w = if page == 0 { 50.0 } else { 200.0 };
                    SizedLeaf { w }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initially target 0");

        target.set(7);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![200.0], "after the switch only target 7");

        target.set(0);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "and back to target 0");
    }

    /// Retarget mid-transition (A→B unfinished, then C): converges on the new target, no panic.
    #[test]
    fn crossfade_retarget_mid_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, page| {
                    let w = match page {
                        0 => 50.0,
                        1 => 100.0,
                        _ => 200.0,
                    };
                    SizedLeaf { w }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..15 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(30));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0]);
        target.set(1);
        recompose(&mut composer);
        for _ in 0..2 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
            recompose(&mut composer);
        }
        // The cross-fade is still running — retarget to 2 on top of it.
        target.set(2);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![200.0], "retarget converges on the new target");
    }

    /// Compose's `Crossfade` keeps the OUTGOING generation composed and fades it alongside the
    /// incoming one (`Crossfade.kt:104`'s `currentlyVisible`). The old implementation faded the
    /// outgoing to nothing, swapped, then faded the incoming in — one generation at a time.
    #[test]
    fn both_generations_are_composed_during_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, page| {
                    let w = if page == 0 { 50.0 } else { 200.0 };
                    SizedLeaf { w }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance_one = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(composer);
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "at rest: one generation");

        target.set(7);
        recompose(&mut composer);
        advance_one(&mut composer);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 200.0],
            "both generations are on screen from the frame after the switch"
        );

        // The outgoing is torn down once it is done.
        for _ in 0..40 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "the outgoing is removed");
    }

    /// One transition plays ONE spec's worth of time, not two (the old serial version needed
    /// 2×300 ms). What is observable is when the animation stops: serial was still fading in at
    /// 400 ms.
    #[test]
    fn the_switch_takes_one_spec_duration_not_two() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone())
                    .animation(TweenSpec::new(
                        std::time::Duration::from_millis(300),
                        crate::animation::interpolator::Linear::new(),
                    ))
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 50.0 } else { 200.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        target.set(7);
        recompose(&mut composer);
        let start = std::time::Instant::now();
        let mut settled_at = None;
        for _ in 0..60 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
            recompose(&mut composer);
            if !crate::animation::is_animating() {
                settled_at = Some(start.elapsed().as_millis() as f32);
                break;
            }
        }
        let settled = settled_at.expect("the transition must finish");
        assert!(
            settled < 450.0,
            "one 300ms spec, not two: still animating at {settled}ms"
        );
        assert_eq!(leaf_widths(&composer), vec![200.0]);
    }

    /// `content_key`: two states with the same key are the same content — the value is swapped in
    /// without a transition (Compose's `contentKey`), so no second generation appears; and the leaf
    /// width proves the content really did re-run with the new value.
    #[test]
    fn the_same_content_key_does_not_animate() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone())
                    // 0..=9 is one piece of content (two renders of the same page); 10 is a new one.
                    .content_key(|p| if *p < 10 { 0 } else { 1 })
                    .build(ctx, |ctx, page| {
                        // The width follows the VALUE, not the key: with a key-derived width this
                        // test could not tell "re-ran with the new value" from "never re-ran".
                        let w = 50.0 + page as f32 * 10.0;
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initial");

        target.set(3);
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![80.0], "the content re-ran with the new value");

        target.set(10);
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![80.0, 150.0], "a new key animates");
    }

    /// A switch made while a cross-fade is STILL RUNNING puts the new target on screen at once, and
    /// the state it interrupted keeps fading out instead of being hard-cut.
    ///
    /// Compose's `currentlyVisible` is a list, so all three are composed for a moment: the one that
    /// was leaving, the one that was arriving, and the new target (`Crossfade.kt:104-136`). The
    /// implementation this replaced kept a single outgoing generation, which threw the interrupted
    /// one away mid-fade; a constant composition key shows up here as the new target never appearing
    /// at all (measured: `[50, 60]` where `[50, 60, 70]` belongs).
    #[test]
    fn a_switch_during_a_transition_puts_the_target_on_screen_at_once() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, page| {
                    SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer, n: usize| {
            for _ in 0..n {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(16));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        target.set(1);
        recompose(&mut composer);
        advance(&mut composer, 2);
        assert_eq!(leaf_widths(&composer), vec![50.0, 60.0], "the first cross-fade is running");

        // …switch again while that one is still in flight
        target.set(2);
        recompose(&mut composer);
        advance(&mut composer, 1);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 60.0, 70.0],
            "the target is on screen immediately, and the state the switch interrupted keeps fading"
        );

        advance(&mut composer, 60);
        assert_eq!(leaf_widths(&composer), vec![70.0]);
    }

    /// How many content slots have ever been created, to tell "the entry kept its slot" from "the
    /// entry was thrown away and rebuilt".
    static SLOT_MARKERS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// Compose replaces an entry that is still on screen IN PLACE (`Crossfade.kt:117-123`:
    /// `currentlyVisible[replacementId] = targetState`), so a state that comes back keeps its own
    /// alpha AND its own slots — it reverses its fade instead of restarting from nothing.
    ///
    /// The width here encodes which slot the content lives in (`remember` runs once per slot, so the
    /// marker increments when a slot is created), which is what makes the difference visible: the
    /// two-generation implementation dropped the outgoing entry and rebuilt the returning one, and
    /// that shows up as a third marker.
    #[test]
    fn a_state_that_comes_back_reuses_its_own_slot() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        SLOT_MARKERS.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, _page| {
                    let slot = ctx
                        .remember(|| SLOT_MARKERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
                        .peek();
                    SizedLeaf { w: 10.0 + slot as f32 * 100.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer, n: usize| {
            for _ in 0..n {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(16));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![10.0], "the first state lives in slot 0");

        target.set(1);
        recompose(&mut composer);
        advance(&mut composer, 2);
        assert_eq!(leaf_widths(&composer), vec![10.0, 110.0], "the second state gets its own slot");

        // …back to 0 while 1 is still fading: 0 is on screen, so it is replaced in place.
        target.set(0);
        recompose(&mut composer);
        advance(&mut composer, 1);
        assert_eq!(
            leaf_widths(&composer),
            vec![10.0, 110.0],
            "the state that came back kept its own slot, and no third one was created"
        );
        assert_eq!(
            SLOT_MARKERS.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "exactly two slots were ever created"
        );

        advance(&mut composer, 60);
        assert_eq!(leaf_widths(&composer), vec![10.0], "and it settles on the state that came back");
    }

    /// A cross-fade leaves nothing running behind it: once only the target is on screen, the
    /// animation engine is idle again.
    #[test]
    fn nothing_is_still_animating_once_a_switch_settles() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone()).build(ctx, |ctx, page| {
                    SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        target.set(1);
        recompose(&mut composer);
        let mut settled = false;
        for _ in 0..80 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
            if !crate::animation::is_animating() && leaf_widths(&composer) == vec![60.0] {
                settled = true;
                break;
            }
        }
        assert!(settled, "the cross-fade finishes and the engine goes idle");
        assert!(!crate::animation::is_animating(), "nothing is left animating");
    }


}
