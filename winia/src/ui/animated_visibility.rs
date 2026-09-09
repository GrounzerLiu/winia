//! `AnimatedVisibility` — 内容出现/消失动画（对标 Compose `AnimatedVisibility`）
//!
//! 用法：
//! ```ignore
//! AnimatedVisibility::new(visible)
//!     .enter(VisibilityTransition::fade_in(TweenSpec::default()).with_expand())
//!     .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
//!     .build(ctx, |ctx| {
//!         Text::new("内容").build(ctx);
//!     });
//! ```
//!
//! 机制（布局层正路——无 force_remeasure 旁路）：
//! - **进度 State<f32>**：`visible=true` → 动画 0→1（enter）；`false` → 1→0（exit）；
//!   同目标重复 push 被动画引擎 dedup
//! - **布局层**：`VisibilityPolicy` 测量期 `progress.get()` 注册 layout_dep——expand/shrink
//!   时容器高度 × 进度（下方内容平滑跟随），动画推进只重测不重组（`Animating` 写值，
//!   组件闭包不因动画值重跑）
//! - **绘制层**：`graphics_layer` 动态闭包——fade（alpha）/ slide（平移）/ scale（缩放），
//!   渲染期 `peek()` 读值，不注册依赖
//! - **exit 延迟移除**：`visible=false` → exit 动画期间内容保留在组合树（progress 1→0）；
//!   动画完成（progress≈0 且不可见）→ `removed` 标记 → 下帧 build 不 start 容器 →
//!   槽回收（内容消失）

use crate::animation::{push_animatable, AnimationSpec};
use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::modifier::{GraphicsLayerParams, Modifier};

/// Slide direction (`VisibilityTransition::slide_in/slide_out`).
/// Combined with [`SlideOffset`]: direction picks the axis+sign, offset picks the distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Slide distance (cf. Compose `slideInHorizontally(initialOffsetX: (fullWidth) -> Int)`).
/// Compose takes a lambda over content size; the common cases are a fixed pixel offset
/// or a fraction of content size — both covered here without a closure in the key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlideOffset {
    /// Fixed pixel offset (status quo 48px default — `SlideOffset::Fixed(48.0)`).
    Fixed(f32),
    /// Fraction of content size along the slide axis (1.0 = full width/height slide-in,
    /// the Compose `initialOffsetX = { fullWidth }` equivalent).
    Fraction(f32),
}

impl Default for SlideOffset {
    fn default() -> Self {
        Self::Fixed(48.0)
    }
}

/// Vertical expand anchor (cf. Compose `expandVertically(expandFrom: Alignment.Top)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFrom {
    /// Content grows downward from the top (status quo).
    #[default]
    Top,
    /// Content grows upward from the bottom.
    Bottom,
}

/// Horizontal expand anchor (cf. Compose `expandHorizontally(expandFrom: Alignment.Start)`).
/// NOTE: Start is unconditionally the left edge and End unconditionally the right edge
/// (no RTL mirroring — Compose parity backlog; RTL callers pick the anchor explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandFromH {
    /// Content grows rightward from the start (left) edge.
    #[default]
    Start,
    /// Content grows leftward from the end (right) edge.
    End,
}

/// Enter/exit transition config: fade/slide/expand/scale effects + animation spec.
/// Combine with `with_*` chaining (cf. Compose `fadeIn() + expandVertically()`).
#[derive(Debug, Clone)]
pub struct VisibilityTransition {
    /// Fade in/out (alpha 0<->1)
    pub fade: bool,
    /// Slide in/out (direction + distance)
    pub slide: Option<(SlideDirection, SlideOffset)>,
    /// Vertical expand/shrink (container height 0<->full, layout layer, followers move along)
    pub expand: bool,
    /// Vertical expand anchor
    pub expand_from: ExpandFrom,
    /// Horizontal expand/shrink (container width 0<->full, layout layer)
    pub expand_h: bool,
    /// Horizontal expand anchor
    pub expand_from_h: ExpandFromH,
    /// Scale (scale_from<->1.0 around transform_origin)
    pub scale: bool,
    /// Scale start value (status quo 0.8 — cf. Compose `scaleIn(initialScale)`)
    pub scale_from: f32,
    /// Scale pivot, normalized (0.5, 0.5) = center (cf. Compose `transformOrigin`)
    pub transform_origin: (f32, f32),
    /// Animation spec
    pub spec: AnimationSpec,
}

