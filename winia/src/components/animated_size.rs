//! `AnimatedSize` — 尺寸变化自动动画（对标 Compose `Modifier.animateContentSize`）
//!
//! 用法：
//! ```ignore
//! AnimatedSize::new(TweenSpec::default())
//!     .build(ctx, |ctx| {
//!         Text::new("内容").build(ctx);   // 内容尺寸变化 → 容器尺寸平滑过渡
//!     });
//! ```
//!
//! 机制（布局层正路——无 force_remeasure 旁路）：
//! - **动画尺寸 State<Size>**：`SizePolicy` 测量期先测子内容得目标尺寸，目标变化 →
//!   `push_animatable` 启动尺寸动画（首次直接 Snap 无动画——Compose 语义）
//! - **layout_dep**：测量期 `size.get()` 注册——动画推进每帧重测本节点（不重组），
//!   父容器因本节点尺寸变化自动跟随（下方内容平滑位移）
//! - **容器组件而非 Modifier**：本框架 Modifier 是纯数据（构建期无组合上下文），
//!   无法内嵌 remember 持有动画 State——容器组件在组合期创建 State（机制等价）

//! What matches Compose's `Modifier.animateContentSize` (`AnimationModifier.kt:69-114`,
//! `:159-247`): the animated size is the container's size, the first measurement snaps rather than
//! animating, a new target while an animation runs re-targets from the current value, the result is
//! CLAMPED into the incoming constraints (`constraints.constrain(it)`, `:217`), and the container is
//! clipped to its own bounds (`this.clipToBounds()`, `:77`) so a child that already took its new size
//! does not paint outside the box still animating toward it.
//!
//! What else matches: `alignment` (Compose's `Alignment.TopStart` default is winia's
//! `Alignment::Start`), `finished_listener` (called with `(start size, target)` when the animation
//! ends, and not for the first measurement, which snaps), and the default spec
//! ([`AnimatedSizeDefaults::size_spec`]).
//!
//! The default spec's MOTION matches too: `IntSize.VectorConverter` runs one spring per axis and the
//! animation ends when every axis is at rest (`AnimationModifier.kt:70-74`), and winia's engine now
//! runs a `Spring` on a `Size` as two component springs with the same 1 px threshold
//! (`animation.rs`: `AnimatableValue::write_components` / `SpringLane`), so neither axis is left
//! inside its threshold and neither drifts in the other's wake.
//!
//! Known differences from Compose, recorded rather than silent:
//! - **A container component, not a `Modifier`.** Compose's node is measured inside the parent's
//!   modifier chain; winia's `Modifier` is pure data with no composition context, so the animation
//!   `State` could not live there (see the mechanism note above). The observable difference is that
//!   this animates the container it builds, not an arbitrary modifier position.
//! - **`Stretch` is winia's own extra.** On either axis of [`ContentAlignment`] it gives the child
//!   the box's size; Compose's alignment only ever offsets, and stretches with `fillMaxSize()`. The
//!   one-axis `alignment(Alignment)` builder is the same value on both axes — `Start` is `TopStart`,
//!   `End` is `BottomEnd`.
//! - **No lookahead.** Compose measures with lookahead constraints when a lookahead scope is present
//!   (`:196-206`); winia has no lookahead scope.
//! - **No `wasInterrupted` restart.** Compose re-targets when the target is unchanged but the
//!   running animation was cancelled (`:236-240`); nothing here cancels an animation behind the
//!   component's back, so the retarget gate is the target comparison alone.
//! - **One visibility threshold for every axis.** `SpringSpec::threshold` is a scalar, where
//!   Compose's spring takes a per-axis threshold vector; the default (1 px per axis) is uniform, so
//!   only a caller asking for different thresholds per axis would notice.
//! - **Sub-pixel sizes, and the threshold is in logical units.** Compose animates an `IntSize`:
//!   `IntSize.VectorConverter` rounds the value to whole PHYSICAL pixels every frame and clamps
//!   negatives to zero (`animation-core/VectorConverters.kt:154-166`), with a 1-physical-pixel
//!   visibility threshold (`VisibilityThresholds.kt:97-98`). winia animates `Size` in logical units
//!   with no rounding step, so the motion is smoother than Compose's on a non-integer-density
//!   display, but the 1.0 default threshold is 1 logical unit — 3 physical pixels on a 3x display —
//!   so the stop band is that much wider than Compose's.
//! - **The listener runs inside the layout pass**, where Compose's runs in a coroutine. A State it
//!   writes still notifies (verified: the demo's counter updates from a click), but work that must
//!   not run during measure has no other hook here.

