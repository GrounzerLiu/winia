//! `AnimatedVisibility` — content appearing and disappearing (Compose's `AnimatedVisibility`).
//!
//! ```ignore
//! AnimatedVisibility::new(visible)
//!     .enter(VisibilityTransition::fade_in(TweenSpec::default()).with_expand())
//!     .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
//!     .build_with_scope(ctx, |scope, ctx| {
//!         // `scope.value()` is the transition's progress: 0 hidden, 1 shown.
//!         Text::new("content").build(ctx);
//!     });
//! ```
//!
//! Mechanism (the real layout path — no `force_remeasure` side channel):
//! - **One `State<f32>` of progress**: `visible = true` animates 0→1 (enter), `false` animates 1→0
//!   (exit); the engine de-duplicates a repeated push at the same target. Its FIRST value is the
//!   initial state, never an animation away from it — see `build_inner`.
//! - **Layout**: `VisibilityPolicy` reads the progress with `get()` while measuring, which registers a
//!   layout dependency, so expand/shrink scales the container by it (followers move along smoothly) and
//!   an animating frame only re-measures, never re-composes.
//! - **Paint**: a dynamic `graphics_layer` closure reads the progress with `peek()` at render time —
//!   fade (alpha), slide (translation), scale — registering no dependency at all.
//! - **Delayed removal**: with `visible = false` the content stays composed for the whole exit
//!   (progress 1→0); once it is at ≈0 and hidden, `removed` is set and the next build does not start the
//!   container, so the slot is freed and the content disappears.
//!
//! Aligned with Compose: the default enter/exit pair (fade + size change, `spring(1.0, 400)`), no
//! transition for the initial state, PER-CHANNEL animation specs (`fade_in(spec) + expand_in(other)` —
//! each channel is its own animatable, so they can run out of step), only the channels a transition
//! actually enables being driven (Compose builds `TransitionData` with the rest `null`; the removal
//! waits for those channels and no others), the progress being readable from the content
//! (`VisibilityScope` ≈ `AnimatedVisibilityScope.transition`), delayed removal, and the full builder
//! vocabulary (fade/slide/expand/shrink/scale, both axes, anchors, offsets).
//!
//! A combination the callers control, and Compose behaves the same way: a transition whose SIZE channel
//! finishes long before its fade clips the content once the container is small (`shrinkOut`'s
//! `clip = true` default, `EnterExitTransition.kt:569-585`) — `fade_out(tween(300, delayMillis = 400)) +
//! shrink_out(tween(10))` hides the content at ~40 ms and keeps the (now empty) container until the fade
//! lands. That is the combination's own doing, not a defect in the removal rule.
//!
//! Deliberate differences (all with their Compose counterpart):
//! - **One shared `spec` remains the fallback.** A channel whose builder did not give it a spec runs on
//!   the transition's shared one, which is what `fade_in(tween).with_expand()` relies on; Compose's
//!   builders always carry one. Note the interaction with `+`: a channel added AFTER the combination
//!   (`fade_in(tween600) + expand_in(spring)`, then `.with_slide(..)`) resolves to the shared spec, not
//!   to any builder's — `fade_in` set the shared one, so the slide borrows its 600 ms. Combine the
//!   builders instead (`fade_in(tween600) + slide_in(dir, other)`) when the channels need different
//!   timing; `plus` keeps each side's own channel specs.
//! - **One default, both axes.** Compose picks the default axis from the scope — `RowScope` expands
//!   horizontally, `ColumnScope` vertically, the top level expands both (`AnimatedVisibility.kt:125-129`,
//!   `:199-203`, `:272-276`) — while winia has one default (both axes) plus the explicit
//!   `expand_in_h`/`with_expand_h` builders for the horizontal case. The default ANCHORS differ too:
//!   Compose's top-level `expandIn`/`shrinkOut` use `Alignment.BottomEnd` (`EnterExitTransition.kt:529-535`,
//!   `:569-575`), where winia's default is `ExpandFrom::Top`/`ExpandFromH::Start` — Compose's
//!   `ColumnScope`/`RowScope` shape, which is what a plain `AnimatedVisibility` in a column reads as.
//! - **No `MutableTransitionState`.** Winia takes a `State<bool>`, and the scope's progress covers what
//!   `isIdle`/`currentState` are read for. Compose also allows an initial state different from the first
//!   target through that type (`Transition.kt:796`); winia's first composition is always the initial
//!   state.
//! - **No `label`** (Android Studio animation preview only) and no `Transition<T>.AnimatedVisibility`
//!   overload (winia has no public `Transition` object).
//! - **`unveilIn`/`veilOut` are not implemented.** They are NOT clip reveals, which the first draft of
//!   this note claimed: they animate a SCRIM over the content — a black 50 %-alpha veil whose alpha
//!   rides the transition (`EnterExitTransition.kt:464-480`, `:491-508`, `@ExperimentalAnimationApi`) —
//!   and winia has no per-node scrim draw channel. Everything else in that file's vocabulary is here.
//! - `Modifier.animateEnterExit` has no counterpart; `build_with_scope` gives the content the same
//!   progress a modifier would need, and winia's content applies it through its own modifier chain.

