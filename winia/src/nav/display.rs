use super::*;
use super::decorator::wrap_entry;
use super::key::{next_nav_host_id, DRAINING_SCOPE, RESULT_EVENT_BUS_SCOPE};

// ═══════════════════════════════════════════════════════════
// NavTransition — 导航滑动过渡（对标 Nav3 transitionSpec 的 push/pop 动画）
// ═══════════════════════════════════════════════════════════

/// 进入过渡（对标 Compose `EnterTransition`；仅枚举常用组合——同侧 Slide+Fade，
/// 其余 Compose `+` 组合需扩变体）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavEnter {
    /// EnterTransition.None——新页立即出现、无动画（过渡期间静止渲染）
    None,
    /// fadeIn()
    FadeIn,
    /// slideInHorizontally(initialOffsetX)
    SlideIn { initial_offset_x: SlideOffset },
    /// slideInHorizontally(initialOffsetX) + fadeIn()（Compose 用 `+` 组合）
    SlideAndFadeIn { initial_offset_x: SlideOffset },
    /// scaleIn(initialScale)——从初始缩放放大到 1.0（围绕中心；
    /// 对标 Compose `scaleIn(initialScale = 0.9f)`）
    ScaleIn { initial_scale: f32 },
}

/// 退出过渡（对标 Compose `ExitTransition`；同侧组合限制同 [`NavEnter`]）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavExit {
    /// ExitTransition.None——旧页**原样保留**到过渡结束（Compose 语义
    /// "keeps the content unchanged until the end of the transition"，**非**
    /// 瞬时消失；常见搭配 `{ enter: FadeIn, exit: None }` = 新页淡入覆盖静止旧页）
    None,
    /// fadeOut()
    FadeOut,
    /// slideOutHorizontally(targetOffsetX)
    SlideOut { target_offset_x: SlideOffset },
    /// slideOutHorizontally(targetOffsetX) + fadeOut()
    SlideAndFadeOut { target_offset_x: SlideOffset },
    /// scaleOut(targetScale)——从 1.0 缩小到目标缩放（围绕中心；
    /// 对标 Compose `scaleOut(targetScale = 0.9f)`）
    ScaleOut { target_scale: f32 },
}

/// 过渡规格（对标 Nav3 `transitionSpec` 返回的 `ContentTransform` =
/// `enterTransition togetherWith exitTransition`）。含自定义时长/缓动曲线
/// （对标各原语的 animationSpec 参数）。
///
/// `PartialEq` 手动实现：interpolator 是 `Arc<dyn Interpolator>`（无 PartialEq）——
/// 相等性比较 enter/exit/duration + 曲线采样指纹（同曲线同参数 → 同采样；
/// 不比较 Debug 字符串——脆弱且分配）。
#[derive(Clone, Debug)]
pub struct NavTransitionSpec {
    pub enter: NavEnter,
    pub exit: NavExit,
    /// 过渡时长（默认 300ms——Nav3 默认 tween(700)，winia 取更快节奏）
    pub duration: std::time::Duration,
    /// Easing curve, default `EaseOutCubic` (fast out, slow in).
    ///
    /// It is deliberately NOT `EaseInOutCubic`: a scene swap is a CROSS-FADE (the entering layer's alpha
    /// is `1 - p`, the leaving layer's `p`), and an ease-in-out curve keeps the entering layer nearly
    /// invisible for the first part of the transition, so the swap reads as "nothing happens, then it
    /// rushes in". Winia's own `EaseInOutCubic` table evaluated at those progress points gives
    /// 0.0002 / 0.0102 / 0.0409 / 0.1038 / 0.2223 / 0.4321 / 0.6816 — below 10 % opacity for the first 29 %
    /// of the duration — while the incoming screen's shared hero is already well into its flight.
    /// `EaseOutCubic` is also the same family as Compose's default tween easing (FastOutSlowIn).
    ///
    /// CAVEAT, recorded rather than hidden: the same curve drives POSITION for the Slide*/Scale*
    /// primitives, where fast-out means the leaving layer covers ~49 % of its travel in the first 20 % of
    /// the duration and then crawls. Compose splits these — `fadeIn`/`fadeOut` default to a tween,
    /// `slideInHorizontally`/`scaleIn` default to a spring — and winia has one spec for all primitives.
    /// Splitting them is open work, not a decision made here.
    ///
    /// 可注入任意 `Interpolator`，见 `winia::animation::interpolator` 的
    /// 29 个内置曲线：EaseIn/Out/InOut × Sine/Quad/Cubic/Quart/Quint/Expo/
    /// Circ/Back/Elastic/Bounce + Linear
    pub interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
}

impl PartialEq for NavTransitionSpec {
    fn eq(&self, other: &Self) -> bool {
        self.enter == other.enter
            && self.exit == other.exit
            && self.duration == other.duration
            // 曲线采样指纹：非对称五点（0/0.25/0.5/0.75/1）插值比较。
            // ⚠ 对称三点（0/0.5/1）不足以区分——EaseInOutSine/Quad/Cubic/
            // Quart/Quint/Expo/Circ 中点均为 0.5，端点 clamp 到 (0,1)，三点
            // 采样完全相同。五点（含 0.25/0.75 非对称位置）可区分绝大多数
            // 内置曲线；极端定制曲线仍可能碰撞——PartialEq 用于 spec 快照
            // 判定，碰撞仅导致过渡规格变化被忽略（保守场景）。
            // 无分配、确定性。
            && self.interpolator.interpolate(0.0) == other.interpolator.interpolate(0.0)
            && self.interpolator.interpolate(0.25) == other.interpolator.interpolate(0.25)
            && self.interpolator.interpolate(0.5) == other.interpolator.interpolate(0.5)
            && self.interpolator.interpolate(0.75) == other.interpolator.interpolate(0.75)
            && self.interpolator.interpolate(1.0) == other.interpolator.interpolate(1.0)
    }
}

impl NavTransitionSpec {
    /// 构造一对过渡（对标 ContentTransform 的字段构造）
    pub fn new(enter: NavEnter, exit: NavExit) -> Self {
        Self {
            enter,
            exit,
            duration: std::time::Duration::from_millis(300),
            // Fast-out easing, not ease-in-out. A scene swap is a CROSS-FADE: the entering layer's
            // opacity is `1 - p` and the leaving layer's is `p`, so an ease-in-out curve keeps the entering
            // layer nearly invisible for the first part of the transition and the swap reads as "nothing
            // happens, then it rushes in". Winia's own `EaseInOutCubic` table at those progress points:
            // 0.0002 / 0.0102 / 0.0409 / 0.1038 / 0.2223 / 0.4321 / 0.6816 — below 10 % for the first 29 %
            // of the duration — which is why the incoming screen looked absent while the shared hero was
            // already well into its flight. EaseOutCubic is also closer to Compose, whose default tween
            // easing is FastOutSlowIn.
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
        }
    }

    /// Nav3 NavDisplay 默认过渡：fadeIn togetherWith fadeOut（Nav3 用
    /// tween(700)——winia 默认 300ms）
    pub fn fade() -> Self {
        Self::new(NavEnter::FadeIn, NavExit::FadeOut)
    }

    /// 无过渡（EnterTransition.None togetherWith ExitTransition.None——瞬时切换）
    pub fn none() -> Self {
        Self::new(NavEnter::None, NavExit::None)
    }

    /// 自定义时长（对标 animationSpec.durationMillis）
    pub fn duration(mut self, d: std::time::Duration) -> Self {
        self.duration = d;
        self
    }

