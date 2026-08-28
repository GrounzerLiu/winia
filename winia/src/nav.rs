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
//! - 过渡动画暂未接入（winia 有 AnimatedContent/Crossfade 可后续接）。

use crate::core::state::State;
use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;

// ═══════════════════════════════════════════════════════════
// NavKey — 路由标识（对标 Nav3 的 NavKey 接口）
// ═══════════════════════════════════════════════════════════

/// 导航路由标识——类型安全取代字符串路由。
///
/// 用户定义自己的路由类型（enum 或 data struct），实现本 trait：
/// ```rust
/// #[derive(Clone, PartialEq, Eq, Debug)]
/// enum Route {
///     Home,
///     Detail(u64),
/// }
/// // Route 自动满足 NavKey（trait 无方法，仅约束）
/// ```
///
/// 对标 Nav3：`NavKey` 是标记接口（`@Serializable` 用于持久化）。winia 无
/// 序列化要求，trait 仅作类型约束（`Clone + PartialEq + Eq + Debug + 'static`）。
pub trait NavKey: Clone + PartialEq + Eq + std::fmt::Debug + 'static {}

impl<T: Clone + PartialEq + Eq + std::fmt::Debug + 'static> NavKey for T {}

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

    /// 内部 State 引用（NavDisplay 用它注册依赖）
    pub(crate) fn state(&self) -> &State<Vec<K>> {
        &self.state
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

/// 一个导航条目：key + 内容构建闭包。
///
/// 对标 Nav3 `NavEntry(key, contentKey, metadata, content)`——winia 简化：
/// 只保留 key + content（metadata/contentKey 后续按需加）。
/// content 为 `'static`（闭包捕获 owned 数据，不借用外部——对标 Nav3 的
/// @Composable content 无生命周期依赖）。
pub struct NavEntry<K: NavKey> {
    key: K,
    content: Box<dyn Fn(&mut ComposeCtx, &K) + 'static>,
}

impl<K: NavKey> NavEntry<K> {
    pub fn new(key: K, content: impl Fn(&mut ComposeCtx, &K) + 'static) -> Self {
        Self { key, content: Box::new(content) }
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    /// 渲染内容（传入 key）
    pub fn build(&self, ctx: &mut ComposeCtx) {
        (self.content)(ctx, &self.key);
    }
}

// ═══════════════════════════════════════════════════════════
// Scene / SceneStrategy — 多 entry 渲染（对标 Nav3 的 Scene/SceneStrategy）
// ═══════════════════════════════════════════════════════════

/// Scene 布局计划——由 SceneStrategy 根据 back stack 计算。
///
/// 对标 Nav3：`SceneStrategy.calculateScene(entries) -> Scene`——Scene 可渲染
/// 一个或多个 entry（多栏/自适应布局）。winia 用 enum 表达（而非 trait object，
/// 同 Modifier 的 enum 哲学——便于 match 分派与测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenePlan {
    /// 渲染单个 entry（栈顶）——对标 SinglePaneSceneStrategy
    Single,
    /// 双栏：list = 栈倒数第二，detail = 栈顶——对标 ListDetailSceneStrategy
    /// （大屏列表 + 详情同时显示；无次栈顶时退化为 Single）
    ListDetail,
    /// 渲染空（空栈）
    Empty,
}

/// Scene 策略——根据 back stack 决定布局（对标 Nav3 `SceneStrategy`）。
///
/// 用法：实现后传给 `NavDisplay::scene_strategy(...)`。
/// 返回 `ScenePlan::Single`（默认语义）时渲染栈顶；`ListDetail` 时双栏。
pub trait SceneStrategy<K: NavKey>: Send + Sync + 'static {
    /// 根据当前栈计算布局计划（stack 非空时调用）
    fn plan(&self, stack: &[K]) -> ScenePlan;
}

/// 默认 SinglePane 策略——渲染栈顶（对标 Nav3 SinglePaneSceneStrategy）
pub struct SinglePaneStrategy;

impl<K: NavKey> SceneStrategy<K> for SinglePaneStrategy {
    fn plan(&self, _stack: &[K]) -> ScenePlan {
        ScenePlan::Single
    }
}

/// ListDetail 策略——宽屏双栏（列表 + 详情）。
///
/// 对标 Nav3 的 ListDetailSceneStrategy（大屏 list-detail 布局）：
/// - 栈 ≥2 项：list = 倒数第二，detail = 栈顶，双栏并排
/// - 栈 1 项：Single（无列表可显示）
pub struct ListDetailStrategy;

impl<K: NavKey> SceneStrategy<K> for ListDetailStrategy {
    fn plan(&self, stack: &[K]) -> ScenePlan {
        if stack.len() >= 2 {
            ScenePlan::ListDetail
        } else {
            ScenePlan::Single
        }
    }
}

// ═══════════════════════════════════════════════════════════
// NavDisplay — 状态的投影（对标 Nav3 的 NavDisplay）
// ═══════════════════════════════════════════════════════════

