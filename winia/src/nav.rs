//! 导航层（对标 Jetpack Navigation 3 设计）——状态驱动的 back stack + 声明式投影。
//!
//! 借鉴 Navigation3 的核心架构（源码：androidx/navigation3）：
//! - **你拥有 back stack**：导航状态就是普通 winia State（`State<Vec<K>>`），
//!   导航 = 改列表（push/pop/remove_last），观察者（NavDisplay）自动重组。
//!   对标 Nav3 的 `NavBackStack = SnapshotStateList<T>` 包装。
//! - **类型安全路由**：`NavKey` trait 取代字符串路由（对标 Nav3 的 `NavKey` 接口）。
//! - **NavDisplay 是状态的投影**：观察 back stack → entry_provider 生成 NavEntry →
//!   渲染当前 Scene。对标 Nav3 的 `NavDisplay = 观察 List + entryProvider`。
//! - **Scene 概念**：Scene 可渲染一个或多个 entry（多栏/自适应布局）。
//!   当前实现 SinglePane（渲染栈顶），为多栏留扩展。
//!
//! 与 Compose 的差异（winia 简化）：
//! - Compose 用 `SnapshotStateList`（细粒度 add/remove 通知）；winia 用
//!   `State<Vec<K>>` 整体替换（PartialEq 去重）——导航操作频率低，整体通知
//!   代价可接受，且与 winia 的 State 系统完全契合。
//! - 过渡动画：NavTransition 双页过渡（对标 Nav3 transitionSpec / popTransitionSpec
//!   的 ContentTransform——`NavEnter`/`NavExit` 原语成对组合，默认 fade；
//!   graphics_layer 渲染期 peek 零重组）。

use crate::core::state::State;
use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::animation::push_animatable;
use crate::modifier::GraphicsLayerParams;
use std::any::Any;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Entry 状态池（对标 Nav3 SaveableStateHolder——外部状态池，不依赖组合槽）
// ═══════════════════════════════════════════════════════════

/// entry 状态作用域——NavDisplay 渲染 entry 时经 CompositionLocal 提供。
///
/// 对标 Nav3 `SaveableStateHolder.SaveableStateProvider(contentKey)`：
/// - Nav3：状态存在外部 holder（HashMap<contentKey, SaveableStateRegistry>），
///   pop 时 removeState 清理，重进时恢复——不依赖组合位置
/// - winia：状态池存 `Arc<Mutex<HashMap<(u64, u32, TypeId), Box<dyn Any>>>>`
///   （NavDisplay remember，跨帧稳定）——entry 内容用 `remember_entry_state`
///   存取，跨 pop/push 保持
///
/// 组合槽机制无法自动保持 remember（槽随子树移除销毁），故用显式状态池
/// 替代（同 Nav3 的显式 SaveableStateProvider 语义）。
///
/// 作用域经 CompositionLocal 提供（而非裸 thread_local）——PopGuard 自动弹栈，
/// panic/嵌套安全（对齐 winia Theme/Density/SelectionRegistrar 的作用域模式）。
#[derive(Clone)]
pub(crate) struct EntryStateScope {
    pool: std::sync::Arc<std::sync::Mutex<HashMap<(u64, u32, std::any::TypeId), Box<dyn Any>>>>,
    key: u64,
    counter: State<u32>,
    /// 过渡滑出层（previous）用只读作用域：命中返回现存槽，miss **不入池**。
    /// 滑出期间旧页内容每帧重跑，若照常 miss→insert 会把刚被 removeState
    /// 清理的槽重新插回（下次 pop 才再清）——破坏 Nav3 removeState 语义。
    draining: bool,
}

/// entry 状态作用域 CompositionLocal——NavDisplay 渲染 entry 时 provides，
/// `remember_entry_state` 读 current（try_current 区分作用域内外）
static ENTRY_STATE_SCOPE: std::sync::LazyLock<crate::core::composition_local::CompositionLocal<EntryStateScope>> =
    std::sync::LazyLock::new(|| {
        // default 不可达（try_current 不调 default）——占位
        crate::core::composition_local::CompositionLocal::new(|| {
            panic!("ENTRY_STATE_SCOPE 无默认值——必须经 NavDisplay 提供")
        })
    });

/// 在 entry 内容中记住状态——跨导航（pop/push）保持。
///
/// 对标 Nav3 `rememberSaveable`（经 SaveableStateHolder 保存）：
/// - 同一 **contentKey** 的 entry 每次进入返回同一 State（值保持）——默认
///   contentKey = key 的确定性 hash；`NavEntry::with_content_key` 可让不同路由
///   共享同一份状态（Nav3 语义：同内容 = 同状态）
/// - 状态池由 NavDisplay 持有（跨帧稳定，Arc<Mutex> 内部可变），不依赖组合槽
/// - 同一 entry 内多次调用按调用顺序分配独立槽
///
/// 用法（entry 内容内）：
/// ```rust
/// let count = nav::remember_entry_state(|| 0i32);
/// ```
///
/// ⚠ 约束（槽 key = (contentKey, 调用序号, 类型)）：
/// - 调用位置/顺序须逐帧稳定——勿放进条件分支或循环内长度可变的路径
///   （条件翻转会导致序号整体移位、槽重映射，语义同 Compose remember 的位置制）；
/// - 同一 back stack 内 **contentKey 应唯一**——重复 contentKey 的实例共享同一
///   池槽（可利用：`Detail(1)`/`Detail(2)` 用同一 contentKey 即共享状态）；
///   同 key 不同 contentKey 则相互独立。
///
/// 注意：只有本函数（外部状态池）保证跨 pop/push 保持；entry 内容里的普通
/// `ctx.remember` 依赖组合槽位置，双层过渡的槽位按调用序分配，pop/push 边界
/// 不保证存活（见 [`RememberStateDecorator`] 文档）。
pub fn remember_entry_state<T: Clone + PartialEq + 'static>(
    init: impl FnOnce() -> T,
) -> State<T> {
    let Some(scope) = ENTRY_STATE_SCOPE.try_current() else {
        panic!("remember_entry_state 必须在 NavDisplay 的 entry 内容内调用（CompositionLocal 作用域外）");
    };
    // 槽 key = (entry key, 调用序号, 类型)——类型参与 key：不同 T 不撞槽，
    // 命中即类型正确（无需 downcast 失败回退）
    let seq = scope.counter.peek();
    scope.counter.set_silent(seq + 1);
    let slot_key = (scope.key, seq, std::any::TypeId::of::<T>());
    let mut pool = scope.pool.lock().unwrap();
    if let Some(b) = pool.get(&slot_key) {
        // TypeId 参与 key——命中必是 State<T>（fail-fast 防御）
        return b.downcast_ref::<State<T>>()
            .expect("状态池槽类型不匹配（TypeId 相同但 downcast 失败——内部错误）")
            .clone();
    }
    if scope.draining {
        // 滑出层只读作用域：miss 不入池（防重污染）——返回一次性 State，
        // 值不跨帧保持（该页正在淡出，视觉上仅是退出动画的临时内容）
        return State::new(init());
    }
    let s = State::new(init());
    pool.insert(slot_key, Box::new(s.clone()));
    s
}

/// 清理 entry 状态（pop 时由装饰器调用——对标 Nav3 removeState）
pub(crate) fn clear_entry_state(key: u64, pool: &std::sync::Arc<std::sync::Mutex<HashMap<(u64, u32, std::any::TypeId), Box<dyn Any>>>>) {
    let mut p = pool.lock().unwrap();
    p.retain(|(k, _, _), _| *k != key);
}

