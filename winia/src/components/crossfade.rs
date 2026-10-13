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
//! Mechanism (EVERY generation that has not finished fading stays on screen — Compose's semantics):
//! - `VisibleSet`, a `remember`ed backchannel, holds the entries on screen, one `State<f32>` alpha per
//!   entry keyed by that entry's era, and the counter the next era comes from
//! - A target change that is not on screen yet pushes an entry at alpha 0; when a visible entry shares
//!   the target's `content_key` (or equals it, with no key) that entry is replaced IN PLACE instead, so it
//!   keeps its alpha and its slots and simply reverses its fade
//! - Each entry's alpha is pushed toward 1 when it matches the target and 0 otherwise; an entry that is
//!   not the target and has bottomed out (alpha <= 0.001) is dropped
//! - Draw layers: each generation's `graphics_layer` reads its own alpha at render time (no remeasure, no
//!   recompose); generations compose oldest first, so the newest one draws on top
//! - Layout: `BoxLayout` stack semantics, the container is the content's size — there is NO size
//!   animation. That is the only thing separating this from `AnimatedContent`, which is how Compose draws
//!   the line too.
//!
//! Compose's `Crossfade` keeps a `currentlyVisible` list (`Crossfade.kt:104`) and gives every visible state
//! its own `animateFloat` alpha; `VisibleSet::entries` is that list, and it really does hold several
//! generations — a switch made mid-fade leaves three on screen for a moment
//! (`a_switch_during_a_transition_puts_the_target_on_screen_at_once` asserts `[50, 60, 70]`).
//!
//! Known differences from Compose:
//! - No `label` (Compose's `label: String = "Crossfade"`, `Crossfade.kt:53`); the engine's own label
//!   parameter is not threaded through here.
//! - `content_key` maps to a `u64` rather than Compose's `Any?`.
//! - A `Decay` spec cannot drive a fade (it coasts from a velocity and has no target), so `build` falls
//!   back to the default tween. Compose cannot spell it at all: its parameter is a `FiniteAnimationSpec`.
//!   A `Repeatable` spec is accepted and settles like Compose's (it is finite — its iterations end).

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