    /// 自定义缓动曲线（对标 animationSpec.easing——动画系统内置插值器任意选：
    /// `EaseInOutCubic`/`EaseOutCubic`/`EaseInCubic`/`Linear`/`EaseOutBack` 等）
    pub fn easing(mut self, f: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>) -> Self {
        self.interpolator = f;
        self
    }

    /// Android 经典视差滑动（Activity/Fragment 平台过渡形态）——返回 (push, pop)
    /// 一对：push = 全幅右入 togetherWith 左 30% 视差滑出+淡出；pop = 左 30% 视差
    /// 滑入 togetherWith 全幅右出。（Nav3 官方文档示例为全幅对称、无淡出——
    /// 本预设取平台形态，视差+淡出观感更接近原生导航）
    pub fn horizontal_slide() -> (Self, Self) {
        (
            Self::new(
                NavEnter::SlideIn { initial_offset_x: SlideOffset::Fraction(1.0) },
                NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Fraction(-0.3) },
            ),
            Self::new(
                NavEnter::SlideIn { initial_offset_x: SlideOffset::Fraction(-0.3) },
                NavExit::SlideOut { target_offset_x: SlideOffset::Fraction(1.0) },
            ),
        )
    }

    /// M3 shared-axis（30 逻辑 px 反向小位移 + 淡入淡出）——返回 (push, pop) 一对
    pub fn shared_axis() -> (Self, Self) {
        (
            Self::new(
                NavEnter::SlideAndFadeIn { initial_offset_x: SlideOffset::Fixed(30.0) },
                NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Fixed(-30.0) },
            ),
            Self::new(
                NavEnter::SlideAndFadeIn { initial_offset_x: SlideOffset::Fixed(-30.0) },
                NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Fixed(30.0) },
            ),
        )
    }

    /// enter/exit 均为 None（瞬时切换）
    fn is_instant(&self) -> bool {
        self.enter == NavEnter::None && self.exit == NavExit::None
    }
}

/// 导航过渡状态机——同时渲染旧页（previous）与新页（current）做方向性过渡。
///
/// 对标 Nav3 `transitionSpec / popTransitionSpec`（AnimatedContent 双内容过渡）：
/// 规格（[`NavTransitionSpec`]）由使用方经 `NavDisplay::transition_spec` /
/// `pop_transition_spec` 给出，enter/exit 原语分别作用于新页/旧页——
/// - **push** 用 `transition_spec`（如 `SlideIn(全宽) togetherWith SlideAndFadeOut(-30%)`）
/// - **pop** 用 `pop_transition_spec`（方向相反的一对）
///
/// 实现（winia 动画哲学——渲染期 peek 零重组）：
/// - `progress` 1→0 过渡（push_animatable 驱动）
/// - 渲染期用 progress 按 enter/exit 原语算 translate/alpha（graphics_layer
///   闭包 peek——不注册依赖）
/// - 完成检测（progress≈0）→ previous 清空（旧页移除）
///
/// Duration and easing are NOT hardcoded any more: `NavTransitionSpec { duration, interpolator }` carries
/// them (default 300 ms + `EaseOutCubic`, see that field's doc), and the spec is snapshotted when a
/// transition starts. See docs/navigation3.md for the Compose comparison.
#[derive(Clone)]
struct NavTransition<K: NavKey> {
    /// 当前场景的场景 key（对标 Nav3 AnimatedSceneKey——过渡的驱动标识）
    current_key: State<u64>,
    /// 过渡中的旧场景（动画完成后清空；直接持有旧场景对象——渲染"上一帧
    /// 画面"本身，策略链切换/栈外 entry 也能正确渲染，对标 Nav3 sceneMap）
    previous: State<Option<SceneHolder<K>>>,
    /// push=true（新页右入旧页左出）/ pop=false（反向）
    forward: State<bool>,
    /// 过渡进度（1→0：1=旧页全显，0=新页全显/无过渡）
    progress: State<f32>,
    /// 过渡启动时固化的规格快照（对标 Nav3 在过渡启动时求值 transitionSpec——
    /// 中途改配置不影响进行中的过渡；完成时清空）。Backchannel：快照只在
    /// 过渡启动/完成瞬间读写，无订阅者需要通知（progress 的动画通知已驱动帧）。
    active_spec: crate::runtime::state::Backchannel<Option<NavTransitionSpec>>,
    /// This display's identity, used to namespace the scenes it publishes (`layer_scene_id`).
    ///
    /// Remembered in `init`, which runs OUTSIDE the per-scene `ctx.key(scene_holder.key, …)` group: the
    /// scene key is part of a remember slot's base, so allocating it inside `render` gave a NEW id per
    /// navigation (one registry entry per navigation, and every published tag re-created). It is stable
    /// across navigations here; it is still a composition-slot identity rather than a process-unique
    /// instance id, which is all the registry needs (two displays at different slots differ).
    host_id: u64,
}

/// 场景句柄（场景 key + 场景对象）。PartialEq 按 key（同 key = 同内容场景——
/// scene key 已含场景类型与内容身份）。
#[derive(Clone)]
struct SceneHolder<K: NavKey> {
    key: u64,
    scene: std::sync::Arc<dyn Scene<K>>,
}

impl<K: NavKey> PartialEq for SceneHolder<K> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<K: NavKey> NavTransition<K> {
    /// 创建（首帧 current = 初始场景）
    fn init(ctx: &mut ComposeCtx, initial_key: u64) -> Self {
        Self {
            // `remember` returns the `State` itself — these slots hold the values, not a `State`
            // wrapping another `State`. `host_id` below still needs `.get()`, but for the other
            // reason: its field is a plain `u64`, so the handle has to be unwrapped once.
            current_key: ctx.remember(move || initial_key),
            previous: ctx.remember(|| None),
            forward: ctx.remember(|| true),
            // 初值 0 = 无过渡（渲染层以此判定静置归位；导航时 detect 复位 1.0）
            progress: ctx.remember(|| 0.0),
            active_spec: ctx.remember_backchannel(|| None),
            host_id: ctx.remember(next_nav_host_id).get(),
        }
    }

    /// 上一过渡的场景（None = 无进行中过渡）
    fn previous_scene(&self) -> Option<SceneHolder<K>> {
        self.previous.peek().clone()
    }

