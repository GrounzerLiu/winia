//! `AnimatedContent` — targetState 驱动的内容切换过渡（对标 Compose `AnimatedContent`）
//!
//! 用法：
//! ```ignore
//! AnimatedContent::new(page)                        // State<Page>
//!     .enter(...) / .exit(...)                      // 进出过渡（默认 = Compose 的默认）
//!     .size_animation(SpringSpec::bouncy().into())  // sizeTransform
//!     .build(ctx, |ctx, p| {                        // 内容闭包接收该代的目标值
//!         Text::new(format!("Page: {:?}", p)).build(ctx);
//!     });
//! ```
//!
//! 机制（**两代同场**——对齐 Compose 的 `currentlyVisible`）：
//! - 内部状态：`current`（新的一代）+ `previous: Option<T>`（离场中的旧的一代）
//!   + 三个进度：`enter`（新的一代）、`exit`（旧的一代）、`size`（容器尺寸）
//! - 切换：target 变化 → **立刻**把旧目标搬进 `previous`、新目标写进 `current`，
//!   两代各自起一个容器槽同时组合（不是"旧内容淡完再换"）——两代各按自己的过渡
//!   淡出/淡入，在时间上重叠
//! - **绘制顺序**：旧的一代先组合、新的一代后组合 → 新内容在上
//!   （`AnimatedContent.kt:189`："the incoming target content will be on top, as it will be
//!   placed last"）
//! - **sizeTransform**：容器尺寸 = lerp(旧内容尺寸, 新内容尺寸, size)——由**新的一代**
//!   驱动（Compose 同规则），且用**独立于 fade 的**进度与规格（Compose 的
//!   `SizeTransform(sizeAnimationSpec)` 默认 spring）；布局层每帧重测（layout_dep）
//! - **clip**：容器默认裁剪到动画尺寸（Compose `SizeTransform(clip = true)` 的默认），
//!   所以尺寸长大时新内容不会提前溢出
//! - `exit` 触底 → 旧的一代拆除；`size` 触顶 → 容器回到"就是内容尺寸"，二者独立
//!   （默认 fadeOut 90ms 比 size 的 spring 短——退出跑完了尺寸可能还在动）
//!
//! 与 Compose 的已知差异：
//! - **没有 90ms 延迟**。Compose 默认 `fadeIn(tween(220, delayMillis = 90))`，而 winia 的
//!   `TweenSpec` 没有 delay 字段。结果是进入提前开始、总时长 220ms（Compose 310ms）。
//! - **只保留一代离场内容**。Compose 的 `currentlyVisible` 是列表——过渡中再次切换会有三代
//!   同场；这里 `previous` 是单个 `Option`，快速连切时中间那代是硬切。
//! - **没有 `contentAlignment`**。Compose 的 `Alignment` 是二维的（默认 `TopStart`），winia 的
//!   `Alignment` 只是单轴（Start/End/Center/Stretch），所以两代都按容器左上放置——与 Compose
//!   的**默认值**一致，只是调不了。

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{interpolator, push_animatable, AnimationSpec, SpringSpec, TweenSpec};
use crate::layout::box_layout::BoxLayout;
use crate::layout::{MeasurePolicy, Placement};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::{Backchannel, State};
use crate::unit::{Offset, Size};
use std::time::Duration;

/// 两代各自的稳定 key 基——`ctx.key` 作用域。位置 key 不行：旧的一代离场后，
/// 新的一代的语句级 key 会往前挪一格、继承旧一代的槽。
const GENERATION_PREV: u64 = 0xA11C_0000_0000_0001;
const GENERATION_CURRENT: u64 = 0xA11C_0000_0000_0002;

/// Compose `tween()` 的默认缓动 `FastOutSlowInEasing` = `CubicBezierEasing(0.2, 0, 0, 1)`。
fn fast_out_slow_in() -> interpolator::CubicBezier {
    interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0)
}

/// Compose `AnimatedContent` 的默认进入过渡：`fadeIn(tween(220)) + scaleIn(0.92)`。
fn default_enter() -> VisibilityTransition {
    VisibilityTransition::fade_in(TweenSpec::new(Duration::from_millis(220), fast_out_slow_in()))
        .with_scale_from(0.92, (0.5, 0.5))
}

/// Compose 的默认退出过渡：`fadeOut(tween(90))` —— 快出慢进。
fn default_exit() -> VisibilityTransition {
    VisibilityTransition::fade_out(TweenSpec::new(Duration::from_millis(90), fast_out_slow_in()))
}

