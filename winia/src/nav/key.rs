use super::*;

pub trait NavKey: Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static {}

impl<T: Clone + PartialEq + Eq + std::hash::Hash + std::fmt::Debug + Send + Sync + 'static> NavKey for T {}

// ═══════════════════════════════════════════════════════════
// NavMetadata — 导航元数据（对标 Nav3 `Map<String, Any>` + NavMetadataKey）
// ═══════════════════════════════════════════════════════════

/// 导航元数据——entry/scene 携带的任意类型化附加信息。
///
/// 对标 Nav3 `metadata { put(Key, value) }`（`Map<String, Any>` + 类型化
/// `NavMetadataKey<T>` 键）：Kotlin 因泛型擦除用字符串键 + 运行时 cast；
/// winia 用 **TypeId 做键**（`HashMap<TypeId, Box<dyn Any>>`）——类型安全、
/// 无碰撞、无需用户声明 key 类型。语义等价：同类型同值 = 同 metadata 项。
///
/// 用途（对齐 Nav3 各 NavMetadataKey 消费者）：
/// - 过渡覆盖（Nav3 TransitionKey/PopTransitionKey——winia 已有 typed 字段，
///   通用 metadata 开放自定义键）
/// - scene 级 metadata（Scene.metadata 默认 = 栈顶 entry 的 metadata）
/// - 自定义扩展（如 DialogProperties、deep link 参数等）
///
/// ```rust
/// // 定义元数据类型
/// struct MyMeta { value: u32 }
///
/// // entry 携带
/// NavEntry::new(key, content)
///     .metadata(NavMetadata::new().with(MyMeta { value: 42 }));
///
/// // 读取（类型安全）
/// let meta = entry.metadata_ref();
/// assert_eq!(meta.get::<MyMeta>().map(|m| m.value), Some(42));
/// ```
#[derive(Clone, Default, Debug)]
pub struct NavMetadata {
    /// ⚠ 值强制 Send+Sync：metadata 随 `Arc<dyn Scene>` 跨帧持有（Scene 可能被
    /// 线程池/动画 tick 访问），且与 `SceneStrategy: Send + Sync` 约束对齐。
    /// 单线程场景略保守，但避免将来多线程化时重构。
    map: std::collections::HashMap<std::any::TypeId, std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

impl NavMetadata {
    /// 空元数据
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入一项（返回 self——链式 builder；同类型覆盖）
    pub fn with<T: 'static + Send + Sync>(mut self, value: T) -> Self {
        self.map.insert(std::any::TypeId::of::<T>(), std::sync::Arc::new(value));
        self
    }

    /// 读取（类型安全——TypeId 精确匹配，无 cast 失败）
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map.get(&std::any::TypeId::of::<T>())
            .map(|v| v.downcast_ref::<T>().expect("TypeId 相同但 downcast 失败——内部错误"))
    }

    /// 是否含某类型
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&std::any::TypeId::of::<T>())
    }

    /// 项数
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════
// NavDialogProperties — 对话框属性（对标 Nav3 `dialog(dialogProperties)`）
// ═══════════════════════════════════════════════════════════

/// 对话框属性（对标 Nav3 `DialogSceneStrategy.DialogKey` metadata 携带的
/// `DialogProperties`——winia 取桌面相关子集）。
///
/// Nav3 DialogProperties 完整字段（dismissOnBackPress / dismissOnClickOutside /
/// usePlatformDefaultWidth / decorFitsSystemWindows / predictiveBack 等）多为
/// Android 平台概念（系统 back 手势、窗口装饰、预测性返回）；winia 桌面场景
/// 仅 `dismiss_on_click_outside` 语义等价（winia overlay 点击外部 dismiss）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavDialogProperties {
    /// 点击对话框外部是否关闭（对标 Compose Dialog 默认
    /// dismissOnClickOutside=true；winia overlay Dialog 同语义）
    pub dismiss_on_click_outside: bool,
}

impl Default for NavDialogProperties {
    fn default() -> Self {
        Self { dismiss_on_click_outside: true }
    }
}

// ═══════════════════════════════════════════════════════════
// ResultEventBus — 页面返回结果（对标 Nav3 result API 的降级版）
// ═══════════════════════════════════════════════════════════

