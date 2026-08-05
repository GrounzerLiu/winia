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
//!   时容器高度 × 进度（下方内容平滑跟随），动画推进只重测不重组（`set_no_wake` 写值，
//!   组件闭包不因动画值重跑）
//! - **绘制层**：`graphics_layer` 动态闭包——fade（alpha）/ slide（平移）/ scale（缩放），
//!   渲染期 `peek()` 读值，不注册依赖
//! - **exit 延迟移除**：`visible=false` → exit 动画期间内容保留在组合树（progress 1→0）；
//!   动画完成（progress≈0 且不可见）→ `removed` 标记 → 下帧 build 不 start 容器 →
//!   槽回收（内容消失）

use crate::animation::{push_animatable, AnimationSpec};
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement, Point, Size};
use crate::modifier::{GraphicsLayerParams, Modifier};

/// 滑动方向（`VisibilityTransition::slide_in/slide_out`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    Left,
    Right,
    Up,
    Down,
}

/// 进入/退出过渡配置：fade/slide/expand/scale 效果组合 + 动画规格。
/// 组合用 `with_*` 链式（对标 Compose `fadeIn() + expandVertically()`）。
#[derive(Debug, Clone)]
pub struct VisibilityTransition {
    /// 淡入淡出（alpha 0↔1）
    pub fade: bool,
    /// 平移进出（方向）
    pub slide: Option<SlideDirection>,
    /// 垂直展开/收缩（容器高度 0↔full——布局层，下方内容跟随）
    pub expand: bool,
    /// 缩放（0.8↔1.0）
    pub scale: bool,
    /// 动画规格
    pub spec: AnimationSpec,
}

impl VisibilityTransition {
    pub fn fade_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: true, slide: None, expand: false, scale: false, spec: spec.into() }
    }
    pub fn fade_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::fade_in(spec)
    }
    pub fn expand_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: false, slide: None, expand: true, scale: false, spec: spec.into() }
    }
    pub fn shrink_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::expand_in(spec)
    }
    pub fn slide_in(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: false, slide: Some(dir), expand: false, scale: false, spec: spec.into() }
    }
    pub fn slide_out(dir: SlideDirection, spec: impl Into<AnimationSpec>) -> Self {
        Self::slide_in(dir, spec)
    }
    pub fn scale_in(spec: impl Into<AnimationSpec>) -> Self {
        Self { fade: false, slide: None, expand: false, scale: true, spec: spec.into() }
    }
    pub fn scale_out(spec: impl Into<AnimationSpec>) -> Self {
        Self::scale_in(spec)
    }
    /// 组合：叠加淡入淡出
    pub fn with_fade(mut self) -> Self {
        self.fade = true;
        self
    }
    /// 组合：叠加垂直展开/收缩
    pub fn with_expand(mut self) -> Self {
        self.expand = true;
        self
    }
    /// 组合：叠加滑动
    pub fn with_slide(mut self, dir: SlideDirection) -> Self {
        self.slide = Some(dir);
        self
    }
    /// 组合：叠加缩放
    pub fn with_scale(mut self) -> Self {
        self.scale = true;
        self
    }
}

impl Default for VisibilityTransition {
    fn default() -> Self {
        Self {
            fade: true,
            slide: None,
            expand: false,
            scale: false,
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
        // set_no_wake 每帧 notify → pending → 下帧 compose）
        if !vis && progress.peek() < 0.001 && !removed.peek() {
            removed.set(true);
        }
        if removed.peek() {
            return; // 已移除：不 start 容器 → 槽 visited=false → 物化回收
        }
        // 驱动进度动画（visible 变化帧 push——同目标 dedup 跳过）
        let target = if vis { 1.0 } else { 0.0 };
        let spec = if vis { enter.spec.clone() } else { exit.spec.clone() };
        push_animatable(progress.clone(), target, spec);
        // 绘制层参数（渲染期 peek——不注册依赖不重组）
        let g = progress.clone();
        let e = enter.clone();
        let x = exit.clone();
        let v = visible.clone();
        let gfx = move || {
            let p = g.peek();
            let cfg = if v.peek() { &e } else { &x };
            let mut params = GraphicsLayerParams::default();
            if cfg.fade {
                params.alpha = p;
            }
            if cfg.scale {
                let s = 0.8 + 0.2 * p;
                params.scale_x = s;
                params.scale_y = s;
            }
            if let Some(dir) = cfg.slide {
                let off = (1.0 - p) * 48.0;
                match dir {
                    SlideDirection::Left => params.translation_x = -off,
                    SlideDirection::Right => params.translation_x = off,
                    SlideDirection::Up => params.translation_y = -off,
                    SlideDirection::Down => params.translation_y = off,
                }
            }
            params
        };
        let modifier = Modifier::new().graphics_layer(gfx);
        let policy = VisibilityPolicy {
            progress: progress.clone(),
            expand: enter.expand || exit.expand,
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

/// 布局 policy：expand/shrink 时容器高度 × 进度（测量期 `get()` 注册
/// layout_dep → 动画推进每帧重测，下方内容平滑跟随；不触发重组）。
#[derive(Clone)]
struct VisibilityPolicy {
    progress: State<f32>,
    expand: bool,
}

impl std::fmt::Debug for VisibilityPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VisibilityPolicy")
            .field("expand", &self.expand)
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
        // expand=true 时高度随动画进度缩放——读 progress 注册 layout_dep
        // （动画推进 → 本节点重测）；expand=false 时布局与进度无关——
        // 不注册依赖，动画推进走 graphics_layer 绘制变换（零重排）
        let p = if self.expand {
            self.progress.get()
        } else {
            self.progress.peek()
        };
        // 测量子节点（content 根，通常 1 个）——取最大宽高（Stack 语义）
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        let mut placements = Vec::with_capacity(children.len());
        for &c in children {
            let (size, _) = measure_node(nodes, policies, c, constraints);
            max_w = max_w.max(size.width);
            max_h = max_h.max(size.height);
            placements.push(Placement { size, position: Point::ZERO });
        }
        let h = if self.expand { max_h * p } else { max_h };
        (Size::new(max_w, h), placements)
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

    /// 测试用简单叶子（Text 需字体环境——用固定尺寸盒子代替）
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

    fn count_text(composer: &Composer) -> usize {
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
