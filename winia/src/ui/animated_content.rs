//! `AnimatedContent` — targetState 驱动的内容切换过渡（对标 Compose `AnimatedContent`）
//!
//! 用法：
//! ```ignore
//! AnimatedContent::new(page)                        // State<Page>
//!     .animation(TweenSpec::default())              // fade 进度动画
//!     .size_animation(SpringSpec::bouncy().into())  // sizeTransform
//!     .build(ctx, |ctx, p| {                        // 内容闭包接收当前显示目标
//!         Text::new(format!("Page: {:?}", p)).build(ctx);
//!     });
//! ```
//!
//! 机制（单内容世代——组合引擎无 Compose 双世代 outgoing 组合）：
//! - 内部状态：`current: State<T>`（显示中目标）+ `progress: State<f32>`（fade 进度）
//!   + `prev_size`（切换前内容尺寸快照）+ `last_size`（上帧内容尺寸）
//! - 切换流程：target 变化（`get()` 注册依赖→重组）→ 当前内容淡出（progress 1→0）
//!   → 淡出完成（progress<0.001）→ 锁定 `prev_size` = 旧内容尺寸 → `current.set(target)`
//!   → 容器槽 Enter → 内容重建（新 target）→ 淡入（progress 0→1）
//! - **sizeTransform**：容器尺寸 = lerp(prev_size, 新内容尺寸, progress)——布局层
//!   每帧重测（layout_dep），内容尺寸跳变被容器平滑吸收（对齐 Compose sizeTransform）
//! - **fade**：绘制层 alpha = progress（渲染期 peek——零重排零重组）
//! - **每帧重组重跑调用方组件闭包**：progress.get() 注册在调用方槽——动画推进
//!   notify → 调用方组件闭包重跑；容器槽仅 current 变化时 Enter（内容重建）

use crate::animation::{push_animatable, AnimationSpec};
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::{MeasurePolicy, Placement, Size};
use crate::modifier::{GraphicsLayerParams, Modifier};

/// 内容切换过渡（fade + sizeTransform）
pub struct AnimatedContent<T> {
    target: State<T>,
    spec: AnimationSpec,
    size_spec: AnimationSpec,
}

/// 容器 MeasurePolicy：尺寸 = lerp(prev_size, 新内容尺寸, progress)
#[derive(Debug)]
struct ContentSizePolicy {
    prev_size: State<Option<(f32, f32)>>,
    last_size: State<Option<(f32, f32)>>,
    progress: State<f32>,
}

impl MeasurePolicy for ContentSizePolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (Size, Vec<Placement>) {
        let child = children[0];
        let (child_size, _) = crate::layout::node::measure_node(nodes, policies, child, constraints.loosen());
        // 布局期读 progress（get 注册 layout_dep → 动画期间每帧重测——
        // 容器高度跟随内容切换动画；peek 不注册 → 高度卡首帧值不动）
        let p = self.progress.get();
        let child_size = (child_size.width, child_size.height);
        // 记录上帧内容尺寸（切换瞬间锁定 prev_size 用）
        self.last_size.set(Some(child_size));
        let (w, h) = match self.prev_size.peek() {
            Some((pw, ph)) => (
                pw + (child_size.0 - pw) * p,
                ph + (child_size.1 - ph) * p,
            ),
            None => child_size,
        };
        (
            Size::new(constraints.constrain_width(w), constraints.constrain_height(h)),
            vec![Placement {
                size: Size::new(child_size.0, child_size.1),
                position: crate::layout::node::Point::new(0.0, 0.0),
            }],
        )
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}