    /// 检测导航变化并启动过渡（每帧调用——scene key 变化即导航）。
    /// `prev_frame` = 上一帧渲染的场景对象（退场层直接持有——策略链切换/栈外
    /// entry 也能正确渲染，对标 Nav3 sceneMap 的场景缓存）；`None` = 首帧。
    /// **entries 未变（仅场景形态/策略变）→ 瞬时切换**：双层同渲同 contentKey
    /// 内容会在非宏节点键空间碰撞（dup-key），且形态切瞬切符合平台惯例。
    /// spec 在过渡启动时固化进快照（对标 Nav3 求值 transitionSpec 的时机）——
    /// 中途改配置不影响进行中的过渡。
    fn detect(
        &self,
        target: &SceneHolder<K>,
        forward: bool,
        spec: &NavTransitionSpec,
        prev_frame: Option<SceneHolder<K>>,
    ) {
        // 进度推进（动画驱动——每帧 update_animations）
        let _p = self.progress.get();
        // entries 是否变化：未变（仅场景形态/策略变）→ 瞬时切换
        let entries_changed = prev_frame
            .as_ref()
            .map(|p| !same_entry_ids(p.scene.entries(), target.scene.entries()))
            .unwrap_or(false);
        let instant = spec.is_instant() || !entries_changed;
        if target.key != self.current_key.peek() {
            if instant || prev_frame.is_none() {
                // 瞬时切换（None togetherWith None / 形态切换 / 首帧）：无动画、
                // 无旧页——清掉进行中的过渡（防切回动画规格后渲染出栈外幽灵页）
                // 与孤儿动画
                crate::animation::cancel_animation(&self.progress);
                self.progress.as_raw().set_backchannel(0.0);
                self.active_spec.set(None);
                self.previous.set(None);
            } else {
                // 旧场景无条件进入过渡——exit==None 时旧场景原样保留到过渡结束
                // （对标 ExitTransition.None "keeps the content unchanged"）。
                // 直接快照上一帧的场景对象（Arc 持有——popped entry 也渲染真实
                // 内容，对标 Nav3 sceneMap 的场景缓存）
                self.previous.set(Some(SceneHolder {
                    key: self.current_key.peek(),
                    scene: std::sync::Arc::clone(&prev_frame.as_ref().unwrap().scene),
                }));
                self.forward.set(forward);
                self.active_spec.set(Some(spec.clone()));
                // 复位进度起点 1.0（旧页全显）——上次动画结束 progress 停在 0，
                // 不复位则 push_animatable 见 peek==target(0) 直接跳过、动画不启动。
                //
                // 过渡中途再次导航（含反向重定向）时角色互换：本帧的 current（可见度 `1 - p`）变成
                // previous（可见度 `p'`），两种做法都错——
                //  - 保留 p：每个场景的可见度在一帧内跳 `|1 - 2p|`（实测 0.7732 → 0.2134、
                //    0.2268 → 0.7866，p = 0.227 时跳 0.546）；
                //  - 复位 1.0：本帧渲染出全显的旧页，下一帧运行中的时钟又把它拉回中途值，闪一帧。
                // 正确起点是 `1 - p` 并**重启时钟**（旧时钟的相位属于上一次过渡），两层可见度都连续。
                // 过渡中途再次导航（含反向重定向）时角色互换：本帧的 current（可见度 `1 - p`）变成
                // previous（可见度 `p'`），两种做法都错——
                //  - 保留 p：每个场景的可见度在一帧内跳 `|1 - 2p|`（实测按稳定 scene key：
                //    0.770 → 0.216，跳 0.554）；
                //  - 复位 1.0：本帧渲染出全显的旧页，下一帧运行中的时钟又把它拉回中途值，闪一帧。
                // 正确起点是 `1 - p` 并**重启时钟**（旧时钟的相位属于上一次过渡），两层可见度都连续。
                if crate::animation::has_animation_for_state(self.progress.state_id()) {
                    let p = self.progress.peek().clamp(0.0, 1.0);
                    crate::animation::cancel_animation(&self.progress);
                    self.progress.as_raw().set_backchannel(1.0 - p);
                } else {
                    self.progress.as_raw().set_backchannel(1.0);
                }
                // 过渡动画：1→0（用 spec 的时长/缓动曲线——自定义 duration/easing）
                push_animatable(
                    self.progress.clone(),
                    0.0,
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        spec.duration,
                        spec.interpolator.clone(),
                    )),
                );
            }
            self.current_key.set(target.key);
        }
        // 完成检测：过渡结束 → 移除旧页、清规格快照
        if self.previous.peek().is_some() && self.progress.peek() < 0.001 {
            self.previous.set(None);
            self.active_spec.set(None);
        }
    }

    /// 渲染双场景过渡（按 spec 快照的 enter/exit 原语逐层计算——对标 Compose
    /// AnimatedContent 的 ContentTransform：进入原语作用于新场景、退出原语作用
    /// 于旧场景；`exit == None` 时旧场景静止渲染到过渡结束）。
    ///
    /// 层序对标 Nav3 的方向 z 序（"z-index increases during navigate and
    /// decreases during pop"，androidx NavDisplay.android.kt）：push 新场景在上、
    /// pop 旧场景在上。（Nav3 的 ContentTransform.targetContentZIndex 用户覆盖
    /// 当前被 androidx 忽略，winia 同样不支持——方向 z 序为唯一规则。）
    ///
    /// 过渡期间渲染全尺寸点击屏蔽层：winia 命中测试不计 graphics_layer 位移
    /// （布局命中盒停在原位），滑动场景的按钮过渡期可被误触（如连点返回清空栈）。
    /// 位移基准 = 容器宽度（`on_size_changed` 上报，对标 Compose onSizeChanged）。
    fn render(
        &self,
        ctx: &mut ComposeCtx,
        previous_scene: Option<&dyn Scene<K>>,
        current_scene: &dyn Scene<K>,
        render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
        spec: &NavTransitionSpec,
    ) {
        // This display's identity, for namespacing the scenes it publishes (see `layer_scene_id`): the
        // registry behind scene tags is shared and keyed by id alone, so two displays rendering the same
        // route must not publish the same id. Allocated once in `init` (outside the scene-keyed group, which
        // would otherwise hand out a new id on every navigation).
        let host_id = self.host_id;
        // 过渡规格：优先用启动时固化的快照；无进行中过渡时用当前配置
        // （此时 previous 为 None，所有公式在 active 门下归位，取值无效果）
        let spec = self.active_spec.peek().clone().unwrap_or_else(|| spec.clone());
        let prev = self.previous.peek();
        let forward = self.forward.peek();
        // 过渡进行中 = previous 非空——cur 层位移以此为门：静置（启动/无过渡）
        // 时 progress 不保证为 0（无过渡必须归位）
        let active = prev.is_some();
        // 容器宽度（fill_max_size 层的测量宽）——首帧测量先于渲染，peek 即得真值
        let width = ctx.remember(|| 0.0f32);
        // 渲染单个过渡层（graphics_layer 动画闭包——每帧 peek progress/width 零重组）
        let render_layer = |ctx: &mut ComposeCtx, scene: &dyn Scene<K>, is_prev: bool| {
            let progress = self.progress.clone();
            let width = width.clone();
            // Scene identity, computed here so the wrapper modifier can carry it as a tag: the shared
            // transition system reads a subtree's scene from a `SceneTag` in its ANCESTRY, because a
            // marker's own modifier element is built once and reused (a captured id freezes).
            //
            // The id must identify THIS published layer, not just the scene: the registry behind it
            // (`NAV_SCENE_VIS`) is process-global and keyed by id alone, so a plain hash of the scene key
            // collides whenever two hosts render the same route (two NavDisplays on one screen) or one
            // host pushes the same route twice (the two layers then share a scene key). The last publisher
            // wins such a collision, and the other end is classified by the wrong `is_prev` — which is the
            // "paired the leaving end with itself" failure. The host id is per NavDisplay instance, the
            // role distinguishes the leaving layer from the entering one.
            let layer_scene_id = layer_scene_id(host_id, scene.scene_key(), is_prev);
            let m = Modifier::new()
                .fill_max_size()
                .scene_tag(layer_scene_id)
                .graphics_layer(move || {
                let p = progress.peek();
                let w = width.peek().max(1.0);
                let mut params = GraphicsLayerParams::default();
                if is_prev {
                    // 退出原语（对标 ExitTransition）
                    match spec.exit {
                        NavExit::None => {}
                        NavExit::FadeOut => params.alpha = p,
                        NavExit::SlideOut { target_offset_x } => {
                            params.translation_x = target_offset_x.resolve(w) * (1.0 - p);
                        }
                        NavExit::SlideAndFadeOut { target_offset_x } => {
                            params.translation_x = target_offset_x.resolve(w) * (1.0 - p);
                            params.alpha = p;
                        }
                        NavExit::ScaleOut { target_scale } => {
                            // p=1（过渡起点）scale=1.0 → p=0（终点）scale=target
                            params.scale_x = 1.0 + (target_scale - 1.0) * (1.0 - p);
                            params.scale_y = 1.0 + (target_scale - 1.0) * (1.0 - p);
                        }
                    }
                } else {
                    // 进入原语（对标 EnterTransition）——以 active 为门：
                    // 无过渡静置时 progress 静置值语义无效，必须归位
                    match spec.enter {
                        NavEnter::None => {}
                        NavEnter::FadeIn => {
                            if active {
                                params.alpha = 1.0 - p;
                            }
                        }
                        NavEnter::SlideIn { initial_offset_x } => {
                            if active {
                                params.translation_x = initial_offset_x.resolve(w) * p;
                            }
                        }
                        NavEnter::SlideAndFadeIn { initial_offset_x } => {
                            if active {
                                params.translation_x = initial_offset_x.resolve(w) * p;
                                params.alpha = 1.0 - p;
                            }
                        }
                        NavEnter::ScaleIn { initial_scale } => {
                            if active {
                                // p=1（起点）scale=initial → p=0（终点）scale=1.0
                                params.scale_x = initial_scale + (1.0 - initial_scale) * (1.0 - p);
                                params.scale_y = initial_scale + (1.0 - initial_scale) * (1.0 - p);
                            }
                        }
                    }
                }
                params
            });
            crate::layout::components::Column::new()
                .modifier(m)
                .build(ctx, |ctx| {
                    // 场景 key 驱动槽身份（对标 Nav3 AnimatedSceneKey(KClass, key)
                    // ——场景切换 = 组合身份切换）：策略链切换时 scene key 变化 →
                    // 槽换新 → 退场/进场层各自新鲜渲染（否则内层组 params 未变会
                    // Skip 重放旧子树）。场景内部的 pane 级 draining（如 detail
                    // pane 退场层）与场景层的 is_prev（整层滑出）取或——任一为真
                    // 即状态池只读
                    ctx.key(scene.scene_key(), |ctx| {
                        // Publish this layer as a SCENE for the shared-transition system: during a nav
                        // transition the outgoing and incoming scenes are composed at once, so a
                        // marked key exists on both sides and the flight system cannot tell which end
                        // is which by tree order. Visibility is a closure, not a snapshot: this layer
                        // animates through a render-time `graphics_layer`, so its compose does not
                        // re-run every frame and a captured value would go stale mid-transition.
                        let layer_is_prev = is_prev;
                        let layer_active = active;
                        let layer_progress = self.progress.clone();
                        let scene_id = layer_scene_id;
                        let visibility = std::sync::Arc::new(move || {
                            let p = layer_progress.peek().clamp(0.0, 1.0);
                            // p runs 1 (just started) → 0 (settled). The leaving layer fades with p,
                            // the entering one with 1 - p; with no transition running the current
                            // layer is fully visible.
                            if layer_is_prev {
                                if layer_active { p } else { 0.0 }
                            } else if layer_active {
                                1.0 - p
                            } else {
                                1.0
                            }
                        });
                        crate::components::shared_transition::with_nav_scene(
                            crate::components::shared_transition::NavSceneInfo {
                                id: scene_id,
                                scene_key: scene.scene_key(),
                                visibility,
                                is_prev: layer_is_prev,
                            },
                            || {
                                let layer_render = |ctx: &mut ComposeCtx, e: &NavEntry<K>, draining: bool| {
                                    render_entry(ctx, e, draining || is_prev);
                                };
                                scene.content(ctx, &layer_render);
                            },
                        );
                    });
                });
        };
        crate::layout::components::Stack::new()
            .modifier(Modifier::new().fill_max_size().on_size_changed({
                let width = width.clone();
                move |w, _| width.set(w)
            }))
            .build(ctx, |ctx| {
                // Each layer's SLOT identity must cover everything its subtree closes over: the scene
                // itself, the role it plays (leaving or entering) and whether a transition is running.
                // The scene key alone is not enough — the two Stack children swap roles by direction, so
                // the same scene can occupy the same child slot in a different role, and a Skip would
                // then reuse the previous frame's `is_prev`/`active` closures (measured symptom: the
                // flight paired the leaving end with itself). With the full identity a Skip is safe, which
                // is what the `SceneTag` equality arm now allows.
                let layer_key = |scene: &dyn Scene<K>, is_prev: bool| -> u64 {
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    std::hash::Hash::hash(&(scene.scene_key(), is_prev, active), &mut hasher);
                    std::hash::Hasher::finish(&hasher)
                };
                if forward {
                    // push：新场景在上层（对标 Nav3 "z-index increases during navigate"）
                    if let Some(p) = previous_scene {
                        let k = layer_key(p, true);
                        ctx.key(k, |ctx| render_layer(ctx, p, true));
                    }
                    let k = layer_key(current_scene, false);
                    ctx.key(k, |ctx| render_layer(ctx, current_scene, false));
                } else {
                    // pop：被弹出的旧场景在上层（对标 Nav3 "decreases during pop"）
                    let k = layer_key(current_scene, false);
                    ctx.key(k, |ctx| render_layer(ctx, current_scene, false));
                    if let Some(p) = previous_scene {
                        let k = layer_key(p, true);
                        ctx.key(k, |ctx| render_layer(ctx, p, true));
                    }
                }
                // 过渡期输入屏蔽层：命中测试不计 graphics_layer 位移（布局命中盒
                // 停在原位）——滑动场景的按钮过渡期可被误触（如连点返回清空栈）。
                // 只有当原语真的位移内容时才需要兜底：`fade`/`none` 不移动命中盒，
                // 挡下来只会让整屏在过渡期间失去响应（实测 800ms fade 下进入页的
                // Back 按钮约 800ms 内点不动），而 Compose 的 AnimatedContent/NavDisplay
                // 从不安插这种层。
                let displaces = matches!(
                    self.active_spec.peek().as_ref().unwrap_or(&spec).exit,
                    NavExit::SlideOut { .. } | NavExit::SlideAndFadeOut { .. } | NavExit::ScaleOut { .. }
                ) || matches!(
                    self.active_spec.peek().as_ref().unwrap_or(&spec).enter,
                    NavEnter::SlideIn { .. } | NavEnter::SlideAndFadeIn { .. } | NavEnter::ScaleIn { .. }
                );
                if active && displaces {
                    crate::layout::components::Column::new()
                        .modifier(Modifier::new().fill_max_size().clickable(|| {}))
                        .build(ctx, |_| {});
                }
            });
    }
}