/// 路由 key 的稳定 hash（状态池槽 key 用）。
/// ⚠ 不能用 DefaultHasher——其种子随机，同一值每次 hash 不同 → 槽 key 漂移
/// → 状态池永不命中。用 FNV-1a（确定性，与 winia mix_key 同源）。
fn key_hash<K: NavKey>(key: &K) -> u64 {
    fnv_hash(key)
}

/// 确定性 FNV-1a（框架内部标识用——scene key 等场景无关 hash）
fn fnv_hash(value: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    struct FnvHasher(u64);
    impl std::hash::Hasher for FnvHasher {
        fn finish(&self) -> u64 { self.0 }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 ^= b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3);
            }
        }
    }
    let mut h = FnvHasher(0xcbf29ce484222325);
    value.hash(&mut h);
    h.finish()
}

// ═══════════════════════════════════════════════════════════
// NavKey — 路由标识（对标 Nav3 的 NavKey 接口）
// ═══════════════════════════════════════════════════════════

/// 导航路由标识——类型安全取代字符串路由。
///
/// 用户定义自己的路由类型（enum 或 data struct），实现本 trait：
/// ```rust
/// #[derive(Clone, PartialEq, Eq, Debug, Hash)]
/// enum Route {
///     Home,
///     Detail(u64),
/// }
/// // Route 自动满足 NavKey（trait 无方法，仅约束）
/// ```
///
/// 对标 Nav3：`NavKey` 是标记接口（`@Serializable` 用于持久化）。winia 无
/// 序列化要求，trait 仅作类型约束（`Clone + PartialEq + Eq + Hash + Debug +
/// Send + Sync + 'static`——Hash 用于状态保持装饰器的固定组合 key；Send+Sync
/// 用于 dismiss 等跨线程回调）。
pub trait NavKey: Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static {}

impl<T: Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static> NavKey for T {}

// ═══════════════════════════════════════════════════════════
// NavBackStack — 导航状态（对标 Nav3 的 NavBackStack）
// ═══════════════════════════════════════════════════════════

/// 导航 back stack——`State<Vec<K>>` 包装，导航 = 改列表。
///
/// 对标 Nav3 `NavBackStack<T> : MutableList<T> by SnapshotStateList`：
/// - Compose 包装 `SnapshotStateList`（细粒度通知）
/// - winia 包装 `State<Vec<K>>`（整体替换，PartialEq 去重）
///
/// 导航操作（push/pop）后，观察者（NavDisplay）因 State 变化自动重组。
#[derive(Clone)]
pub struct NavBackStack<K: NavKey> {
    state: State<Vec<K>>,
}

impl<K: NavKey> NavBackStack<K> {
    /// 创建空 back stack
    pub fn new() -> Self {
        Self { state: State::new(Vec::new()) }
    }

    /// 创建带初始路由的 back stack
    pub fn with_initial(initial: K) -> Self {
        Self { state: State::new(vec![initial]) }
    }

    /// 当前栈（读——注册依赖；观察者用）
    pub fn stack(&self) -> Vec<K> {
        self.state.get()
    }

    /// 栈顶（当前路由）——空栈返回 None
    pub fn top(&self) -> Option<K> {
        self.state.get().last().cloned()
    }

    /// 栈长度
    pub fn len(&self) -> usize {
        self.state.get().len()
    }

    pub fn is_empty(&self) -> bool {
        self.state.get().is_empty()
    }

    /// 导航到新路由（push）——对标 Nav3 `backStack.add(key)`
    pub fn push(&self, key: K) {
        self.state.update(|v| v.push(key));
    }

    /// 返回（pop 栈顶）——对标 Nav3 `backStack.removeLastOrNull()`
    pub fn pop(&self) -> Option<K> {
        let mut popped = None;
        self.state.update(|v| { popped = v.pop(); });
        popped
    }

    /// 移除栈顶（无返回值版本——与 pop 相同语义）
    pub fn remove_last(&self) {
        self.state.update(|v| { v.pop(); });
    }

    /// 清空（回到空栈）
    pub fn clear(&self) {
        self.state.set(Vec::new());
    }

    /// 替换整个栈（程序化导航，如 deep link 恢复）
    pub fn set_stack(&self, stack: Vec<K>) {
        self.state.set(stack);
    }
}

impl<K: NavKey> Default for NavBackStack<K> {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// NavEntry — 路由 → 内容（对标 Nav3 的 NavEntry）
// ═══════════════════════════════════════════════════════════

/// 一个导航条目：key + contentKey + 内容构建闭包。
///
/// 对标 Nav3 `NavEntry(key, contentKey, metadata, content)`——winia 简化：
/// metadata 后续按需加（见 docs/navigation3.md §五 P0-3）。
/// content 为 `'static`（闭包捕获 owned 数据，不借用外部——对标 Nav3 的
/// @Composable content 无生命周期依赖）。
///
/// **contentKey**（对标 Nav3 `NavEntry.contentKey`）：稳定内容 id——状态池槽、
/// 组合 key、on_pop 清理全部按它关联。默认 = key 的确定性 hash（key_hash）；
/// 用 `NavEntry::with_content_key` 覆盖可表达"不同路由、同一内容"（Nav3 语义：
/// 同内容 = 同状态，如 `Detail(id)` 各 id 共享一个编辑态）或"同 key、不同内容"
/// （各自独立状态）。
pub struct NavEntry<K: NavKey> {
    key: K,
    content_key: u64,
    /// push 方向过渡覆盖（对标 Nav3 `NavDisplay.TransitionKey` metadata——
    /// 本 entry 作为 push 前景时优先于 NavDisplay 默认）
    transition_spec: Option<NavTransitionSpec>,
    /// pop 方向过渡覆盖（对标 Nav3 `NavDisplay.PopTransitionKey` metadata——
    /// 本 entry 被弹出作为 pop 前景时优先于 NavDisplay 默认）
    pop_transition_spec: Option<NavTransitionSpec>,
    /// 对话框标记（对标 Nav3 `dialog()` metadata / DialogSceneStrategy）——
    /// 栈顶连续的 dialog entry 渲染为模态覆盖层（主树不渲染其内容），
    /// dismiss = 弹栈
    dialog: bool,
    content: std::sync::Arc<dyn Fn(&mut ComposeCtx, &K) + 'static>,
}

impl<K: NavKey> Clone for NavEntry<K> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content_key: self.content_key,
            transition_spec: self.transition_spec,
            pop_transition_spec: self.pop_transition_spec,
            dialog: self.dialog,
            content: self.content.clone(),
        }
    }
}

/// 相等性 = **内容身份**相等（key + contentKey；spec/内容闭包不参与）——
/// 同 contentKey 即同内容（与状态池/组合 key 的关联语义一致）
impl<K: NavKey> PartialEq for NavEntry<K> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.content_key == other.content_key
    }
}

impl<K: NavKey> NavEntry<K> {
    pub fn new(key: K, content: impl Fn(&mut ComposeCtx, &K) + 'static) -> Self {
        let content_key = key_hash(&key);
        Self {
            key,
            content_key,
            transition_spec: None,
            pop_transition_spec: None,
            dialog: false,
            content: std::sync::Arc::new(content),
        }
    }