use crate::animation::push_animatable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};
use crate::modifier::{Modifier};
pub use crate::animation::visibility::{
    ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};

/// The visibility container: its content appears and disappears smoothly with the `visible` State.
pub struct AnimatedVisibility {
    visible: State<bool>,
    enter: VisibilityTransition,
    exit: VisibilityTransition,
    /// Start from the hidden end and animate in even when `visible` is already true on the first
    /// composition — Compose's `MutableTransitionState` pattern (see `animate_on_appear`).
    animate_on_appear: bool,
}

/// What `AnimatedVisibility` hands its content — Compose's `AnimatedVisibilityScope`, whose
/// `transition: Transition<EnterExitState>` exists so custom animations can ride the same clock the
/// container does ("Custom enter/exit animations can be created using the `Transition<EnterExitState>`
/// object from the `AnimatedVisibilityScope`", `AnimatedVisibility.kt:160-163`).
///
/// winia has no public `Transition` object, so the scope carries the progress itself: read it with
/// `value()` inside a composition (the read re-runs that composition while the transition moves) or with
/// `peek()` at render/measure time (a graphics layer, a placement — no recomposition).
///
/// Compose's `MutableTransitionState.isIdle` ("all the animations have finished") and `currentState`
/// ("the initial state of the current animations") have no direct counterpart: the container's own
/// removal already waits for the exit to finish, and `peek() == 1.0`/`== 0.0` says the same thing when a
/// caller needs to chain an action.
#[derive(Clone)]
pub struct VisibilityScope {
    /// The SIZE channel — the clock the container's layout is sized by (0 = collapsed, 1 = full). A
    /// spring overshoots past 1, exactly like the container it drives.
    pub progress: State<f32>,
    /// The fade channel (Compose's `Fade.alpha`).
    pub alpha: State<f32>,
    /// The slide channel (Compose's `Slide.slideOffset` progress).
    pub slide: State<f32>,
    /// The scale channel (Compose's `Scale.scale` progress).
    pub scale: State<f32>,
    /// The visibility this transition is playing towards — Compose's `Transition<Boolean>.targetState`,
    /// which a child's own enter/exit follows.
    pub visible: State<bool>,
}

impl VisibilityScope {
    /// The SIZE channel right now, as a composition dependency.
    pub fn value(&self) -> f32 {
        self.progress.get()
    }

    /// The same value without registering a dependency — for render-time closures.
    pub fn peek(&self) -> f32 {
        self.progress.peek()
    }

    /// All three draw channels at once, for a render-time consumer that wants to apply the same
    /// conversion the container does (`VisibilityTransition::layer_params`).
    pub fn channels(&self) -> crate::animation::visibility::ChannelValues {
        crate::animation::visibility::ChannelValues {
            fade: self.alpha.peek(),
            slide: self.slide.peek(),
            scale: self.scale.peek(),
        }
    }

