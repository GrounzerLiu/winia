//! `Crossfade` — 内容切换过渡（对标 Compose `Crossfade`）
//!
//! 用法：
//! ```ignore
//! Crossfade::new(page)                            // State<Page>
//!     .animation(TweenSpec::default())
//!     .build(ctx, |ctx, p| {                      // 内容闭包接收当前显示目标
//!         Text::new(format!("Page: {:?}", p)).build(ctx);
//!     });
//! ```
//!
//! 机制（顺序淡入淡出——组合引擎单内容世代，无 Compose 双世代 outgoing 组合）：
//! - **内部状态**：`current: State<T>`（显示中目标）+ `progress: State<f32>`（动画进度）
//! - **切换流程**：target 变化（`get()` 注册依赖→重组）→ 当前内容淡出（progress 1→0，
//!   绘制层 alpha=progress）；淡出完成（progress<0.001）→ `current.set(target)`（notify）
//!   → 容器槽 Enter → 内容重建（新 target）→ 淡入（progress 0→1）
//! - **不触发重排**：布局尺寸 = 内容尺寸（BoxLayout Stack 语义），动画纯绘制层
//!   （graphics_layer 动态闭包渲染期 peek）——零重排零重组
//! - **每帧重组重跑调用方组件闭包**：progress.get() 注册在调用方槽——动画推进
//!   notify → 调用方组件闭包重跑（本 build 重执行）；容器槽仅 current 变化时
//!   Enter（内容重建），动画期间内容子树保持 Skip 不重建

use crate::animation::{push_animatable, AnimationSpec};
use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::box_layout::BoxLayout;
use crate::modifier::{GraphicsLayerParams, Modifier};

/// 内容切换过渡（顺序淡入淡出）
pub struct Crossfade<T> {
    target: State<T>,
    spec: AnimationSpec,
}

impl<T: Clone + PartialEq + 'static> Crossfade<T> {
    pub fn new(target: State<T>) -> Self {
        Self {
            target,
            spec: AnimationSpec::Tween(Default::default()),
        }
    }

    /// 切换动画规格（默认 300ms 线性 tween）
    pub fn animation(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.spec = spec.into();
        self
    }

    /// 构建交叉淡入淡出容器。
    /// ⚠ 不宏化：内部 progress.get() 依赖必须注册到**调用点 scope**（父容器
    /// 每帧重跑 → 淡出完成检测执行）——宏化会封闭内部 scope，父容器感知不到
    /// 进度变化 → Column Skip → 检测冻结 → 内容残留（animated_visibility
    /// 宏化回归同因）。内部 remember 靠调用点语句 base（稳定）。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx, T)) {
        // 依赖注册：target 变化 → 外层 scope 重组 → 本 build 重跑（切换启动）
        let target = self.target.get();
        // 内部状态（remember——语句级 key 稳定，跨重组保留）
        let current: State<T> = ctx.remember(|| target.clone());
        let progress: State<f32> = ctx.remember(|| 1.0);
        // 动画推进 → 外层重组（淡出完成检测执行）
        let _p = progress.get();
        let cur = current.peek().clone();
        // 淡出完成检测：切换中且进度≈0 → 切换 current（notify → 下帧容器槽 Enter → 内容重建）
        if cur != target && progress.peek() < 0.001 {
            current.set(target.clone());
        }
        // 动画目标：切换中 → 淡出（0）；显示中 → 淡入（1）——同目标 dedup 跳过
        let goal = if cur == target { 1.0 } else { 0.0 };
        push_animatable(progress.clone(), goal, self.spec.clone());
        // 绘制层：alpha = progress（淡出 1→0 / 淡入 0→1）——渲染期 peek 不注册依赖
        let g = progress.clone();
        let gfx = move || {
            let mut params = GraphicsLayerParams::default();
            params.alpha = g.peek();
            params
        };
        let modifier = Modifier::new().graphics_layer(gfx);
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, modifier, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 注册容器槽依赖：current 变化 → 容器槽 dirty → 下帧 Enter → 内容重建
                let _c = current.get();
                content(ctx, cur);
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

    /// 测试用叶子：宽度反映 target（page=0 → 50，其他 → 200）——切换后可断言
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

    /// 查布局树中第一个非根节点的测量宽度（Crossfade 内容叶子）
    fn leaf_width(composer: &Composer) -> f32 {
        let Some(root) = composer.layout_root_idx() else { return -1.0 };
        let nodes = composer.arena_nodes();
        fn first_leaf(nodes: &[LayoutNode], idx: usize) -> f32 {
            let n = &nodes[idx];
            if n.children.is_empty() {
                n.measured_size.width
            } else {
                first_leaf(nodes, n.children[0])
            }
        }
        first_leaf(nodes, root)
    }

    /// 集成：target 切换 → 内容淡出 → 重建（新 target）→ 淡入。
    /// 通过多次 compose + 手动推进动画模拟切换生命周期。
    #[test]
    fn crossfade_switches_content_on_target_change() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone())
                    .animation(crate::animation::TweenSpec::default())
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 50.0 } else { 200.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 推进动画帧（真实时间驱动 Tween）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };

        // 首帧：显示 target 0（叶子宽 50）
        recompose(&mut composer);
        assert_eq!(leaf_width(&composer), 50.0, "初始显示 target 0");

        // 切换 target → 淡出（progress 1→0）→ 完成 → 内容重建（宽 200）→ 淡入
        target.set(7);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(leaf_width(&composer), 200.0, "切换后内容重建为 target 7");

        // 切回 target 0 → 内容再切回
        target.set(0);
        recompose(&mut composer);
        advance(&mut composer);
        assert_eq!(leaf_width(&composer), 50.0, "切回后内容重建为 target 0");
    }

    /// 淡出中途 retarget（A→B 淡出未完成改 C）——旧动画移除 + 平滑过渡到新目标
    #[test]
    fn crossfade_retarget_mid_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Crossfade::new(t.clone())
                    .animation(crate::animation::TweenSpec::default())
                    .build(ctx, |ctx, page| {
                        let w = match page {
                            0 => 50.0,
                            1 => 100.0,
                            _ => 200.0,
                        };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..15 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(30));
                recompose(composer);
            }
        };

        // 0 → 1（开始淡出）→ 仅推 2 帧（淡出未完成）→ 改 2（retarget）
        recompose(&mut composer);
        assert_eq!(leaf_width(&composer), 50.0);
        target.set(1);
        recompose(&mut composer);
        for _ in 0..2 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
            recompose(&mut composer);
        }
        // 淡出未完成（progress 还在 1→0 途中）→ 直接 retarget 到 2
        target.set(2);
        recompose(&mut composer);
        advance(&mut composer);
        // 最终收敛到目标 2——且没有 panic/卡死（retarget 移除旧动画）
        assert_eq!(leaf_width(&composer), 200.0, "retarget 后应收敛到新目标 2");
    }
}