/// 页面返回结果总线（对标 Nav3 `ResultEventBus`/`ResultEffect` 的 winia 降级版）。
///
/// Nav3 用协程 Channel + Flow（`sendResult`/`ResultEffect` 持续订阅）；winia
/// 无协程——降级为 **最新结果存储 + 一次性消费（take）**：
/// - `send(key, result)`：存最新结果（覆盖同 key 旧值——对标 Channel 的
///   conflate 语义）
/// - `take(key)`：读取并**消费**（取走——A 在 B pop 后重组时读一次）
/// - `peek(key)`：只看不消费
///
/// 典型用法（A push B，B 返回时带结果）：
/// ```rust
/// // B 中（返回前）：
/// result_bus.send("edit-result", EditResult::Saved(id));
/// bs.pop();
///
/// // A 中（B pop 后重组——每次 build 读）：
/// if let Some(r) = result_bus.take::<EditResult>("edit-result") {
///     // 处理结果
/// }
/// ```
///
/// winia 组合模型下 entry 内容每次重组都重跑——take 的"读一次"语义天然匹配
/// "返回后处理一次结果"，无需持续订阅（对标 ResultEffect 的协程收集）。
///
/// 结果**不跨进程/配置变更持久**（对齐 Nav3 语义）。
#[derive(Clone, Default)]
pub struct ResultEventBus {
    inner: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Box<dyn std::any::Any + Send + Sync>>>>,
}

impl ResultEventBus {
    /// 创建空总线
    pub fn new() -> Self {
        Self::default()
    }

    /// 发送结果（覆盖同 key 旧值——对标 Nav3 `sendResult`）
    pub fn send<T: Send + Sync + 'static>(&self, key: impl Into<String>, result: T) {
        let key = key.into();
        self.inner.lock().unwrap().insert(key, Box::new(result));
    }

    /// 读取并消费（取走——再次 take 返回 None；对标 Nav3 conflateAsState
    /// 的"最新结果"语义 + 一次性处理）。
    ///
    /// ⚠ **类型必须与 send 的 T 一致**：key 是字符串，`take::<T>` 在 downcast
    /// 失败时返回 None——但**结果已被取走销毁**（先 remove 后 downcast）。
    /// 用错类型 = 结果永久丢失且无提示。推荐用 [`result_event_bus_consume`]
    /// （类型安全高级原语）。
    pub fn take<T: 'static>(&self, key: &str) -> Option<T> {
        let mut m = self.inner.lock().unwrap();
        m.remove(key).and_then(|b| b.downcast::<T>().ok().map(|b| *b))
    }

    /// 只看不消费（可多次读）——返回克隆值（T: Clone）。
    /// 类型不匹配返回 None（同 [`Self::take`]——不 panic）
    pub fn peek<T: Clone + 'static>(&self, key: &str) -> Option<T> {
        let m = self.inner.lock().unwrap();
        m.get(key)
            .and_then(|b| b.downcast_ref::<T>())
            .cloned()
    }

    /// 移除某 key 的结果
    pub fn remove(&self, key: &str) {
        self.inner.lock().unwrap().remove(key);
    }

    /// 清空所有结果
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// 当前结果数
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

/// 结果总线 CompositionLocal——NavDisplay 渲染 entry 时 provides（对标 Nav3
/// `LocalResultEventBus` + `ResultEventBusNavEntryDecorator`）；entry 内容经
/// [`result_event_bus`] 读取。try_current 区分作用域内外。
pub(super) static RESULT_EVENT_BUS_SCOPE: std::sync::LazyLock<crate::runtime::composition_local::CompositionLocal<ResultEventBus>> =
    std::sync::LazyLock::new(|| {
        crate::runtime::composition_local::CompositionLocal::new(|| {
            panic!("RESULT_EVENT_BUS_SCOPE 无默认值——必须经 NavDisplay 提供")
        })
    });

/// 读取当前 entry 的结果总线（entry 内容内调用——NavDisplay 已 provides）。
///
/// 对标 Nav3 `LocalResultEventBus.current`。每个 NavDisplay 实例一个总线
/// （remember 跨帧稳定）；entry 内容用 [`ResultEventBus::take`] 消费返回结果。
pub fn result_event_bus() -> ResultEventBus {
    RESULT_EVENT_BUS_SCOPE.current().clone()
}