/// Compose `Crossfade`'s default `animationSpec = tween()`: 300 ms, `FastOutSlowInEasing`.
fn default_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(
        Duration::from_millis(300),
        interpolator::fast_out_slow_in(),
    ))
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
        // A fade needs a spec that REACHES its target. `Decay` has none — it coasts from a velocity — so an
        // incoming generation would sit at alpha 0 while the outgoing one never left; the engine normalises a
        // Decay on a multi-component value the same way (`normalise_multi_component_decay`). Compose cannot
        // express this at all: `Crossfade`'s parameter is a `FiniteAnimationSpec<Float>`.
        let spec = match self.spec {
            AnimationSpec::Decay(_) => default_spec(),
            ref other => other.clone(),
        };
        let fade = VisibilityTransition::fade_in(spec.clone());

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
                            // The entry moves to the end so it is composed last and drawn on top,
                            // which is Compose's order (`Crossfade.kt:189`). That used to be unsafe
                            // here: `start_slot` matched a child's key only at its current index and
                            // otherwise truncated, so moving a keyed sibling rebuilt it. The keyed
                            // lookup added to `start_slot` is what makes this safe — and
                            // `a_state_that_comes_back_reuses_its_own_slot` is the component-level
                            // check that it still is.
                            set.entries[i].value = target_now.clone();
                            if i + 1 != set.entries.len() {
                                let moved = set.entries.remove(i);
                                set.entries.push(moved);
                            }
                        }
                        None => set.push(target_now.clone()),
                    }
                    core.set(set.clone());
                }

                // ── One alpha per entry: toward 1 if it is the target, 0 otherwise ──
                for entry in set.entries.iter() {
                    let goal = if self.matches(&entry.value, &target_now) { 1.0 } else { 0.0 };
                    push_animatable(set.alpha(entry.era), goal, spec.clone());
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

    /// Whether an entry's value counts as the same content as the target — the one comparison, kept in
    /// [`same`] so `VisibleSet::position_by_key` and this agree by construction. Compose's `contentKey`
    /// comparison, which is plain equality when the caller gave no key.
    fn matches(&self, a: &T, b: &T) -> bool {
        same(a, b, &self.content_key)
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

/// The content-key comparison, in one place: `key(a) == key(b)` when the caller gave a key, plain value
/// equality otherwise (Compose's `contentKey` defaults to the state itself). `Crossfade::matches` and
/// `VisibleSet::position_by_key` both go through this, so they cannot drift apart.
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

    /// Compose's `Crossfade` default is `animationSpec = tween()`, i.e. 300 ms on the
    /// `FastOutSlowInEasing` curve (`animation/Crossfade.kt:53`, `core/AnimationSpec.kt:816-820`).
    /// This used to pass M3's emphasized curve `(0.2, 0, 0, 1)` while naming FastOutSlowIn.
    #[test]
    fn the_default_spec_is_compares_300ms_fast_out_slow_in() {
        match default_spec() {
            crate::animation::AnimationSpec::Tween(spec) => {
                assert_eq!(spec.duration, std::time::Duration::from_millis(300));
                assert!(spec.delay.is_zero());
                // The curve, sampled where the two easings differ most.
                // Values solved independently (bisection on the bezier) and pinned in
                // `animation::interpolator::tests::fast_out_slow_in_is_compares_default_easing`.
                assert!((spec.interpolator.interpolate(0.25) - 0.2366).abs() < 0.005,
                    "FastOutSlowIn(0.25) = {}", spec.interpolator.interpolate(0.25));
                assert!((spec.interpolator.interpolate(0.75) - 0.9594).abs() < 0.005,
                    "FastOutSlowIn(0.75) = {}", spec.interpolator.interpolate(0.75));
            }
            other => panic!("Crossfade's default must be a tween, got {other:?}"),
        }
    }

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

    /// A spec that cannot reach a target must not be able to strand a generation. `Decay` has no target —
    /// it coasts from a velocity — so an incoming generation would sit at alpha 0 while the outgoing one
    /// never left; `build` normalises it to the default tween, the same way the engine normalises a Decay
    /// on a multi-component value. Compose's `FiniteAnimationSpec` cannot even name a decay here.
    ///
    /// Falsified by removing the normalisation: the fade never settles (the loop below reports the leaves
    /// still on screen).
    #[test]
    fn a_decay_spec_still_settles_the_cross_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone())
                    .animation(crate::animation::DecaySpec::default())
                    .build(ctx, |ctx, page| {
                        SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "the first state is alone");
        target.set(1);
        recompose(&mut composer);

        let mut left = Vec::new();
        let mut settled = false;
        for _ in 0..80 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
            left = leaf_widths(&composer);
            if left == vec![60.0] && !crate::animation::is_animating() {
                settled = true;
                break;
            }
        }
        assert!(
            settled,
            "a decay spec must not strand the fade: leaves on screen = {left:?}"
        );
    }

    /// The container's `modifier` is applied to the cross-fading box itself (Compose's `modifier`
    /// parameter), which no test had checked.
    #[test]
    fn the_container_modifier_is_applied() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            Crossfade::new(State::new(0u32))
                .modifier(Modifier::new().size(123.0, 45.0))
                .build(ctx, |ctx, page| {
                    SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let size = composer.arena_nodes()[root].measured_size;
        assert_eq!(
            (size.width, size.height),
            (123.0, 45.0),
            "the modifier sizes the container, and the content is clipped to it by the layout"
        );
    }

    /// Each generation's layer alpha paired with the width of the leaf inside it (50 = the first state,
    /// 60 = the second): the alpha alone cannot say which generation is which on an early frame.
    fn alphas_by_width(composer: &Composer) -> Vec<(f32, f32)> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let mut out = Vec::new();
        fn leaves(nodes: &[LayoutNode], idx: usize, out: &mut Vec<f32>) {
            if nodes[idx].children.is_empty() {
                out.push(nodes[idx].measured_size.width);
                return;
            }
            for &c in &nodes[idx].children {
                leaves(nodes, c, out);
            }
        }
        fn walk(nodes: &[LayoutNode], idx: usize, out: &mut Vec<(f32, f32)>) {
            for el in nodes[idx].modifier.elements() {
                if let crate::modifier::ModifierElement::GraphicsLayer { params_fn } = el {
                    let mut widths = Vec::new();
                    leaves(nodes, idx, &mut widths);
                    if let Some(w) = widths.first() {
                        out.push((*w, (params_fn)().alpha));
                    }
                }
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, out);
            }
        }
        walk(composer.arena_nodes(), root, &mut out);
        out
    }

    /// The cross-fade invariant: the outgoing generation and the incoming one run the SAME curve from the
    /// same moment, one from 1 down to 0 and one from 0 up to 1, so at any instant their alphas sum to 1 —
    /// and the target's alpha RISES while the other's falls. Compose gets this from one `animateFloat` per
    /// visible state (`Crossfade.kt:127-131`); a component that faded only one side, or gave the two sides
    /// different clocks, fails here. The width-based assertions elsewhere never looked at the alpha at all.
    #[test]
    fn both_generations_share_the_fade_so_their_alphas_sum_to_one() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
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
        assert_eq!(
            alphas_by_width(&composer),
            vec![(50.0, 1.0)],
            "one generation, fully opaque"
        );
        target.set(1);
        recompose(&mut composer);

        let mut sampled = None;
        for _ in 0..30 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
            let alphas = alphas_by_width(&composer);
            if alphas.len() == 2 && alphas.iter().all(|(_, a)| *a > 0.02 && *a < 0.98) {
                sampled = Some(alphas);
                break;
            }
        }
        let first = sampled.expect("both generations are mid-fade on some frame");
        let sum: f32 = first.iter().map(|(_, a)| *a).sum();
        assert!(
            (sum - 1.0).abs() < 0.05,
            "the two alphas are complementary (got {first:?}, sum {sum})"
        );

        for _ in 0..3 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
        }
        let later = alphas_by_width(&composer);
        let alpha_of = |v: &[(f32, f32)], w: f32| {
            v.iter().find(|(width, _)| *width == w).map(|(_, a)| *a).expect("that generation")
        };
        assert!(
            alpha_of(&later, 60.0) > alpha_of(&first, 60.0),
            "the target's alpha rises ({first:?} → {later:?})"
        );
        assert!(
            alpha_of(&later, 50.0) < alpha_of(&first, 50.0),
            "and the generation it replaced falls ({first:?} → {later:?})"
        );
    }
}