    /// 指定 contentKey 的构造（对标 Nav3 `NavEntry(key, contentKey, content)`）。
    ///
    /// ⚠ 应为 key 的**确定性纯函数**（同 key → 同 contentKey）——状态恢复依赖它。
    pub fn with_content_key(
        key: K,
        content_key: u64,
        content: impl Fn(&mut ComposeCtx, &K) + 'static,
    ) -> Self {
        Self {
            key,
            content_key,
            transition_spec: None,
            pop_transition_spec: None,
            dialog: false,
            content: std::sync::Arc::new(content),
        }
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    /// 稳定内容 id（对标 Nav3 contentKey）
    pub fn content_key(&self) -> u64 {
        self.content_key
    }

    /// 标记为**对话框 entry**（对标 Nav3 `dialog()` metadata / DialogSceneStrategy）：
    /// 本 entry 位于栈顶时渲染为模态覆盖层（主树不渲染其内容），dismiss（点击
    /// 外部/用户关闭）= 弹栈。
    ///
    /// ⚠ **仅栈顶连续 dialog 生效**（v1 overlay 单层限制）：从栈顶向下收集连续
    /// dialog 标记——若 dialog 之上 push 了普通 entry（如 [Home, About(dialog),
    /// Detail]），About 会作为普通 base 页面渲染（非模态覆盖层）。与 Nav3
    /// DialogSceneStrategy（所有 dialog metadata entry 均弹窗）不同，为设计取舍。
    pub fn as_dialog(mut self) -> Self {
        self.dialog = true;
        self
    }

    /// 是否为对话框 entry
    pub fn is_dialog(&self) -> bool {
        self.dialog
    }

    /// 覆盖本 entry 作为 push 前景（新栈顶）时的过渡——对标 Nav3
    /// `NavDisplay.transitionSpec(...)` entry metadata；优先级高于
    /// `NavDisplay::transition_spec` 默认。
    pub fn transition_spec(mut self, spec: NavTransitionSpec) -> Self {
        self.transition_spec = Some(spec);
        self
    }

    /// 覆盖本 entry 被弹出（pop 前景）时的过渡——对标 Nav3
    /// `NavDisplay.popTransitionSpec(...)` entry metadata；优先级高于
    /// `NavDisplay::pop_transition_spec` 默认。
    pub fn pop_transition_spec(mut self, spec: NavTransitionSpec) -> Self {
        self.pop_transition_spec = Some(spec);
        self
    }

    fn transition_spec_override(&self) -> Option<NavTransitionSpec> {
        self.transition_spec
    }

    fn pop_transition_spec_override(&self) -> Option<NavTransitionSpec> {
        self.pop_transition_spec
    }

    /// 渲染内容（传入 key）
    pub fn build(&self, ctx: &mut ComposeCtx) {
        (self.content)(ctx, &self.key);
    }
}

// ═══════════════════════════════════════════════════════════
// NavTransition — 导航滑动过渡（对标 Nav3 transitionSpec 的 push/pop 动画）
// ═══════════════════════════════════════════════════════════

/// 滑动位移来源（对标 Compose `slideInHorizontally(initialOffsetX: (Int) -> Int)`
/// 的常用取值——Compose 闭包入参为内容宽度：`{ it }` = 全宽、`{ -it / 3 }` =
/// 反向 1/3 视差；winia 组合模型无布局期闭包，声明式表达为比例或固定值）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlideOffset {
    /// 容器宽度比例（1.0 = 全宽——对标 `{ it }`；-0.3 = 反向 30%——对标 `{ -it / 3 }`）
    Fraction(f32),
    /// 固定逻辑 px（对标固定 dp 位移——M3 shared-axis 的 30dp；当前按逻辑 px
    /// 解析、随密度缩放后续接）
    Px(f32),
}

impl SlideOffset {
    fn resolve(&self, width: f32) -> f32 {
        match self {
            SlideOffset::Fraction(f) => f * width,
            SlideOffset::Px(d) => *d,
        }
    }
}

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
}

/// 过渡规格（对标 Nav3 `transitionSpec` 返回的 `ContentTransform` =
/// `enterTransition togetherWith exitTransition`）。时长/曲线当前统一为
/// 300ms EaseInOutCubic（对标各原语的 animationSpec 参数，自定义后续接）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavTransitionSpec {
    pub enter: NavEnter,
    pub exit: NavExit,
}

impl NavTransitionSpec {
    /// 构造一对过渡（对标 ContentTransform 的字段构造）
    pub fn new(enter: NavEnter, exit: NavExit) -> Self {
        Self { enter, exit }
    }

    /// Nav3 NavDisplay 默认过渡：fadeIn togetherWith fadeOut（Nav3 用
    /// tween(700)——winia 共享 300ms EaseInOutCubic）
    pub fn fade() -> Self {
        Self { enter: NavEnter::FadeIn, exit: NavExit::FadeOut }
    }

    /// 无过渡（EnterTransition.None togetherWith ExitTransition.None——瞬时切换）
    pub fn none() -> Self {
        Self { enter: NavEnter::None, exit: NavExit::None }
    }