impl VisibilityTransition {
    fn base(spec: AnimationSpec) -> Self {
        Self {
            fade: false,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec,
        }
    }
    pub fn fade_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: true, ..Self::base(spec.into()) }
    }
    pub fn fade_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::fade_in(spec)
    }
    pub fn expand_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand: true, ..Self::base(spec.into()) }
    }
    pub fn shrink_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in(spec)
    }
    /// Horizontal expand (cf. Compose `expandHorizontally`).
    pub fn expand_in_h(spec: impl Into<AnimationSpec>) -> Self {
        Self { expand_h: true, ..Self::base(spec.into()) }
    }
    /// Horizontal shrink (cf. Compose `shrinkHorizontally`).
    pub fn shrink_out_h(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in_h(spec)
    }
    pub fn slide_in(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self { slide: Some((dir, SlideOffset::default())), ..Self::base(spec.into()) }
    }
    pub fn slide_out(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self::slide_in(dir, spec)
    }
    /// Slide with explicit distance (cf. Compose `initialOffsetX/Y` lambda).
    pub fn slide_in_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self { slide: Some((dir, offset)), ..Self::base(spec.into()) }
    }
    pub fn slide_out_offset(
        dir: SlideDirection,
        offset: SlideOffset,
        spec: impl Into<AnimationSpec>,
    ) -> Self {
        Self::slide_in_offset(dir, offset, spec)
    }
    pub fn scale_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { scale: true, ..Self::base(spec.into()) }
    }
    pub fn scale_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::scale_in(spec)
    }
    /// Combine: overlay fade in/out
    pub fn with_fade(mut self) -> Self {
        self.fade = true;
        self
    }
    /// Combine: overlay vertical expand/shrink
    pub fn with_expand(mut self) -> Self {
        self.expand = true;
        self
    }
    /// Combine: overlay vertical expand/shrink with explicit anchor
    pub fn with_expand_from(mut self, from: ExpandFrom) -> Self {
        self.expand = true;
        self.expand_from = from;
        self
    }
    /// Combine: overlay horizontal expand/shrink
    pub fn with_expand_h(mut self) -> Self {
        self.expand_h = true;
        self
    }
    /// Combine: overlay horizontal expand/shrink with explicit anchor
    pub fn with_expand_h_from(mut self, from: ExpandFromH) -> Self {
        self.expand_h = true;
        self.expand_from_h = from;
        self
    }
    /// Combine: overlay slide (default 48px distance)
    pub fn with_slide(mut self, dir: SlideDirection) -> Self {
        self.slide = Some((dir, SlideOffset::default()));
        self
    }
    /// Combine: overlay slide with explicit distance
    pub fn with_slide_offset(mut self, dir: SlideDirection, offset: SlideOffset) -> Self {
        self.slide = Some((dir, offset));
        self
    }
    /// Combine: overlay scale (default 0.8 from center)
    pub fn with_scale(mut self) -> Self {
        self.scale = true;
        self
    }
    /// Combine: overlay scale with explicit start value and pivot
    /// (cf. Compose `scaleIn(initialScale, transformOrigin)`)
    pub fn with_scale_from(mut self, scale_from: f32, transform_origin: (f32, f32)) -> Self {
        self.scale = true;
        self.scale_from = scale_from;
        self.transform_origin = transform_origin;
        self
    }
}

impl Default for VisibilityTransition {
    fn default() -> Self {
        Self {
            fade: true,
            slide: None,
            expand: false,
            expand_from: ExpandFrom::Top,
            expand_h: false,
            expand_from_h: ExpandFromH::Start,
            scale: false,
            scale_from: 0.8,
            transform_origin: (0.5, 0.5),
            spec: AnimationSpec::Tween(Default::default()),
        }
    }
}

/// 可见性容器：内容随 `visible` State 平滑出现/消失。
pub struct AnimatedVisibility {
    visible: State<bool>,
    enter: VisibilityTransition,
    exit: VisibilityTransition,
}

