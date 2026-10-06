//! `AnimatedContent` — a transition driven by a target state (mirrors Compose `AnimatedContent`)
//!
//! ```ignore
//! AnimatedContent::new(page)                        // State<Page>
//!     .enter(...) / .exit(...)                      // in/out transitions (defaults = Compose's)
//!     .size_animation(SpringSpec::bouncy().into())  // sizeTransform
//!     .build(ctx, |ctx, p| {                        // the closure gets that generation's target
//!         Text::new(format!("Page: {:?}", p)).build(ctx);
//!     });
//! ```
//!
//! Mechanism (BOTH generations on screen — Compose's `currentlyVisible`):
//! - State: `current` (the incoming generation) + `previous: Option<T>` (the outgoing one) plus
//!   three progresses: `enter` (incoming), `exit` (outgoing) and `size` (the container's size)
//! - Switch: a target change moves the old target into `previous` and the new one into `current`
//!   IMMEDIATELY — the two compose into their own container slots at once, not "fade the old one
//!   out, then swap". Each runs its own transition, overlapping in time.
//! - **Draw order**: the outgoing composes first, the incoming last, so the new content is on top
//!   (`AnimatedContent.kt:189`: "the incoming target content will be on top, as it will be placed
//!   last")
//! - **sizeTransform**: the container's size = lerp(old content size, new content size, size),
//!   driven by the INCOMING generation (Compose's rule) and using a progress and spec INDEPENDENT
//!   of the fade (Compose's `SizeTransform(sizeAnimationSpec)`, a spring by default); the layout
//!   phase re-measures every frame (a layout dep)
//! - **clip**: the container is clipped to the animated size by default (Compose's
//!   `SizeTransform(clip = true)`), so growing content does not spill out early
//! - `exit` bottoming out tears the outgoing generation down; `size` reaching 1 returns the
//!   container to "exactly the content's size". The two are independent — the default fadeOut
//!   (90 ms) is shorter than the size spring, so the exit can finish while the size still moves.
//!
//! Known differences from Compose:
//! - **No 90 ms delay.** Compose's default is `fadeIn(tween(220, delayMillis = 90))` and winia's
//!   `TweenSpec` has no delay field. The enter starts early and the total is 220 ms (Compose: 310).
//! - **One outgoing generation only.** Compose's `currentlyVisible` is a list, so a switch made
//!   mid-transition can put three on screen; `previous` is a single `Option` here and a rapid
//!   re-switch hard-cuts the middle generation.
//! - **No `contentAlignment`.** Compose's `Alignment` is 2-D (`TopStart` by default); winia's is a
//!   single axis (Start/End/Center/Stretch), so both generations sit at the container's top-left —
//!   the same as Compose's DEFAULT, just not adjustable.

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{interpolator, push_animatable, AnimationSpec, SpringSpec, TweenSpec};
use crate::layout::box_layout::BoxLayout;
use crate::layout::{MeasurePolicy, Placement};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::{Backchannel, State};
use crate::unit::{Offset, Size};
use std::time::Duration;

/// Stable key bases for the two generations (`ctx.key` scopes). Positional keys do not work: once
/// the outgoing generation leaves, the incoming one's statement key shifts up a slot and inherits
/// it.
const GENERATION_PREV: u64 = 0xA11C_0000_0000_0001;
const GENERATION_CURRENT: u64 = 0xA11C_0000_0000_0002;

/// Compose `tween()`'s default easing, `FastOutSlowInEasing` = `CubicBezierEasing(0.2, 0, 0, 1)`.
fn fast_out_slow_in() -> interpolator::CubicBezier {
    interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0)
}

/// Compose `AnimatedContent`'s default enter: `fadeIn(tween(220)) + scaleIn(0.92)`.
fn default_enter() -> VisibilityTransition {
    VisibilityTransition::fade_in(TweenSpec::new(Duration::from_millis(220), fast_out_slow_in()))
        .with_scale_from(0.92, (0.5, 0.5))
}

/// Compose's default exit: `fadeOut(tween(90))` — out fast, in slow.
fn default_exit() -> VisibilityTransition {
    VisibilityTransition::fade_out(TweenSpec::new(Duration::from_millis(90), fast_out_slow_in()))
}