    /// Android 经典视差滑动（Activity/Fragment 平台过渡形态）——返回 (push, pop)
    /// 一对：push = 全幅右入 togetherWith 左 30% 视差滑出+淡出；pop = 左 30% 视差
    /// 滑入 togetherWith 全幅右出。（Nav3 官方文档示例为全幅对称、无淡出——
    /// 本预设取平台形态，视差+淡出观感更接近原生导航）
    pub fn horizontal_slide() -> (Self, Self) {
        (
            Self {
                enter: NavEnter::SlideIn { initial_offset_x: SlideOffset::Fraction(1.0) },
                exit: NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Fraction(-0.3) },
            },
            Self {
                enter: NavEnter::SlideIn { initial_offset_x: SlideOffset::Fraction(-0.3) },
                exit: NavExit::SlideOut { target_offset_x: SlideOffset::Fraction(1.0) },
            },
        )
    }

    /// M3 shared-axis（30 逻辑 px 反向小位移 + 淡入淡出）——返回 (push, pop) 一对
    pub fn shared_axis() -> (Self, Self) {
        (
            Self {
                enter: NavEnter::SlideAndFadeIn { initial_offset_x: SlideOffset::Px(30.0) },
                exit: NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Px(-30.0) },
            },
            Self {
                enter: NavEnter::SlideAndFadeIn { initial_offset_x: SlideOffset::Px(-30.0) },
                exit: NavExit::SlideAndFadeOut { target_offset_x: SlideOffset::Px(30.0) },
            },
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
/// 共享 tween（300ms EaseInOutCubic）当前硬编码——对标各原语 animationSpec
/// 参数的自定义时长/曲线后续接（见 docs/navigation3.md）。
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
    /// 中途改配置不影响进行中的过渡；完成时清空）
    active_spec: State<Option<NavTransitionSpec>>,
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
            current_key: ctx.remember(move || State::new(initial_key)).get(),
            previous: ctx.remember(|| State::new(None)).get(),
            forward: ctx.remember(|| State::new(true)).get(),
            // 初值 0 = 无过渡（渲染层以此判定静置归位；导航时 detect 复位 1.0）
            progress: ctx.remember(|| State::new(0.0)).get(),
            active_spec: ctx.remember(|| State::new(None)).get(),
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
                self.progress.set_silent(0.0);
                self.active_spec.set_silent(None);
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
                self.active_spec.set_silent(Some(*spec));
                // 复位进度起点 1.0（旧页全显）——上次动画结束 progress 停在 0，
                // 不复位则 push_animatable 见 peek==target(0) 直接跳过、动画不启动。
                // ⚠ 仅在无进行中动画时复位：过渡中途再次导航时，旧动画与新动画
                // 同目标(0.0)、会被 push_animatable 去重保留并按原时间轴继续——
                // 此时若复位 1.0，本帧会渲染出 p=1 的满血旧页、下一帧 tick 又弹回
                // 中途值（一帧闪跳）；跳过复位则从中途值平滑续走。
                if !crate::animation::has_animation_for_state(self.progress.state_id()) {
                    self.progress.set_silent(1.0);
                }
                // 过渡动画：1→0（对标各原语共享的 tween——300ms EaseInOutCubic）
                push_animatable(
                    self.progress.clone(),
                    0.0,
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(300),
                        crate::animation::interpolator::EaseInOutCubic::new(),
                    )),
                );
            }
            self.current_key.set(target.key);
        }
        // 完成检测：过渡结束 → 移除旧页、清规格快照
        if self.previous.peek().is_some() && self.progress.peek() < 0.001 {
            self.previous.set(None);
            self.active_spec.set_silent(None);
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
        // 过渡规格：优先用启动时固化的快照；无进行中过渡时用当前配置
        // （此时 previous 为 None，所有公式在 active 门下归位，取值无效果）
        let spec = self.active_spec.peek().unwrap_or(*spec);
        let prev = self.previous.peek();
        let forward = self.forward.peek();
        // 过渡进行中 = previous 非空——cur 层位移以此为门：静置（启动/无过渡）
        // 时 progress 不保证为 0（无过渡必须归位）
        let active = prev.is_some();
        // 容器宽度（fill_max_size 层的测量宽）——首帧测量先于渲染，peek 即得真值
        let width = ctx.remember(|| State::new(0.0f32)).get();
        // 渲染单个过渡层（graphics_layer 动画闭包——每帧 peek progress/width 零重组）
        let render_layer = |ctx: &mut ComposeCtx, scene: &dyn Scene<K>, is_prev: bool| {
            let progress = self.progress.clone();
            let width = width.clone();
            let m = Modifier::new().fill_max_size().graphics_layer(move || {
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
                    }
                }
                params
            });
            crate::ui::layout_components::Column::new()
                .modifier(m)
                .build(ctx, |ctx| {
                    // 场景 key 驱动槽身份（对标 Nav3 AnimatedSceneKey(KClass, key)
                    // ——场景切换 = 组合身份切换）：策略链切换时 scene key 变化 →
                    // 槽换新 → 退场/进场层各自新鲜渲染（否则内层组 params 未变会
                    // Skip 重放旧子树）。场景内部的 pane 级 draining（如 detail
                    // pane 退场层）与场景层的 is_prev（整层滑出）取或——任一为真
                    // 即状态池只读
                    ctx.key(scene.scene_key(), |ctx| {
                        let layer_render = |ctx: &mut ComposeCtx, e: &NavEntry<K>, draining: bool| {
                            render_entry(ctx, e, draining || is_prev);
                        };
                        scene.content(ctx, &layer_render);
                    });
                });
        };
        crate::ui::layout_components::Stack::new()
            .modifier(Modifier::new().fill_max_size().on_size_changed({
                let width = width.clone();
                move |w, _| width.set(w)
            }))
            .build(ctx, |ctx| {
                if forward {
                    // push：新场景在上层（对标 Nav3 "z-index increases during navigate"）
                    if let Some(p) = previous_scene {
                        render_layer(ctx, p, true);
                    }
                    render_layer(ctx, current_scene, false);
                } else {
                    // pop：被弹出的旧场景在上层（对标 Nav3 "decreases during pop"）
                    render_layer(ctx, current_scene, false);
                    if let Some(p) = previous_scene {
                        render_layer(ctx, p, true);
                    }
                }
                // 过渡期输入屏蔽层：命中测试不计 graphics_layer 位移（布局命中盒
                // 停在原位）——滑动场景的按钮过渡期可被误触（如连点返回清空栈）。
                // 全尺寸可点击层兜底吞掉过渡期全部点击（透明、无波纹）
                if active {
                    crate::ui::layout_components::Column::new()
                        .modifier(Modifier::new().fill_max_size().clickable(|| {}))
                        .build(ctx, |_| {});
                }
            });
    }
}

// ═══════════════════════════════════════════════════════════
// NavEntryDecorator — entry 包装/生命周期（对标 Nav3 的 NavEntryDecorator）
// ═══════════════════════════════════════════════════════════

/// 导航条目装饰器——包装 entry 内容 + 生命周期回调。
///
/// 对标 Nav3 `NavEntryDecorator(onPop, decorate)`：
/// - `on_pop`：entry 从 back stack 移除时回调（对标 Nav3 onPop——如状态清理）。
///   广播语义：列表内全部装饰器依次收到回调。参数为 **contentKey**（对标 Nav3
///   onPop(contentKey)——按内容身份清理）。
/// - `wrap`：包装 entry 内容（对标 Nav3 decorate）。**链式语义**：列表按顺序
///   依次包裹（首个装饰器最外层），内层是后续装饰器与 entry 内容的组合——
///   每个装饰器的 wrap 都会生效，而非只有最后一个。
pub trait NavEntryDecorator<K: NavKey>: Send + Sync + 'static {
    /// entry 从 back stack 移除时（对标 Nav3 onPop(contentKey)）
    fn on_pop(&self, _content_key: u64) {}

    /// 包装 entry 内容（对标 Nav3 decorate）——在自身作用域内调用 `inner`
    /// （inner 渲染后续装饰器包裹的 entry 内容）。默认原样渲染。
    fn wrap(&self, ctx: &mut ComposeCtx, _entry: &NavEntry<K>, inner: &dyn Fn(&mut ComposeCtx)) {
        inner(ctx);
    }
}

/// 按列表顺序链式应用装饰器的 wrap（首个最外层），末层渲染 entry 内容。
fn wrap_entry<K: NavKey>(
    ctx: &mut ComposeCtx,
    entry: &NavEntry<K>,
    decorators: &[Box<dyn NavEntryDecorator<K>>],
) {
    fn rec<K: NavKey>(
        ctx: &mut ComposeCtx,
        entry: &NavEntry<K>,
        decorators: &[Box<dyn NavEntryDecorator<K>>],
        i: usize,
    ) {
        match decorators.get(i) {
            None => entry.build(ctx),
            Some(d) => {
                let inner = |ctx: &mut ComposeCtx| rec(ctx, entry, decorators, i + 1);
                d.wrap(ctx, entry, &inner);
            }
        }
    }
    rec(ctx, entry, decorators, 0);
}

/// 状态保持装饰器——用**固定组合 key** 包裹 entry 内容。
///
/// 对标 Nav3 `SaveableStateHolderNavEntryDecorator`（SaveableStateProvider(contentKey)）：
/// 同一 contentKey 的 entry 每次渲染都在相同组合 key 位置——组合 key 与
/// `remember_entry_state` 状态池槽都按 contentKey 关联，跨 pop/push 保持。
/// 默认随 NavDisplay 启用（对标 Nav3 的默认
/// rememberSaveableStateHolderNavEntryDecorator）。
///
/// ⚠ 保持范围仅限 `remember_entry_state`：entry 内容里的普通 `ctx.remember`
/// 存放在组合槽中，而槽位身份按**组合位置**分配（winia 槽表无 movableContent
/// 级跨位置身份——对标 Nav3 SceneSetupNavEntryDecorator）——双层过渡渲染时
/// 两层内容共享调用序基，pop/push 边界会触发槽 truncate 重建，普通 remember
/// 不保证存活。
pub struct RememberStateDecorator;

impl<K: NavKey> NavEntryDecorator<K> for RememberStateDecorator {
    fn wrap(&self, ctx: &mut ComposeCtx, entry: &NavEntry<K>, inner: &dyn Fn(&mut ComposeCtx)) {
        ctx.key(entry.content_key(), |ctx| inner(ctx));
    }
}