    /// Compose's `Modifier.animateEnterExit(enter, exit)`: a CHILD inside this `AnimatedVisibility` runs
    /// its own enter and exit on the same visibility target, so it can fade, slide or scale on a curve of
    /// its own instead of travelling with the container. Each child gets its own animatables — the
    /// container's clock is not shared — with the specs its own builders set, so a child can fade late or
    /// slide further than the box it sits in.
    ///
    /// `extent` is what a fraction slide offset travels over. Compose reads the child's measured size;
    /// a paint-only modifier cannot see it, so the caller passes it (the container's content extent is the
    /// usual answer).
    ///
    /// Deviation recorded: a size channel on a CHILD (`expand_in`/`shrink_out`) would need a layout pass,
    /// which this modifier does not run — those flags are ignored on a child, and the container's own size
    /// channel stays the only one that moves layout.
    pub fn animate_enter_exit(
        &self,
        ctx: &mut ComposeCtx,
        enter: VisibilityTransition,
        exit: VisibilityTransition,
        extent: (f32, f32),
    ) -> Modifier {
        let vis = self.visible.get();
        // The child is born on the frame the container first composes its content — which is the frame the
        // visibility turned on — so its channels start at the enter's beginning. (Remembered per call site:
        // a child that stays composed keeps its own value; a child added later plays its enter too.)
        let initial = || 0.0;
        let alpha: State<f32> = ctx.remember(initial);
        let slide_ch: State<f32> = ctx.remember(initial);
        let scale_ch: State<f32> = ctx.remember(initial);
        // Subscribe to every channel this call drives: an animation notifies, but only a read re-runs the
        // child's build — without this the channels would stop being pushed after the first frame.
        let _moving = (alpha.get(), slide_ch.get(), scale_ch.get());
        let target = if vis { 1.0 } else { 0.0 };
        let cfg = if vis { &enter } else { &exit };
        if cfg.fade {
            crate::animation::push_animatable(alpha.clone(), target, cfg.fade_spec().clone());
        }
        if cfg.slide.is_some() {
            crate::animation::push_animatable(slide_ch.clone(), target, cfg.slide_spec().clone());
        }
        if cfg.scale {
            crate::animation::push_animatable(scale_ch.clone(), target, cfg.scale_spec().clone());
        }
        let (a, sl, sc) = (alpha.clone(), slide_ch.clone(), scale_ch.clone());
        let (e, x, v) = (enter, exit, self.visible.clone());
        Modifier::new().graphics_layer(move || {
            let cfg = if v.peek() { &e } else { &x };
            cfg.layer_params(
                crate::animation::visibility::ChannelValues {
                    fade: a.peek(),
                    slide: sl.peek(),
                    scale: sc.peek(),
                },
                extent,
            )
        })
    }
}

impl AnimatedVisibility {
    pub fn new(visible: State<bool>) -> Self {
        Self {
            visible,
            enter: VisibilityTransition::default(),
            exit: VisibilityTransition::default(),
            animate_on_appear: false,
        }
    }

    /// Play the enter transition on the FIRST composition even though `visible` is already true.
    ///
    /// Compose's `MutableTransitionState(false)` with `targetState = true` (the `InitialStateSample` its
    /// `rememberTransition` KDoc points at, `Transition.kt:796-800`, `:864-868`) is the way to animate
    /// content in as it first appears; a plain `MutableTransitionState(true)` — or winia's default —
    /// starts at the target with no animation at all. winia takes one `State<bool>` instead of a
    /// transition-state object, so the initial state is opted into here.
    pub fn animate_on_appear(mut self) -> Self {
        self.animate_on_appear = true;
        self
    }

    /// The enter transition (played when `visible` turns true).
    pub fn enter(mut self, t: VisibilityTransition) -> Self {
        self.enter = t;
        self
    }