/// Content switch transition (both generations on screen: one layer per enter/exit, plus
/// sizeTransform).
pub struct AnimatedContent<T> {
    target: State<T>,
    enter: VisibilityTransition,
    exit: VisibilityTransition,
    size_spec: AnimationSpec,
    clip: bool,
    modifier: Modifier,
    /// Content identity (Compose's `contentKey`): two states with the same key are the SAME
    /// content — the new state is swapped in with no transition. Without one, values are compared
    /// with `PartialEq`. The caller maps its state to a `u64`.
    content_key: Option<Box<dyn Fn(&T) -> u64 + Send + Sync>>,
}

impl<T> AnimatedContent<T> {
    /// The same key means the same content: the state changes but nothing animates (Compose's
    /// `contentKey` semantics). Only two different keys run an enter/exit.
    pub fn content_key(mut self, f: impl Fn(&T) -> u64 + Send + Sync + 'static) -> Self {
        self.content_key = Some(Box::new(f));
        self
    }
}

/// Container MeasurePolicy: size = lerp(old content size, new content size, size).
///
/// It interpolates while both generations are up, or while the size animation is still running;
/// at rest the container is exactly the content's size.
#[derive(Debug)]
struct ContentSizePolicy {
    prev_size: State<Option<(f32, f32)>>,
    /// The incoming generation's content size, to be locked into `prev_size` at switch time (the
    /// sizeTransform's starting point). A Backchannel: it is read once, at the switch, so notifying
    /// every frame would recompose the caller for nothing. (`Backchannel::set` still takes a write
    /// lock, so this is "no notification", not "free".)
    last_size: Backchannel<Option<(f32, f32)>>,
    /// The CONTAINER's size, which is what a slide resolves against — Compose's `slideIntoContainer`
    /// measures `currentSize`, the container (`AnimatedContent.kt:451`). Both generations read this
    /// one: letting the outgoing read the INCOMING generation's size pushed a 50-wide outgoing the
    /// 200 that the incoming asked for.
    container_size: Backchannel<Option<(f32, f32)>>,
    size: State<f32>,
}

impl MeasurePolicy for ContentSizePolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut measured = Vec::with_capacity(children.len());
        for &child in children {
            let (child_size, _) =
                crate::layout::node::measure_node(nodes, policies, child, constraints.loosen());
            measured.push(child_size);
        }
        // The incoming generation is the last child (composed last = drawn on top).
        let incoming = measured.last().copied().unwrap_or(Size::new(0.0, 0.0));
        self.last_size.set(Some((incoming.width, incoming.height)));
        // Read `size` at layout time (`get` registers a layout dep, so the container re-measures
        // every frame while the animation runs and its size follows the switch; `peek` would not
        // register, and the size would stick at its first-frame value).
        let p = self.size.get();
        let (w, h) = match self.prev_size.peek() {
            Some((pw, ph)) if p < 0.999 => (
                pw + (incoming.width - pw) * p,
                ph + (incoming.height - ph) * p,
            ),
            _ => (incoming.width, incoming.height),
        };
        // Both generations' layers resolve a slide against the container size (Compose's rule).
        self.container_size.set(Some((w, h)));
        let placements = children
            .iter()
            .zip(measured.iter())
            .map(|(_, s)| Placement {
                size: Size::new(s.width, s.height),
                position: Offset::new(0.0, 0.0),
            })
            .collect();
        (
            Size::new(constraints.constrain_width(w), constraints.constrain_height(h)),
            placements,
        )
    }

    fn place(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        children: &[usize],
        placements: &[Placement],
    ) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

/// Compose's `SizeTransform` default spring: `Spring.StiffnessMediumLow` = 400, no bounce
/// (`AnimatedContent.kt:217-223`). `SpringSpec::default()` is stiffness 200 (winia's own
/// `StiffnessLow`), which makes the container about 1.4x slower.
fn default_size_spec() -> AnimationSpec {
    AnimationSpec::Spring(SpringSpec {
        damping_ratio: 1.0,
        stiffness: 400.0,
        ..SpringSpec::default()
    })
}