use crate::animation::{push_animatable, AnimationSpec};
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, Alignment, ContentAlignment, LayoutDirection, LayoutNode, MeasurePolicy, Placement};
use crate::modifier::Modifier;
use crate::unit::{Offset, Size};

/// Defaults matching Compose's `Modifier.animateContentSize` overloads.
pub struct AnimatedSizeDefaults;

impl AnimatedSizeDefaults {
    /// The spec Compose animates with when the caller passes none: `spring(stiffness =
    /// StiffnessMediumLow, visibilityThreshold = IntSize.VisibilityThreshold)`
    /// (`AnimationModifier.kt:70-74`).
    ///
    /// `StiffnessMediumLow` is 400 — winia spells that constant `SpringSpec::STIFFNESS_MEDIUM`, and
    /// `SpringSpec::default()` is 200 (Compose's `StiffnessLow`), which is why the default here is
    /// built explicitly rather than left to `SpringSpec::default()`. `IntSize.VisibilityThreshold`
    /// is one pixel, so the spring stops once both axes are within 1 dp instead of the 0.01 that
    /// suits a unit-interval float.
    pub fn size_spec() -> AnimationSpec {
        AnimationSpec::Spring(crate::animation::SpringSpec {
            damping_ratio: crate::animation::SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            stiffness: crate::animation::SpringSpec::STIFFNESS_MEDIUM,
            mass: 1.0,
            threshold: 1.0,
        })
    }
}

impl Default for AnimatedSize {
    /// Compose's default motion ([`AnimatedSizeDefaults::size_spec`]) with the default alignment.
    fn default() -> Self {
        Self::new(AnimatedSizeDefaults::size_spec())
    }
}

/// 尺寸变化自动动画容器
pub struct AnimatedSize {
    spec: AnimationSpec,
    modifier: Modifier,
    /// Where a child sits inside the animated box, on both axes. `TopStart` is Compose's default.
    alignment: ContentAlignment,
    /// Compose's `finishedListener`: called with `(size the animation started from, target)` when it
    /// ends `Finished`.
    finished_listener: Option<std::sync::Arc<dyn Fn(Size, Size) + Send + Sync>>,
}

impl AnimatedSize {
    /// 尺寸动画规格
    pub fn new(spec: impl Into<AnimationSpec>) -> Self {
        Self {
            spec: spec.into(),
            modifier: Modifier::new(),
            alignment: ContentAlignment::TOP_START,
            finished_listener: None,
        }
    }

    /// Where the content sits inside the animated box, for the frames where the two differ — the box
    /// lags the content on every grow and leads it on every shrink. Compose's parameter is 2-D
    /// (`Alignment.TopStart` by default, `AnimationModifier.kt:109`).
    pub fn content_alignment(mut self, a: ContentAlignment) -> Self {
        self.alignment = a;
        self
    }