// ═══════════════════════════════════════════════════════════
// Scene / SceneStrategy — 多 entry 渲染（对标 Nav3 的 Scene/SceneStrategy，
// androidx.navigation3.scene 包）
// ═══════════════════════════════════════════════════════════

/// Scene——渲染一个或多个 NavEntry 的具体布局（对标 Nav3 `Scene` trait）。
///
/// 场景实例由 [`SceneStrategy`] 从 entries 计算；同一 entry 可被不同 Scene 渲染，
/// 过渡期间只会由最新的目标场景渲染（去重——对标 Nav3 contentKey 覆盖规则）。
///
/// **重要**：实现应为数据类语义（同 key = 同场景）——scene_key 驱动顶层过渡。
pub trait Scene<K: NavKey>: 'static {
    /// 场景标识。**须包含实现类型区分**（对标 Nav3 AnimatedSceneKey(KClass, key)：
    /// 同类型同 key = 同场景；不同类型即便 key 相同也是不同场景）。
    /// 内置场景用"类型 tag + entry contentKey"的确定性 hash。
    fn scene_key(&self) -> u64;

    /// 本场景可渲染的 entries（对标 Nav3 Scene.entries——过渡期间 entry 只由
    /// 最新的目标场景渲染）
    fn entries(&self) -> &[NavEntry<K>];

    /// 上一场景的 entries（对标 Nav3 Scene.previousEntries——predictive back /
    /// 过渡期间旧场景内容来源）。winia 的退场场景由 NavTransition 持有
    /// （previous_scene）——本方法供需要显式访问上一场景内容的自定义场景使用；
    /// 默认返回空（内置场景过渡由 NavTransition 管理，不依赖此值）。
    fn previous_entries(&self) -> &[NavEntry<K>] {
        &[]
    }

    /// 场景级元数据（对标 Nav3 Scene.metadata——**默认 = 栈顶 entry 的
    /// metadata**；过渡覆盖优先级：过渡中 NavEntry.metadata > Scene.metadata >
    /// NavDisplay 默认）。自定义场景可覆盖以提供场景级过渡/装饰信息。
    fn metadata(&self) -> &NavMetadata {
        self.entries().last().map(|e| e.metadata_ref())
            .unwrap_or_else(|| empty_nav_metadata())
    }

    /// 渲染场景内容：自身装饰 + 逐个调用 `render_entry`（每个 entry 至多一次）。
    /// `render_entry` 由 NavDisplay 提供（状态作用域 + 装饰器链包裹）；
    /// `draining=true` 用于**同 contentKey 已在别处渲染**的退场内容——跳过
    /// ctx.key（避免双实例 dup-key）且状态池只读（对标 Nav3 contentKey 去重
    /// + movableContent 的组合语义在 winia 槽表上的等价实现）。
    fn content(
        &self,
        ctx: &mut ComposeCtx,
        render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
    );
}

