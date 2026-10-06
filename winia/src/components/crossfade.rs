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
const GENERATION_PREV: u64 = 0xC205_0000_0000_0001;
const GENERATION_CURRENT: u64 = 0xC205_0000_0000_0002;

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
        // Internal state (`remember` — the statement key is stable, so it survives recomposition).
        let current: State<T> = ctx.remember(|| target.clone());
        let previous: State<Option<T>> = ctx.remember(|| None);
        let enter: State<f32> = ctx.remember(|| 1.0);
        let exit: State<f32> = ctx.remember(|| 1.0);
        let generation: State<u64> = ctx.remember(|| 0);
        let enter_cfg = VisibilityTransition::fade_in(self.spec.clone());
        let exit_cfg = VisibilityTransition::fade_out(self.spec.clone());

        let key = ctx.next_key();
        match ctx.start_restartable_group(key, self.modifier.clone(), BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Everything the frame needs happens INSIDE this branch — see the same comment in
                // `animated_content.rs`: a write made before the container starts marks the
                // container's slot dirty during the pass that is already consuming it, the mark is
                // gone by the next frame, and the container then skips and replays stale children
                // while the generations have already moved on.
                let target_now = self.target.get();
                // The container must also depend on `current`: without this, a same-key value
                // change dirties only `current`, the whole subtree is replayed, and the content
                // never re-runs with the new value.
                let _c = current.get();
                // The exit completion check runs once a frame — and this is also what keeps the
                // container entering every frame while the animation runs.
                let _x = exit.get();

                if current.peek() != target_now {
                    let same_content = match &self.content_key {
                        Some(k) => {
                            let shown_now = current.peek();
                            k(&shown_now) == k(&target_now)
                        }
                        None => false,
                    };
                    if same_content {
                        // The same content: swap the value, do not animate (progress and
                        // `previous` are untouched).
                        current.set(target_now.clone());
                    } else {
                        // Swap generations at once: both are on screen, which is what makes this a
                        // crossfade.
                        previous.set(Some(current.peek().clone()));
                        current.set(target_now.clone());
                        exit.set(1.0);
                        enter.set(0.0);
                        generation.set(generation.peek().wrapping_add(1));
                    }
                }

                push_animatable(enter.clone(), 1.0, self.spec.clone());
                if previous.peek().is_some() {
                    push_animatable(exit.clone(), 0.0, self.spec.clone());
                    if exit.peek() <= 0.001 {
                        // The outgoing generation is done — it stops composing next frame
                        // (`previous.get()` is a dependency).
                        previous.set(None);
                    }
                }

                // The outgoing generation composes first (underneath); the incoming one last (on
                // top).
                let outgoing = previous.get();
                if let Some(outgoing) = outgoing {
                    let cfg = exit_cfg.clone();
                    let g = exit.clone();
                    let layer = Modifier::new().graphics_layer(move || cfg.layer_params(g.peek(), (0.0, 0.0)));
                    let gen_id = self.generation_key(&outgoing, generation.peek());
                    ctx.key((GENERATION_PREV, gen_id), |ctx| {
                        // Declare this generation's value as the slot parameter: a value change
                        // re-runs the content without needing the key to change.
                        ctx.changed(&outgoing);
                        let mkey = ctx.next_key();
                        if let GroupStatus::Enter =
                            ctx.start_restartable_group(mkey, layer, BoxLayout::new())
                        {
                            content(ctx, outgoing.clone());
                        }
                        ctx.end_restartable_group();
                    });
                }
                let shown = current.peek().clone();
                let cfg = enter_cfg.clone();
                let g = enter.clone();
                let layer = Modifier::new().graphics_layer(move || cfg.layer_params(g.peek(), (0.0, 0.0)));
                let gen_id = self.generation_key(&shown, generation.peek());
                ctx.key((GENERATION_CURRENT, gen_id), |ctx| {
                    let _c = current.get();
                    // As above: with a `content_key`, a same-key value change leaves the key alone
                    // and this is the only thing that re-runs the content.
                    ctx.changed(&shown);
                    let mkey = ctx.next_key();
                    if let GroupStatus::Enter =
                        ctx.start_restartable_group(mkey, layer, BoxLayout::new())
                    {
                        content(ctx, shown);
                    }
                    ctx.end_restartable_group();
                });
            }
        }
        ctx.end_restartable_group();
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

    /// Regression: a switch made while a cross-fade is STILL RUNNING shows the new pair at once.
    ///
    /// Each generation needs its own composition key. With a constant one the outgoing group replays
    /// its recorded slots instead of re-running the content closure with the new outgoing value —
    /// measured, one frame after switching to the third value: `[50, 60]` where `[60, 70]` belongs,
    /// settling on `[70]` only later. (The `changed` declaration alone does not cover this: it
    /// re-runs the closure, but the slots underneath are the previous generation's.)
    #[test]
    fn a_switch_during_a_transition_shows_the_new_pair_at_once() {
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
            vec![60.0, 70.0],
            "the new pair replaces the old one immediately, with no stale outgoing"
        );

        advance(&mut composer, 60);
        assert_eq!(leaf_widths(&composer), vec![70.0]);
    }
}