    /// The exit transition (played when `visible` turns false — the content stays composed while it runs).
    pub fn exit(mut self, t: VisibilityTransition) -> Self {
        self.exit = t;
        self
    }

    /// Builds the animated container and hands the `VisibilityScope` to the content — Compose's
    /// `content: @Composable AnimatedVisibilityScope.() -> Unit`.
    pub fn build_with_scope(
        self,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&VisibilityScope, &mut ComposeCtx),
    ) {
        self.build_inner(ctx, content)
    }

    /// Builds the animated container for content that does not read the progress (Compose's equivalent
    /// is simply ignoring the scope).
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        self.build_inner(ctx, move |_scope, ctx| content(ctx))
    }

    fn build_inner(
        self,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&VisibilityScope, &mut ComposeCtx),
    ) {
        let visible = self.visible;
        let enter = self.enter;
        let exit = self.exit;
        // Dependency registration (`get`, not `peek`): `visible` changing re-runs this build (that is
        // what makes a button click take effect), and `progress`/`removed` changing re-runs it every
        // animated frame (so the removal check runs and the removal itself lands). Re-running this build
        // is O(1) while the transition plays — the container slot has no dependency of its own, so it
        // Skips and the content is not re-run.
        let vis = visible.get();
        // Internal state (`remember` — statement-level keys are stable, so it survives recomposition).
        // The FIRST value is the initial state, not an animation away from it: Compose's
        // `AnimatedVisibility(visible)` builds `updateTransition(visible)`, whose initial state is that
        // first target — `Transition.kt:796` contrasts it with `rememberTransition(MutableTransitionState)`,
        // "this function supports a different initial state than the first targetState", i.e. only THAT
        // variant can animate out of a different initial state. So a component composed already visible
        // shows at full size and runs no enter (measured before this: the container's height was 3.5e-7
        // against a 50-unit content) — unless the caller opted into that initial state with
        // `animate_on_appear()`, which is Compose's `MutableTransitionState(false) + targetState = true`.
        // One animated value PER CHANNEL, as Compose's `Transition<EnterExitState>` has: `Fade.alpha`,
        // `ChangeSize`, `Slide.slideOffset` and `Scale.scale` each carry their own `animationSpec`
        // (`EnterExitTransition.kt:304-306`, `:529-535`, `:409-415`), so channels can run out of step —
        // `fade_in(delayed) + expand_in(spring)` fades late while the box already grows. The SIZE channel
        // is the one the container's layout reads, and it is the scope's `progress`.
        let initial = || if vis && !self.animate_on_appear { 1.0 } else { 0.0 };
        let size: State<f32> = ctx.remember(initial);
        let alpha: State<f32> = ctx.remember(initial);
        let slide_ch: State<f32> = ctx.remember(initial);
        let scale_ch: State<f32> = ctx.remember(initial);
        let removed: State<bool> = ctx.remember(|| !vis);
        // Subscribe to EVERY channel: an animation notifies its state each frame, but only a READ creates
        // the dependency that makes this build run again — and the removal check below has to run while
        // the transition moves. Reading just the size channel stops working the moment a transition does
        // not change size (measured: a fade-only exit then never removed its content).
        let _moving = (size.get(), alpha.get(), slide_ch.get(), scale_ch.get());
        let _r = removed.get(); // the removal flag re-runs it, so the `return` below takes effect
        // Removed and shown again → reset (the notify requests the recomposition, this frame starts).
        if removed.peek() && vis {
            removed.set(false);
        }
        // Which channels the transition in play actually enables. Compose creates an animatable only for
        // the channels its builders turned on (`EnterTransitionImpl(TransitionData(fade = …, changeSize =
        // …, …))` — a `None` field gets no animation at all), and so does this: driving a channel nothing
        // consumes cost a registered animation per frame and, worse, held the removal open until a spec
        // for another channel landed (measured before this: `VisibilityTransition::empty()` on both sides
        // — Compose's `EnterTransition.None` + `ExitTransition.None` — delayed the removal by ~320 ms).
        let target = if vis { 1.0 } else { 0.0 };
        let cfg_pair = if vis { &enter } else { &exit };
        let drive_size = cfg_pair.expand || cfg_pair.expand_h;
        let drive_fade = cfg_pair.fade;
        let drive_slide = cfg_pair.slide.is_some();
        let drive_scale = cfg_pair.scale;
        // Exit-complete detection: hidden, nothing left running, and the channels that are actually
        // playing back at the hidden end — the container is gone (this build runs every frame: the
        // animating values notify each frame → pending → composed on the next one). Every ENABLED channel
        // has to be settled, not just the size one: a delayed fade still has to finish before the content
        // may be dropped, and a channel nobody enabled must not gate anything.
        let still_animating = [
            (drive_size, &size),
            (drive_fade, &alpha),
            (drive_slide, &slide_ch),
            (drive_scale, &scale_ch),
        ]
        .iter()
        .any(|(on, s)| *on && crate::animation::has_animation_for_state(s.state_id()));
        let settled = (!drive_size || size.peek() < 0.001)
            && (!drive_fade || alpha.peek() < 0.001)
            && (!drive_slide || slide_ch.peek() < 0.001)
            && (!drive_scale || scale_ch.peek() < 0.001);
        if !vis && settled && !still_animating && !removed.peek() {
            removed.set(true);
        }
        if removed.peek() {
            return; // removed: do not start the container → the slot stays unvisited → materialization frees it
        }
        // Drive the enabled channels only (push on visible-change frames — same-target dedup skips). The
        // spec comes from the transition playing NOW, per channel; a channel whose builder did not set one
        // falls back to the transition's shared spec.
        if drive_size {
            push_animatable(size.clone(), target, cfg_pair.size_spec().clone());
        }
        if drive_fade {
            push_animatable(alpha.clone(), target, cfg_pair.fade_spec().clone());
        }
        if drive_slide {
            push_animatable(slide_ch.clone(), target, cfg_pair.slide_spec().clone());
        }
        if drive_scale {
            push_animatable(scale_ch.clone(), target, cfg_pair.scale_spec().clone());
        }
        // Content size write-back for Fraction slide offsets (Backchannel —
        // no recompose; the gfx closure reads it via peek at render time).
        let content_size = ctx.remember_backchannel(|| (0.0, 0.0));
        // Draw-layer params (render-time peek — no dependency, no recompose). The channels come from
        // their own values, so a staggered pair stays staggered all the way into the layer.
        let a = alpha.clone();
        let sl = slide_ch.clone();
        let sc = scale_ch.clone();
        let e = enter.clone();
        let x = exit.clone();
        let v = visible.clone();
        let cs = content_size.clone();
        let gfx = move || {
            let cfg = if v.peek() { &e } else { &x };
            // The transition vocabulary has exactly one implementation
            // (`VisibilityTransition::layer_params`), shared with `AnimatedContent` — which runs the
            // same conversion once per generation, with one clock for every channel.
            cfg.layer_params(
                crate::animation::visibility::ChannelValues {
                    fade: a.peek(),
                    slide: sl.peek(),
                    scale: sc.peek(),
                },
                cs.peek(),
            )
        };
        // Direction-active config: the transition playing NOW (enter when visible,
        // exit when hidden). Clip, expand flags AND anchors all follow it — NOT an
        // OR over both (an enter-expand + exit-slide combo must not clip its exit,
        // and a mid-flight toggle must not jump anchors; see latched anchors below).
        let cfg = if vis { &enter } else { &exit };
        // Latched anchors: captured at animation start, frozen mid-flight. A mid-flight
        // toggle flips `vis` (hence `cfg`) — without latching the anchor would jump
        // (child shifts ~half content size at p=0.5). Re-latch only when settled at an
        // endpoint (fresh animation about to start) or when the config pair changes.
        let anchor_v = ctx.remember_backchannel(|| cfg.expand_from);
        let anchor_h = ctx.remember_backchannel(|| cfg.expand_from_h);
        let settled = size.peek() <= 0.001 || size.peek() >= 0.999;
        if settled && (anchor_v.peek() != cfg.expand_from || anchor_h.peek() != cfg.expand_from_h)
        {
            anchor_v.set(cfg.expand_from);
            anchor_h.set(cfg.expand_from_h);
        }
        // Clip to the animated bounds when expanding (cf. Compose expand's clipToBounds):
        // the policy reports a narrow container mid-animation but places the child at
        // full size — without clip the child overflows and the expand reads as a flash
        // (panel D repro). Clip follows the container bounds, so the content wipes in.
        // Slide-only transitions must NOT clip (content starts outside bounds).
        let want_clip = cfg.expand || cfg.expand_h;
        let modifier = if want_clip {
            Modifier::new()
                .graphics_layer(gfx)
                .clip(crate::graphics::Shape::Rectangle)
        } else {
            Modifier::new().graphics_layer(gfx)
        };
        let policy = VisibilityPolicy {
            progress: size.clone(),
            expand: cfg.expand,
            expand_from: anchor_v.peek(),
            expand_h: cfg.expand_h,
            expand_from_h: anchor_h.peek(),
            content_size: content_size.clone(),
        };
        let key = ctx.next_key();
        let scope = VisibilityScope {
            progress: size.clone(),
            alpha: alpha.clone(),
            slide: slide_ch.clone(),
            scale: scale_ch.clone(),
            visible: visible.clone(),
        };
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                content(&scope, ctx);
            }
        }
        ctx.end_restartable_group();
    }
}