const SINGLE_PANE_SCENE_TAG: &str = "winia/nav/SinglePaneScene";
const LIST_DETAIL_SCENE_TAG: &str = "winia/nav/ListDetailScene";

/// 静态空 metadata（Scene::metadata 默认实现的兜底引用——避免每帧分配）
fn empty_nav_metadata() -> &'static NavMetadata {
    static EMPTY: std::sync::LazyLock<NavMetadata> = std::sync::LazyLock::new(NavMetadata::new);
    &EMPTY
}


/// 单栏场景（对标 Nav3 `SinglePaneScene`）——渲染栈顶 entry
struct SinglePaneScene<K: NavKey> {
    entries: Vec<NavEntry<K>>,
}

impl<K: NavKey> Scene<K> for SinglePaneScene<K> {
    fn scene_key(&self) -> u64 {
        fnv_hash(&(SINGLE_PANE_SCENE_TAG, self.entries.last().map(|e| e.content_key())))
    }

    fn entries(&self) -> &[NavEntry<K>] {
        &self.entries
    }

    fn content(
        &self,
        ctx: &mut ComposeCtx,
        render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
    ) {
        if let Some(e) = self.entries.last() {
            render_entry(ctx, e, false);
        }
    }
}

/// 双栏场景（对标 Nav3 `ListDetailSceneStrategy` 的场景）——list = 倒数第二、
/// detail = **栈顶**并排；entries = [list, detail]（构造时取末两位，见
/// [`ListDetailStrategy`]）
struct ListDetailScene<K: NavKey> {
    entries: Vec<NavEntry<K>>,
}

impl<K: NavKey> Scene<K> for ListDetailScene<K> {
    fn scene_key(&self) -> u64 {
        // 场景 key 含 list/detail 的 contentKey——**detail 变化即场景变化**，
        // 走 NavDisplay 场景级过渡（按 transition_spec/pop_transition_spec）。
        // 注：Compose 的 ListDetail 场景 key 恒定、pane 内容零动画（pane 级
        // 动画需 movableContent 级基建——winia 槽表暂缺，见路线图 P1-9）；
        // winia 取整场景过渡（列表栏随场景淡入淡出）为当前基建下的最优观感
        fnv_hash(&(
            LIST_DETAIL_SCENE_TAG,
            self.entries.first().map(|e| e.content_key()),
            self.entries.last().map(|e| e.content_key()),
        ))
    }

    fn entries(&self) -> &[NavEntry<K>] {
        &self.entries
    }

    fn content(
        &self,
        ctx: &mut ComposeCtx,
        render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
    ) {
        let (Some(list), Some(detail)) = (self.entries.first(), self.entries.get(1)) else {
            return;
        };
        crate::layout::components::Row::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_height().layout_weight(2.0))
                    .build(ctx, |ctx| render_entry(ctx, list, false));
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_height().layout_weight(3.0))
                    .build(ctx, |ctx| render_entry(ctx, detail, false));
            });
    }
}

/// 场景策略——从 entries 计算场景（对标 Nav3 `SceneStrategy` fun interface）。
///
/// 返回 `None` 表示本策略不接手，尝试策略链中的下一个；全部 `None` 时
/// SinglePane 兜底（对标 `calculateSceneWithSinglePaneFallback`）。
///
/// 用法：`NavDisplay::scene_strategies(vec![...])` / `add_scene_strategy(...)`。
pub trait SceneStrategy<K: NavKey>: Send + Sync + 'static {
    /// entries 非空时调用；接手则返回拥有这些 entries 的场景
    fn calculate_scene(&self, entries: &[NavEntry<K>]) -> Option<Box<dyn Scene<K>>>;
}

/// 场景装饰策略（对标 Nav3 `SceneDecoratorStrategy`）——给计算出的 scene
/// 追加内容（如底部导航栏、全局 Toolbar），**对话框（overlay）场景豁免**
/// （Nav3 语义：overlay scene 独立动画，不参与 scene 装饰）。
///
/// 实现：`decorate_scene` 接收原 scene，返回**包装后的新 scene**——新 scene
/// 的 `content` 里先渲染装饰（如底部栏）再委托原 scene 的 content；`scene_key`
/// /`entries` 原样透传（装饰不改变场景身份）。链式应用：策略列表按序逐个
/// 包装（后加入的在最外层）。
///
/// 用法：`NavDisplay::scene_decorator_strategies(vec![...])`。
pub trait SceneDecoratorStrategy<K: NavKey>: Send + Sync + 'static {
    /// 装饰给定 scene——返回新 scene（可包含或不包含原 scene 内容）。
    ///
    /// ⚠ **实现必须原样透传 `scene_key()` 与 `entries()`**（委托 inner scene）：
    /// - scene_key 变了 → NavDisplay 视为新场景 → 触发**虚假过渡动画**（每帧
    ///   装饰重建也不应改变场景身份）；
    /// - entries 错了 → 渲染缺失/错位（entry 去重、状态作用域都按 entries 走）。
    /// 包装器只需在 `content()` 里追加装饰 + 委托 inner；`metadata()` 默认
    /// 实现读 entries（透传后自动正确）。
    fn decorate_scene(&self, scene: Box<dyn Scene<K>>) -> Box<dyn Scene<K>>;
}

/// 策略链求值 + SinglePane 兜底（对标 Nav3 `calculateSceneWithSinglePaneFallback`）
pub(crate) fn calculate_scene<K: NavKey>(
    strategies: &[Box<dyn SceneStrategy<K>>],
    entries: &[NavEntry<K>],
) -> Box<dyn Scene<K>> {
    for s in strategies {
        if let Some(scene) = s.calculate_scene(entries) {
            return scene;
        }
    }
    Box::new(SinglePaneScene { entries: entries.to_vec() })
}

/// SinglePane 策略——渲染栈顶（对标 Nav3 `SinglePaneSceneStrategy`；
/// 亦为策略链兜底语义的实现）
pub struct SinglePaneStrategy;

