//! AnimatedVisibility — 内容出现/消失动画（对标 Jetpack Compose `AnimatedVisibility`）。
//!
//! 语义：
//! - `visible` 变 true → 立即组合 content（透明度 0 不可见）→ 播放 enter 动画
//! - `visible` 变 false → 播放 exit 动画 → **动画完成后** content 才从组合移除
//!   （Compose 语义：退出动画播放期间内容仍存在，结束后才释放）
//! - 动画通过 graphics_layer（alpha + 位移）应用——只触发重绘，不触发布局
//!
//! 实现要点：
//! - `shown: State<bool>` 表示"是否在组合中"——exit 动画完成回调里置 false，
//!   触发重组后 `if shown.get()` 分支不再组合 content（Slot 树结构变化）
//! - 动画期间不重组（set_no_wake 推进）；仅动画完成/方向切换时 notify 重组
//! - 快速切换（exit 中途 re-enter）：push 同 state 不同 target 会自动重定向

use crate::animation::{push_animation, AnimationSpec};
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::{Alignment, Arrangement, ColumnLayout, LayoutDirection, MeasurePolicy, Placement, Size};
use crate::modifier::{GraphicsLayerParams, Modifier};
use crate::ui::layout_components::Column;

/// 进入过渡：透明度 + 位移 + 可选布局展开
pub struct EnterTransition {
    pub(crate) spec: AnimationSpec,
    /// 从下往上的进入位移（像素；alpha 0→1 时 y 从 +offset_y 滑到 0）
    pub(crate) offset_y: f32,
    /// 是否布局动画：高度 0→full 展开（下方组件随布局平滑下移）
    pub(crate) layout: bool,
}

/// 退出过渡：透明度 + 位移 + 可选布局收缩
pub struct ExitTransition {
    pub(crate) spec: AnimationSpec,
    pub(crate) offset_y: f32,
    /// 是否布局动画：高度 full→0 收缩（下方组件随布局平滑上移）
    pub(crate) layout: bool,
}

impl EnterTransition {
    /// 自定义进入过渡（默认带布局展开动画）
    pub fn new(spec: AnimationSpec, offset_y: f32) -> Self {
        Self { spec, offset_y, layout: true }
    }

    /// 关闭布局展开（纯渲染动画——下方组件不移动）
    pub fn no_layout(mut self) -> Self {
        self.layout = false;
        self
    }
}

impl ExitTransition {
    /// 自定义退出过渡（默认带布局收缩动画）
    pub fn new(spec: AnimationSpec, offset_y: f32) -> Self {
        Self { spec, offset_y, layout: true }
    }

    /// 关闭布局收缩（纯渲染动画——下方组件不移动）
    pub fn no_layout(mut self) -> Self {
        self.layout = false;
        self
    }
}

fn default_spec() -> AnimationSpec {
    AnimationSpec::Tween(crate::animation::TweenSpec::default())
}

/// 纯淡入（300ms 线性）——布局不动
pub fn fade_in() -> EnterTransition {
    EnterTransition { spec: default_spec(), offset_y: 0.0, layout: false }
}

/// 纯淡出（300ms 线性）——布局不动
pub fn fade_out() -> ExitTransition {
    ExitTransition { spec: default_spec(), offset_y: 0.0, layout: false }
}

/// 淡入 + 从下方 20px 滑入 + 高度展开
pub fn expand_in() -> EnterTransition {
    EnterTransition { spec: default_spec(), offset_y: 20.0, layout: true }
}

/// 淡出 + 向下方 20px 滑出 + 高度收缩
pub fn shrink_out() -> ExitTransition {
    ExitTransition { spec: default_spec(), offset_y: 20.0, layout: true }
}

/// 内容出现/消失动画容器
pub struct AnimatedVisibility {
    visible: State<bool>,
    enter: EnterTransition,
    exit: ExitTransition,
}

impl AnimatedVisibility {
    /// `visible` 由调用方 remember 持有（切换即触发动画）
    pub fn new(visible: State<bool>) -> Self {
        Self { visible, enter: fade_in(), exit: fade_out() }
    }

    pub fn enter(mut self, t: EnterTransition) -> Self { self.enter = t; self }
    pub fn exit(mut self, t: ExitTransition) -> Self { self.exit = t; self }

    /// 组合构建（普通函数——调用点应在 #[composable] 函数内，获取稳定的
    /// STMT_STACK 上下文；build 自身不注入宏——宏对方法体的语句 id 注入与
    /// 调用方闭包语句交互，实测会破坏 key 稳定性）
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let visible = self.visible.clone();

        // 实际是否在组合中（exit 动画完成后置 false——content 移除）
        let shown: State<bool> = ctx.remember(|| visible.get());
        // 透明度动画值（graphics_layer 渲染期读取——零重组）
        let alpha: State<f32> = ctx.remember(|| if visible.get() { 1.0 } else { 0.0 });
        // 当前是否处于退出方向（决定位移取 enter 还是 exit 的 offset）
        let exiting: State<bool> = ctx.remember(|| !visible.get());

        let vis_now = visible.get();
        let shown_now = shown.get();