/// Layout policy: expand/shrink scales the container by progress (measure-time `get()`
/// registers a layout_dep → per-frame remeasure while animating, followers move along
/// smoothly; no recompose).
#[derive(Clone)]
struct VisibilityPolicy {
    progress: State<f32>,
    expand: bool,
    expand_from: ExpandFrom,
    expand_h: bool,
    expand_from_h: ExpandFromH,
    /// Measured content size write-back (for Fraction slide offsets —
    /// Backchannel write, render-time peek; zero recompose).
    content_size: crate::runtime::state::Backchannel<(f32, f32)>,
}

impl std::fmt::Debug for VisibilityPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VisibilityPolicy")
            .field("expand", &self.expand)
            .field("expand_h", &self.expand_h)
            .finish()
    }
}

impl MeasurePolicy for VisibilityPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // When expanding, read progress (registers layout_dep → remeasure per frame);
        // otherwise peek (draw-layer graphics_layer transform, zero re-layout).
        let p = if self.expand || self.expand_h {
            self.progress.get()
        } else {
            self.progress.peek()
        };
        // Measure children (usually 1 content root) — Stack semantics, take max.
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        let mut placements = Vec::with_capacity(children.len());
        for &c in children {
            let (size, _) = measure_node(nodes, policies, c, constraints);
            max_w = max_w.max(size.width);
            max_h = max_h.max(size.height);
            placements.push(Placement { size, position: Offset::ZERO });
        }
        self.content_size.set((max_w, max_h));
        let w = if self.expand_h { max_w * p } else { max_w };
        let h = if self.expand { max_h * p } else { max_h };
        // Anchors: Bottom keeps the bottom edge fixed (children shift up by the
        // collapsed amount); End keeps the right edge fixed. Top/Start are no-ops
        // (children stay at origin, container shrinks from far edge).
        if (self.expand && self.expand_from == ExpandFrom::Bottom)
            || (self.expand_h && self.expand_from_h == ExpandFromH::End)
        {
            let dy = if self.expand && self.expand_from == ExpandFrom::Bottom {
                max_h - h
            } else {
                0.0
            };
            let dx = if self.expand_h && self.expand_from_h == ExpandFromH::End {
                max_w - w
            } else {
                0.0
            };
            for pl in placements.iter_mut() {
                pl.position.x += dx;
                pl.position.y += dy;
            }
        }
        (Size::new(w, h), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}


#[cfg(test)]
mod tests;