    /// The same [`Alignment`] on both axes — `Start` is `TopStart`, `End` is `BottomEnd`.
    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = ContentAlignment::both(a);
        self
    }

    /// Called with `(start size, target size)` once the size animation finishes, as Compose's
    /// `finishedListener` is (`AnimationModifier.kt:242-245`). Not called for the first measurement,
    /// which snaps rather than animating, and not called for a target the animation abandoned.
    pub fn finished_listener(
        mut self,
        f: impl Fn(Size, Size) + Send + Sync + 'static,
    ) -> Self {
        self.finished_listener = Some(std::sync::Arc::new(f));
        self
    }

    /// 容器层外观修饰符（background/border 等）——**画在容器上跟随尺寸动画**：
    /// 内容尺寸变化时外观平滑过渡（对齐 Compose `Modifier.animateContentSize`——
    /// 动画的是容器边界，外观修饰属容器）。内容自身的尺寸/背景不参与动画。
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    /// 构建尺寸动画容器。
    /// ⚠ 不宏化：尺寸动画靠 measure 期 `size.get()` 注册 layout_dep + 动画推进
    /// Animating 写驱动每帧重测——宏化封闭 scope 后父容器 Skip，重测链路
    /// 可能被切断（同 animated_visibility/crossfade/animated_content 宏化回归）。
    /// 内部 remember 靠调用点语句 base（稳定）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 动画尺寸 + 上次目标（remember——语句级 key 稳定，跨重组保留）
        // target 不能放 policy 实例（每次 build 重建→Enter 时重置为 None→首帧
        // 逻辑重复→每次 Enter 都直接跳转无动画）
        let size: State<Size> = ctx.remember(|| Size::new(0.0, 0.0));
        let target = ctx.remember_backchannel(|| None);
        let animation_start = ctx.remember_backchannel(|| None);
        let notified = ctx.remember_backchannel(|| None);
        // Direction captured at composition: modifier override first, then the ambient local — the
        // pattern `Row`/`Column` use (`layout/components.rs:61`).
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        let policy = SizePolicy {
            size: size.clone(),
            target: target.clone(),
            alignment: self.alignment,
            direction,
            animation_start: animation_start.clone(),
            notified: notified.clone(),
            finished_listener: self.finished_listener,
            spec: self.spec,
        };
        let key = ctx.next_key();
        // Clip to the container's own bounds, the way Compose's `animateContentSize` starts with
        // `this.clipToBounds()` (`AnimationModifier.kt:77`). Without it a child that has already taken
        // its NEW size paints outside the box that is still animating toward it — measured on a
        // 60 → 400 dp grow with a 6 s spec: with the container at 142 dp, the pixel 91 dp beyond its
        // right edge was the child's own colour (`255 0 0`). The clip goes after the caller's
        // modifiers, which is where Compose puts it too.
        let modifier = self.modifier.then(Modifier::new().clip(crate::graphics::Shape::Rectangle));
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                content(ctx);
            }
        }
        ctx.end_restartable_group();
    }
}

/// 布局 policy：测量子内容 → 目标尺寸变化时启动动画 → 返回动画当前尺寸
#[derive(Clone)]
struct SizePolicy {
    /// 动画值（当前显示尺寸）
    size: State<Size>,
    /// 上次目标尺寸（None = 首帧——直接跳转无动画）——Backchannel 而非
    /// RefCell：policy 实例每次 build 重建，跨重组保留且不触发通知
    target: crate::runtime::state::Backchannel<Option<Size>>,
    /// Where a child sits inside the animated box (Compose's `alignment`, both axes).
    alignment: ContentAlignment,
    /// The direction its horizontal half mirrors under.
    direction: LayoutDirection,
    /// The size the running animation started from, and the target it is heading for — what the
    /// listener is called with, and the guard that keeps it from firing twice for one target.
    animation_start: crate::runtime::state::Backchannel<Option<Size>>,
    notified: crate::runtime::state::Backchannel<Option<Size>>,
    finished_listener: Option<std::sync::Arc<dyn Fn(Size, Size) + Send + Sync>>,
    spec: AnimationSpec,
}

impl std::fmt::Debug for SizePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SizePolicy").finish()
    }
}