        if vis_now && !shown_now {
            // 进入：立即组合 content（alpha=0 不可见），再播放淡入。
            // 注意 set_no_wake：当前 compose 继续执行 if shown.get() 即组合 content，
            // 无需 notify 触发同帧二次 compose（会消耗 prev_node_by_key 导致 Skip 恢复失败）
            exiting.set_no_wake(false);
            shown.set_no_wake(true);
            start_anim(alpha.clone(), 1.0, self.enter.spec.clone(), None);
        } else if !vis_now && shown_now {
            // 退出：播放淡出，完成后 shown=false → content 从组合移除（延迟移除）。
            // exiting 仅渲染期读取（graphics_layer 闭包）——set_no_wake 不触发二次 compose；
            // 防重复：exiting 已 true（动画播放中）不再 start_anim——否则 demo 每帧重跑时
            // 每帧重建动画对象（alpha 永不达终值、on_done 不触发、pending 每帧空转）
            if !exiting.get() {
                exiting.set_no_wake(true);
                let s2 = shown.clone();
                start_anim(alpha.clone(), 0.0, self.exit.spec.clone(), Some(Box::new(move || {
                    s2.set(false); // 必须 notify——触发重组移除 content
                })));
            }
        }

        if shown.get() {
            let alpha2 = alpha.clone();
            let exiting2 = exiting.clone();
            let enter_off = self.enter.offset_y;
            let exit_off = self.exit.offset_y;
            let gfx = Modifier::new().graphics_layer(move || {
                let a = alpha2.peek();
                let off = if exiting2.peek() { exit_off } else { enter_off };
                GraphicsLayerParams {
                    alpha: a,
                    // 进入：y 从 +off 滑到 0；退出：y 从 0 滑到 +off（同方向公式）
                    translation_y: (1.0 - a) * off,
                    ..Default::default()
                }
            });
            // 退出动画期间高度收缩（对标 Compose shrinkVertically）：alpha 1→0 时
            // 高度 full→0、内容向下滚出——下方组件随布局平滑上移（不再瞬间跳变）。
            // force_remeasure：每帧重测（布局帧读最新 alpha，零重组）
            let key = ctx.next_key();
            let dir = crate::ui::theme::WiniaTheme::direction();
            let policy = ShrinkPolicy {
                inner: Box::new(ColumnLayout::new()
                    .arrangement(Arrangement::Start)
                    .alignment(Alignment::Start)
                    .spacing(0.0)
                    .direction(dir)),
                alpha: alpha.clone(),
                exiting: exiting.clone(),
                enter_layout: self.enter.layout,
                exit_layout: self.exit.layout,
            };
            ctx.start_container(key, gfx, policy);
            content(ctx);
            ctx.end_node();
        }
    }
}

/// 布局收缩策略：退出方向时高度按 alpha（1→0）收缩、内容向下滚出容器。
/// `force_remeasure() -> true`——每帧渲染的 layout 阶段重测（及所有祖先），
/// 用最新 alpha 值计算收缩高度；动画值用 `peek` 读取（不注册依赖、零重组）。
struct ShrinkPolicy {
    inner: Box<dyn MeasurePolicy>,
    alpha: State<f32>,
    exiting: State<bool>,
    /// 进入方向是否布局动画（展开）——fade_in=false / expand_in=true
    enter_layout: bool,
    /// 退出方向是否布局动画（收缩）——fade_out=false / shrink_out=true
    exit_layout: bool,
}

impl std::fmt::Debug for ShrinkPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShrinkPolicy").finish_non_exhaustive()
    }
}

impl MeasurePolicy for ShrinkPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (Size, Vec<Placement>) {
        let (size, mut placements) = self.inner.measure(nodes, policies, children, constraints);
        let r = self.alpha.peek().clamp(0.0, 1.0);
        let exiting = self.exiting.peek();
        // 仅当该方向配置了布局动画（expand_in/shrink_out）才收缩/展开高度；
        // fade_in/fade_out 纯渲染动画——布局保持全高（下方组件不移动）
        let layout = if exiting { self.exit_layout } else { self.enter_layout };
        if layout && r < 1.0 {
            let full_h = size.height;
            if exiting {
                // 退出：内容向下滚出容器（与 alpha 淡出同步）
                let dy = (1.0 - r) * full_h;
                for p in &mut placements {
                    p.position.y += dy;
                }
            }
            // 进入：内容顶部对齐，面板从顶部向下展开
            (Size::new(size.width, full_h * r), placements)
        } else {
            (size, placements)
        }
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[Placement]) {
        self.inner.place(nodes, children, placements);
    }

    fn force_remeasure(&self) -> bool {
        // 任一方向配置了布局动画就每帧重测（读最新 alpha 计算收缩高度）
        self.enter_layout || self.exit_layout
    }
}

/// 启动动画并注册到全局驱动列表（带完成回调；目标已是终值则立即回调——snap 语义）
fn start_anim<T: Clone + PartialEq + crate::animation::AnimatableValue + Send + Sync + 'static>(
    state: State<T>,
    target: T,
    spec: AnimationSpec,
    on_done: Option<Box<dyn FnOnce() + Send>>,
) {
    if state.peek() == target {
        if let Some(f) = on_done { f(); }
        return;
    }
    let mut anim = crate::animation::Animatable::new(state);
    if let Some(f) = on_done {
        anim = anim.on_complete(f);
    }
    anim.animate_to(target, spec);
    anim.update();
    push_animation(Box::new(anim));
    crate::core::state::wake_loop();
}