impl AnimatedVisibility {
    pub fn new(visible: State<bool>) -> Self {
        Self {
            visible,
            enter: VisibilityTransition::default(),
            exit: VisibilityTransition::default(),
        }
    }

    /// 进入过渡（visible 变 true 时播放）
    pub fn enter(mut self, t: VisibilityTransition) -> Self {
        self.enter = t;
        self
    }

    /// 退出过渡（visible 变 false 时播放——动画期间内容保留）
    pub fn exit(mut self, t: VisibilityTransition) -> Self {
        self.exit = t;
        self
    }

    /// 构建动画容器（对比测试：临时去掉 #[composable]）
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let visible = self.visible;
        let enter = self.enter;
        let exit = self.exit;
        // 依赖注册（get 而非 peek）：visible 变化 → 重组 → 本 build 重跑（点击生效）；
        // progress/removed 变化 → 每帧重组（removed 检测执行 / 移除生效）。
        // 动画期间每帧重组仅重跑本 build（O(1)）——容器槽无依赖 → Skip → content 不重跑。
        let vis = visible.get();
        // 内部状态（remember——语句级 key 稳定，跨重组保留）
        let progress: State<f32> = ctx.remember(|| 0.0);
        let removed: State<bool> = ctx.remember(|| !vis);
        let _p = progress.get(); // 动画推进 → 重组（removed 完成检测执行）
        let _r = removed.get(); // removed 标记 → 下帧重组（return 生效）
        // 已移除且重新显示 → 重置（notify 触发重组，本帧继续走 start）
        if removed.peek() && vis {
            removed.set(false);
        }
        // exit 完成检测：不可见且进度≈0 → 标记移除（build 每帧跑——动画值
        // Animating 写每帧 notify → pending → 下帧 compose）
        if !vis && progress.peek() < 0.001 && !removed.peek() {
            removed.set(true);
        }
        if removed.peek() {
            return; // 已移除：不 start 容器 → 槽 visited=false → 物化回收
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
            let mut params = GraphicsLayerParams::default();
            if cfg.fade {
                params.alpha = p;
            }
            if cfg.scale {
                let s = cfg.scale_from + (1.0 - cfg.scale_from) * p;
                params.scale_x = s;
                params.scale_y = s;
                params.transform_origin =
                    crate::modifier::TransformOrigin(cfg.transform_origin.0, cfg.transform_origin.1);
            }
            if let Some((dir, offset)) = cfg.slide {
                let dist = match offset {
                    SlideOffset::Fixed(px) => px,
                    SlideOffset::Fraction(f) => {
                        let (w, h) = cs.peek();
                        let full = match dir {
                            SlideDirection::Left | SlideDirection::Right => w,
                            SlideDirection::Up | SlideDirection::Down => h,
                        };
                        f * full
                    }
                };
                let off = (1.0 - p) * dist;
                match dir {
                    SlideDirection::Left => params.translation_x = -off,
                    SlideDirection::Right => params.translation_x = off,
                    SlideDirection::Up => params.translation_y = -off,
                    SlideDirection::Down => params.translation_y = off,
                }
            }
            params
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
                .clip(crate::modifier::Shape::Rectangle)
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
                content(ctx);
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
    content_size: crate::core::state::Backchannel<(f32, f32)>,
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
            placements.push(Placement { size, position: Point::ZERO });
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
    use crate::core::composer::Composer;
    use crate::core::state::State;
    use crate::layout::constraints::Constraints;
    use crate::ui::layout_components::Column;

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
                        TextLeaf::new("AV content").build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 推进动画帧（真实时间驱动 Tween）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..8 {
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
    struct TextLeaf {
        label: &'static str,
    }
    impl TextLeaf {
        fn new(label: &'static str) -> Self {
            Self { label }
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
            content_size: crate::core::state::Backchannel::new((0.0, 0.0)),
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
        use crate::ui::layout_components::Column;
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
            match offset {
                SlideOffset::Fixed(px) => px,
                SlideOffset::Fraction(f) => {
                    f * match dir {
                        SlideDirection::Left | SlideDirection::Right => w,
                        SlideDirection::Up | SlideDirection::Down => h,
                    }
                }
            }
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
