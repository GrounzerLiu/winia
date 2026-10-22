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
use crate::layout::node::{ContentAlignment, LayoutDirection};
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

/// Compose `AnimatedContent`'s default enter: `fadeIn(tween(220, delayMillis = 90)) +
/// scaleIn(0.92, tween(220, delayMillis = 90))` — the incoming content waits out the outgoing one's
/// 90 ms fade before it starts, which is the staggered default the component's docs describe.
fn default_enter() -> VisibilityTransition {
    VisibilityTransition::fade_in(
        TweenSpec::new(Duration::from_millis(220), interpolator::fast_out_slow_in())
            .delay(Duration::from_millis(90)),
    )
    .with_scale_from(0.92, (0.5, 0.5))
}

/// Compose's default exit: `fadeOut(tween(90))` — out fast, in slow.
fn default_exit() -> VisibilityTransition {
    VisibilityTransition::fade_out(TweenSpec::new(Duration::from_millis(90), interpolator::fast_out_slow_in()))
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
    /// The direction its horizontal half mirrors under.
    direction: LayoutDirection,
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
                let (x, y) = self.content_alignment.anchor(child, space, self.direction);
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
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        let policy = ContentSizePolicy {
            content_alignment: self.content_alignment,
            direction,
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
mod tests;
