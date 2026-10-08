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
//! Mechanism (EVERY state on screen keeps its own alpha — Compose's `currentlyVisible`):
//! - State: a `VisibleSet` of the states on screen, one alpha per entry, plus one `size` progress for
//!   the container. Each entry fades toward 1 if it is the target and 0 otherwise, with the enter
//!   transition while rising and the exit transition while leaving.
//! - Switch: a new target becomes a new entry IMMEDIATELY, so the outgoing states keep leaving
//!   alongside it rather than being swapped out; a target that is already on screen is replaced IN
//!   PLACE, which keeps its alpha and its slots, so it reverses instead of restarting.
//! - **Draw order**: the outgoing composes first, the incoming last, so the new content is on top
//!   (`AnimatedContent.kt:189`: "the incoming target content will be on top, as it will be placed
//!   last")
//! - **sizeTransform**: the container's size = lerp(old content size, new content size, size),
//!   driven by the INCOMING generation (Compose's rule) and using a progress and spec INDEPENDENT
//!   of the fade (Compose's `SizeTransform(sizeAnimationSpec)`, a spring by default); the layout
//!   phase re-measures every frame (a layout dep)
//! - **clip**: the container is clipped to the animated size by default (Compose's
//!   `SizeTransform(clip = true)`), so growing content does not spill out early
//! - An entry whose alpha bottoms out is dropped; `size` reaching 1 returns the container to
//!   "exactly the content's size". The two are independent — the default fadeOut (90 ms) is shorter
//!   than the size spring, so an exit can finish while the size still moves.
//!
//! No differences from Compose are recorded here any more: the fade defaults (including the 90 ms
//! delay), the two-generation behaviour, the composition keys, `contentAlignment` and the size
//! transform are all in place. What remains winia's own is the engine underneath, not this API.

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{interpolator, push_animatable, AnimationSpec, SpringSpec, TweenSpec};
use crate::layout::box_layout::BoxLayout;
use crate::layout::node::ContentAlignment;
use crate::layout::{MeasurePolicy, Placement};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::{Backchannel, State};
use crate::unit::{Offset, Size};
use std::time::Duration;

/// Stable key bases for the two generations (`ctx.key` scopes). Positional keys do not work: once
/// the outgoing generation leaves, the incoming one's statement key shifts up a slot and inherits
/// it.
const GENERATION_ENTRY: u64 = 0xA11C_0000_0000_0001;

/// Compose `tween()`'s default easing, `FastOutSlowInEasing` = `CubicBezierEasing(0.2, 0, 0, 1)`.
fn fast_out_slow_in() -> interpolator::CubicBezier {
    interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0)
}

