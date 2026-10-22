use super::*;

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
    pub(super) key: K,
    content_key: u64,
    /// push 方向过渡覆盖（对标 Nav3 `NavDisplay.TransitionKey` metadata——
    /// 本 entry 作为 push 前景时优先于 NavDisplay 默认）
    transition_spec: Option<NavTransitionSpec>,
    /// pop 方向过渡覆盖（对标 Nav3 `NavDisplay.PopTransitionKey` metadata——
    /// 本 entry 被弹出作为 pop 前景时优先于 NavDisplay 默认）
    pop_transition_spec: Option<NavTransitionSpec>,
    /// 对话框属性（对标 Nav3 `dialog()` metadata 的 DialogProperties）——
    /// Some = 本 entry 是对话框（栈顶时渲染为模态覆盖层）；None = 普通 entry
    dialog: Option<NavDialogProperties>,
    /// 通用类型化元数据（对标 Nav3 `metadata {}`——TypeId 键，类型安全）
    metadata: NavMetadata,
    content: std::sync::Arc<dyn Fn(&mut ComposeCtx, &K) + 'static>,
}

impl<K: NavKey> Clone for NavEntry<K> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            content_key: self.content_key,
            transition_spec: self.transition_spec.clone(),
            pop_transition_spec: self.pop_transition_spec.clone(),
            dialog: self.dialog,
            metadata: self.metadata.clone(),
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
            dialog: None,
            metadata: NavMetadata::new(),
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
            dialog: None,
            metadata: NavMetadata::new(),
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

    /// 附加通用类型化元数据（对标 Nav3 `metadata { put(...) }`——TypeId 键
    /// 类型安全；同类型覆盖）。过渡覆盖已有专用 typed 字段
    /// （[`Self::transition_spec`]/[`Self::pop_transition_spec`]），通用 metadata
    /// 用于自定义扩展（如 DialogProperties、deep link 参数）。
    pub fn metadata(mut self, metadata: NavMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// 读取元数据（场景级 metadata 默认 = 栈顶 entry 的 metadata——Nav3 语义）
    pub fn metadata_ref(&self) -> &NavMetadata {
        &self.metadata
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
        self.dialog = Some(NavDialogProperties::default());
        self
    }

    /// 带属性标记为对话框（对标 Nav3 `dialog(dialogProperties)` metadata——
    /// 可配置 dismiss_on_click_outside 等）
    pub fn as_dialog_with(mut self, props: NavDialogProperties) -> Self {
        self.dialog = Some(props);
        self
    }

    /// 是否为对话框 entry
    pub fn is_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// 对话框属性（None = 非对话框）
    pub fn dialog_properties(&self) -> Option<NavDialogProperties> {
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

    pub(super) fn transition_spec_override(&self) -> Option<NavTransitionSpec> {
        self.transition_spec.clone()
    }

    pub(super) fn pop_transition_spec_override(&self) -> Option<NavTransitionSpec> {
        self.pop_transition_spec.clone()
    }

    /// 渲染内容（传入 key）
    pub fn build(&self, ctx: &mut ComposeCtx) {
        (self.content)(ctx, &self.key);
    }
}