/// 内容切换过渡（两代同场：enter/exit 各自成层 + sizeTransform）。
pub struct AnimatedContent<T> {
    target: State<T>,
    enter: VisibilityTransition,
    exit: VisibilityTransition,
    size_spec: AnimationSpec,
    clip: bool,
    modifier: Modifier,
    /// 内容身份（对标 Compose `contentKey`）：键相同 = 同一份内容——新状态直接换进去、
    /// 不播过渡；`None` 时按 `PartialEq` 值比较。调用方自己把状态映射成 `u64`。
    content_key: Option<Box<dyn Fn(&T) -> u64 + Send + Sync>>,
}

impl<T> AnimatedContent<T> {
    /// 键相同就是同一份内容：状态换了但不播过渡（Compose `contentKey` 的语义）。
    /// 键不同的两个状态才会走 enter/exit。
    pub fn content_key(mut self, f: impl Fn(&T) -> u64 + Send + Sync + 'static) -> Self {
        self.content_key = Some(Box::new(f));
        self
    }
}

/// 容器 MeasurePolicy：尺寸 = lerp(旧内容尺寸, 新内容尺寸, size)。
///
/// 两代都在、或尺寸动画还没走完时容器插值；静止时容器就是内容的尺寸。
#[derive(Debug)]
struct ContentSizePolicy {
    prev_size: State<Option<(f32, f32)>>,
    /// 新旧一代的内容尺寸——`last_size` 供切换时锁为 `prev_size`（sizeTransform 起点），
    /// `content_size` 供 `SlideOffset::Fraction` 的过渡在绘制期解析距离。
    /// 两个都是 Backchannel：只在切换那一刻/绘制期读，每帧 notify 会白白重组调用方。
    last_size: Backchannel<Option<(f32, f32)>>,
    content_size: Backchannel<Option<(f32, f32)>>,
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
        // 新一代是最后一个子节点（后组合 = 画在上层）。
        let incoming = measured.last().copied().unwrap_or(Size::new(0.0, 0.0));
        self.last_size.set(Some((incoming.width, incoming.height)));
        self.content_size.set(Some((incoming.width, incoming.height)));
        // 布局期读 size（get 注册 layout_dep → 动画期间每帧重测——容器尺寸跟随切换动画；
        // peek 不注册 → 尺寸卡首帧值不动）
        let p = self.size.get();
        let (w, h) = match self.prev_size.peek() {
            Some((pw, ph)) if p < 0.999 => (
                pw + (incoming.width - pw) * p,
                ph + (incoming.height - ph) * p,
            ),
            _ => (incoming.width, incoming.height),
        };
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

impl<T: Clone + PartialEq + 'static> AnimatedContent<T> {
    /// Compose 的默认过渡：`fadeIn(220) + scaleIn(0.92) togetherWith fadeOut(90)`，
    /// sizeTransform 用 `spring()`，容器裁剪到动画尺寸。
    pub fn new(target: State<T>) -> Self {
        Self {
            target,
            enter: default_enter(),
            exit: default_exit(),
            size_spec: AnimationSpec::Spring(SpringSpec::default()),
            clip: true,
            modifier: Modifier::new(),
            content_key: None,
        }
    }

    /// 进入过渡——作用于新来的那一代。
    pub fn enter(mut self, t: VisibilityTransition) -> Self {
        self.enter = t;
        self
    }

    /// 退出过渡——作用于离场的那一代。
    pub fn exit(mut self, t: VisibilityTransition) -> Self {
        self.exit = t;
        self
    }