impl MeasurePolicy for SizePolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // 测量子内容（取最大宽高——Stack 语义）
        let mut child_w = 0.0f32;
        let mut child_h = 0.0f32;
        let mut child_sizes = Vec::with_capacity(children.len());
        for &c in children {
            let (size, _) = measure_node(nodes, policies, c, constraints);
            child_w = child_w.max(size.width);
            child_h = child_h.max(size.height);
            child_sizes.push(size);
        }
        // Positions are computed AFTER the animated size is known — the child is aligned inside the
        // box, not inside its own size, which is the whole point of Compose's `alignment.align(size =
        // measuredSize, space = IntSize(width, height))` (`AnimationModifier.kt:225-230`).
        let mut placements: Vec<Placement> = Vec::with_capacity(child_sizes.len());
        // 目标变化 → 启动尺寸动画（首帧 Snap 直接跳转）
        let goal = Size::new(child_w, child_h);
        let prev = self.target.peek();
        if prev != Some(goal) {
            if prev.is_none() {
                // 首帧：无动画直接跳转（Compose 语义）
                self.size.as_raw().set_animating(goal);
                // …and no listener call: Compose's runs when `animateTo` finishes, and the first
                // measurement creates the `Animatable` at the target instead of animating to it.
            } else {
                // The value the animation starts from, for the listener's first argument.
                self.animation_start.set(Some(self.size.peek()));
                push_animatable(self.size.clone(), goal, self.spec.clone());
            }
            self.target.set(Some(goal));
        }
        // layout_dep：动画推进每帧重测本节点（get 注册——值变化才 notify）
        let cur = self.size.get();
        #[cfg(test)]
        if std::env::var("WINIA_ANIM_SIZE_TRACE").is_ok() {
            eprintln!("[anim-size] goal={:?} prev={:?} cur={:?} tid={:?}", goal, prev, cur, std::thread::current().id());
        }
        // Fired once per target, when the animated value HAS ARRIVED and the animation has left the
        // registry — Compose's coroutine awaits `animateTo` and calls the listener only when it ends
        // `Finished` (`AnimationModifier.kt:242-245`).
        //
        // Both halves are load-bearing. Arrival alone is not enough: a bouncy spec crosses its target
        // mid-flight, and a cancelled animation can be sitting exactly on it. The registry alone is
        // not enough either — but here it is the second half, so an animation that never ran (value
        // already at the target) still reports, and an interrupted one (retargeted before arrival)
        // reports nothing, the way `Finished`-only does upstream.
        if let (Some(listener), Some(start)) = (&self.finished_listener, self.animation_start.peek()) {
            let arrived =
                cur == goal && !crate::animation::is_animating_state(self.size.state_id());
            if arrived && self.notified.peek() != Some(goal) {
                self.notified.set(Some(goal));
                listener(start, goal);
            }
        }
        // The box the frame ends up with, then each child inside it.
        let box_w = constraints.constrain_width(cur.width);
        let box_h = constraints.constrain_height(cur.height);
        let space = Size::new(box_w, box_h);
        for size in &child_sizes {
            // Compose's alignment never resizes the child — growing it is `fillMaxSize()`'s job. A
            // stretching axis is winia's own extra, so there the child takes the box's size, the way
            // `BoxLayout` does.
            let (x, y) = self.alignment.anchor(*size, space, self.direction);
            placements.push(Placement {
                size: self.alignment.child_size(*size, space),
                position: Offset::new(x, y),
            });
        }
        // Constrain into the INCOMING constraints, as Compose does (`constraints.constrain(it)` —
        // "so that parent doesn't force center this layout", `AnimationModifier.kt:217`). It matters
        // when the constraints tighten while an animation toward a larger size is still running: the
        // animated value is then a size the parent never allowed, and reporting it would place the
        // siblings after this one outside the parent.
        (Size::new(box_w, box_h), placements)
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
    use crate::layout::node::LayoutNode;

    /// 测试用叶子：宽度由外部 State 驱动（50 / 200）
    struct SizedLeaf {
        w: f32,
        /// Height too, so a test can give the vertical half of an alignment something to do.
        h: f32,
    }
    impl SizedLeaf {
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

    /// 查 AnimatedSize 容器节点（第一个非叶子）的测量宽度
    fn container_width(composer: &Composer) -> f32 {
        let Some(root) = composer.layout_root_idx() else { return -1.0 };
        let nodes = composer.arena_nodes();
        fn first_container(nodes: &[LayoutNode], idx: usize) -> f32 {
            let n = &nodes[idx];
            if n.children.is_empty() {
                -1.0 // 叶子——不应出现（容器必有子）
            } else {
                n.measured_size.width
            }
        }
        first_container(nodes, root)
    }

    /// The child of the AnimatedSize container, as (x, y, width, height).
    fn child_box(composer: &Composer) -> (f32, f32, f32, f32) {
        let Some(root) = composer.layout_root_idx() else { return (-1.0, -1.0, -1.0, -1.0) };
        let nodes = composer.arena_nodes();
        let c = nodes[root].children.first().copied().unwrap_or(usize::MAX);
        if c == usize::MAX {
            return (-1.0, -1.0, -1.0, -1.0);
        }
        let n = &nodes[c];
        (n.position.x, n.position.y, n.measured_size.width, n.measured_size.height)
    }

    /// A child is aligned inside the ANIMATED box, not at the origin — Compose's
    /// `alignment.align(size = measuredSize, space = IntSize(width, height))`
    /// (`AnimationModifier.kt:225-230`).
    ///
    /// The visible half of this is a shrunk child in a box that has not caught up yet, so the test
    /// shrinks 400 -> 60 and reads the child's x while the box is still wide.
    #[test]
    fn animated_size_aligns_the_child_inside_the_animating_box() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<State<f32>>);
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 400.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(5000),
                    crate::animation::interpolator::Linear::new(),
                ))
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    // 10 tall against the box's 30: the vertical half has slack to work with.
                    SizedLeaf { w: w.get(), h: if w.get() > 100.0 { 30.0 } else { 10.0 } }.build(ctx);
                });
            });
        };

        recompose(&mut composer);
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 400.0));
        let shrunk = holder.borrow().as_ref().unwrap().clone();
        shrunk.set(60.0);
        recompose(&mut composer);
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 400.0));

        let (x, y, w, h) = child_box(&composer);
        let box_w = container_width(&composer);
        assert_eq!((w, h), (60.0, 10.0), "the child keeps its own measured size");
        assert!(box_w > 100.0, "the box is still wide while it shrinks (box_w={box_w})");
        assert!(
            (x - (box_w - 60.0) / 2.0).abs() < 0.51,
            "the child is centred in the box, not at the origin (x={x}, box_w={box_w})"
        );
        let box_h = 30.0;
        assert!(
            (y - (box_h - 10.0) / 2.0).abs() < 0.51,
            "and centred vertically too (y={y}, box_h={box_h})"
        );
    }

    /// Compose's `finishedListener` runs once, with `(start size, target)`, when the animation ends
    /// `Finished` — and never for the first measurement, which snaps (`AnimationModifier.kt:242-245`).
    #[test]
    fn animated_size_reports_the_size_it_animated_from_and_to() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let reported: std::sync::Arc<std::sync::Mutex<Vec<((f32, f32), (f32, f32))>>> =
            Default::default();
        let sink = reported.clone();
        let holder = std::cell::RefCell::new(None::<State<f32>>);
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(80),
                    crate::animation::interpolator::Linear::new(),
                ))
                .finished_listener({
                    let sink = sink.clone();
                    move |from: Size, to: Size| {
                        sink.lock().unwrap().push((
                            (from.width, from.height),
                            (to.width, to.height),
                        ));
                    }
                })
                .build(ctx, |ctx| {
                    SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..10_000 {
                if !crate::animation::update_animations() {
                    break;
                }
                recompose(composer);
            }
            recompose(composer);
        };

        recompose(&mut composer);
        assert!(reported.lock().unwrap().is_empty(), "the first measurement snaps, so it reports nothing");

        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(200.0);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(
            reported.lock().unwrap().as_slice(),
            &[((50.0, 30.0), (200.0, 30.0))],
            "one call, with the size it animated from and the one it animated to"
        );
    }

    /// The default is Compose's own default motion, not `SpringSpec::default()`.
    #[test]
    fn the_default_spec_is_compeses_spring() {
        match AnimatedSizeDefaults::size_spec() {
            crate::animation::AnimationSpec::Spring(s) => {
                assert_eq!(s.stiffness, crate::animation::SpringSpec::STIFFNESS_MEDIUM, "StiffnessMediumLow is 400");
                assert_eq!(s.damping_ratio, crate::animation::SpringSpec::DAMPING_RATIO_NO_BOUNCY);
                assert_eq!(s.threshold, 1.0, "IntSize.VisibilityThreshold is one pixel");
                assert_ne!(
                    s.stiffness,
                    crate::animation::SpringSpec::default().stiffness,
                    "…and SpringSpec::default() is 200, so this had to be built explicitly"
                );
            }
            other => panic!("the default should be a spring, got {other:?}"),
        }
    }

    /// The animated value is reported through the INCOMING constraints, as Compose does
    /// (`constraints.constrain(it)`, `AnimationModifier.kt:217`).
    ///
    /// The reachable case: the constraints tighten while an animation toward a larger size is still
    /// running. Without the clamp the container reports a size the parent never allowed, and the
    /// sibling after it is placed outside the parent.
    #[test]
    fn animated_size_reports_a_size_the_constraints_allow() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<State<f32>>);
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(5000),
                    crate::animation::interpolator::Linear::new(),
                ))
                .build(ctx, |ctx| {
                    SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                });
            });
        };

        recompose(&mut composer);
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 400.0));
        assert_eq!(container_width(&composer), 50.0, "first frame jumps to the content size");

        // Grow toward 400 while the parent allows it, and let it get PAST the width the parent is
        // about to allow — otherwise the clamp has nothing to do and the test passes either way
        // (measured: it did, at `mid` 60 against a 100 limit).
        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(400.0);
        recompose(&mut composer);
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 400.0));
        let mut mid = container_width(&composer);
        let mut waited = 0;
        while mid <= 120.0 && waited < 4000 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            crate::animation::update_animations();
            waited += 20;
            recompose(&mut composer);
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 400.0));
            mid = container_width(&composer);
        }
        assert!(mid > 120.0, "the animation got past the limit the parent is about to set (mid={mid})");

        // …and tighten the parent while it runs.
        composer.layout(Constraints::new(0.0, 100.0, 0.0, 400.0));
        let clamped = container_width(&composer);
        assert!(
            clamped <= 100.0,
            "the container never reports more than the parent allows (got {clamped})"
        );
    }

    #[test]
    fn animated_size_tracks_content_change() {
        // 动画注册表是进程级全局单例——必须持串行锁，否则并行测试的
        // clear_all_animations 会抹掉本测试在飞的动画（中途冻结偶发失败）
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        // 内容宽度 State 必须走 remember（owner queue——State::new 的 notify 不推送）
        let holder = std::cell::RefCell::new(None::<State<f32>>);

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::default()).build(ctx, |ctx| {
                    SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 短时长 tween（100ms）——10×40ms 推进累计远超时长，即使个别 tick 被
        // 调度延迟吞掉也保证收敛（墙钟测试的时序裕度原则）。
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(100),
                    crate::animation::interpolator::Linear::new(),
                ))
                .build(ctx, |ctx| {
                    SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 推进至动画注册表排空（update_animations 返回 false）——不依赖墙钟节奏：
        // 负载/抢占只影响耗时（空转 tick），不影响最终收敛结果。上限防死循环。
        let mut advance = |composer: &mut Composer| {
            for _ in 0..10_000 {
                if !crate::animation::update_animations() {
                    break;
                }
                recompose(composer);
            }
            recompose(composer);
        };

        // 首帧：内容宽 50 → 容器直接 50（无动画）
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 50.0, "首帧直接跳转到内容尺寸");

        // 内容变 200 → 容器动画过渡 → 收敛 200
        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(200.0);
        recompose(&mut composer);
        // 过渡中宽度必须落在 [旧, 新] 区间（tween 有界不超调）。
        // ⚠ 不断言 mid < 200：注册与测量之间若线程被抢占 ≥ 动画时长，
        // 并行测试的其他线程 tick 全局动画表也能推进进度——合法动画会
        // 合法到达终值，严格小于断言测的是调度器不是框架（曾致偶发失败）。
        let mid = container_width(&composer);
        assert!(
            (50.0..=200.0).contains(&mid),
            "动画过渡中：宽度应介于旧新之间（mid={}）",
            mid
        );
        advance(&mut composer);
        assert_eq!(container_width(&composer), 200.0, "动画完成后到达新尺寸");
    }

    /// 内容 State 变化但尺寸不变——不注册新动画（无重测风暴）
    #[test]
    fn animated_size_no_animation_when_size_unchanged() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<State<f32>>);

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                // 内容宽度恒为 80——w 变化只触发内容重建（不改变尺寸）
                AnimatedSize::new(crate::animation::TweenSpec::default()).build(ctx, |ctx| {
                    let _ = w.get();
                    SizedLeaf { w: 80.0, h: 30.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(container_width(&composer), 80.0, "首帧显示内容尺寸");

        // w 变化 → 内容重建但尺寸不变 → 容器保持 80（无动画注册）
        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(999.0);
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 80.0, "尺寸不变时容器不应有动画/跳变");
        // 推进几帧——若错误注册了动画，宽度会短暂偏离 80
        for _ in 0..5 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
            recompose(&mut composer);
            assert_eq!(container_width(&composer), 80.0, "尺寸不变时动画推进不应改变宽度");
        }
    }

    /// The listener runs under a spring too, and what it writes is visible — a State write from
    /// inside the layout pass (which is where the listener is called).
    ///
    /// This is the test that caught the engine's substitution: it used to compare the value with
    /// `goal` alone and the demo showed it never running, because a spring on a `Size` was silently
    /// driven by a tween that stopped inside the threshold while the registry still held the
    /// animation. The default spec now lands on the exact target (`SpringLane`), and the check is
    /// arrival AND "no longer animating", which is Compose's `Finished`.
    #[test]
    fn animated_size_reports_through_the_default_spring_too() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<State<f32>>);
        let reported = std::cell::RefCell::new(None::<State<u32>>);
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                // A State the listener writes into, read by something else in the tree — the shape
                // the demo uses for its report.
                let calls: State<u32> = ctx.remember(|| 0);
                *reported.borrow_mut() = Some(calls.clone());
                AnimatedSize::default()
                    .finished_listener({
                        let calls = calls.clone();
                        move |_from: Size, _to: Size| { calls.update(|v| *v += 1); }
                    })
                    .build(ctx, |ctx| {
                        SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(200.0);
        recompose(&mut composer);
        for _ in 0..10_000 {
            if !crate::animation::update_animations() { break; }
            std::thread::sleep(std::time::Duration::from_millis(8));
            recompose(&mut composer);
        }
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 200.0, "the spring reached the target");
        assert_eq!(
            reported.borrow().as_ref().unwrap().peek(),
            1,
            "the listener ran once, and its State write is visible"
        );
    }

    /// Compose's default spec is a spring on the SIZE VECTOR — `spring(stiffness =
    /// StiffnessMediumLow, visibilityThreshold = IntSize.VisibilityThreshold)` is a
    /// `FiniteAnimationSpec<IntSize>`, and `IntSize.VectorConverter` runs one spring per axis, each
    /// finishing when it is within a pixel (`AnimationModifier.kt:70-74`).
    ///
    /// This test was written as a falsification for the engine defect it now guards: winia used to
    /// carry a single scalar displacement per animation and `AnimatableValue::from_f32` is not
    /// injective for a `Size` (`Size::new(v, v)`), so a spring on a `Size` was silently substituted
    /// with a plain tween, and the container's motion was that tween while the spec said spring
    /// (measured: largest gap 47.95). The container's motion is compared against BOTH references
    /// driven from the same tick loop: it must track the scalar spring, and it must not be that tween.
    #[test]
    fn the_default_spec_moves_the_size_the_way_a_spring_does() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<State<f32>>);
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::default().build(ctx, |ctx| {
                    SizedLeaf { w: w.get(), h: 30.0 }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        // First measurement: the box snaps to the content, no animation to observe.
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 50.0, "the first frame snaps");

        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(200.0);
        recompose(&mut composer);

        // Two references on the same 50 -> 200 move, ticked by the same loop as the container: one
        // through the spec the container claims to use, one through what the engine substitutes.
        let spring_ref: State<f32> = State::new(50.0);
        let tween_ref: State<f32> = State::new(50.0);
        push_animatable(spring_ref.clone(), 200.0, AnimatedSizeDefaults::size_spec());
        push_animatable(
            tween_ref.clone(),
            200.0,
            AnimationSpec::Tween(crate::animation::TweenSpec::default()),
        );

        let mut max_spring_gap = 0.0f32;
        let mut max_tween_gap = 0.0f32;
        let mut frames = 0;
        for _ in 0..2_000 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
            recompose(&mut composer);
            let box_w = container_width(&composer);
            max_spring_gap = max_spring_gap.max((box_w - spring_ref.peek()).abs());
            max_tween_gap = max_tween_gap.max((box_w - tween_ref.peek()).abs());
            frames += 1;
        }
        recompose(&mut composer);

        assert!(frames > 5, "the loop sampled the motion (frames={frames})");
        assert!(
            max_spring_gap < 1.0,
            "the box follows the scalar spring the default spec names (largest gap={max_spring_gap})"
        );
        assert!(
            max_tween_gap > 10.0,
            "…and is not the tween the engine substitutes for a spring on a Size \
             (largest gap={max_tween_gap})"
        );
        assert_eq!(container_width(&composer), 200.0, "the move ends exactly on the target");
    }
}