/// 生命周期装饰器——entry 是否在 back stack 中决定渲染。
///
/// 对标 Nav3 `BackStackAwareLifecycleNavEntryDecorator`（栈内 RESUMED / 栈外
/// CREATED）：winia 无 Lifecycle 系统，等价语义 = 仅渲染栈内 entry（SinglePane
/// 天然只渲染栈顶；ListDetail 渲染前两个）。此装饰器目前是占位——真正的
/// 生命周期作用域（如后台 entry 冻结）后续可扩展。
pub struct BackStackAwareDecorator;

impl<K: NavKey> NavEntryDecorator<K> for BackStackAwareDecorator {
    fn on_pop(&self, _content_key: u64) {
        // 占位：entry 移除时清理钩子（后续扩展：取消动画/释放资源；
        // 对标 Nav3 BackStackAwareLifecycle——离栈 CREATED 封顶，见路线图 P1-8）
        debug_log!("[nav] entry popped: content_key={}", _content_key);
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
        crate::ui::layout_components::Row::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                crate::ui::layout_components::Column::new()
                    .modifier(Modifier::new().fill_max_height().layout_weight(2.0))
                    .build(ctx, |ctx| render_entry(ctx, list, false));
                crate::ui::layout_components::Column::new()
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
fn is_pop<K: NavKey>(old: &[K], new: &[K]) -> bool {
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
#[derive(Clone, Copy, PartialEq)]
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
        // 为整栈构建 entries（对标 Nav3 rememberDecoratedNavEntries——全量构建，
        // route 元信息映射需要非渲染位 entry 的 contentKey/过渡覆盖；导航栈短，
        // 代价可接受。⚠ entry_provider 应为纯函数：同 key → 同 contentKey/覆盖）
        let entries: Vec<NavEntry<K>> =
            stack.iter().map(|k| (self.entry_provider)(ctx, k)).collect();
        // route hash → entry 元信息映射（跨帧 remember——被移除 key 不在当前
        // entries 里：on_pop 的 contentKey、pop 方向的过渡覆盖均取上一帧映射）
        let route_meta: State<HashMap<u64, EntryRouteMeta>> = ctx.remember(|| HashMap::new());
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
        // 更新 route→entry 元信息映射（仅 peek/clone 读取——set_silent 免通知）
        route_meta.set_silent(
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
            calculate_scene(&self.scene_strategies, &entries[..base_len]).into()
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
                .and_then(|m| m.pop_transition_spec)
                .unwrap_or(self.pop_transition_spec)
        };
        // 上一帧渲染的场景对象（退场层来源）——读取须在 last_scene 更新前
        let last_scene: State<Option<SceneHolder<K>>> = ctx.remember(|| State::new(None)).get();
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
        let render_entry = |ctx: &mut ComposeCtx, entry: &NavEntry<K>, draining: bool| {
            let counter = ctx.remember(|| State::new(0u32)).get();
            counter.set_silent(0); // 每帧重置——seq 按 entry 内调用顺序分配（槽 key 稳定）
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
                if draining {
                    entry.build(ctx);
                } else {
                    wrap_entry(ctx, entry, decorators);
                }
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
        // 记录本帧场景（下一帧的退场场景来源）
        last_scene.set_silent(Some(scene_holder));
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
        let dialog_content: Option<(u64, NavEntry<K>)> = dialog_top.map(|t| (t.content_key(), t));
        crate::ui::overlay::Dialog::new(dialog_visible)
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
                    let counter = ctx.remember(|| State::new(0u32)).get();
                    counter.set_silent(0);
                    let scope = EntryStateScope {
                        pool: pool.clone(),
                        key: *ck,
                        counter,
                        draining: false,
                    };
                    ENTRY_STATE_SCOPE.provides(scope, || entry.build(ctx));
                }
            });
    }
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Eq, Debug, Hash)]
    enum TestRoute {
        Home,
        Detail(u64),
        Settings,
    }

    /// 遍历 arena 树收集所有 TextContent 文本（测试辅助）
    fn collect_texts(nodes: &[crate::layout::node::LayoutNode], root: usize, out: &mut Vec<String>) {
        let node = &nodes[root];
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::TextContent { content, .. } = el {
                out.push(content.clone());
            }
        }
        for &c in &node.children {
            collect_texts(nodes, c, out);
        }
    }

    /// isPop 列表差分（对标 NavDisplay.isPop）：前缀子集=pop、整栈替换/发散/更长
    /// =navigate；空栈侧 winia 扩展（空→非空=push、清空=pop）
    #[test]
    fn is_pop_direction_diffing() {
        // push：更长 → 非 pop
        assert!(!is_pop(&[TestRoute::Home], &[TestRoute::Home, TestRoute::Detail(1)]));
        // pop：前缀子集且更短
        assert!(is_pop(&[TestRoute::Home, TestRoute::Detail(1)], &[TestRoute::Home]));
        // 整栈替换（首元素不同）→ 非 pop
        assert!(!is_pop(&[TestRoute::Home, TestRoute::Detail(1)], &[TestRoute::Settings]));
        // 等长但中途发散 → 非 pop（替换）
        assert!(!is_pop(
            &[TestRoute::Home, TestRoute::Detail(1)],
            &[TestRoute::Home, TestRoute::Detail(2)]
        ));
        // 前缀相同但发散且不长于旧栈 → 非 pop（[A,B,C]→[A,X]）
        assert!(!is_pop(
            &[TestRoute::Home, TestRoute::Detail(1), TestRoute::Settings],
            &[TestRoute::Home, TestRoute::Detail(2)]
        ));
        // 空栈侧（winia 扩展）
        assert!(!is_pop::<TestRoute>(&[], &[TestRoute::Home]), "空栈 push → 非 pop");
        assert!(is_pop(&[TestRoute::Home], &[]), "清空 → pop");
        // 同栈无变化 → 非 pop
        assert!(!is_pop(&[TestRoute::Home], &[TestRoute::Home]));
    }

    #[test]
    fn back_stack_push_pop_works() {
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        assert_eq!(bs.top(), Some(TestRoute::Home));
        assert_eq!(bs.len(), 1);
        bs.push(TestRoute::Detail(42));
        assert_eq!(bs.len(), 2);
        assert_eq!(bs.top(), Some(TestRoute::Detail(42)));
        assert_eq!(bs.pop(), Some(TestRoute::Detail(42)));
        assert_eq!(bs.top(), Some(TestRoute::Home));
        assert_eq!(bs.pop(), Some(TestRoute::Home));
        assert!(bs.is_empty());
        assert_eq!(bs.pop(), None, "空栈 pop 返回 None");
    }

    #[test]
    fn back_stack_remove_last_and_clear() {
        let bs = NavBackStack::<TestRoute>::new();
        bs.push(TestRoute::Home);
        bs.push(TestRoute::Detail(1));
        bs.remove_last();
        assert_eq!(bs.top(), Some(TestRoute::Home));
        bs.clear();
        assert!(bs.is_empty());
        bs.set_stack(vec![TestRoute::Detail(9), TestRoute::Detail(8)]);
        assert_eq!(bs.len(), 2);
        assert_eq!(bs.top(), Some(TestRoute::Detail(8)));
    }

    #[test]
    fn nav_display_renders_top_entry() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id; // 复制（&u64 → u64）——move 闭包需 owned
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 推进动画帧（过渡动画需时间——push/pop 后等滑动完成）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };

        // 栈顶是 Home——树里应有 HomeScreen 文本
        build(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "SinglePane 应渲染栈顶 HomeScreen");

        // push Detail(7) → 推进动画 → 渲染 Detail7
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("Detail7")), "push 后应渲染 Detail7");
        // pop → 回到 Home
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "pop 后应回到 HomeScreen");
        assert!(!texts.iter().any(|t| t.contains("Detail7")), "pop 后 Detail7 应移除");
    }

    /// ListDetail 双栏 Scene：栈 ≥2 时同时渲染 list（倒数第二）与 detail（栈顶）
    #[test]
    fn list_detail_scene_renders_both_entries() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, bs: &NavBackStack<TestRoute>| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![Box::new(ListDetailStrategy)])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        // 栈 1 项：SinglePane（只有 Home）
        build(&mut composer, &bs);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")));
        assert!(!texts.iter().any(|t| t.contains("Detail")), "栈 1 项无双栏");
        // 推进动画帧（场景化后所有场景变化都走过渡——含 ListDetail↔Single）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer, &bs);
            }
        };
        // push Detail(5)：栈 2 项 → ListDetail 场景（Home 在左栏 + Detail5 在右栏）
        bs.push(TestRoute::Detail(5));
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "双栏左栏渲染 list（Home）");
        assert!(texts.iter().any(|t| t.contains("Detail5")), "双栏右栏渲染 detail（Detail5）");
        // push Settings：栈 3 项 → 场景过渡（detail 变化即场景变化——按
        // transition_spec 整屏过渡；场景只渲染末两位：list=Detail5、detail=Settings，
        // Home 不在新场景内。回归覆盖 first()/get(1) 错位 bug）
        bs.push(TestRoute::Settings);
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("Detail5")), "栈 3 项左栏应=倒数第二（Detail5）");
        assert!(texts.iter().any(|t| t.contains("SettingsScreen")), "栈 3 项右栏应=栈顶（Settings）");
        assert!(!texts.iter().any(|t| t.contains("HomeScreen")), "场景只渲染末两位，Home 不应出现");
        // pop Settings：场景过渡反向，回 [H,D5] 双栏（Home 回到 list pane）
        bs.pop();
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")) && texts.iter().any(|t| t.contains("Detail5")),
            "pop 后回 [H,D5] 双栏");
        assert!(!texts.iter().any(|t| t.contains("SettingsScreen")), "Settings 应已移除");
        // pop 回 1 项 → 场景边界（ListDetail→SinglePane）有过渡，完成后仅 Home
        bs.pop();
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")));
        assert!(!texts.iter().any(|t| t.contains("Detail")), "pop 过渡完成后无双栏");
    }

    /// RememberStateDecorator：entry 内 remember 状态跨 pop/push 保持
    /// （固定组合 key——对标 Nav3 SaveableStateHolder 的 contentKey 状态保持）
    #[test]
    fn remember_state_decorator_preserves_entry_state() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                    NavEntry::new(key.clone(), move |ctx, _| {
                        // 状态池保持（对标 Nav3 SaveableStateHolder）
                        let counter = remember_entry_state(|| 0i32);
                            counter.update(|v| *v += 1);
                            crate::ui::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let read_counter = |composer: &mut Composer| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = "Detail7-counter";
                t.find(p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).expect("应渲染 Detail7-counterN")
        };

        // 场景 1（Nav3 核心语义——覆盖返回保持）：Detail(7) → 计数累加 → push
        // Settings（Detail 仍在栈中）→ pop 回 Detail(7) → counter 应从旧值继续
        // （Detail 未从 back stack 移除——状态池保持，对标 Nav3 同一 entry
        // 生命周期内状态保持）
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let first = read_counter(&mut composer);
        assert!(first > 0, "首次进入 counter 应 >0，实际 {first}");
        bs.push(TestRoute::Settings); // 覆盖 Detail（不 pop）
        build(&mut composer);
        advance(&mut composer);
        bs.pop(); // 回到 Detail(7)
        build(&mut composer);
        advance(&mut composer);
        let covered = read_counter(&mut composer);
        assert!(
            covered > first,
            "覆盖返回保持：counter 应从 {first} 继续（实际 {covered}）"
        );

        // 场景 2（Nav3 语义——pop 清理）：pop Detail（从 back stack 移除 →
        // removeState 清理）→ 再 push 同一 Detail(7) → counter 重置（从 0 重新计数）
        bs.pop(); // 移除 Detail(7)
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let fresh = read_counter(&mut composer);
        assert!(
            fresh <= covered,
            "pop 后重进应重置（Nav3 removeState 语义）——covered={covered} 实际 {fresh}"
        );
    }

    /// 过渡中间帧：push/pop 后新旧两页应同树（Stack 双页层叠——对标
    /// AnimatedContent 双内容过渡），动画完成后旧页移除。
    /// 显式采用 horizontal_slide（Android 形态）——默认 fade 由
    /// `transition_spec_fade_and_none` 覆盖。
    #[test]
    fn transition_midframe_renders_both_pages() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (push_spec, pop_spec) = NavTransitionSpec::horizontal_slide();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(push_spec)
                .pop_transition_spec(pop_spec)
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 单帧推进（~60ms——300ms 过渡的中段），整轮推进（>300ms——过渡完成）
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(composer);
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // push 中间帧：HomeScreen（旧页滑出）+ Detail7（新页滑入）同树
        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        tick(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "push 中间帧应含旧页 HomeScreen");
        assert!(t.iter().any(|x| x.contains("Detail7")), "push 中间帧应含新页 Detail7");
        for _ in 0..12 { tick(&mut composer); }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "push 完成后旧页应移除");

        // pop 中间帧：Detail7（滑出）+ HomeScreen（滑入）同树；完成后 Detail7 移除
        bs.pop();
        build(&mut composer);
        tick(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "pop 中间帧应含新页 HomeScreen");
        assert!(t.iter().any(|x| x.contains("Detail7")), "pop 中间帧应含滑出旧页 Detail7");
        for _ in 0..12 { tick(&mut composer); }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail7")), "pop 完成后旧页应移除");
    }

    /// dialog 标记 entry（对标 Nav3 `dialog()` metadata / DialogSceneStrategy）：
    /// 栈顶 dialog entry 渲染为模态覆盖层——**主树不渲染其内容**（测试 composer
    /// 不物化 overlay）；base 场景不受对话框开/关影响；连续 dialog 栈仅栈顶
    /// 渲染覆盖层；pop 即关闭、无残留
    #[test]
    fn dialog_entry_renders_as_overlay() {
        #[derive(Clone, PartialEq, Eq, Debug, Hash)]
        enum DialogRoute {
            Home,
            About,
        }
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<DialogRoute>::with_initial(DialogRoute::Home);
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    DialogRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    DialogRoute::About => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("AboutScreen").build(ctx);
                    })
                    .as_dialog(),
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 开对话框：主树只渲染 base（Home）——About 内容不进主树（覆盖层由
        // app 层独立物化，测试 composer 不渲染）；base 场景不变（无过渡）
        build(&mut composer);
        bs.push(DialogRoute::About);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "base 场景应正常渲染");
        assert!(!t.iter().any(|x| x.contains("AboutScreen")), "dialog 内容不应进主树");
        // 连续 dialog 栈：仅栈顶渲染覆盖层，base 仍为 Home
        bs.push(DialogRoute::About);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("AboutScreen")), "连续 dialog 仅覆盖层，主树无内容");
        // pop 一个对话框：主树不变、无残留
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        // pop 到底：回到纯 base
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("AboutScreen")));
    }

    /// P1 回归：对话框弹到**空栈**后程序化 pop 关闭——Dialog::build 必须仍
    /// 无条件执行（记录 active=false → app 层 sync 删除 overlay），不得泄漏。
    /// 修复前 `if stack.is_empty() { return }` 提前返回 → 无 active=false 记录
    /// → overlay 永久残留（可见、拦截点击）。
    #[test]
    fn dialog_on_empty_stack_close_records_inactive() {
        #[derive(Clone, PartialEq, Eq, Debug, Hash)]
        enum DialogRoute {
            About,
        }
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<DialogRoute>::new(); // 空栈开始
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    DialogRoute::About => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("AboutScreen").build(ctx);
                    })
                    .as_dialog(),
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        // 空栈：build 执行（无 scene、无 dialog）——Dialog::build(false) 应记录 active=false
        build(&mut composer);
        let inactive_ids: Vec<u64> = composer.overlay_active.iter()
            .filter(|(_, a)| !**a)
            .map(|(&id, _)| id)
            .collect();
        // 空栈首帧无 overlay 记录（从未注册）——只验证不 panic
        build(&mut composer);

        // 弹入对话框（栈 [About]——唯一 entry）
        bs.push(DialogRoute::About);
        build(&mut composer);
        let active_ids: Vec<u64> = composer.overlay_active.iter()
            .filter(|(_, a)| **a)
            .map(|(&id, _)| id)
            .collect();
        assert_eq!(active_ids.len(), 1, "对话框应注册 active overlay");
        let ov_id = active_ids[0];

        // 程序化 pop 关闭（按钮路径——不经过 app 层 overlay_down）
        bs.pop();
        build(&mut composer);
        // ⚠ P1 修复点：栈空后 Dialog::build 仍执行 → overlay_active 记录
        // (ov_id, false) → app 层 sync_overlays 能删除该 overlay。
        // 断言"记录存在且为 false"——`unwrap_or(false)` 会把"本帧无记录"
        // （修复前：空栈 early-return → Dialog::build 不执行）与"记录为
        // false"混为一谈，测试对 P1 不敏感。必须区分：
        // 修复前 get=Some(true)（上帧残留）或 None（被 clear）→ 断言失败；
        // 修复后 get=Some(&false) → 通过
        assert_eq!(
            composer.overlay_active.get(&ov_id),
            Some(&false),
            "对话框关闭后必须记录 active=false（ov_id={ov_id}）——否则 overlay 泄漏"
        );
    }

    /// pop 滑出期间旧页内容每帧重跑——不得把已 removeState 清理的池槽重新插回
    /// （回归测试：draining 只读作用域。组合期写入的内容最易触发重污染——
    /// 若无 draining，滑出 12 帧会把计数累加进重插的槽，重进首帧远大于 1）
    #[test]
    fn pop_slideout_does_not_repollute_entry_state_pool() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            // 组合期写入（重污染路径的最小复现）
                            let counter = remember_entry_state(|| 0i32);
                            counter.update(|v| *v += 1);
                            crate::ui::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let read_counter = |composer: &mut Composer| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = "Detail7-counter";
                t.find(p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).expect("应渲染 Detail7-counterN")
        };

        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        bs.pop(); // removeState 清理 → 滑出 12 帧（draining 不得写池）
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer); // 重进首帧（未推进动画——读到的即首帧值）
        let first = read_counter(&mut composer);
        assert_eq!(
            first, 1,
            "pop 完成后重进：池应为空（滑出层未重污染），首帧计数恰为 1——实际 {first}"
        );
    }

    /// contentKey（对标 NavEntry.contentKey）：同 contentKey 的不同路由共享状态池
    /// 槽（Nav3 语义：同内容 = 同状态）；默认（key hash）不同路由相互独立；
    /// 同 contentKey 多实例时弹出其一不清理（"最后一个实例"语义）
    #[test]
    fn content_key_shares_state_across_routes() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        let key = key.clone();
                        // Detail(1)/Detail(2) 共享 contentKey=42（同内容语义）；
                        // Detail(3) 用默认（key hash）——独立状态
                        let entry = if id <= 2 {
                            NavEntry::with_content_key(key, 42, move |ctx, _| {
                                let counter = remember_entry_state(|| 0i32);
                                counter.update(|v| *v += 1);
                                crate::ui::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                            })
                        } else {
                            NavEntry::new(key, move |ctx, _| {
                                let counter = remember_entry_state(|| 0i32);
                                counter.update(|v| *v += 1);
                                crate::ui::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                            })
                        };
                        entry
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let counter_of = |composer: &mut Composer, id: u64| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = format!("Detail{id}-counter");
                t.find(&p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).unwrap_or_else(|| panic!("应渲染 Detail{id}-counterN"))
        };

        // Detail(1)（ck=42）计数累加
        bs.push(TestRoute::Detail(1));
        build(&mut composer);
        advance(&mut composer);
        let n1 = counter_of(&mut composer, 1);
        assert!(n1 > 0, "Detail(1) 首轮应计数 >0");
        // push Settings 覆盖 → pop → Detail(1) 计数保持（同 ck 槽未清理）
        bs.push(TestRoute::Settings);
        build(&mut composer);
        advance(&mut composer);
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let n1b = counter_of(&mut composer, 1);
        assert!(n1b > n1, "覆盖返回保持：{} 应 > {n1}", n1b);
        // push Detail(2)（同 ck=42）：共享同一池槽——计数从 Detail(1) 的值继续
        bs.push(TestRoute::Detail(2));
        build(&mut composer);
        advance(&mut composer);
        let n2 = counter_of(&mut composer, 2);
        assert!(n2 > n1b, "同 contentKey 应共享池槽：Detail(2) 计数 {n2} 应 > Detail(1) 的 {n1b}");
        // 弹出 Detail(2) 时 Detail(1)（同 ck）仍在栈——不清理（最后实例语义）
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let n1c = counter_of(&mut composer, 1);
        assert!(n1c >= n2, "同 ck 多实例弹出其一不清理：Detail(1) {n1c} 应 ≥ {n2}");
        // 全部弹出（ck=42 槽清理）→ push Detail(3)（默认 ck）：全新状态
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(3));
        build(&mut composer);
        let n3 = counter_of(&mut composer, 3);
        assert_eq!(n3, 1, "默认 contentKey 独立状态：首帧计数应恰为 1——实际 {n3}");
    }

    /// 过渡规格：默认 fade（Nav3 NavDisplay 默认——fadeIn togetherWith fadeOut）
    /// 中间帧双页同树；`Spec::none()`（EnterTransition.None togetherWith
    /// ExitTransition.None）瞬时切换——无旧页、无动画
    #[test]
    fn transition_spec_fade_and_none() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let build_with = |composer: &mut Composer, spec: NavTransitionSpec| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(spec)
                .pop_transition_spec(spec)
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
        };

        // —— 默认 fade：中间帧双页同树，完成后旧页移除 ——
        build_with(&mut composer, NavTransitionSpec::fade());
        bs.push(TestRoute::Detail(7));
        build_with(&mut composer, NavTransitionSpec::fade());
        tick(&mut composer);
        build_with(&mut composer, NavTransitionSpec::fade());
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "fade 中间帧应含旧页");
        assert!(t.iter().any(|x| x.contains("Detail7")), "fade 中间帧应含新页");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build_with(&mut composer, NavTransitionSpec::fade());
        }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "fade 完成后旧页应移除");

        // —— none：瞬时切换（无旧页层、无动画）——
        build_with(&mut composer, NavTransitionSpec::none());
        bs.pop();
        bs.push(TestRoute::Settings);
        build_with(&mut composer, NavTransitionSpec::none());
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")), "none 应立即渲染新页");
        assert!(!t.iter().any(|x| x.contains("Detail7")), "none 不应有旧页残留");
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "none 不应渲染栈外页");
    }

    /// 策略链切换（如自适应模式切换）：退场层 = 上一帧的**场景对象快照**——
    /// 形态不随新策略链重建（SinglePane→ListDetail 切换时退场层仍是单栏画面，
    /// 对标 Nav3 sceneMap 缓存场景实例）；每次切换的过渡收敛、可反复切换
    /// （回归覆盖"退场场景按当前链重建形态突变"）
    #[test]
    /// 策略链切换（如自适应模式切换）：**entries 未变 → 瞬时切换**（形态切
    /// 无过渡——双层同渲同 contentKey 内容会在非宏节点键空间 dup-key，且形态
    /// 切瞬切符合平台惯例）；切换后单帧即稳态、可反复切换
    #[test]
    fn scene_strategy_switch_converges() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let list_detail = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![Box::new(ListDetailStrategy)])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let single = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .build(ctx); // 空链 → SinglePane 兜底
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 阶段 1（single）：push D5 → 场景过渡（fade）→ 收敛后单栏仅栈顶
        single(&mut composer);
        bs.push(TestRoute::Detail(5));
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            single(&mut composer);
        }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail5")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "SinglePane 收敛后只渲染栈顶");

        // 阶段 2（切 ListDetail）：entries 未变 → 瞬时切换，单帧即双栏稳态
        list_detail(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "切双栏应瞬时稳态 H+D5");

        // 阶段 3（切回 single）：瞬时回单栏（H 退场）
        single(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail5")) && !t.iter().any(|x| x.contains("HomeScreen")),
            "切回单栏瞬时仅栈顶");

        // 阶段 4（再切双栏）：反复切换后仍稳态（无冻结/无残留）
        list_detail(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "二次切双栏仍稳态（无冻结）");
    }

    /// SceneStrategy 策略链（对标 Nav3 `List<SceneStrategy>` + SinglePane 兜底）：
    /// 自定义策略优先接手（自定义 Scene 渲染全部 entries）、ListDetail 次之
    /// （<2 条返回 None 落空）、SinglePane 兜底——验证链序与回退语义
    #[test]
    fn scene_strategy_chain_priority_and_fallback() {
        use crate::core::composer::Composer;

        /// 三栏场景：一列渲染全部 entries（自定义 Scene 形态）
        struct TriPaneScene<K: NavKey> { entries: Vec<NavEntry<K>> }
        impl<K: NavKey> Scene<K> for TriPaneScene<K> {
            fn scene_key(&self) -> u64 {
                fnv_hash(&("winia/test/TriPaneScene", self.entries.len()))
            }
            fn entries(&self) -> &[NavEntry<K>] { &self.entries }
            fn content(
                &self,
                ctx: &mut ComposeCtx,
                render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
            ) {
                crate::ui::layout_components::Column::new().build(ctx, |ctx| {
                    for e in &self.entries {
                        render_entry(ctx, e, false);
                    }
                });
            }
        }
        /// 栈 ≥3 时接手（三栏）
        struct TriPaneStrategy;
        impl<K: NavKey> SceneStrategy<K> for TriPaneStrategy {
            fn calculate_scene(&self, entries: &[NavEntry<K>]) -> Option<Box<dyn Scene<K>>> {
                (entries.len() >= 3).then(|| Box::new(TriPaneScene { entries: entries.to_vec() }) as Box<dyn Scene<K>>)
            }
        }

        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![
                    Box::new(TriPaneStrategy),
                    Box::new(ListDetailStrategy),
                ])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 栈 1 项：TriPane 落空、ListDetail 落空 → SinglePane 兜底
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail")), "栈 1 项应 SinglePane 兜底");

        // 栈 2 项：TriPane 落空 → ListDetail 接手（list+detail 双栏）
        bs.push(TestRoute::Detail(5));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "栈 2 项应 ListDetail 接手");

        // 栈 3 项：TriPane 接手（自定义场景渲染全部 entries）
        bs.push(TestRoute::Settings);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(
            t.iter().any(|x| x.contains("HomeScreen"))
                && t.iter().any(|x| x.contains("Detail5"))
                && t.iter().any(|x| x.contains("SettingsScreen")),
            "栈 3 项应自定义 TriPaneScene 接手（渲染全部 entries）"
        );
    }

    /// entry 级过渡覆盖（对标 Nav3 NavDisplay.TransitionKey/PopTransitionKey
    /// metadata 优先级"过渡中 entry > NavDisplay 默认"）：push 前景 = 新栈顶、
    /// pop 前景 = 被弹出的旧条目。覆盖 `none()` 生效的判别：单帧内无旧页层
    /// （若覆盖失效回退默认 fade，旧页会保留一整个过渡期）
    #[test]
    fn entry_transition_override_wins_over_display_default() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        // Detail 覆盖双向为 none（瞬时）——Settings 不覆盖（对照组）
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                        .transition_spec(NavTransitionSpec::none())
                        .pop_transition_spec(NavTransitionSpec::none())
                    }
                })
                .build(ctx); // 默认 fade（display 级）
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
        };

        build(&mut composer);
        // push Detail（覆盖 none）→ 单帧内旧页即移除
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")),
            "entry transition_spec 覆盖应生效（none 瞬时，无旧页层）");

        // 对照：push Settings（无覆盖）→ 默认 fade → 中间帧双页
        bs.push(TestRoute::Settings);
        build(&mut composer);
        tick(&mut composer);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "无覆盖 entry 应回退 NavDisplay 默认 fade（中间帧双页）");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }

        // pop Settings（无 pop 覆盖）→ 默认 fade → 中间帧双页
        bs.pop();
        build(&mut composer);
        tick(&mut composer);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "无覆盖 entry 的 pop 应回退默认 fade");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }

        // pop Detail（覆盖 pop none）→ 单帧内旧页即移除
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail7")),
            "entry pop_transition_spec 覆盖应生效（none 瞬时，无旧页层）");
    }

    /// shared_axis（M3 30px 反向小位移 + 淡入淡出）——覆盖 `SlideOffset::Px`
    /// 求值与 `SlideAndFade*` 原语分支：中间帧双页同树、完成后旧页移除
    #[test]
    fn transition_shared_axis_midframe() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (push_spec, pop_spec) = NavTransitionSpec::shared_axis();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(push_spec)
                .pop_transition_spec(pop_spec)
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };

        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(60));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "shared_axis 中间帧应含旧页");
        assert!(t.iter().any(|x| x.contains("Detail7")), "shared_axis 中间帧应含新页");
        advance(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "shared_axis 完成后旧页应移除");
    }

    /// 方向选择语义：push 用 `transition_spec`（滑动——中间帧双页）、pop 用
    /// `pop_transition_spec`（none——瞬时单页）；两个 setter 必须独立生效
    #[test]
    fn transition_specs_selected_per_direction() {
        use crate::core::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (slide_push, _slide_pop_unused) = NavTransitionSpec::horizontal_slide();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(slide_push)
                .pop_transition_spec(NavTransitionSpec::none())
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // push：走 slide spec——中间帧双页同树
        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(60));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "push 应走 transition_spec（滑动中间帧双页）");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }
        // pop：走 none spec——瞬时单页（若误用 slide，单帧内仍有旧页层）
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "pop 后应渲染 Home");
        assert!(!t.iter().any(|x| x.contains("Detail7")), "pop 应走 pop_transition_spec（none 瞬时，无旧页层）");
    }
}
