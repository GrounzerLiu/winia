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
//! transition for the initial state, the progress being readable from the content
//! (`VisibilityScope` ≈ `AnimatedVisibilityScope.transition`), delayed removal, and the full builder
//! vocabulary (fade/slide/expand/shrink/scale, both axes, anchors, offsets).
//!
//! Deliberate differences (all with their Compose counterpart):
//! - **One spec per transition.** Winia's `VisibilityTransition` carries a single `AnimationSpec` for
//!   every channel; Compose gives each channel its own (`fadeIn(spring) + expandIn(spring2)`).
//! - **One default, both axes.** Compose picks the default axis from the scope — `RowScope` expands
//!   horizontally, `ColumnScope` vertically, the top level expands both (`AnimatedVisibility.kt:125-129`,
//!   `:199-203`, `:272-276`) — while winia has one default (both axes) plus the explicit
//!   `expand_in_h`/`with_expand_h` builders for the horizontal case.
//! - **No `MutableTransitionState`.** Winia takes a `State<bool>`, and the scope's progress covers what
//!   `isIdle`/`currentState` are read for. Compose also allows an initial state different from the first
//!   target through that type (`Transition.kt:796`); winia's first composition is always the initial
//!   state.
//! - **No `label`** (Android Studio animation preview only) and no `Transition<T>.AnimatedVisibility`
//!   overload (winia has no public `Transition` object).
//! - **`unveilIn`/`veilOut` are not implemented.** They change only the CLIP (a reveal that does not
//!   touch layout, `EnterExitTransition.kt:464`, `:491`); every size channel winia has is a layout one.
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
}

/// What `AnimatedVisibility` hands its content — Compose's `AnimatedVisibilityScope`, whose
/// `transition: Transition<EnterExitState>` exists so custom animations can ride the same clock the
/// container does ("Custom enter/exit animations can be created using the `Transition<EnterExitState>`
/// object from the `AnimatedVisibilityScope`", `AnimatedVisibility.kt:160-163`).
///
/// winia has no public `Transition` object, so the scope carries the progress itself: read it with
/// `get()` inside a composition (the read re-runs that composition while the transition moves) or with
/// `peek()` at render/measure time (a graphics layer, a placement — no recomposition).
#[derive(Clone)]
pub struct VisibilityScope {
    /// 0 = hidden, 1 = shown; a spring overshoots past 1, exactly like the container it sizes.
    pub progress: State<f32>,
}

impl VisibilityScope {
    /// The transition's progress right now, as a composition dependency.
    pub fn value(&self) -> f32 {
        self.progress.get()
    }

    /// The same value without registering a dependency — for render-time closures.
    pub fn peek(&self) -> f32 {
        self.progress.peek()
    }
}