/// Compose `AnimatedContent`'s default enter: `fadeIn(tween(220, delayMillis = 90)) +
/// scaleIn(0.92, tween(220, delayMillis = 90))` — the incoming content waits out the outgoing one's
/// 90 ms fade before it starts, which is the staggered default the component's docs describe.
fn default_enter() -> VisibilityTransition {
    VisibilityTransition::fade_in(
        TweenSpec::new(Duration::from_millis(220), fast_out_slow_in())
            .delay(Duration::from_millis(90)),
    )
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
    /// Where each generation sits inside the container — Compose's `contentAlignment`, 2-D and
    /// `TopStart` by default (`AnimatedContent.kt:137`).
    content_alignment: ContentAlignment,
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
    /// Compose's `contentAlignment` — applied to every generation, resolved against the animated
    /// container size (`AnimatedContent.kt:692-694`).
    content_alignment: ContentAlignment,
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
        // Each generation is aligned inside the container the frame ends up with, the way Compose
        // resolves `contentAlignment.align(placeable, measuredSize)` (`AnimatedContent.kt:692-694`).
        let space = Size::new(w, h);
        let placements = children
            .iter()
            .zip(measured.iter())
            .map(|(_, s)| {
                let child = Size::new(s.width, s.height);
                let (x, y) = self.content_alignment.anchor(child, space);
                Placement {
                    size: self.content_alignment.child_size(child, space),
                    position: Offset::new(x, y),
                }
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
            content_alignment: ContentAlignment::TOP_START,
        }
    }

    /// Where each generation sits inside the container. Compose's default is `TopStart`, which is
    /// what this used to hard-code; the two generations overlap while the transition runs, so a
    /// `Center` is what puts a small page in the middle of a larger one.
    pub fn content_alignment(mut self, a: ContentAlignment) -> Self {
        self.content_alignment = a;
        self
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

    /// Whether an entry's value counts as the same content as the target — Compose's `contentKey`
    /// comparison, which is value equality when the caller supplied no key.
    fn matches(&self, a: &T, b: &T) -> bool {
        match &self.content_key {
            Some(k) => k(a) == k(b),
            None => a == b,
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
        // Every state on screen, plus one alpha per entry — Compose's `currentlyVisible` and the
        // `animateFloat` it gives each entry (`AnimatedContent.kt:186-189`), in one `remember`ed
        // value. Indexed by entry rather than position, so an entry's alpha survives the switches of
        // the entries around it.
        let core = ctx.remember_backchannel(|| VisibleSet::new(target.clone()));
        // The container's size animation runs once per switch, not once per entry: it lerps from the
        // size the previous target had to the size the new one measures.
        let size: State<f32> = ctx.remember(|| 1.0);
        let prev_size: State<Option<(f32, f32)>> = ctx.remember(|| None);
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
            content_alignment: self.content_alignment,
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
                // Subscribe to the entries and to every entry's alpha. The second half is what keeps
                // the container entering each frame while a fade runs — where the next frame's
                // pushes and the drop rule live. Without it the composition is skipped after the
                // first frame and nothing ever leaves (measured in `crossfade.rs`, same shape).
                let mut set = core.peek();
                for entry in set.entries.iter() {
                    let _ = set.alpha(entry.era).get();
                }
                let _s = size.get();

                // ── The target is not on screen yet, or is on screen already ──
                // Compose asks two different questions here: whether the target VALUE is the state
                // currently shown, and whether any visible state shares its `contentKey`. A value
                // that is new but matches a visible state's key REPLACES that entry in place, which
                // is what makes a state that comes back keep its alpha (and its slots) and reverse
                // instead of restarting.
                let newest_is_the_target = set
                    .entries
                    .last()
                    .map(|e| e.value == target_now)
                    .unwrap_or(false);
                if !newest_is_the_target {
                    match set.position_by_key(&target_now, &self.content_key) {
                        Some(i) => {
                            // In place: era, alpha and slots all travel with the entry. A
                            // `content_key` match therefore updates the content with no animation at
                            // all, which is the point of the key. It deliberately does NOT move to
                            // the end — winia matches a child's key at its POSITION (or by its
                            // path-hash prefix there), so a reorder truncates from that index and
                            // rebuilds both subtrees (measured in a minimal probe; `crossfade.rs`
                            // carries the same note).
                            set.entries[i].value = target_now.clone();
                        }
                        None => {
                            // A new entry starts at 0 and the container's size animation starts
                            // from the size the previous target measured.
                            prev_size.set(last_size.peek());
                            size.set(0.0);
                            set.push(target_now.clone());
                        }
                    }
                    core.set(set.clone());
                }

                // ── One alpha per entry, with the direction's own transition ──
                for entry in set.entries.iter() {
                    let rising = self.matches(&entry.value, &target_now);
                    let spec = if rising { self.enter.spec.clone() } else { self.exit.spec.clone() };
                    push_animatable(set.alpha(entry.era), if rising { 1.0 } else { 0.0 }, spec);
                }

                // ── Drop what has finished leaving. The target never drops: a brand new entry
                // starts at 0 and has to be allowed to rise ──
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

                // ── The container's size, once per switch ──
                if prev_size.peek().is_some() {
                    push_animatable(size.clone(), 1.0, self.size_spec.clone());
                    if size.peek() >= 0.999 {
                        // The size animation is settling: the container goes back to "exactly the
                        // content's size" and stops interpolating next frame.
                        prev_size.set(None);
                    }
                }

                if std::env::var("AC_TRACE2").is_ok() {
                    let dump: Vec<(u64, f32)> =
                        set.entries.iter().map(|e| (e.era, set.alpha(e.era).peek())).collect();
                    eprintln!("ACTRACE entries(era,alpha)={:?}", dump);
                }
                // ── Compose what is left, oldest first so the newest is drawn on top ──
                for entry in set.entries.iter() {
                    let rising = self.matches(&entry.value, &target_now);
                    let cfg = if rising { self.enter.clone() } else { self.exit.clone() };
                    let g = set.alpha(entry.era);
                    let cs = container_size.clone();
                    let layer = Modifier::new().graphics_layer(move || {
                        cfg.layer_params(g.peek(), cs.peek().unwrap_or((0.0, 0.0)))
                    });
                    let value = entry.value.clone();
                    let gen_id = self.generation_key(&value, entry.era);
                    ctx.key((GENERATION_ENTRY, gen_id), |ctx| {
                        // Declare this entry's value as the slot parameter (Compose compares the
                        // content lambda's argument the same way): a value change re-runs the content
                        // without needing the key to change.
                        ctx.changed(&value);
                        let mkey = ctx.next_key();
                        if let GroupStatus::Enter =
                            ctx.start_restartable_group(mkey, layer, BoxLayout::default())
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

    /// Every leaf's placement, as (x, width) — the alignment test needs WHERE a generation sits, not
    /// just how wide it is.
    fn leaf_placements(composer: &Composer) -> Vec<(f32, f32)> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize, x: f32, out: &mut Vec<(f32, f32)>) {
            let here = x + nodes[idx].position.x;
            if nodes[idx].children.is_empty() {
                out.push((here, nodes[idx].measured_size.width));
                return;
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, here, out);
            }
        }
        let mut out = Vec::new();
        walk(nodes, root, 0.0, &mut out);
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        out
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


    /// `contentAlignment` puts each generation where Compose puts it: the child is aligned inside the
    /// container, not at its origin (`AnimatedContent.kt:692-694`).
    ///
    /// The case that shows it is a small page in a container that is still the large page's size —
    /// which is what the size transform spends its time doing. The size spec is slow ON PURPOSE: with
    /// `Snap` the container is already the new size on the frame of the switch, so the child and the
    /// box agree and a broken alignment looks correct (measured — that version of this test passed
    /// with the alignment removed).
    #[test]
    fn animated_content_aligns_each_generation_inside_the_container() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .content_alignment(ContentAlignment::CENTER)
                    .size_animation(AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(5000),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 200.0 } else { 60.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        // The switch: the small page arrives while the container is still the large one's size.
        target.set(1);
        recompose(&mut composer);
        let box_w = container_width_of(&composer);
        let leaves = leaf_placements(&composer);
        let small = leaves
            .iter()
            .find(|(_, w)| (*w - 60.0).abs() < 0.01)
            .map(|(x, w)| (*x, *w))
            .expect("the small generation is on screen");
        assert!(
            (small.0 - (box_w - small.1) / 2.0).abs() < 0.51,
            "the child is centred in the container, not at its origin (x={}, box_w={box_w})",
            small.0
        );
    }

    /// A switch made while a transition is STILL RUNNING keeps the state it interrupted, so three
    /// states are on screen at once.
    ///
    /// Compose's `currentlyVisible` is a list and only empties when the transition settles, so the
    /// state that was leaving, the one that was arriving and the new target are all composed
    /// together. The implementation this replaced held ONE outgoing generation: when the third
    /// target arrived it overwrote the first outgoing state, dropping a state that was still
    /// visible. The specs here widen that window (a 600 ms exit against an 80 ms enter) so the
    /// difference is not a matter of sampling luck.
    #[test]
    fn a_switch_during_a_transition_keeps_the_state_it_interrupted() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(TweenSpec::new(
                        std::time::Duration::from_millis(80),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .exit(VisibilityTransition::fade_out(TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(AnimationSpec::Snap)
                    .build(ctx, |ctx, page| {
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
        advance(&mut composer, 10);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 60.0],
            "the second state is in while the first is still leaving"
        );

        // …switch to a third while both are still on screen.
        target.set(2);
        recompose(&mut composer);
        advance(&mut composer, 1);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 60.0, 70.0],
            "the target is on screen at once and neither state it interrupted was dropped"
        );

        advance(&mut composer, 90);
        assert_eq!(leaf_widths(&composer), vec![70.0], "and it settles on the target alone");
    }

    /// How many content slots have ever been created, to tell "the entry kept its slot" from "the
    /// entry was thrown away and rebuilt".
    static SLOT_MARKERS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// Compose replaces an entry that is still on screen IN PLACE, so a state that comes back keeps
    /// its own alpha AND its own slots — it reverses instead of restarting from nothing. The width
    /// here encodes which slot the content lives in (`remember` runs once per slot), which is what
    /// makes the difference visible: the single-outgoing implementation dropped the leaving entry
    /// and rebuilt the returning one, and that shows up as a third slot.
    #[test]
    fn a_state_that_comes_back_reuses_its_own_slot() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        SLOT_MARKERS.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(TweenSpec::new(
                        std::time::Duration::from_millis(80),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .exit(VisibilityTransition::fade_out(TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(AnimationSpec::Snap)
                    .build(ctx, |ctx, _page| {
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
        advance(&mut composer, 6);
        assert_eq!(leaf_widths(&composer), vec![10.0, 110.0], "the second state gets its own slot");

        // …back to 0 while 1 is still leaving: 0 is on screen, so it is replaced in place.
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

        advance(&mut composer, 90);
        assert_eq!(leaf_widths(&composer), vec![10.0], "and it settles on the state that came back");
    }

    /// A transition leaves nothing running behind it: once only the target is on screen — which also
    /// means the container's size animation has settled — the engine is idle again.
    #[test]
    fn nothing_is_still_animating_once_a_switch_settles() {
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

        recompose(&mut composer);
        target.set(1);
        recompose(&mut composer);
        let mut settled = false;
        for _ in 0..120 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
            if !crate::animation::is_animating() && leaf_widths(&composer) == vec![200.0] {
                settled = true;
                break;
            }
        }
        assert!(settled, "the transition finishes and the engine goes idle");
        assert!(!crate::animation::is_animating(), "nothing is left animating");
    }
}
