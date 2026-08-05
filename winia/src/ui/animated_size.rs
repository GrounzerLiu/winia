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

use crate::animation::{push_animatable, AnimationSpec};
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size as LayoutSize};
use crate::modifier::Modifier;
use crate::unit::Size;

/// 尺寸变化自动动画容器
pub struct AnimatedSize {
    spec: AnimationSpec,
}

impl AnimatedSize {
    /// 尺寸动画规格（默认 300ms 线性 tween）
    pub fn new(spec: impl Into<AnimationSpec>) -> Self {
        Self { spec: spec.into() }
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 动画尺寸 + 上次目标（remember——语句级 key 稳定，跨重组保留）
        // target 不能放 policy 实例（每次 build 重建→Enter 时重置为 None→首帧
        // 逻辑重复→每次 Enter 都直接跳转无动画）
        let size: State<Size> = ctx.remember(|| Size::new(0.0, 0.0));
        let target: State<Option<Size>> = ctx.remember(|| None);
        let policy = SizePolicy {
            size: size.clone(),
            target: target.clone(),
            spec: self.spec,
        };
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), policy) {
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
    /// 上次目标尺寸（None = 首帧——直接跳转无动画）——State 而非 RefCell：
    /// policy 实例每次 build 重建，State 跨重组保留
    target: State<Option<Size>>,
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
    ) -> (LayoutSize, Vec<Placement>) {
        // 测量子内容（取最大宽高——Stack 语义）
        let mut child_w = 0.0f32;
        let mut child_h = 0.0f32;
        let mut placements = Vec::with_capacity(children.len());
        for &c in children {
            let (size, _) = measure_node(nodes, policies, c, constraints);
            child_w = child_w.max(size.width);
            child_h = child_h.max(size.height);
            placements.push(Placement { size, position: Point::ZERO });
        }
        // 目标变化 → 启动尺寸动画（首帧 Snap 直接跳转）
        let goal = Size::new(child_w, child_h);
        let prev = self.target.get();
        if prev != Some(goal) {
            if prev.is_none() {
                // 首帧：无动画直接跳转（Compose 语义）
                self.size.set_no_wake(goal);
            } else {
                push_animatable(self.size.clone(), goal, self.spec.clone());
            }
            self.target.set_no_wake(Some(goal));
        }
        // layout_dep：动画推进每帧重测本节点（get 注册——值变化才 notify）
        let cur = self.size.get();
        (LayoutSize::new(cur.width, cur.height), placements)
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
    use crate::layout::node::LayoutNode;

    /// 测试用叶子：宽度由外部 State 驱动（50 / 200）
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

    #[test]
    fn animated_size_tracks_content_change() {
        let mut composer = Composer::new();
        // 内容宽度 State 必须走 remember（owner queue——State::new 的 notify 不推送）
        let holder = std::cell::RefCell::new(None::<State<f32>>);

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let w: State<f32> = ctx.remember(|| 50.0);
                *holder.borrow_mut() = Some(w.clone());
                AnimatedSize::new(crate::animation::TweenSpec::default()).build(ctx, |ctx| {
                    SizedLeaf { w: w.get() }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..10 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(40));
                recompose(composer);
            }
        };

        // 首帧：内容宽 50 → 容器直接 50（无动画）
        recompose(&mut composer);
        assert_eq!(container_width(&composer), 50.0, "首帧直接跳转到内容尺寸");

        // 内容变 200 → 容器动画过渡 → 收敛 200
        let w = holder.borrow().as_ref().unwrap().clone();
        w.set(200.0);
        recompose(&mut composer);
        let mid = container_width(&composer);
        assert!(mid < 200.0, "动画过渡中：宽度应介于旧新之间（mid={}）", mid);
        advance(&mut composer);
        assert_eq!(container_width(&composer), 200.0, "动画完成后到达新尺寸");
    }
}