impl AnimatedVisibility {
    pub fn new(visible: State<bool>) -> Self {
        Self {
            visible,
            enter: VisibilityTransition::default(),
            exit: VisibilityTransition::default(),
        }
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
        // against a 50-unit content).
        let progress: State<f32> = ctx.remember(|| if vis { 1.0 } else { 0.0 });
        let removed: State<bool> = ctx.remember(|| !vis);
        let _p = progress.get(); // the transition moving re-runs this build (the removal check runs)
        let _r = removed.get(); // the removal flag re-runs it, so the `return` below takes effect
        // Removed and shown again → reset (the notify requests the recomposition, this frame starts).
        if removed.peek() && vis {
            removed.set(false);
        }
        // Exit-complete detection: hidden and progress ≈ 0 → mark removed (this build runs every frame —
        // the animating value notifies each frame → pending → composed on the next one).
        if !vis && progress.peek() < 0.001 && !removed.peek() {
            removed.set(true);
        }
        if removed.peek() {
            return; // removed: do not start the container → the slot stays unvisited → materialization frees it
        }
        // Drive progress animation (push on visible-change frames — same-target dedup skips)
        let target = if vis { 1.0 } else { 0.0 };
        let spec = if vis { enter.spec.clone() } else { exit.spec.clone() };
        push_animatable(progress.clone(), target, spec);
        // Content size write-back for Fraction slide offsets (Backchannel —
        // no recompose; the gfx closure reads it via peek at render time).
        let content_size = ctx.remember_backchannel(|| (0.0, 0.0));
        // Draw-layer params (render-time peek — no dependency, no recompose)
        let g = progress.clone();
        let e = enter.clone();
        let x = exit.clone();
        let v = visible.clone();
        let cs = content_size.clone();
        let gfx = move || {
            let p = g.peek();
            let cfg = if v.peek() { &e } else { &x };
            // The transition vocabulary has exactly one implementation
            // (`VisibilityTransition::layer_params`), shared with `AnimatedContent` — which runs the
            // same conversion once per generation.
            cfg.layer_params(p, cs.peek())
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
        let settled = progress.peek() <= 0.001 || progress.peek() >= 0.999;
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
            progress: progress.clone(),
            expand: cfg.expand,
            expand_from: anchor_v.peek(),
            expand_h: cfg.expand_h,
            expand_from_h: anchor_h.peek(),
            content_size: content_size.clone(),
        };
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                content(&VisibilityScope { progress: progress.clone() }, ctx);
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
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;
    use crate::runtime::state::State;
    use crate::layout::constraints::Constraints;
    use crate::layout::components::Column;

    /// 集成：visible 切换 → 内容进入/退出组合树（exit 完成才移除）。
    /// 通过多次 compose + 手动推进动画模拟可见性生命周期。
    #[test]
    fn visibility_toggle_enters_and_exits() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v1 = visible.clone();
        let mut present = Vec::new();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedVisibility::new(v1.clone())
                    .build(ctx, |ctx| {
                        TextLeaf::new().build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // Advance animation frames (real time drives them). The default spec is Compose's
        // spring(1.0, 400), whose tail is longer than the 300 ms tween this test used to budget for, so
        // the loop runs 20 frames × 60 ms ≈ 1.2 s.
        let mut advance = |composer: &mut Composer| {
            for _ in 0..20 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };

        // 首帧：visible=false → 内容不在组合
        recompose(&mut composer);
        present.push(count_text(&composer));

        // visible=true → 内容进入组合（enter 动画 0→1）
        visible.set(true);
        recompose(&mut composer);
        present.push(count_text(&composer));
        advance(&mut composer); // progress → 1.0

        // visible=false → exit 动画中：内容保留（progress 1→0 途中）
        visible.set(false);
        recompose(&mut composer);
        present.push(count_text(&composer));

        // 推进到 exit 完成（progress → 0 → removed 标记 → 下帧移除）
        advance(&mut composer);
        present.push(count_text(&composer));

        assert_eq!(present[0], 0, "初始不可见：内容不在组合");
        assert!(present[1] > 0, "visible=true：内容进入组合");
        assert!(present[2] > 0, "exit 动画中：内容保留");
        assert_eq!(present[3], 0, "exit 完成：内容移除");
    }

    /// Compose never runs a transition for its INITIAL state: `AnimatedVisibility(visible = true)` on the
    /// first composition shows its content at full size with no enter animation (the rule
    /// `MutableTransitionState`'s initial `targetState` follows too — the transition's first value is its
    /// target, not an animation away from it). winia started `progress` at 0 and pushed the enter, so the
    /// content wiped in from nothing on the frame it first appeared.
    #[test]
    fn an_initially_visible_content_does_not_play_an_enter() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(true);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(VisibilityTransition::expand_in(
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        let root = composer.layout_root_idx().expect("root");
        let av = composer.arena_nodes()[root].children[0];
        let size = composer.arena_nodes()[av].measured_size;
        assert_eq!(
            (size.width, size.height),
            (100.0, 50.0),
            "the first layout is already the full content size, not a 0-progress container"
        );
        assert!(
            !crate::animation::update_animations(),
            "no enter animation may be running on the first composition"
        );
    }

    /// The content can ride the same clock the container does — Compose's `AnimatedVisibilityScope`,
    /// whose `transition: Transition<EnterExitState>` exists for exactly this ("Custom enter/exit
    /// animations can be created using the `Transition<EnterExitState>` object from the
    /// `AnimatedVisibilityScope`", `AnimatedVisibility.kt:160-163`). winia hands the progress itself
    /// through `build_with_scope`; reading it must track the container's own size, which is
    /// `content × progress` while a size-changing transition plays.
    #[test]
    fn the_content_can_read_the_transition_progress() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let sampled = std::rc::Rc::new(std::cell::Cell::new(-1.0f32));
        let s = sampled.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    let s = s.clone();
                    AnimatedVisibility::new(v.clone()).build_with_scope(
                        ctx,
                        move |scope, ctx| {
                            s.set(scope.value());
                            TextLeaf::new().build(ctx);
                        },
                    );
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer); // hidden: no container, no content
        visible.set(true);
        let mut mid_flight = false;
        for _ in 0..40 {
            build(&mut composer);
            let p = sampled.get();
            if p > 0.05 && p < 0.95 {
                mid_flight = true;
                let root = composer.layout_root_idx().expect("root");
                let av = composer.arena_nodes()[root].children[0];
                let h = composer.arena_nodes()[av].measured_size.height;
                assert!(
                    (h - 50.0 * p).abs() < 0.5,
                    "the container is sized by the same progress the content reads: h={h} p={p}"
                );
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(mid_flight, "the content observed a mid-flight progress");
    }

    /// A plain `AnimatedVisibility::new(v)` (no `.enter`/`.exit`) must fade AND change the container's
    /// size: every Compose overload's default pair is `fadeIn() + expand*`/`fadeOut() + shrink*`
    /// (`AnimatedVisibility.kt:125-129` top level, `:199-203` RowScope, `:272-276` ColumnScope). winia's
    /// default faded only, so the container kept the full content height for the whole transition and the
    /// siblings below never moved.
    #[test]
    fn the_default_transition_expands_as_well_as_fades() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone()).build(ctx, |ctx| {
                        TextLeaf::new().build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let height = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let av = composer.arena_nodes()[root].children[0];
            composer.arena_nodes()[av].measured_size.height
        };
        build(&mut composer); // hidden: no container
        visible.set(true);
        build(&mut composer); // the frame the enter starts on
        let start = height(&composer);
        assert!(
            start < 50.0,
            "the default enter starts from a collapsed container (got {start}) — a fade-only default \
             leaves it at the full content height"
        );
        for _ in 0..40 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            build(&mut composer);
        }
        assert_eq!(height(&composer), 50.0, "and it reaches the content's own height");
    }

    /// 复现 demo（animated_visibility_demo panel_c）回归：Column 内
    /// [兄弟A, AnimatedVisibility, 兄弟B]——toggle AV 后兄弟 A（按钮位置）必须
    /// 保留。用户反馈：点第三个按钮后按钮本身消失。
    /// ⚠ visible 必须用 remember 创建（绑定 owner queue——State::new 外部
    /// 创建的 notify 不入队，不会触发重组——测试环境的既有陷阱）。
    #[test]
    fn siblings_survive_visibility_toggle() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let visible_holder = std::cell::RefCell::new(None::<State<bool>>);
        let mut counts = Vec::new();
        // AV content 执行计数——区分「Column Skip 冻结 AV」vs「动画未推进」
        let content_runs = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let cr = content_runs.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(crate::compose!(|ctx| {
                Column::new().build(ctx, |ctx| {
                    // 兄弟 A（模拟 toggle 按钮——AV 的兄弟，应始终保留）
                    LeafBox::new(40.0, 24.0).build(ctx);
                    // AV：show 变化 → exit 动画 → removed → 子树回收
                    let visible = ctx.remember(|| true); // remember 绑定 owner queue
                    *visible_holder.borrow_mut() = Some(visible.clone());
                    AnimatedVisibility::new(visible.clone())
                        .enter(VisibilityTransition::fade_in(crate::animation::TweenSpec::default()))
                        .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::default()))
                        .build(ctx, |ctx| {
                            cr.set(cr.get() + 1);
                            LeafBox::new(80.0, 40.0).build(ctx);
                        });
                    // 兄弟 B（固定锚点——AV 收缩后应保留）
                    LeafBox::new(20.0, 20.0).build(ctx);
                });
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        // 推进动画帧
        let mut advance = |composer: &mut Composer| {
            for _ in 0..8 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };
        // 单次 toggle：翻转 visible → 逐帧推进 → 断言 AV 子树被回收（A/B 保留）
        let mut toggle = |composer: &mut Composer| {
            let v = visible_holder.borrow().clone().unwrap();
            v.set(!v.get()); // notify → 下帧重组（remember 创建的 State 绑定队列）
            recompose(composer);
            advance(composer);
            node_count(composer)
        };

        // 首帧：全显示 + enter 动画完成（progress 0→1——用户场景 toggle 时动画已就绪）
        recompose(&mut composer);
        advance(&mut composer);
        let n0 = node_count(&composer);
        counts.push(n0);
        assert!(n0 >= 4, "首帧应含 Column+A+B+AV内容（实际 {n0}）");

        // toggle(false)：exit 动画 → removed → AV 子树回收（Column+A+B=3；A 不应消失）
        let n1 = toggle(&mut composer);
        counts.push(n1);
        assert_eq!(n1, 3, "AV 移除后应剩 Column+A+B（实际 {n1}，序列 {counts:?}）——兄弟 A 不应消失");

        // toggle(true)：enter 动画 → AV 内容恢复（A/B 仍保留）
        let n2 = toggle(&mut composer);
        counts.push(n2);
        assert!(n2 >= 4, "AV 恢复后应含 Column+A+B+AV内容（实际 {n2}，序列 {counts:?}）");
    }

    /// 测试用固定尺寸叶子（Text 需字体环境——用固定尺寸盒子代替）
    struct TextLeaf;
    impl TextLeaf {
        const fn new() -> Self {
            Self
        }
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(100.0, 50.0),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    /// 指定尺寸的叶子（siblings_survive_visibility_toggle 用）
    struct LeafBox {
        w: f32,
        h: f32,
    }
    impl LeafBox {
        fn new(w: f32, h: f32) -> Self {
            Self { w, h }
        }
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(self.w, self.h),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    fn count_text(composer: &Composer) -> usize {
        node_count(composer)
    }

    /// Measure the AV policy math at a fixed progress (pure, no animation timing).
    /// Builds a minimal arena with one 120x60 leaf child and runs policy.measure.
    /// Returns (container size, child placements).
    fn measure_policy_at(
        expand: bool,
        expand_from: ExpandFrom,
        expand_h: bool,
        expand_from_h: ExpandFromH,
        progress: f32,
    ) -> (Size, Vec<Placement>) {
        let policy = VisibilityPolicy {
            progress: State::new(progress),
            expand,
            expand_from,
            expand_h,
            expand_from_h,
            content_size: crate::runtime::state::Backchannel::new((0.0, 0.0)),
        };
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(120.0, 60.0))];
        let policies: Vec<Box<dyn MeasurePolicy>> = Vec::new();
        let (size, placements) =
            policy.measure(&mut nodes, &policies, &[0], Constraints::new(0.0, 400.0, 0.0, 400.0));
        policy.place(&mut nodes, &[0], &placements);
        (size, placements)
    }

    #[test]
    fn expand_vertical_scales_height_by_progress() {
        // Vertical expand at p=0.5 on 120x60 content → 120x30, Top anchor keeps origin.
        let (size, placements) =
            measure_policy_at(true, ExpandFrom::Top, false, ExpandFromH::Start, 0.5);
        assert!((size.width - 120.0).abs() < 0.5, "width untouched, got {}", size.width);
        assert!((size.height - 30.0).abs() < 0.5, "height = 60*0.5 at p=0.5, got {}", size.height);
        assert!((placements[0].position.y - 0.0).abs() < 0.5, "Top anchor keeps y=0");
    }

    #[test]
    fn expand_horizontal_scales_width_by_progress() {
        // NEW horizontal expand at p=0.5 on 120x60 content → 60x60.
        let (size, _) = measure_policy_at(false, ExpandFrom::Top, true, ExpandFromH::Start, 0.5);
        assert!((size.width - 60.0).abs() < 0.5, "width = 120*0.5 at p=0.5, got {}", size.width);
        assert!((size.height - 60.0).abs() < 0.5, "height untouched, got {}", size.height);
    }

    #[test]
    fn expand_bottom_anchor_shifts_child_up() {
        // Bottom anchor at p=0.5: container 120x30, child y = 60-30 = 30 (bottom fixed).
        let (_, placements) =
            measure_policy_at(true, ExpandFrom::Bottom, false, ExpandFromH::Start, 0.5);
        assert!(
            (placements[0].position.y - 30.0).abs() < 0.5,
            "Bottom anchor child y should be 30, got {}",
            placements[0].position.y
        );
    }

    #[test]
    fn expand_end_anchor_shifts_child_left() {
        // End anchor at p=0.5: container 60x60, child x = 120-60 = 60 (right fixed).
        let (_, placements) =
            measure_policy_at(false, ExpandFrom::Top, true, ExpandFromH::End, 0.5);
        assert!(
            (placements[0].position.x - 60.0).abs() < 0.5,
            "End anchor child x should be 60, got {}",
            placements[0].position.x
        );
    }

    #[test]
    fn expand_h_demo_replica_grows_monotonically() {
        // Exact demo-D replica: Column > AV(expand_in_h) > Text-like sized leaf.
        // Step real animation frames; container width must grow monotonically 0→300.
        use crate::layout::components::Column;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let build = |composer: &mut Composer, v: &State<bool>| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(VisibilityTransition::expand_in_h(
                            crate::animation::TweenSpec::default(),
                        ))
                        .exit(VisibilityTransition::shrink_out_h(
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            LeafBox::new(300.0, 44.0).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &v);
        v.set(true);
        let mut widths = Vec::new();
        // Read AV container width: Column root child[0].
        let read_w = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let av = nodes[root].children[0];
            nodes[av].measured_size.width
        };
        for _ in 0..10 {
            build(&mut composer, &v);
            widths.push(read_w(&composer));
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(widths[0] < 100.0, "first frame should be narrow, widths={widths:?}");
        assert!(
            (widths[widths.len() - 1] - 300.0).abs() < 5.0,
            "last frame should settle at 300, widths={widths:?}"
        );
        for pair in widths.windows(2) {
            assert!(
                pair[1] + 1.0 >= pair[0],
                "width must grow monotonically, widths={widths:?}"
            );
        }
        crate::animation::clear_all_animations();
    }
    #[test]
    fn expand_h_first_frame_starts_collapsed() {
        // First frame after toggle from removed: progress ~0 so the container must be
        // ~0 wide — full width here means the animation never played (progress jumped
        // to 1 or expand_h miswired in build).
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let v1 = v.clone();
        composer.compose(|ctx| {
            AnimatedVisibility::new(v1.clone())
                .enter(VisibilityTransition::expand_in_h(
                    crate::animation::TweenSpec::default(),
                ))
                .exit(VisibilityTransition::shrink_out_h(
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(300.0, 44.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        // Frame 1: removed, nothing composed (layout_root None → count 0).
        assert_eq!(node_count(&composer), 0, "removed AV leaves nothing composed");
        // Toggle visible, single frame, no animation stepping.
        v.set(true);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in_h(
                    crate::animation::TweenSpec::default(),
                ))
                .exit(VisibilityTransition::shrink_out_h(
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(300.0, 44.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // AV container IS the root here (top-level compose) — its own size, not the child's.
        let w = nodes[root].measured_size.width;
        assert!(
            w < 290.0,
            "frame 1 after toggle should be mid-expand (w<290), got {w} — animation never played"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn mid_flight_toggle_keeps_latched_anchor() {
        // P1-1: anchors latch at animation start — a mid-flight toggle must NOT jump
        // the anchor. Enter Top + exit Bottom: toggle visible mid-enter, the latched
        // anchor stays Top (child y=0) until the animation settles.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let build = |composer: &mut Composer, v: &State<bool>| {
            composer.compose(|ctx| {
                AnimatedVisibility::new(v.clone())
                    .enter(
                        VisibilityTransition::expand_in(crate::animation::TweenSpec::default())
                            .with_expand_from(ExpandFrom::Top),
                    )
                    .exit(
                        VisibilityTransition::shrink_out(crate::animation::TweenSpec::default())
                            .with_expand_from(ExpandFrom::Bottom),
                    )
                    .build(ctx, |ctx| {
                        LeafBox::new(120.0, 60.0).build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &v);
        v.set(true);
        build(&mut composer, &v);
        // Step the enter forward to mid-flight (progress ~0.5): each cycle advances
        // the animation clock and rebuilds so progress leaves ~0 (otherwise the exit
        // below would start from ~0 and complete instantly — nothing to latch).
        for _ in 0..3 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer, &v);
        }
        v.set(false);
        build(&mut composer, &v);
        // After toggle mid-flight, child y must still be 0 (Top latched), not 30 (Bottom).
        // (Top-level compose: the AV container IS the root.)
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let child = nodes[root].children[0];
        let y = nodes[child].position.y;
        assert!(
            y.abs() < 1.0,
            "latched Top anchor must survive mid-flight toggle (child y=0), got {y}"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn slide_only_exit_is_not_clipped() {
        // P1-2: enter-expand + exit-slide combo — the exit slide must NOT be clipped.
        // Policy expand flag follows the active direction: exit slide → expand=false,
        // so measure peeks (no layout_dep) and build skips clip.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(true);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in(crate::animation::TweenSpec::default()))
                .exit(VisibilityTransition::slide_out(
                    SlideDirection::Right,
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(120.0, 60.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        // Settle enter, then toggle to exit-slide and check the container has no clip.
        for _ in 0..8 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            composer.compose(|ctx| {
                AnimatedVisibility::new(v.clone())
                    .enter(VisibilityTransition::expand_in(
                        crate::animation::TweenSpec::default(),
                    ))
                    .exit(VisibilityTransition::slide_out(
                        SlideDirection::Right,
                        crate::animation::TweenSpec::default(),
                    ))
                    .build(ctx, |ctx| {
                        LeafBox::new(120.0, 60.0).build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        }
        v.set(false);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in(crate::animation::TweenSpec::default()))
                .exit(VisibilityTransition::slide_out(
                    SlideDirection::Right,
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(120.0, 60.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let av = nodes[root].children[0];
        let has_clip = nodes[av]
            .modifier
            .elements()
            .iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::Clip { .. }));
        assert!(!has_clip, "exit-slide direction must not clip the container");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn slide_offset_fraction_resolves_against_content_size() {
        // Fraction(1.0) on 120-wide content = 120px full slide; Fixed(48) stays 48.
        // Resolve through the same match arms the gfx closure uses.
        let resolve = |offset: SlideOffset, dir: SlideDirection, w: f32, h: f32| -> f32 {
            let extent = match dir {
                SlideDirection::Left | SlideDirection::Right => w,
                SlideDirection::Up | SlideDirection::Down => h,
            };
            offset.resolve(extent)
        };
        assert!((resolve(SlideOffset::Fixed(48.0), SlideDirection::Right, 120.0, 60.0) - 48.0).abs() < 0.01);
        assert!((resolve(SlideOffset::Fraction(1.0), SlideDirection::Right, 120.0, 60.0) - 120.0).abs() < 0.01);
        assert!((resolve(SlideOffset::Fraction(0.5), SlideDirection::Down, 120.0, 60.0) - 30.0).abs() < 0.01);
    }

    #[test]
    fn scale_from_and_origin_defaults_match_status_quo() {
        // Default transition = old behavior: scale_from 0.8, center pivot.
        // Old gfx math s = 0.8 + 0.2*p must equal new math with defaults.
        let t = VisibilityTransition::scale_in(crate::animation::TweenSpec::default());
        assert!((t.scale_from - 0.8).abs() < 1e-6);
        assert_eq!(t.transform_origin, (0.5, 0.5));
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let old = 0.8 + 0.2 * p;
            let new = t.scale_from + (1.0 - t.scale_from) * p;
            assert!((old - new).abs() < 1e-6, "scale math unchanged at p={p}");
        }
    }

    fn node_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(nodes: &[LayoutNode], idx: usize, count: &mut usize) {
            *count += 1;
            for &c in &nodes[idx].children {
                walk(nodes, c, count);
            }
        }
        walk(nodes, root, &mut count);
        count
    }
}