impl<T: Clone + PartialEq + 'static> AnimatedContent<T> {
    pub fn new(target: State<T>) -> Self {
        Self {
            target,
            spec: AnimationSpec::Tween(Default::default()),
            size_spec: AnimationSpec::Tween(Default::default()),
        }
    }

    /// fade 进度动画规格（默认 300ms 线性 tween）
    pub fn animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.spec = spec.into();
        self
    }

    /// sizeTransform 动画规格（默认同 fade）
    pub fn size_animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.size_spec = spec.into();
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx, T)) {
        // 依赖注册：target 变化 → 外层 scope 重组 → 本 build 重跑（切换启动）
        let target = self.target.get();
        // 内部状态（remember——语句级 key 稳定，跨重组保留）
        let current: State<T> = ctx.remember(|| target.clone());
        let progress: State<f32> = ctx.remember(|| 1.0);
        // ⚠ sizeTransform 用独立 State——与 fade 共享 progress 会被
        // push_animatable 的 same_target 去重跳过（同 state 同目标第二次 push
        // 被忽略）→ size_spec（Spring）从未生效。
        let size_progress: State<f32> = ctx.remember(|| 1.0);
        let prev_size: State<Option<(f32, f32)>> = ctx.remember(|| None);
        let last_size: State<Option<(f32, f32)>> = ctx.remember(|| None);
        // 动画推进 → 外层重组（淡出完成检测执行）
        let _p = progress.get();
        let cur = current.peek().clone();
        // 淡出完成检测：切换中且进度≈0 → 锁定旧尺寸 → 切换 current（notify →
        // 下帧容器槽 Enter → 内容重建）
        if cur != target && progress.peek() < 0.001 {
            prev_size.set(last_size.peek()); // 旧内容尺寸（上帧测量）
            current.set(target.clone());
        }
        // 动画目标用**切换后**的 current（切换帧 cur 是 set 前的旧值——用旧值
        // 算 goal 会得 0 → 淡入永不启动 → progress 卡 0 卡片透明）
        let shown_now = current.peek().clone();
        let goal = if shown_now == target { 1.0 } else { 0.0 };
        // 淡入完成 → 重置 prev_size（下次切换重新锁定；残留会让下一次淡出
        // 阶段错误地 lerp（旧尺寸收缩）而非保持当前内容尺寸）
        if goal >= 1.0 && progress.peek() >= 0.999 && prev_size.peek().is_some() {
            prev_size.set(None);
        }
        push_animatable(progress.clone(), goal, self.spec.clone());
        push_animatable(size_progress.clone(), goal, self.size_spec.clone());
        // 绘制层：alpha = progress（淡出 1→0 / 淡入 0→1）——渲染期 peek 不注册依赖
        let g = progress.clone();
        let gfx = move || {
            let mut params = GraphicsLayerParams::default();
            params.alpha = g.peek();
            params
        };
        let modifier = Modifier::new().graphics_layer(gfx);
        let policy = ContentSizePolicy { prev_size, last_size, progress: size_progress };
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 注册容器槽依赖：current 变化 → 容器槽 dirty → Enter → 内容重建
                let _c = current.get();
                // ⚠ 必须读 current 最新值而非外层 cur：切换瞬间 set 发生在本帧
                // 容器槽 start 之前——外层 cur 是 set 前读取的旧值，会导致内容
                // 永远停留在旧 target（容器槽之后 Skip——current 不再变化）
                content(ctx, current.peek().clone());
            }
        }
        ctx.end_restartable_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::core::state::State;
    use crate::layout::constraints::Constraints;
    use crate::layout::node::LayoutNode;

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

    /// 查容器节点（Crossfade/AnimatedContent 的 start_restartable_group 根）测量宽度
    fn container_width(composer: &Composer) -> f32 {
        let Some(root) = composer.layout_root_idx() else { return -1.0 };
        let nodes = composer.arena_nodes();
        fn find_content_root(nodes: &[LayoutNode], idx: usize, depth: usize) -> f32 {
            let n = &nodes[idx];
            if n.children.len() == 1 && !n.children.is_empty() && nodes[n.children[0]].children.is_empty() {
                n.measured_size.width // 容器 = 有且仅有一个叶子子节点
            } else if n.children.is_empty() {
                n.measured_size.width
            } else {
                find_content_root(nodes, n.children[0], depth + 1)
            }
        }
        find_content_root(nodes, root, 0)
    }

    /// 集成：target 切换 → 淡出 → 内容重建 → 淡入 + 容器尺寸从旧平滑动画到新
    #[test]
    fn animated_content_switches_with_size_transform() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .animation(crate::animation::TweenSpec::default())
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

        // 首帧：显示 target 0（容器宽 50）
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 50.0, "初始显示 target 0");

        // 切换 target → 淡出（~300ms）→ 完成 → 内容重建（宽 200）→ 淡入 + 尺寸动画
        target.set(7);
        recompose(&mut composer);
        // 淡出 ~20 帧（300ms）→ 切换完成 → 淡入开始 2 帧
        for _ in 0..22 {
            advance_one(&mut composer);
        }
        // 尺寸动画中途：容器宽度应在 50..200 之间（从旧尺寸平滑过渡）
        let mid_w = container_width(&composer);
        assert!(
            mid_w > 50.0 && mid_w < 200.0,
            "尺寸动画中途容器宽度应在 50..200（实际 {mid_w}）"
        );
        // 推进到完成——容器宽 = 新内容宽 200
        for _ in 0..20 {
            advance_one(&mut composer);
        }
        assert_eq!(container_width(&composer), 200.0, "完成后容器宽度 = 新内容宽");

        // 切回 target 0 → 淡出（~300ms）→ 淡入（~300ms）→ 尺寸动画回 50
        target.set(0);
        recompose(&mut composer);
        for _ in 0..48 {
            advance_one(&mut composer);
        }
        assert_eq!(container_width(&composer), 50.0, "切回后容器宽度 = 50");
    }

    /// sizeTransform 独立生效（回归：与 fade 共享 progress 会被 same_target
    /// 去重跳过——size_spec 从未生效）。size_animation 用 Snap：淡入阶段容器
    /// 宽度应立即 = 新内容宽（而非 fade 的 lerp 中途值）
    #[test]
    fn size_animation_independent_of_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .animation(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    ))
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
        assert_eq!(container_width(&composer), 50.0, "初始显示 target 0");

        // 切换 → 淡出（600ms fade）→ 切换完成 → 淡入（600ms）
        target.set(7);
        recompose(&mut composer);
        for _ in 0..45 {
            advance_one(&mut composer);
        }
        // 此刻 fade 淡入中（~300ms/600ms）——若 size 独立（Snap）容器宽应已 = 200；
        // 若共享 progress（旧 bug）→ size 走 600ms Tween → 宽是 lerp 中途值（<200）
        let w = container_width(&composer);
        assert!(
            (w - 200.0).abs() < 0.5,
            "sizeTransform(Snap) 应在淡入中途即到达 200（实际 {w}）"
        );
    }
}