/// 当前是否处于 **draining 渲染**（退场层——同 contentKey 已在别处渲染的
/// 滑出内容）。entry 内容内调用（NavDisplay 已 provides）。
///
/// 用途：**一次性消费的副作用应跳过 draining 帧**——如 [`ResultEventBus::take`]
/// （退场层每帧重跑会抢先消费结果，重进时取不到）；`remember_entry_state`
/// 写入同理（draining 池只读）。对标 Nav3 中退场内容与主内容共享 movable
/// 实例（副作用只发生一次）在 winia 槽表模型上的等价判断。
pub fn nav_is_draining() -> bool {
    DRAINING_SCOPE.try_current().unwrap_or(false)
}

/// 消费结果总线中的一次结果并**持久化到 entry 状态**（组合期调用——entry 内容内）。
///
/// 对标 Nav3 `ResultEffect`/`conflateAsState` 的 winia 高级原语——内部自动处理
/// 裸 [`ResultEventBus::take`] 的三个非显然要求：
/// 1. **跳过 draining 帧**（退场层每帧重跑会抢先消费结果）——内部用
///    [`nav_is_draining`] 守卫，退场帧不消费；
/// 2. **持久化到 `remember_entry_state`**（过渡进入层消费后，稳定帧 take 已为
///    None——结果存入状态，跨过渡稳定显示）；
/// 3. 类型安全消费（take 的 T 与 send 的 T 一致）。
///
/// 返回 `State<Option<T>>`：无结果 = None；消费到结果 = Some（**粘性**——
/// 持续保持到下次 send 覆盖或 entry 状态清理；重进 entry 无新结果时仍显示
/// 旧结果。与 Nav3 一次性发射语义不同——winia 取"最近结果状态"语义，
/// 如需一次性可在消费后手动 `received.set(None)`）。
///
/// ⚠ **同一 key 多消费者 = first-wins**：多个 entry（或同 entry 多次）consume
/// 同一 key 时，首个在非 draining 帧调用的消费者 take 到结果，其余为 None
/// （结果只消费一次）。
///
/// ```rust
/// // A 页收 B 页返回的结果（B pop 前 send("key", result)）
/// let result = nav::result_event_bus_consume::<MyResult>("key");
/// if let Some(r) = result.get() { /* 显示结果 */ }
/// ```
pub fn result_event_bus_consume<T: Clone + PartialEq + 'static>(
    key: &str,
) -> crate::runtime::state::State<Option<T>> {
    let stored: crate::runtime::state::State<Option<T>> = remember_entry_state(|| None);
    if nav_is_draining() {
        return stored;
    }
    if let Some(v) = result_event_bus().take::<T>(key) {
        stored.set(Some(v));
    }
    stored
}

/// draining 渲染标记作用域（NavDisplay 的 render_entry 按 draining 参数 provides）
pub(super) static DRAINING_SCOPE: std::sync::LazyLock<crate::runtime::composition_local::CompositionLocal<bool>> =
    std::sync::LazyLock::new(|| {
        crate::runtime::composition_local::CompositionLocal::new(|| false)
    });

/// Identity for one `NavDisplay` instance, used to namespace the scenes it publishes.
static NEXT_NAV_HOST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) fn next_nav_host_id() -> u64 {
    NEXT_NAV_HOST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The id a scene host publishes for ONE LAYER it composes: the host, the scene and the layer's role.
///
/// All three are needed. The registry the shared-transition system reads is process-global and keyed by id
/// alone, so hashing the scene key would let two hosts rendering the same route — or one host pushing the
/// same route twice, which puts the same scene key in both layers — publish the same id and overwrite each
/// other; the surviving entry then classifies the other end with the wrong role, which is the "paired the
/// leaving end with itself" failure this branch fixed.
pub(crate) fn layer_scene_id(host_id: u64, scene_key: u64, is_prev: bool) -> u64 {
    fnv_hash(&(host_id, scene_key, is_prev))
}