    /// sizeTransform 的动画规格（对标 Compose `SizeTransform(sizeAnimationSpec)`）。
    pub fn size_animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.size_spec = spec.into();
        self
    }

    /// 是否把容器裁剪到动画尺寸（对标 Compose `SizeTransform(clip)`，默认 true）。
    pub fn clip(mut self, v: bool) -> Self {
        self.clip = v;
        self
    }

    /// 容器自身的 modifier（对标 Compose 的 `modifier` 参数）。
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = m;
        self
    }

    /// 构建内容切换容器。
    /// ⚠ 不宏化：内部 `exit.get()`/`size.get()` 依赖必须注册到**调用点 scope**
    /// （父容器每帧重跑 → 退出完成/尺寸完成检测执行）——宏化封闭内部 scope 会切断
    /// 失效传播（同 animated_visibility/crossfade 宏化回归）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx, T)) {
        // 依赖注册：target 变化 → 外层 scope 重组（本 build 重跑，容器槽随之 dirty → Enter）
        let target = self.target.get();
        // 内部状态（remember——语句级 key 稳定，跨重组保留）
        let current: State<T> = ctx.remember(|| target.clone());
        let previous: State<Option<T>> = ctx.remember(|| None);
        let enter: State<f32> = ctx.remember(|| 1.0);
        let exit: State<f32> = ctx.remember(|| 1.0);
        let size: State<f32> = ctx.remember(|| 1.0);
        let prev_size: State<Option<(f32, f32)>> = ctx.remember(|| None);
        // 每一代有自己的 key：固定 key 会让"新的一代"复用上一代的槽（旧内容离场后，
        // 新来的那代继承它的 `remember` 状态——实测就是树里显示旧内容）。
        // Compose 用 `key(contentKey(state))` 做同一件事。
        let generation: State<u64> = ctx.remember(|| 0);
        let last_size = ctx.remember_backchannel(|| None);
        let content_size = ctx.remember_backchannel(|| None);

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
            content_size: content_size.clone(),
            size: size.clone(),
        };
        let key = ctx.next_key();
        let ac_status = ctx.start_restartable_group(key, modifier, policy);
        match ac_status {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Everything the frame needs happens INSIDE this branch, and that is load-bearing:
                // a write made in the CALLER's scope (before the container is started) marks the
                // container's slot dirty while the pass that is consuming it is already running,
                // and the mark is gone by the next frame — so the container skipped and replayed
                // its previous children while `previous`/`current` had already moved on. Measured:
                // a switch made mid-transition left the old pair on screen for four frames and then
                // hard-cut to the incoming generation alone. Here the container re-enters because
                // its own dependencies changed, so the swap and the children it composes happen in
                // one frame.
                let target_now = self.target.get();
                // 完成检测要每帧跑一次——订阅两个进度（fadeOut 90ms 与 size 的 spring 时长不同，
                // 两个都可能先完成），同时这正是"动画期间容器每帧 Enter"的来源
                let _x = exit.get();
                let _s = size.get();

                if current.peek() != target_now {
                    // 内容身份：键相同 = 同一份内容——状态换进去但不播过渡（Compose `contentKey`）
                    let same_content = match &self.content_key {
                        Some(k) => {
                            let shown_now = current.peek();
                            k(&shown_now) == k(&target_now)
                        }
                        None => false,
                    };
                    if same_content {
                        // 同一份内容：只换值（不播过渡）。`current.get()` 在下面订阅着 →
                        // 内容以新值重跑，previous/三个进度都不动
                        current.set(target_now.clone());
                    } else {
                        // 立刻换代：旧的一代进 previous（它自己的 exit 从 1 开始往下走），
                        // 新的一代进 current。不等待——两代同时在场上，这才是 AnimatedContent。
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
                        // 旧的一代走完了——它的槽下一帧不再组合（previous.get() 是依赖）
                        previous.set(None);
                    }
                }
                if prev_size.peek().is_some() {
                    push_animatable(size.clone(), 1.0, self.size_spec.clone());
                    if size.peek() >= 0.999 {
                        // 尺寸动画收尾：容器回到"就是内容尺寸"，下一帧不再插值
                        prev_size.set(None);
                    }
                }

                // previous 变化（Some → None）→ 容器槽 dirty → Enter → 旧的一代拆除
                let outgoing = previous.get();
                if let Some(outgoing) = outgoing {
                    let t = self.exit.clone();
                    let g = exit.clone();
                    let cs = content_size.clone();
                    let layer = Modifier::new().graphics_layer(move || {
                        t.layer_params(g.peek(), cs.peek().unwrap_or((0.0, 0.0)))
                    });
                    let gen_id = generation.peek();
                    ctx.key((GENERATION_PREV, gen_id), |ctx| {
                        let mkey = ctx.next_key();
                        if let GroupStatus::Enter =
                            ctx.start_restartable_group(mkey, layer, BoxLayout::default())
                        {
                            content(ctx, outgoing.clone());
                        }
                        ctx.end_restartable_group();
                    });
                }
                // 新的一代最后组合 → 画在旧的一代上面
                let shown = current.peek().clone();
                let t = self.enter.clone();
                let g = enter.clone();
                let cs = content_size.clone();
                let layer = Modifier::new().graphics_layer(move || {
                    t.layer_params(g.peek(), cs.peek().unwrap_or((0.0, 0.0)))
                });
                let gen_id = generation.peek();
                ctx.key((GENERATION_CURRENT, gen_id), |ctx| {
                    let _c = current.get();
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

    /// 测试用叶子：宽度反映 target（page=0 → 50，其他 → 200）
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

    /// 两代同场时容器有两个子节点，静止时只有一个——容器 = 有两个子节点的那个节点。
    fn container_child_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize) -> usize {
            let n = &nodes[idx];
            if n.children.is_empty() {
                return 0;
            }
            // 容器：子节点数 1..=2 且每个子节点自己还有孩子（那层是 content 的包装）
            if n.children.len() <= 2 && n.children.iter().all(|&c| !nodes[c].children.is_empty()) {
                return n.children.len();
            }
            walk(nodes, n.children[0])
        }
        walk(nodes, root)
    }

    /// 每个叶子的宽度，升序——两代同场时两个宽度都在。
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

    /// 集成：target 切换 → 两代同场（旧 50 淡出 + 新 200 淡入）→ 容器尺寸从旧动画到新
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

        // 首帧：显示 target 0（容器宽 50，一代）
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "初始显示 target 0");
        assert_eq!(container_child_count(&composer), 1, "静止时只有一代");

        // 切换：两代同场，容器尺寸随 size 进度从 50 插值到 200
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
        assert!(saw_both, "过渡期间两代同时在场上");
        assert!(
            mid_w > 50.0 && mid_w < 200.0,
            "尺寸动画中途容器宽度应在 50..200（实际 {mid_w}）"
        );

        // 推进到完成——一代在场上，容器宽 = 新内容宽 200
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "旧的一代已拆除");
        assert_eq!(container_width_of(&composer), 200.0, "完成后容器宽度 = 新内容宽");

        // 切回 target 0
        target.set(0);
        recompose(&mut composer);
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![50.0], "切回后只剩一代");
        assert_eq!(container_width_of(&composer), 50.0, "容器宽度回到 50");
    }

    /// 容器自身的测量宽度：两代同场时它是插值结果，取子节点里最宽的那个节点的父节点。
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

    /// sizeTransform 独立生效（回归：与 fade 共享进度会被 same_target 去重跳过——
    /// size_spec 从未生效）。size_animation 用 Snap：切换那一刻容器宽度立即 = 新内容宽，
    /// 而 fade 还在慢慢走。
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
        assert_eq!(leaf_widths(&composer), vec![50.0], "初始显示 target 0");

        target.set(7);
        recompose(&mut composer);
        for _ in 0..20 {
            advance_one(&mut composer);
        }
        // 此刻 fade 才走了 ~320ms/600ms——若 size 独立（Snap）容器宽应已 = 200；
        // 若两者共享进度（旧 bug）→ 宽还在插值中途
        let w = container_width_of(&composer);
        assert!(
            (w - 200.0).abs() < 0.5,
            "sizeTransform(Snap) 应在淡入中途即到达 200（实际 {w}）"
        );
    }

    /// Compose 的 `AnimatedContent` 在过渡期间把**离场的那一代**留在组合树里
    /// （`currentlyVisible`），并把新的一代画在上面（`AnimatedContent.kt:189`）。
    /// 旧实现是"淡出到底 → 换内容 → 从空淡入"，任一时刻只有一代。
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
                assert_eq!(container_child_count(&composer), 2, "两个子节点：旧 + 新");
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

    /// `contentKey`：键相同的两个状态是"同一份内容"——值换进去但不播过渡（Compose 的
    /// `contentKey` 语义），所以不会出现两代同场。
    #[test]
    fn the_same_content_key_updates_in_place_without_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    // 0..=9 是同一份内容（同一页的两次渲染），10 才换内容
                    .content_key(|p| if *p < 10 { 0 } else { 1 })
                    .build(ctx, |ctx, page| {
                        // 宽度跟 KEY 走，而不是跟值走——同一份内容换值不该改尺寸
                        let w = if page < 10 { 50.0 } else { 200.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initial");

        // 键不变：值换掉，内容以新值重跑，但没有第二代入场
        target.set(3);
        recompose(&mut composer);
        assert_eq!(container_child_count(&composer), 1, "no transition for the same key");
        assert_eq!(leaf_widths(&composer), vec![50.0]);

        // 键变了：正常过渡——两代同场
        target.set(10);
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0, 200.0], "a new key animates");
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