impl<K: NavKey> SceneStrategy<K> for SinglePaneStrategy {
    fn calculate_scene(&self, entries: &[NavEntry<K>]) -> Option<Box<dyn Scene<K>>> {
        Some(Box::new(SinglePaneScene { entries: entries.to_vec() }))
    }
}

/// ListDetail 策略——宽屏双栏（列表 + 详情）。
///
/// 对标 Nav3 的 ListDetailSceneStrategy（大屏 list-detail 布局）：
/// - 栈 ≥2 项：接手，list = 倒数第二，detail = 栈顶，双栏并排
/// - 栈 1 项：返回 None（不接手——策略链继续，最终 SinglePane 兜底）
///
/// 与 Nav3 的差异：Nav3 靠 entry metadata 标注 list/detail 角色决定双栏归属；
/// winia 用**位置启发**（倒数第二=列表、栈顶=详情）——重复路由连续 push
/// （如 `Detail(1)`、`Detail(2)`）时"列表栏"语义取位置而非角色标注。
pub struct ListDetailStrategy;

impl<K: NavKey> SceneStrategy<K> for ListDetailStrategy {
    fn calculate_scene(&self, entries: &[NavEntry<K>]) -> Option<Box<dyn Scene<K>>> {
        if entries.len() >= 2 {
            // list = 倒数第二、detail = 栈顶——**只取末两位**（栈更深时右栏跟随
            // 栈顶变化；场景不持有的 entry 由下层场景渲染）
            Some(Box::new(ListDetailScene {
                entries: vec![entries[entries.len() - 2].clone(), entries[entries.len() - 1].clone()],
            }))
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 方向判定（对标 NavDisplay.isPop 列表差分）
// ═══════════════════════════════════════════════════════════

/// 新栈相对旧栈是否为 **pop**——对标 Nav3 `NavDisplay.isPop`（列表差分，
/// 替代长度启发：`set_stack` 整栈替换、等长替换不再误判方向）：
/// - 首元素不同 → 整栈替换，非 pop（navigate）
/// - 新栈更长 → navigate，非 pop
/// - 新栈是旧栈的**前缀子集**且更短 → pop
/// - 长度相同但中途发散 → 替换，非 pop
///
/// 空栈侧为 winia 扩展（Nav3 require 非空栈）：旧栈空 = push；清空 = pop。
pub(super) fn is_pop<K: NavKey>(old: &[K], new: &[K]) -> bool {
    match (old.first(), new.first()) {
        (None, None) => false,
        (None, Some(_)) => false,
        (Some(_), None) => true,
        (Some(o), Some(n)) => {
            if o != n {
                return false; // 整栈替换
            }
            if new.len() > old.len() {
                return false; // navigate
            }
            let diverging = new.iter().zip(old.iter()).position(|(a, b)| a != b);
            diverging.is_none() && new.len() != old.len() // 前缀子集 → pop
        }
    }
}

/// 两组 entries 的内容身份是否相同（contentKey 序列一致）——场景形态/策略
/// 切换（entries 未变）判定用
fn same_entry_ids<K: NavKey>(a: &[NavEntry<K>], b: &[NavEntry<K>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| x.content_key() == y.content_key())
}

// ═══════════════════════════════════════════════════════════
// NavDisplay — 状态的投影（对标 Nav3 的 NavDisplay）
// ═══════════════════════════════════════════════════════════

/// route hash → entry 元信息（NavDisplay 跨帧 remember 映射的值）——被移除 key
/// 不在当前帧 entries 里，on_pop 清理与 pop 方向过渡覆盖都取上一帧映射
#[derive(Clone, PartialEq)]
struct EntryRouteMeta {
    content_key: u64,
    transition_spec: Option<NavTransitionSpec>,
    pop_transition_spec: Option<NavTransitionSpec>,
}

/// 导航显示——观察 back stack，用 entry_provider 生成内容，按 SceneStrategy 渲染。
///
/// 对标 Nav3 `NavDisplay(backStack, entryProvider, sceneStrategy, ...)`：
/// - `back_stack`：观察的导航状态（读注册依赖——变化自动重组）
/// - `entry_provider`：路由 → NavEntry 的映射（对标 Nav3 的 entryProvider）
/// - `scene_strategy`：Scene 计算（默认 SinglePane——渲染栈顶 entry；
///   `ListDetailStrategy` 可双栏）
/// - 过渡：SinglePane 下栈顶变化走 NavTransition 双页过渡——规格经
///   `transition_spec` / `pop_transition_spec` 给出（对标 Nav3 transitionSpec /
///   popTransitionSpec 的 ContentTransform：enter/exit 原语成对；默认 fade）
/// - 装饰器：默认含 `RememberStateDecorator`（状态保持）；`add_decorator`
///   追加的自定义装饰器与默认装饰器**链式包裹** entry 内容（首个最外层），
///   `on_pop` 为广播
///
/// 用法：
/// ```rust
/// NavDisplay::new(&back_stack, |ctx, key| match key {
///     Route::Home => NavEntry::new(key.clone(), |ctx, _| Text::new("Home").build(ctx)),
///     ...
/// })
/// .scene_strategies(vec![Box::new(ListDetailStrategy)])
/// .build(ctx);
/// ```
pub struct NavDisplay<'a, K: NavKey> {
    back_stack: &'a NavBackStack<K>,
    entry_provider: Box<dyn Fn(&mut ComposeCtx, &K) -> NavEntry<K> + 'a>,
    scene_strategies: Vec<Box<dyn SceneStrategy<K>>>,
    /// scene 级装饰策略链（对标 Nav3 sceneDecoratorStrategies——依次包装
    /// 计算出的 scene；对话框场景豁免）
    scene_decorator_strategies: Vec<Box<dyn SceneDecoratorStrategy<K>>>,
    entry_decorators: Vec<Box<dyn NavEntryDecorator<K>>>,
    /// push 过渡（对标 Nav3 transitionSpec）——默认 fade（Nav3 本体默认）
    transition_spec: NavTransitionSpec,
    /// pop 过渡（对标 Nav3 popTransitionSpec）——默认 fade
    pop_transition_spec: NavTransitionSpec,
}

impl<'a, K: NavKey> NavDisplay<'a, K> {
    pub fn new(
        back_stack: &'a NavBackStack<K>,
        entry_provider: impl Fn(&mut ComposeCtx, &K) -> NavEntry<K> + 'a,
    ) -> Self {
        Self {
            back_stack,
            entry_provider: Box::new(entry_provider),
            // 默认无自定义策略——策略链为空时 SinglePane 兜底（对标 Nav3 默认
            // listOf(SinglePaneSceneStrategy()) 的兜底语义）
            scene_strategies: Vec::new(),
            // 默认无 scene 装饰策略（对标 Nav3 sceneDecoratorStrategies 默认空）
            scene_decorator_strategies: Vec::new(),
            // 默认状态保持装饰器（对标 Nav3 默认 rememberSaveableStateHolderNavEntryDecorator）
            entry_decorators: vec![Box::new(RememberStateDecorator)],
            // 默认过渡 = fade（对标 Nav3 NavDisplay 默认——fadeIn togetherWith fadeOut；
            // 水平滑动由使用方显式给出，见 NavTransitionSpec::horizontal_slide）
            transition_spec: NavTransitionSpec::fade(),
            pop_transition_spec: NavTransitionSpec::fade(),
        }
    }

    /// 设置 push 过渡（对标 Nav3 `transitionSpec`——新页进入 + 旧页退出的
    /// `ContentTransform`）。pop 用 [`Self::pop_transition_spec`]（未设置时
    /// 沿用 Nav3 默认 fade；通常两者成对给出一对方向相反的规格，
    /// 见 [`NavTransitionSpec::horizontal_slide`] / [`NavTransitionSpec::shared_axis`]）。
    pub fn transition_spec(mut self, spec: NavTransitionSpec) -> Self {
        self.transition_spec = spec;
        self
    }

    /// 设置 pop 过渡（对标 Nav3 `popTransitionSpec`——返回/弹出方向的过渡）。
    pub fn pop_transition_spec(mut self, spec: NavTransitionSpec) -> Self {
        self.pop_transition_spec = spec;
        self
    }

    /// 设置场景策略链（按顺序依次尝试，首个非 None 接手；全 None 时 SinglePane
    /// 兜底——对标 Nav3 `sceneStrategies: List<SceneStrategy>`）。运行时可换
    /// （如自适应布局）；进行中的过渡不受影响（spec 已快照）。
    pub fn scene_strategies(mut self, strategies: Vec<Box<dyn SceneStrategy<K>>>) -> Self {
        self.scene_strategies = strategies;
        self
    }

    /// 追加单个场景策略到链尾（便捷版 [`Self::scene_strategies`]）
    pub fn add_scene_strategy(mut self, strategy: Box<dyn SceneStrategy<K>>) -> Self {
        self.scene_strategies.push(strategy);
        self
    }

    /// 设置 scene 装饰策略链（对标 Nav3 `sceneDecoratorStrategies`——依次包装
    /// 计算出的 scene；**对话框场景豁免**——overlay 独立动画不参与装饰）。
    /// 后加入的在外层（先加入的先被包装）。
    pub fn scene_decorator_strategies(mut self, strategies: Vec<Box<dyn SceneDecoratorStrategy<K>>>) -> Self {
        self.scene_decorator_strategies = strategies;
        self
    }

    /// 追加单个 scene 装饰策略到链尾
    pub fn add_scene_decorator(mut self, strategy: Box<dyn SceneDecoratorStrategy<K>>) -> Self {
        self.scene_decorator_strategies.push(strategy);
        self
    }

    /// 添加 entry 装饰器（默认已含 RememberStateDecorator；可追加自定义，
    /// 如生命周期/状态清理——对标 Nav3 的 entryDecorators 列表）
    pub fn add_decorator(mut self, decorator: impl NavEntryDecorator<K>) -> Self {
        self.entry_decorators.push(Box::new(decorator));
        self
    }

    /// 渲染当前 Scene（默认 SinglePane：栈顶 entry；ListDetail：双栏；空栈渲染空）
    /// SinglePane 栈顶变化按 `transition_spec` / `pop_transition_spec` 过渡
    /// （默认 fade——对标 Nav3 本体默认；见 [`NavTransitionSpec`]）。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // 观察 back stack——变化触发重组
        let stack = self.back_stack.stack();
        // entry 状态池（跨帧稳定——NavDisplay 的 remember；Arc<Mutex> 内部可变，
        // 不依赖组合槽——对标 Nav3 SaveableStateHolder 的外部状态池）
        let entry_pool: State<std::sync::Arc<std::sync::Mutex<HashMap<(u64, u32, std::any::TypeId), Box<dyn Any>>>>> =
            ctx.remember(|| std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())));
        let entry_pool = entry_pool.get();
        // 结果总线（跨帧稳定——对标 Nav3 rememberResultEventBus；每个 NavDisplay
        // 实例一个总线，entry 内容经 RESULT_EVENT_BUS_SCOPE 读取）
        let result_bus: State<ResultEventBus> = ctx.remember(|| ResultEventBus::new());
        let result_bus = result_bus.get();
        // 为整栈构建 entries（对标 Nav3 rememberDecoratedNavEntries——全量构建，
        // route 元信息映射需要非渲染位 entry 的 contentKey/过渡覆盖；导航栈短，
        // 代价可接受。⚠ entry_provider 应为纯函数：同 key → 同 contentKey/覆盖）
        let entries: Vec<NavEntry<K>> =
            stack.iter().map(|k| (self.entry_provider)(ctx, k)).collect();
        // route hash → entry 元信息映射（跨帧 remember——被移除 key 不在当前
        // entries 里：on_pop 的 contentKey、pop 方向的过渡覆盖均取上一帧映射）
        let route_meta = ctx.remember_backchannel(|| HashMap::new());
        let old_meta: HashMap<u64, EntryRouteMeta> = route_meta.peek().clone();
        // on_pop：对比上一帧栈——被移除的 entry 触发装饰器回调 + 清理状态池
        // （对标 Nav3 onPop(contentKey)：pop 时 removeState 清理）
        let prev_stack: State<Vec<K>> = ctx.remember(|| Vec::new());
        let prev = prev_stack.get();
        if prev != stack {
            for key in prev.iter() {
                if !stack.contains(key) {
                    // 按上一帧映射解析 contentKey；同 contentKey 仍有栈内实例时
                    // 不清理/不回调（对标 Nav3 onPop 触发条件"该 contentKey 的
                    // 最后一个实例被弹出"——共享 contentKey 的多实例状态保持）
                    let kh = key_hash(key);
                    let ck = old_meta.get(&kh).map(|m| m.content_key).unwrap_or(kh);
                    let still_shared = entries.iter().any(|e| e.content_key() == ck);
                    if !still_shared {
                        clear_entry_state(ck, &entry_pool);
                        for d in &self.entry_decorators {
                            d.on_pop(ck);
                        }
                    }
                }
            }
            prev_stack.set(stack.clone());
        }
        // Update route->entry meta map (peek/clone reads only — Backchannel,
        // no notify needed).
        route_meta.set(
            entries
                .iter()
                .map(|e| {
                    (
                        key_hash(e.key()),
                        EntryRouteMeta {
                            content_key: e.content_key(),
                            transition_spec: e.transition_spec_override(),
                            pop_transition_spec: e.pop_transition_spec_override(),
                        },
                    )
                })
                .collect(),
        );
        // ⚠ 空栈不提前 return：dialog 块（下方）必须无条件执行 Dialog::build
        // （visible 参数化）——栈空时对话框是唯一 entry，程序化 pop 关闭依赖
        // record_overlay_active(id,false) → sync 删除 overlay；提前 return 会让
        // 关闭的对话框永久残留（泄漏）。base 场景渲染由 `scene: Option` 空跳过。
        // 分离栈顶连续的 **dialog entry**（对标 Nav3 DialogSceneStrategy——从栈顶
        // 向下收集连续 dialog 标记；更深处的 dialog entry 按普通 entry 渲染）：
        // base 部分走场景/过渡主流，dialog 部分渲染为模态覆盖层（主树不渲染其内容）
        let dialog_len = if stack.is_empty() {
            0 // 空栈：无 dialog（仅需下方案件 build 记录 active=false）
        } else {
            stack
                .iter()
                .rev()
                .zip(entries.iter().rev())
                .take_while(|(_, e)| e.is_dialog())
                .count()
        };
        let base_len = stack.len() - dialog_len;
        let dialog_entries: Vec<NavEntry<K>> = entries[base_len..].to_vec();
        // 策略链计算 base 场景（依次尝试，SinglePane 兜底——对标
        // calculateSceneWithSinglePaneFallback）。持有于 Arc——过渡的退场层
        // 直接引用上一帧的场景对象（对标 Nav3 sceneMap）
        let scene: Option<std::sync::Arc<dyn Scene<K>>> = (base_len > 0).then(|| {
            let mut scene = calculate_scene(&self.scene_strategies, &entries[..base_len]);
            // scene 装饰策略链（对标 Nav3 sceneDecoratorStrategies——依次包装；
            // 后加入的在外层；对话框场景不经过这里）
            for d in &self.scene_decorator_strategies {
                scene = d.decorate_scene(scene);
            }
            scene.into()
        });
        if let Some(scene) = &scene {
        // 导航过渡状态机（跨帧 remember——scene key 维度）
        let transition = NavTransition::init(ctx, scene.scene_key());
        // 方向：isPop 列表差分（对标 NavDisplay.isPop）
        let forward = !is_pop(&prev, &stack);
        // 过渡规格：过渡中 entry 的覆盖 > NavDisplay 默认（对标 Nav3 优先级
        // "transitioning NavEntry.metadata > NavDisplay defaults"——前景 =
        // push 新场景末位 entry / pop 被弹出的旧条目；旧条目覆盖取上一帧映射）
        let spec = if forward {
            scene
                .entries()
                .last()
                .and_then(|e| e.transition_spec_override())
                .unwrap_or(self.transition_spec)
        } else {
            let popped = prev.last().and_then(|k| old_meta.get(&key_hash(k)));
            popped
                .and_then(|m| m.pop_transition_spec.clone())
                .unwrap_or(self.pop_transition_spec)
        };
        // 上一帧渲染的场景对象（退场层来源）——读取须在 last_scene 更新前
        let last_scene = ctx.remember_backchannel(|| None);
        let prev_frame = last_scene.peek().clone().unwrap_or_else(|| {
            // 首帧无上一帧场景——用当前场景兜底（首帧 key 必相同，不会触发过渡）
            SceneHolder { key: scene.scene_key(), scene: std::sync::Arc::clone(&scene) }
        });
        let scene_holder = SceneHolder { key: scene.scene_key(), scene: std::sync::Arc::clone(&scene) };
        // 检测导航变化并启动过渡
        transition.detect(&scene_holder, forward, &spec, Some(prev_frame));
        // 渲染双场景过渡（Stack 层叠：旧场景退出 + 新场景进入）
        // 状态作用域：过渡期两场景的 entries 都提供（滑出层只读——draining）
        let decorators = &self.entry_decorators;
        let entry_pool = entry_pool.clone();
        let result_bus = result_bus.clone();
        let render_entry = |ctx: &mut ComposeCtx, entry: &NavEntry<K>, draining: bool| {
            let counter = ctx.remember_backchannel(|| 0u32);
            counter.set(0); // 每帧重置——seq 按 entry 内调用顺序分配（槽 key 稳定）
            let scope = EntryStateScope {
                pool: entry_pool.clone(),
                key: entry.content_key(),
                counter,
                draining,
            };
            // CompositionLocal provides——PopGuard 自动弹栈（panic/嵌套安全）。
            // draining 内容**裸渲染**：跳过 ctx.key（同 contentKey 已在别处渲染时
            // 会 dup-key）且状态池只读（draining 作用域——remember_entry_state
            // miss 不入池），对标 Nav3 contentKey 去重 + movableContent 语义
            ENTRY_STATE_SCOPE.provides(scope, || {
                // 结果总线作用域（对标 Nav3 ResultEventBusNavEntryDecorator 的
                // LocalResultEventBus provides——entry 内容经 result_event_bus() 读取）
                RESULT_EVENT_BUS_SCOPE.provides(result_bus.clone(), || {
                    // draining 标记作用域（entry 内容经 nav_is_draining() 查询——
                    // 一次性副作用跳过退场帧）
                    DRAINING_SCOPE.provides(draining, || {
                        if draining {
                            entry.build(ctx);
                        } else {
                            wrap_entry(ctx, entry, decorators);
                        }
                    });
                });
            });
        };
        // 场景 key 驱动渲染语句的组合身份（keyed_stmt）：策略/场景切换时
        // scene key 变化 → 渲染语句组换新 → 强制 Enter（否则语句组 params
        // 比较看不见策略变化 → Skip 重放旧子树）。内部 ctx.key(scene_key)
        // 同理作用于场景内容子树
        ctx.key(scene_holder.key, |ctx| {
            transition.render(
                ctx,
                transition.previous_scene().as_ref().map(|h| h.scene.as_ref()),
                scene.as_ref(),
                &render_entry,
                &spec,
            );
        });
        // Record this frame's scene (next frame's exit source).
        last_scene.set(Some(scene_holder));
        }
        // dialog 覆盖层：栈顶 dialog entry 注册为模态覆盖层（渲染于主树之上，
        // 主树不渲染其内容；dismiss = 弹栈）。winia overlay v1 单层限制 →
        // 仅渲染栈顶一个；⚠ 覆盖层内容由独立 Composer 渲染——entry 内 plain
        // ctx.remember 不跨帧持久（remember_entry_state 池化状态不受影响）。
        // ⚠ build 必须**无条件调用**（visible 参数化）：overlay sync 依赖
        // record_overlay_active(id, active)——pop 后 dialog_len=0 时记录
        // active=false → sync 删除 overlay。条件 build（if dialog_len>0）会让
        // 注册方 Skip → sync 视为"无记录保留" → 关闭的对话框残留。
        // 但仅当栈顶 dialog 存在时有内容可渲染——visible=false 时 build 不渲染
        let dialog_visible = dialog_len > 0;
        let dialog_top = (dialog_len > 0).then(|| dialog_entries.last().cloned()).flatten();
        let bs = self.back_stack.clone();
        let pool = entry_pool.clone();
        // 栈顶 dialog 的 key——dismiss 回调幂等防护用（直接 key 相等比较——
        // K: PartialEq + Eq；不用 hash：避免 FNV 碰撞击穿防护误弹下层 entry）
        let dialog_key = dialog_top.as_ref().map(|t| t.key.clone());
        // 对话框属性（对标 Nav3 dialog(dialogProperties)——dismiss_on_click_outside）
        let dialog_dismiss_outside = dialog_top.as_ref()
            .and_then(|t| t.dialog_properties())
            .map(|p| p.dismiss_on_click_outside)
            .unwrap_or(true);
        let dialog_content: Option<(u64, NavEntry<K>)> = dialog_top.map(|t| (t.content_key(), t));
        let mut dialog = crate::overlay::Dialog::new(dialog_visible);
        if !dialog_dismiss_outside {
            dialog = dialog.dismiss_on_outside(false);
        }
        dialog
            .on_dismiss_request(move || {
                // dismiss 幂等防护：外部点击 / overlay 关闭 sync / 按钮显式 pop
                // 都可能触发 on_dismiss——只在 dialog 顶仍在栈顶时 pop，
                // 否则重复 pop 会误弹下层 entry（实测：按钮关闭后 sync 又触发
                // on_dismiss → Detail 被连带弹掉）
                let guard_bs = bs.clone();
                let is_dialog_top = match (&dialog_key, guard_bs.top()) {
                    (Some(dk), Some(tk)) => dk == &tk,
                    _ => false,
                };
                if is_dialog_top {
                    guard_bs.pop();
                }
            })
            .build(ctx, move |ctx| {
                if let Some((ck, entry)) = &dialog_content {
                    let counter = ctx.remember_backchannel(|| 0u32);
                    counter.set(0);
                    let scope = EntryStateScope {
                        pool: pool.clone(),
                        key: *ck,
                        counter,
                        draining: false,
                    };
                    ENTRY_STATE_SCOPE.provides(scope, || {
                        RESULT_EVENT_BUS_SCOPE.provides(result_bus.clone(), || {
                            DRAINING_SCOPE.provides(false, || entry.build(ctx));
                        });
                    });
                }
            });
    }
}