/// 导航显示——观察 back stack，用 entry_provider 生成内容，按 SceneStrategy 渲染。
///
/// 对标 Nav3 `NavDisplay(backStack, entryProvider, sceneStrategy, ...)`：
/// - `back_stack`：观察的导航状态（读注册依赖——变化自动重组）
/// - `entry_provider`：路由 → NavEntry 的映射（对标 Nav3 的 entryProvider）
/// - `scene_strategy`：Scene 计算（默认 SinglePane——渲染栈顶 entry；
///   `ListDetailStrategy` 可双栏）
///
/// 用法：
/// ```rust
/// NavDisplay::new(&back_stack, |ctx, key| match key {
///     Route::Home => NavEntry::new(key.clone(), |ctx, _| Text::new("Home").build(ctx)),
///     ...
/// })
/// .scene_strategy(ListDetailStrategy)
/// .build(ctx);
/// ```
pub struct NavDisplay<'a, K: NavKey> {
    back_stack: &'a NavBackStack<K>,
    entry_provider: Box<dyn Fn(&mut ComposeCtx, &K) -> NavEntry<K> + 'a>,
    scene_strategy: Box<dyn SceneStrategy<K>>,
}

impl<'a, K: NavKey> NavDisplay<'a, K> {
    pub fn new(
        back_stack: &'a NavBackStack<K>,
        entry_provider: impl Fn(&mut ComposeCtx, &K) -> NavEntry<K> + 'a,
    ) -> Self {
        Self {
            back_stack,
            entry_provider: Box::new(entry_provider),
            scene_strategy: Box::new(SinglePaneStrategy),
        }
    }

    /// 设置 Scene 策略（默认 SinglePane；ListDetailStrategy 双栏）。
    /// 接收 `Box<dyn SceneStrategy<K>>`——支持运行时切换策略（如自适应布局）。
    pub fn scene_strategy(mut self, strategy: Box<dyn SceneStrategy<K>>) -> Self {
        self.scene_strategy = strategy;
        self
    }

    /// 便捷：具体类型策略自动装箱（`ListDetailStrategy`、`SinglePaneStrategy`）
    pub fn scene_strategy_of<S: SceneStrategy<K>>(mut self, strategy: S) -> Self {
        self.scene_strategy = Box::new(strategy);
        self
    }

    /// 渲染当前 Scene（默认 SinglePane：栈顶 entry；ListDetail：双栏；空栈渲染空）
    /// SinglePane 用 Crossfade 过渡（栈顶变化时交叉淡化——对标 Nav3 的
    /// transitionSpec 的淡化语义；方向性滑动过渡后续可扩展）。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // 观察 back stack——变化触发重组
        let stack = self.back_stack.stack();
        if stack.is_empty() {
            return; // 空栈：渲染空
        }
        // SceneStrategy 决定布局
        match self.scene_strategy.plan(&stack) {
            ScenePlan::Single => {
                // 过渡目标：当前栈顶 key（Crossfade 动画）
                let target: State<Option<K>> = ctx.remember(|| stack.last().cloned());
                let new_top = stack.last().cloned();
                if target.get() != new_top {
                    target.set(new_top);
                }
                let provider = &self.entry_provider;
                crate::ui::crossfade::Crossfade::new(target).build(ctx, |ctx, top| {
                    if let Some(top) = top {
                        let entry = (provider)(ctx, &top);
                        entry.build(ctx);
                    }
                });
            }
            ScenePlan::ListDetail => {
                // 双栏：list = 倒数第二，detail = 栈顶（Row 并排）
                let detail = stack.last().unwrap();
                let list = &stack[stack.len() - 2];
                let detail_entry = (self.entry_provider)(ctx, detail);
                let list_entry = (self.entry_provider)(ctx, list);
                                crate::ui::layout_components::Row::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        // 左栏：list entry（weight 2）；右栏：detail entry（weight 3）
                        crate::ui::layout_components::Column::new()
                            .modifier(Modifier::new().fill_max_height().layout_weight(2.0))
                            .build(ctx, |ctx| list_entry.build(ctx));
                        crate::ui::layout_components::Column::new()
                            .modifier(Modifier::new().fill_max_height().layout_weight(3.0))
                            .build(ctx, |ctx| detail_entry.build(ctx));
                    });
            }
            ScenePlan::Empty => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 单元测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Eq, Debug)]
    enum TestRoute {
        Home,
        Detail(u64),
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
        // 推进动画帧（Crossfade 过渡需时间——push/pop 后等淡化完成）
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
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, bs: &NavBackStack<TestRoute>| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::ui::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::ui::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategy_of(ListDetailStrategy)
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
        // push Detail(5)：栈 2 项 → ListDetail 双栏（Home 在左栏 + Detail5 在右栏）
        bs.push(TestRoute::Detail(5));
        build(&mut composer, &bs);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "双栏左栏渲染 list（Home）");
        assert!(texts.iter().any(|t| t.contains("Detail5")), "双栏右栏渲染 detail（Detail5）");
        // pop 回 1 项 → SinglePane
        bs.pop();
        build(&mut composer, &bs);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")));
        assert!(!texts.iter().any(|t| t.contains("Detail")), "pop 后无双栏");
    }
}