impl<T: Clone + PartialEq + 'static> AnimatedContent<T> {
    /// Compose's default transition: `fadeIn(220) + scaleIn(0.92) togetherWith fadeOut(90)`,
    /// sizeTransform on `spring(stiffness = StiffnessMediumLow)`, container clipped to the
    /// animated size.
    pub fn new(target: State<T>) -> Self {
        Self {
            target,
            enter: default_enter(),
            exit: default_exit(),
            size_spec: default_size_spec(),
            clip: true,
            modifier: Modifier::new(),
            content_key: None,
        }
    }

    /// The enter transition — it acts on the incoming generation.
    pub fn enter(mut self, t: VisibilityTransition) -> Self {
        self.enter = t;
        self
    }

    /// The exit transition — it acts on the generation that is leaving.
    pub fn exit(mut self, t: VisibilityTransition) -> Self {
        self.exit = t;
        self
    }

    /// The sizeTransform's animation spec (Compose's `SizeTransform(sizeAnimationSpec)`).
    pub fn size_animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.size_spec = spec.into();
        self
    }

    /// Whether to clip the container to the animated size (Compose's `SizeTransform(clip)`,
    /// true by default).
    pub fn clip(mut self, v: bool) -> Self {
        self.clip = v;
        self
    }

    /// A modifier for the container itself (Compose's `modifier` parameter).
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = m;
        self
    }

    /// One generation's composition key (Compose's `key(contentKey(it))`, `AnimatedContent.kt:873`).
    ///
    /// With a `content_key` the caller's key is used, so a key that comes back REUSES its slots
    /// (A → B → A returns with A's own `remember`ed state). Without one it falls back to a counter:
    /// a fresh slot per generation. Either way it has to vary per generation — a fixed key replays
    /// the previous generation's recorded slots, so the content closure never re-runs.
    fn generation_key(&self, value: &T, counter: u64) -> u64 {
        match &self.content_key {
            Some(k) => k(value),
            None => counter,
        }
    }

    /// Build the content switch container.
    ///
    /// ⚠ Do not wrap this in a macro: the `target` / `exit` / `size` dependencies have to be read
    /// by THIS function's own scope. A macro would close them into an inner scope, the parent
    /// container would no longer see the progress move, and the advance/completion checks would
    /// freeze (the same regression `animated_visibility` and `crossfade` hit).
    ///
    /// `target.get()` is read here so the CALLER's scope subscribes to it (a target change re-runs
    /// this `build`); the per-frame advance and completion checks are read inside the container's
    /// `Enter` branch instead — see the comment there.
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx, T)) {
        // Dependency: a target change recomposes the caller's scope, so this `build` re-runs and
        // the container's slot goes dirty with it.
        let target = self.target.get();
        // Internal state (`remember` — the statement key is stable, so it survives recomposition).
        let current: State<T> = ctx.remember(|| target.clone());
        let previous: State<Option<T>> = ctx.remember(|| None);
        let enter: State<f32> = ctx.remember(|| 1.0);
        let exit: State<f32> = ctx.remember(|| 1.0);
        let size: State<f32> = ctx.remember(|| 1.0);
        let prev_size: State<Option<(f32, f32)>> = ctx.remember(|| None);
        // Every generation gets its own key: a fixed key lets a NEW generation reuse the previous
        // one's slots (once the old content leaves, the new one inherits its `remember`ed state —
        // measured as the tree still showing the old content). Compose does the same thing with
        // `key(contentKey(state))`.
        let generation: State<u64> = ctx.remember(|| 0);
        let last_size = ctx.remember_backchannel(|| None);
        let container_size = ctx.remember_backchannel(|| None);

        let container_layer = {
            let clip = self.clip;
            Modifier::new().graphics_layer(move || {
                let mut params = crate::graphics::GraphicsLayerParams::default();
                params.clip = clip;
                params
            })
        };
        let modifier = self.modifier.clone().then(container_layer);
        let policy = ContentSizePolicy {
            prev_size: prev_size.clone(),
            last_size: last_size.clone(),
            container_size: container_size.clone(),
            size: size.clone(),
        };
        let key = ctx.next_key();
        let ac_status = ctx.start_restartable_group(key, modifier, policy);
        match ac_status {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Everything the frame needs happens INSIDE this branch, and that is load-bearing:
                // a write made in the CALLER's scope (before the container starts) marks the
                // container's slot dirty during the very pass that is already consuming it, and the
                // mark is gone by the next frame — so the container skipped and replayed its
                // previous children while `previous`/`current` had already moved on. Measured: a
                // switch made mid-transition left the old pair on screen for four frames and then
                // hard-cut to the incoming generation alone. Here the container re-enters because
                // its own dependencies changed, so the swap and the children it composes happen in
                // one frame.
                let target_now = self.target.get();
                // The container must also depend on `current`. Without this, an update where the
                // `content_key` stays the same but the value changes dirties only `current`, the
                // container stays clean, the whole subtree is replayed and the content never
                // re-runs with the new value — measured: with the same key, a target moving from 0
                // to 3 left the leaf width at 50.
                let _c = current.get();
                // The completion checks run once a frame — subscribing to both progresses matters
                // because the 90 ms fadeOut and the size spring differ in length and either can
                // finish first. This is also what keeps the container entering every frame while
                // the animation runs.
                let _x = exit.get();
                let _s = size.get();

                if current.peek() != target_now {
                    // Content identity: the same key means the same content — swap the state in
                    // without animating (Compose's `contentKey`).
                    let same_content = match &self.content_key {
                        Some(k) => {
                            let shown_now = current.peek();
                            k(&shown_now) == k(&target_now)
                        }
                        None => false,
                    };
                    if same_content {
                        // The same content: only swap the value (no transition). `current.get()`
                        // subscribes below, so the content re-runs with the new value while
                        // `previous` and the three progresses stay put.
                        current.set(target_now.clone());
                    } else {
                        // Swap generations at once: the old value moves into `previous` (its own
                        // exit starts at 1 and runs down) and the new one into `current`. No
                        // waiting — both on screen at once is what makes this AnimatedContent.
                        previous.set(Some(current.peek().clone()));
                        prev_size.set(last_size.peek());
                        current.set(target_now.clone());
                        exit.set(1.0);
                        enter.set(0.0);
                        size.set(0.0);
                        generation.set(generation.peek().wrapping_add(1));
                    }
                }

                push_animatable(enter.clone(), 1.0, self.enter.spec.clone());
                if previous.peek().is_some() {
                    push_animatable(exit.clone(), 0.0, self.exit.spec.clone());
                    if exit.peek() <= 0.001 {
                        // The outgoing generation is done — it stops composing next frame
                        // (`previous.get()` is a dependency).
                        previous.set(None);
                    }
                }
                if prev_size.peek().is_some() {
                    push_animatable(size.clone(), 1.0, self.size_spec.clone());
                    if size.peek() >= 0.999 {
                        // The size animation is settling: the container goes back to "exactly the
                        // content's size" and stops interpolating next frame.
                        prev_size.set(None);
                    }
                }

                // A `previous` change (Some → None) dirties the container's slot → Enter → the
                // outgoing generation is torn down.
                let outgoing = previous.get();
                if let Some(outgoing) = outgoing {
                    let t = self.exit.clone();
                    let g = exit.clone();
                    let cs = container_size.clone();
                    let layer = Modifier::new().graphics_layer(move || {
                        t.layer_params(g.peek(), cs.peek().unwrap_or((0.0, 0.0)))
                    });
                    let gen_id = self.generation_key(&outgoing, generation.peek());
                    ctx.key((GENERATION_PREV, gen_id), |ctx| {
                        // Declare this generation's value as the child slot's parameter (Compose
                        // compares the content lambda's argument the same way): a value change
                        // re-runs the content without needing the key to change.
                        ctx.changed(&outgoing);
                        let mkey = ctx.next_key();
                        if let GroupStatus::Enter =
                            ctx.start_restartable_group(mkey, layer, BoxLayout::default())
                        {
                            content(ctx, outgoing.clone());
                        }
                        ctx.end_restartable_group();
                    });
                }
                // The incoming generation composes last → drawn over the outgoing one.
                let shown = current.peek().clone();
                let t = self.enter.clone();
                let g = enter.clone();
                let cs = container_size.clone();
                let layer = Modifier::new().graphics_layer(move || {
                    t.layer_params(g.peek(), cs.peek().unwrap_or((0.0, 0.0)))
                });
                let gen_id = self.generation_key(&shown, generation.peek());
                ctx.key((GENERATION_CURRENT, gen_id), |ctx| {
                    let _c = current.get();
                    // As above: with a `content_key`, a same-key value change leaves the key alone
                    // and this is the only thing that re-runs the content.
                    ctx.changed(&shown);
                    let mkey = ctx.next_key();
                    if let GroupStatus::Enter =
                        ctx.start_restartable_group(mkey, layer, BoxLayout::default())
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

    /// Test leaf: its width reflects the target (page=0 → 50, anything else → 200).
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

    /// Two generations on screen means the container has two children; at rest it has one.
    ///
    /// A shape heuristic, with two blind spots worth knowing before reusing it: a generation whose
    /// content composes NO node (legal — `if flag { Text(..) }` with `flag` false) leaves that
    /// wrapper childless, and a tree whose root is not the container (every demo wraps it in a
    /// `Column`) sends the `children[0]` descent past it. Both hold for the trees in this module.
    fn container_child_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize) -> usize {
            let n = &nodes[idx];
            if n.children.is_empty() {
                return 0;
            }
            // The container: 1..=2 children, each of which has children of its own (the wrapper
            // around the content).
            if n.children.len() <= 2 && n.children.iter().all(|&c| !nodes[c].children.is_empty()) {
                return n.children.len();
            }
            walk(nodes, n.children[0])
        }
        walk(nodes, root)
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

    /// Integration: a target change puts both generations on screen (old 50 fading out, new 200
    /// fading in) while the container animates from the old size to the new one.
    #[test]
    fn animated_content_switches_with_size_transform() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(crate::animation::TweenSpec::default()))
                    .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::default()))
                    .size_animation(crate::animation::TweenSpec::default())
                    .build(ctx, |ctx, page| {
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

        // First frame: target 0 is showing (container 50 wide, one generation)
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initially target 0");
        assert_eq!(container_child_count(&composer), 1, "one generation at rest");

        // Switch: both generations up, the container interpolating 50 → 200 with the size progress
        target.set(7);
        recompose(&mut composer);
        let mut saw_both = false;
        let mut mid_w = -1.0;
        for _ in 0..80 {
            advance_one(&mut composer);
            let w = leaf_widths(&composer);
            if w.contains(&50.0) && w.contains(&200.0) {
                saw_both = true;
            }
            let cw = container_width_of(&composer);
            if cw > 50.0 && cw < 200.0 {
                mid_w = cw;
                break;
            }
        }
        assert!(saw_both, "both generations are on screen during the transition");
        assert!(
            mid_w > 50.0 && mid_w < 200.0,
            "the container should be mid-way through 50..200 while the size animates (got {mid_w})"
        );

        // Advance to completion — one generation, container width = the new content's 200
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "the outgoing generation is gone");
        assert_eq!(container_width_of(&composer), 200.0, "the container ends at the new content's width");

        // Switch back to target 0
        target.set(0);
        recompose(&mut composer);
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![50.0], "switching back leaves one generation");
        assert_eq!(container_width_of(&composer), 50.0, "the container width is back to 50");
    }

    /// The container's own measured width: it is the interpolated value while two generations are
    /// up.
    fn container_width_of(composer: &Composer) -> f32 {
        let Some(root) = composer.layout_root_idx() else { return -1.0 };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize) -> f32 {
            let n = &nodes[idx];
            if n.children.is_empty() {
                return -1.0;
            }
            if n.children.len() <= 2 && n.children.iter().all(|&c| !nodes[c].children.is_empty()) {
                return n.measured_size.width;
            }
            walk(nodes, n.children[0])
        }
        walk(nodes, root)
    }

    /// sizeTransform is independent (regression: sharing the progress with the fade made
    /// `same_target` dedupe skip it, so `size_spec` never took effect). `size_animation` is Snap
    /// here, so the container reaches the new width the moment of the switch while the fade is
    /// still running.
    #[test]
    fn size_animation_independent_of_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let slow = || {
            VisibilityTransition::fade_in(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(600),
                crate::animation::interpolator::Linear::new(),
            ))
        };

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(slow())
                    .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(crate::animation::AnimationSpec::Snap)
                    .build(ctx, |ctx, page| {
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
        assert_eq!(leaf_widths(&composer), vec![50.0], "initially target 0");

        target.set(7);
        recompose(&mut composer);
        for _ in 0..20 {
            advance_one(&mut composer);
        }
        // The fade is only ~320ms into its 600ms — with an independent size (Snap) the container
        // should already be 200; sharing the progress (the old bug) would leave it mid-interpolation.
        let w = container_width_of(&composer);
        assert!(
            (w - 200.0).abs() < 0.5,
            "sizeTransform(Snap) reaches 200 mid-fade (got {w})"
        );
    }

    /// Compose's `AnimatedContent` keeps the OUTGOING generation composed while the incoming one
    /// enters (`currentlyVisible`) and draws the incoming on top (`AnimatedContent.kt:189`). The
    /// old implementation faded the outgoing to nothing, swapped, then faded the incoming in — one
    /// generation at a time.
    #[test]
    fn both_generations_are_composed_during_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone()).build(ctx, |ctx, page| {
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
        assert_eq!(container_child_count(&composer), 1);

        target.set(7);
        recompose(&mut composer);
        let mut both = false;
        for _ in 0..40 {
            advance_one(&mut composer);
            let widths = leaf_widths(&composer);
            if widths.contains(&50.0) && widths.contains(&200.0) {
                both = true;
                assert_eq!(container_child_count(&composer), 2, "two children: outgoing + incoming");
                break;
            }
        }
        assert!(both, "both generations are on screen mid-transition");

        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "the outgoing is removed");
        assert_eq!(container_child_count(&composer), 1);
    }

    /// `contentKey`: two states with the same key are the same content — the value is swapped in
    /// without a transition (Compose's `contentKey`), so no second generation appears.
    #[test]
    fn the_same_content_key_updates_in_place_without_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    // 0..=9 is one piece of content (two renders of the same page); 10 is a new one.
                    .content_key(|p| if *p < 10 { 0 } else { 1 })
                    .build(ctx, |ctx, page| {
                        // The width must follow the VALUE, not the key: with a key-derived width this
                        // test could not tell "the content re-ran with the new value" from "the content
                        // never re-ran" — both give the same leaf. Review finding.
                        let w = 50.0 + page as f32 * 10.0;
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initial");

        // Same key: the value is swapped and the content re-runs with it (the width 80 is the
        // proof it re-ran), but no second generation enters
        target.set(3);
        recompose(&mut composer);
        assert_eq!(container_child_count(&composer), 1, "no transition for the same key");
        assert_eq!(leaf_widths(&composer), vec![80.0], "the content re-ran with the new value");

        // A new key: a normal transition with both generations up
        target.set(10);
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![80.0, 150.0], "a new key animates");
    }


    /// Regression: a switch made while a transition is STILL RUNNING shows the new pair at once.
    ///
    /// Each generation needs its own composition key. With a fixed key the outgoing group replayed
    /// its recorded slots instead of re-running the content closure with the new outgoing value —
    /// measured: the tree kept showing `[50, 60]` for four frames after switching to the third
    /// value, then jumped to `[70]` alone (the incoming generation, with no outgoing). The same
    /// fixed key would also hand one generation's `remember`ed state to the next.
    #[test]
    fn a_switch_during_a_transition_shows_the_new_pair_at_once() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone()).build(ctx, |ctx, page| {
                    SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
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
        target.set(1);
        recompose(&mut composer);
        advance_one(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0, 60.0], "first transition is running");

        // …switch again while that one is in flight
        target.set(2);
        recompose(&mut composer);
        advance_one(&mut composer);
        assert_eq!(
            leaf_widths(&composer),
            vec![60.0, 70.0],
            "the new pair replaces the old one immediately, with no stale outgoing"
        );

        // …and the container catches up to the final generation
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![70.0]);
    }
}
