# winia 导航层（Navigation3 风格）

> 对标 Jetpack Navigation 3（androidx.navigation3，2025-11 稳定 1.0）的状态驱动导航设计，
> 在 winia 的命令式组合架构下的适配实现。
> 实现位置：`winia/src/nav.rs`（`NavKey` / `NavBackStack` / `NavEntry` / `NavDisplay`）

## 一、Navigation3 源码核心设计（研究结论）

来源：AOSP `navigation3/`（androidx-main 分支，稀疏克隆研究）。

> ⚠ **对标基准更新（2026-08-30）**：本文 §一 最初基于 2025-11 的 Nav3 1.0。主线此后
> 经一轮大重构：**Scene/SceneStrategy 拆到独立包 `androidx.navigation3.scene`**（仍在
> navigation3-ui 模块内），NavDisplay 重写为 SeekableTransitionState 驱动的 scene 级
> AnimatedContent，新增 OverlayScene/预测性返回/Scene 生命周期/共享元素过渡/多
> back stack 拼接等能力。模块现为：navigation3-runtime（entry/backstack/decorator/
> metadata/entryProvider）+ navigation3-ui（NavDisplay + scene 包）+ navigation3-appstate
> （新占位）。与 winia 的完整差距见 **§七**，路线图已按差距重排（§五）。

### 1. NavBackStack = SnapshotStateList 包装

```kotlin
@Serializable(with = NavBackStackSerializer::class)
public class NavBackStack<T : NavKey> public constructor(internal val base: SnapshotStateList<T>) :
    MutableList<T> by base, StateObject by base, RandomAccess by base
```

- 包装 `SnapshotStateList<T>`——更新自动触发观察者重组
- 实现 `StateObject`——直接参与 Compose snapshot 系统
- **导航 = 改列表**：`backStack.add(key)` / `backStack.removeLastOrNull()`

### 2. NavKey = 标记接口

```kotlin
public interface NavKey  // 需 @Serializable（持久化用）
```
类型安全路由，取代 Nav2 的字符串路由。

### 3. NavEntry = key + contentKey + metadata + content

```kotlin
public class NavEntry<T : Any>(
    private val key: T,
    public val contentKey: Any = defaultContentKey(key),
    public val metadata: Map<String, Any> = emptyMap(),
    private val content: @Composable (T) -> Unit,
)
```

### 4. NavDisplay = 观察 List + entryProvider + sceneStrategy

```kotlin
@Composable
public fun <T : Any> NavDisplay(
    backStack: List<T>,
    onBack: () -> Unit,
    entryDecorators: List<NavEntryDecorator<T>>,
    sceneStrategy: SceneStrategy<T> = SinglePaneSceneStrategy(),
    transitionSpec / popTransitionSpec / predictivePopTransitionSpec,
    entryProvider: (key: T) -> NavEntry<T>,
)
```
- NavDisplay **观察 back stack 列表**，变化时更新 UI
- entryProvider：路由 → NavEntry 的映射
- sceneStrategy：计算 Scene（SinglePane 默认；Scene 可渲染多 entry——多栏/自适应）

### 5. Scene = key + entries + content

```kotlin
public interface Scene<T : Any> {
    public val key: Any
    public val entries: List<NavEntry<T>>   // 可渲染多个 entry
    public val content: @Composable () -> Unit
}
```

### 6. SinglePaneSceneStrategy

```kotlin
public class SinglePaneSceneStrategy<T : Any> : SceneStrategy<T> {
    override fun SceneStrategyScope<T>.calculateScene(entries: List<NavEntry<T>>): Scene<T> {
        return SinglePaneScene(
            key = entries.last().contentKey,
            entry = entries.last(),
            previousEntries = entries.dropLast(1),  // 过渡用
        )
    }
}
```
取栈顶渲染，previousEntries 用于过渡动画。

## 二、winia 适配设计决策

| 概念 | Nav3（Compose） | winia 实现 | 决策理由 |
|------|-----------------|-----------|---------|
| 路由标识 | `NavKey` 接口 + `@Serializable` | `NavKey` trait（`Clone + PartialEq + Eq + Hash + Debug + 'static`，空实现自动派生） | winia 无序列化要求；trait 约束即可（Hash 用于状态保持装饰器的固定组合 key） |
| back stack | `SnapshotStateList` 包装（细粒度 add/remove 通知） | `State<Vec<K>>` 包装（整体替换 + PartialEq 去重） | winia State 系统无细粒度列表；导航频率低，整体通知代价可接受 |
| 导航操作 | `add` / `removeLastOrNull` | `push` / `pop` / `remove_last` / `clear` / `set_stack` | 语义对齐，命名更明确 |
| 条目 | `NavEntry(key, contentKey, metadata, content)` | `NavEntry<K>(key, content: Box<dyn Fn(&mut ComposeCtx, &K)>)` | 去 metadata/contentKey（按需加）；content 为 `'static` 闭包（捕获 owned 数据） |
| 显示 | `NavDisplay(backStack, entryProvider, sceneStrategy, ...)` | `NavDisplay::new(&back_stack, entry_provider).build(ctx)` | `#[composable]` build；读 back_stack 注册依赖 → 变化自动重组 |
| Scene | Scene 可渲染多 entry（多栏；角色由 entry metadata 标注） | SinglePane（默认渲染栈顶）/ ListDetail（双栏，位置启发：倒数第二=list、栈顶=detail） | SceneStrategy trait 对标；角色标注（metadata）后续接 |
| 过渡动画 | AnimatedContent + transitionSpec/popTransitionSpec（ContentTransform = enter togetherWith exit；默认 fade） | `NavDisplay::transition_spec` / `pop_transition_spec` + NavTransitionSpec：`NavEnter`/`NavExit` 原语（None/Fade/Slide/Slide+Fade）成对组合，位移用 `SlideOffset::Fraction`(对标 `{ it }` 全宽闭包)/`Px` 表达；共享 300ms EaseInOutCubic；**spec 在过渡启动时快照固化**（对标 Nav3 求值时机）；层序对标 Nav3 方向 z 序（push 新页上/pop 旧页上，不支持用户 zIndex——androidx 亦忽略）；过渡期有输入屏蔽层（命中测试不计 graphics_layer 位移的兜底） | graphics_layer 渲染期 peek 零重组；自定义时长/曲线、per-entry metadata 覆盖、predictivePop 后续接 |

## 三、关键实现细节

### NavBackStack（winia）

```rust
#[derive(Clone)]
pub struct NavBackStack<K: NavKey> {
    state: State<Vec<K>>,   // Clone 共享内部 State
}
```

- **Clone 共享**：`State<Vec<K>>` 是 Arc 句柄，NavBackStack clone 后内部同一 State——
  按钮闭包可持有 clone 触发导航，NavDisplay 观察原栈，双方同步
- **push/pop 用 `state.update()`**：始终通知（无需 PartialEq 比较——栈内容变了）
- **观察者自动重组**：NavDisplay build 里 `back_stack.stack()`（`.get()`）注册依赖

### NavDisplay（winia）

```rust
pub struct NavDisplay<'a, K: NavKey> {
    back_stack: &'a NavBackStack<K>,
    entry_provider: Box<dyn Fn(&mut ComposeCtx, &K) -> NavEntry<K> + 'a>,
    scene_strategy: Box<dyn SceneStrategy<K>>,
    entry_decorators: Vec<Box<dyn NavEntryDecorator<K>>>,
    transition_spec: NavTransitionSpec,      // 默认 fade（对标 Nav3 默认）
    pop_transition_spec: NavTransitionSpec,  // pop 方向独立默认
}

#[composable]
pub fn build(self, ctx: &mut ComposeCtx) {
    let stack = self.back_stack.stack();   // 观察——变化触发重组
    match self.scene_strategy.plan(&stack) {
        ScenePlan::Single => {
            // NavTransition 双页过渡：spec 在启动时快照固化
            let transition = NavTransition::init(ctx, Some(top.clone()));
            let forward = stack.len() > prev.len();
            let spec = if forward { self.transition_spec } else { self.pop_transition_spec };
            transition.detect(&Some(top.clone()), forward, &spec);
            transition.render(ctx, provider, |ctx, key, entry, is_prev| {
                // EntryStateScope：draining=is_prev（滑出层只读）
                ENTRY_STATE_SCOPE.provides(scope, || wrap_entry(ctx, key, entry, decorators));
            }, &spec);
        }
        ScenePlan::ListDetail => { /* 双栏 Row：list（weight 2）+ detail（weight 3） */ }
        ScenePlan::Empty => {}
    }
}
```

## 四、验证

- 单元测试（nav.rs，17 个）：
  - `back_stack_push_pop_works`、`back_stack_remove_last_and_clear`（back stack 操作）
  - `nav_display_renders_top_entry`（Home → push Detail7 → pop 回 Home，含过渡动画推进）
  - `list_detail_scene_renders_both_entries`（ListDetail 双栏：栈 2 项时 list+detail 同渲染；pop 回 1 项退化为 SinglePane）
  - `remember_state_decorator_preserves_entry_state`（状态池：覆盖返回保持 + pop 清理双场景）
  - `transition_midframe_renders_both_pages`（过渡中间帧：新旧两页同树，完成后旧页移除）
  - `pop_slideout_does_not_repollute_entry_state_pool`（滑出层 draining 只读——池不被重污染）
  - `transition_spec_fade_and_none`（默认 fade 中间帧；`Spec::none()` 瞬时切换）
  - `transition_shared_axis_midframe`（shared_axis + `SlideOffset::Px` 路径）
  - `transition_specs_selected_per_direction`（push 用 transition_spec / pop 用 pop_transition_spec）
  - `is_pop_direction_diffing`（isPop 列表差分：前缀子集=pop、整栈替换/发散/更长=navigate）
  - `content_key_shares_state_across_routes`（同 contentKey 共享池槽、最后实例才清理、默认独立）
  - `entry_transition_override_wins_over_display_default`（entry 过渡覆盖优先级：push/pop 前景判别）
  - `scene_strategy_chain_priority_and_fallback`（自定义 Scene/策略链优先级 + SinglePane 兜底）
  - `scene_strategy_switch_converges`（策略链切换：entries 未变 → 瞬时切（防 dup-key），可反复切换无冻结）
  - `dialog_entry_renders_as_overlay`（as_dialog 标记：主树不渲染 dialog 内容、base 恒定、pop 生命周期）
- 完整 `cargo test -p winia --lib` 657 项通过（animated_size / lazy_column 等时序敏感 flaky 与 nav 无关）
- 真实 demo（`examples/nav_demo.rs`，debug-server 实操）：
  - Home → 点「打开 Detail 42」→ `[Home, Detail(42)]` + Detail 页面 ✓
  - Detail → 点「返回」→ `[Home]` + Home 页面 ✓
  - **ListDetail 模式**：双栏渲染（Home 左栏 x=16 / Detail 右栏 x=243）✓
  - **模式切换**：SinglePane ↔ ListDetail 实时生效（Detail 从右栏 x=243 变单栏 x=16）✓
  - 多次导航循环稳定 ✓；动画埋点日志确认 push/pop 方向、完成清理、静置归零 ✓


## 五、路线图（按差距分级，2026-08-30 重排）

已完成的基础层（1.0 基准时期）：SceneStrategy trait（ScenePlan 简化版）、NavTransition
spec 过渡、NavEntryDecorator（on_pop 广播 + wrap 链式）、remember_entry_state 状态池。
以下按 §七 差距分析重排：

**P0——核心语义差距（低成本高收益，先做）**
1. [x] **contentKey**：NavEntry 稳定内容 id（默认 = key 的确定性 hash，可经
   `NavEntry::with_content_key` 覆盖）——状态池槽、组合 key、on_pop 清理全部改按
   contentKey 关联。解锁 Nav3 语义：同 contentKey 的不同路由共享状态（如
   `Detail(id)` 全部共享一个编辑态）；同 key 不同 contentKey 也可共存
2. [x] **isPop 列表差分**：方向判定替换长度启发——首元素不同 = 整栈替换（非 pop）、
   新栈为旧栈前缀子集 = pop（对标 `NavDisplay.isPop`）；`set_stack` 整栈替换、
   等长替换不再误判方向/选错过渡规格。空栈侧扩展：旧栈空 = push、清空 = pop
   （winia 允许空栈，Nav3 require 非空）
3. [x] **entry 过渡覆盖**：`NavEntry::transition_spec` / `pop_transition_spec`
   builder（对标 Nav3 NavDisplay.TransitionKey/PopTransitionKey metadata 的类型化
   等价——typed 字段替代 Map<String,Any> 擦除）；优先级 = 过渡中 entry > NavDisplay
   默认（push 前景=新栈顶 / pop 前景=被弹旧条目，被弹条目覆盖取上一帧 route 元信息
   映射）
4. [x] **onPop 时机**——以 draining 方案等效解决并关闭：pop 帧清理 + 滑出层只读
   作用域已杜绝池重污染；Nav3 延迟到"离开组合"清理是因为 Compose 退出内容仍被
   组合，winia 的 draining 语义等价且更简单（见 §六 踩坑 7）

**P1——架构级差距（按需逐个对标）**
5. [x] **Scene trait + SceneStrategy 策略链**：`Scene` trait（scene_key/entries/
   content——自定义场景形态开放）+ 策略列表依次尝试、SinglePane 兜底（对标
   calculateSceneWithSinglePaneFallback）；ListDetail 转为内置策略（<2 条返回
   None 落空）；NavTransition 换 scene key 维度，旧场景由 entry keys 经策略链
   确定性重建；scene key 须含实现类型区分（对标 AnimatedSceneKey(KClass, key)）。
   **双栏动画（设计决断 2026-08-30）**：ListDetail 场景 key 含 list/detail 的
   contentKey——detail 变化即场景变化，走场景级过渡（按设置的 transition_spec；
   列表栏随场景参与过渡）。Compose 的 pane 级方案（sceneKey 恒定 + AnimatedPane，
   material3-adaptive）依赖 movableContentOf 与 contentKey 去重渲染——winia 槽表
   暂缺该基建（dup-key fail-fast / 非 composable 路径 remember 逐帧漂移，实测），
   pane 级 deferred 至 P1-9 落地后重评
6. [x] **对话框导航**（对标 Nav3 OverlayScene/DialogScene 的 winia 形）：
   `NavEntry::as_dialog()` 标记（类型化等价 `dialog()` metadata）——栈顶连续
   dialog entry 经 winia 顶层 overlay 基建（`ui::overlay::Dialog`）渲染为模态
   覆盖层（主树不渲染其内容、base 场景不变 → 对话框开/关无场景级过渡、
   dismiss = 弹栈）；偏差：单层覆盖（winia overlay v1）、无退出动画、
   覆盖层内容 plain remember 不跨帧持久（remember_entry_state 池化不受影响）
7. [ ] **rememberNavBackStack 持久化**：进程死亡/配置变更恢复（对标
   rememberSerializable + NavBackStackSerializer 开放多态——需 winia 序列化基建）
   ——**已评估不做（2026-09）**：需 NavKey Serialize + 开放多态 serializer 全套
   基建；桌面进程死亡恢复需求弱。待有真实需求再立项。
8. [ ] **Scene/entry 生命周期**：过渡期封顶 STARTED、落定 RESUMED、离栈 CREATED
   （需 winia Lifecycle 等价物；`BackStackAwareDecorator` 占位转正）
   ——**已评估不做（2026-09）**：为导航单引入 Lifecycle 系统不值；过渡期
   draining 只读已覆盖核心语义（滑出层不污染状态池）。
9. [ ] **movableContentOf 等价物**：槽表跨位置身份——entry 在组合树任意位置间移动
   状态不丢（plain remember 跨 pop/push 限制的根解，框架级工作）
   ——**已评估不做（2026-09）**：槽表跨位置身份是架构级重构；`remember_entry_state`
   池已覆盖"覆盖返回保持"主场景，无倒逼需求。
10. [ ] **预测性返回**：手势 seek + 取消/完成回放 + predictivePopTransitionSpec
    （桌面相关性低；seekable 过渡思想可单独借鉴）
    ——**已评估不做（2026-09）**：Android back 手势，桌面平台不对。

**P2——外围**
11. [ ] 共享元素过渡（SharedTransitionScope）/sizeTransform
    ——**已评估不做（2026-09）**：P2 外围，无倒逼需求。
12. [x] 多 back stack——NavBackStack 为普通值天然多实例；entries 拼接显示按需再加
13. [x] EntryProvider 类型化 DSL——winia 用 match 闭包（Rust 惯用，不追）

**补充缺口（2026-09-02 源码复核新增）**——此前文档未列：
14. [x] **result API**（Nav3 ResultEffect/ResultEventBus/ResultEventBusNavEntryDecorator）：
    页面返回结果传递（A → B → 返回 → A 收 B 的结果）。winia 降级版（无协程）：
    `ResultEventBus`（send/take 一次性消费 + peek——对标 conflateAsState 的
    "最新结果"语义）+ NavDisplay remember + `RESULT_EVENT_BUS_SCOPE`
    CompositionLocal 提供（entry 内容经 `result_event_bus()` 读取，含 dialog
    覆盖层）。winia 组合模型下 entry 每次重组重跑——take 语义天然匹配
    "返回后处理一次结果"，无需协程订阅
15. [ ] **navigation event**（Nav3 rememberNavigationEventState/NavigationBackHandler）：
    依赖独立 navigationevent 库 + Android 系统 back 手势——winia 桌面场景相关性低，
    仅可借鉴 SceneInfo/previousScenes 建模。成本：高，建议暂缓
    ——**已评估不做（2026-09）**：同预测性返回，平台不对。
16. [x] **通用 metadata**（Nav3 NavMetadataKey + metadata{} DSL）：`NavMetadata`
    = `HashMap<TypeId, Box<dyn Any>>`（TypeId 键类型安全——Kotlin 字符串键 +
    cast 的 Rust 等价）；NavEntry::metadata()/metadata_ref()、Scene::metadata()
    （默认 = 栈顶 entry 的 metadata——Nav3 语义）
17. [x] **SceneDecoratorStrategy**（Nav3 scene 级装饰，overlay 豁免）：给 scene 内容
    追加装饰（如底部导航栏）。`SceneDecoratorStrategy` trait（decorate_scene 包装
    返回新 scene）+ NavDisplay::scene_decorator_strategies/add_scene_decorator
    （链式应用，后加入在外层；对话框场景不经过）
18. [x] **Scene trait 补 previousEntries/metadata**：`Scene::previous_entries()`
    （默认空——winia 退场场景由 NavTransition 持有，功能等价；供自定义场景
    显式访问）+ `Scene::metadata()`（默认 = 栈顶 entry metadata）
19. [x] **DialogProperties 参数化**（Nav3 dialog(dialogProperties)：dismissOnBackPress/
    dismissOnClickOutside）：`NavDialogProperties`（桌面相关子集：
    dismiss_on_click_outside）+ `NavEntry::as_dialog_with(props)`；dismiss
    幂等防护不变

**过渡扩展（2026-09-02）**：
20. [x] **自定义时长/缓动**：`NavTransitionSpec::duration(Duration)` /
    `easing(Arc<dyn Interpolator>)`——用动画系统内置 29 个插值器
    （EaseIn/Out/InOut × Sine/Quad/Cubic/Quart/Quint/Expo/Circ/Back/Elastic/
    Bounce + Linear），不再硬编码 300ms EaseInOutCubic；spec 快照语义保持
21. [x] **Scale 原语**：`NavEnter::ScaleIn { initial_scale }` /
    `NavExit::ScaleOut { target_scale }`（围绕中心缩放——对标 Compose
    scaleIn/scaleOut）

## 六、状态保持实现（remember_entry_state 状态池）

**问题**：winia 的 `remember` 状态绑定**组合槽生命周期**——entry 的槽随子树移除销毁，
普通 `remember` 跨 pop/push 必然丢失（实测：真实 GUI 里 `ctx.key(key)` 包裹的 remember
在 pop 后重建，State id 漂移）。

**方案**（对标 Nav3 SaveableStateHolder 的外部状态池）：
- NavDisplay 持有 `Arc<Mutex<HashMap<(u64, u32, TypeId), Box<dyn Any>>>>` 状态池（remember 跨帧稳定）
- `remember_entry_state(init)`：从池读/建 `State<T>`——槽 key = (entry key hash, 调用序号, TypeId)
- pop 时 `clear_entry_state` 清理该 entry 的槽（对标 Nav3 removeState）
- entry 渲染时经 `ENTRY_STATE_SCOPE` CompositionLocal 提供作用域（池 + key + 槽计数器）；
  **滑出层（过渡中的旧页）用 draining 只读作用域**——命中返回现存槽、miss 不入池，
  防止滑出期间内容每帧重跑把刚清理的槽重新插回

**关键实现细节**（踩坑记录）：
1. **key hash 必须确定性**：`DefaultHasher` 种子随机——同一值每次 hash 不同 → 池永不命中。
   改用 FNV-1a 手写 hasher（与 winia mix_key 同源）。
2. **槽计数器每帧重置**：`counter.set_silent(0)` 在 entry 渲染前——seq 按"entry 内第 N 个
   状态"分配（每帧相同调用位置用相同 seq），否则跨帧累积导致槽 key 漂移。
3. **counter 读写用 peek/set_silent**：`get()` 注册依赖 + `set()` 通知会触发重组风暴
   （remember_entry_state 每帧被调用 → counter 变化 → 重组 → 无限循环）。
4. **池用 Arc<Mutex> 而非 State<HashMap>**：HashMap<Box<dyn Any>> 不满足 State 的
   Clone/PartialEq 约束；Arc<Mutex> 内部可变，仅需 remember 持久化。
5. **作用域用 CompositionLocal 而非裸 thread_local**（重构）：初版用裸
   `thread_local RefCell<Option<Scope>>` + 手动 set/None——panic 时残留、
   嵌套串扰、`expect` 运行时 panic。重构为 `CompositionLocal.provides`（PopGuard
   自动弹栈，panic/嵌套安全——对齐 winia Theme/Density/SelectionRegistrar 的
   作用域模式）+ `try_current`（作用域外优雅报错）。
6. **槽 key 含 TypeId**（类型安全）：槽 key = (entry key, 序号, TypeId)——不同
   类型不撞槽，命中即类型正确（downcast 失败 fail-fast，而非静默新建）。
7. **滑出层 draining 只读**（重污染防御）：pop 帧先清理池、后渲染——滑出的
   300ms 内旧页内容每帧重跑，`remember_entry_state` 若照常 miss→insert 会把
   刚清理的槽重新插回（下次 pop 才再清，破坏 removeState 语义）。滑出层作用域
   置 `draining=true`：命中返回现存槽、miss 返回不入池的一次性 State。
   回归测试：`pop_slideout_does_not_repollute_entry_state_pool`。

**Nav3 语义对齐**：
- **覆盖返回保持**：entry 在 back stack 中（被覆盖未 pop）→ 返回时状态保持
  （实测：Detail 计数 3 → Settings → 返回 → 计数 3）
- **pop 清理**：entry 从 back stack 移除 → 状态池清理 → 重新 push 全新状态
  （实测：返回 Home → 再进 Detail → 计数 0）

**验证**：单元测试 `remember_state_decorator_preserves_entry_state`（覆盖返回保持 + pop 清理
双场景）+ `pop_slideout_does_not_repollute_entry_state_pool`（draining 回归）；demo 实操验证；
完整测试 657 项通过。

## 七、差距分析（androidx-main 2026-08 vs winia，2026-08-30）

逐文件核对 androidx-main `navigation3`（runtime 11 文件 + ui/scene 包 14 文件）得出。
分级：✅ 已对齐 / **P0 核心语义**（低成本高收益）/ **P1 架构级** / **P2 外围** / 🔀 设计取舍不同（保留 winia 行为）。

### 已对齐 ✅
NavKey 标记接口、NavBackStack 状态包装、NavEntry、entryProvider（winia 用 match 闭包——
Rust 惯用）、NavEntryDecorator（on_pop 广播 + wrap 链式）、SaveableStateHolder 对标
（remember_entry_state 池 + draining）、ContentTransform 同构过渡规格（enter/exit 原语、
默认 fade、spec 快照、方向 z 序）、onBack 语义（winia 由用户直接 pop，等价）。

### P0 核心语义
| # | Nav3 机制（源码位置） | winia 现状 | 说明 |
|---|---|---|---|
| 1 | `contentKey`（NavEntry.kt：默认 `"$key:$class"`）——装饰器状态共享、过渡期 entry 去重、动画 key、onPop 判定全靠它 | 用 key hash 替代，不可覆盖 | 解锁：同 contentKey 不同路由共享状态；同 key 不同 contentKey 共存。[x] 已实现 |
| 2 | `isPop` 列表差分（NavDisplay.kt:898-909）：首元素不同=整栈替换非 pop；前缀子集=pop；发散=navigate | 长度启发（len 比较） | set_stack/等长替换会误判方向。[x] 已实现（空栈侧 winia 扩展：空→非空=push、清空=pop） |
| 3 | metadata typed key DSL（NavMetadataKey + metadata{}）；过渡覆盖优先级：过渡中 NavEntry.metadata > Scene.metadata(默认=栈顶 entry) > NavDisplay 默认 | 无通用 metadata；过渡覆盖已做类型化等价：NavEntry::transition_spec/pop_transition_spec（[x]） | per-scene/通用 metadata 待 Scene trait（P1-5） |
| 4 | onPop 时机：popped && 离开组合 && 该 contentKey 最后实例，反序回调（DecoratedNavEntries.kt:206-221） | pop 帧立即回调 + 清池 + draining 只读 | 等效已解决（[x] 关闭）；"最后实例"语义已随 contentKey 对齐 |

### P1 架构级
| # | Nav3 机制 | winia 现状 |
|---|---|---|
| 5 | Scene 为 trait（key/entries/previousEntries/content/metadata 默认=栈顶 entry）+ SceneStrategy 策略链（List 依次尝试，SinglePane 兜底）+ SceneDecoratorStrategy（scene 级装饰，overlay 豁免） | Scene trait + 策略链已对齐（[x]：scene_key/entries/content，SinglePane 兜底）；双栏动画=场景级过渡（pane 级 deferred 至 P1-9）；SceneDecoratorStrategy 与 scene 级 metadata 未做 |
| 6 | OverlayScene/DialogScene：覆盖层导航（overlaidEntries、onRemove suspend 退出动画、渲染于 AnimatedContent 之上、仅顶层 RESUMED、`dialog()` metadata 声明） | as_dialog 标记 + winia overlay 模态覆盖层（[x]：base 场景恒定、dismiss=弹栈、池化状态保持）；onRemove 退出动画/多层叠加未做 |
| 7 | rememberNavBackStack：rememberSerializable + NavBackStackSerializer 开放多态，进程死亡恢复 | 内存态 |
| 8 | Scene 生命周期：过渡期 STARTED/落定 RESUMED；离栈 entry 封顶 CREATED（BackStackAwareLifecycleNavEntryDecorator 真实现） | 无 Lifecycle 系统；同名 decorator 为占位 |
| 9 | movableContentOf（SceneSetupNavEntryDecorator）：entry 包为可移动内容，跨组合位置状态不丢——**winia plain remember 限制的 Compose 解** | 槽表按组合位置分配身份，无跨位置移动。实测另有两个关联缺口：①非 #[composable] 路径（Scene::content 等 trait 方法）的 remember 槽 key 逐帧漂移（计数器不重置），需 remember_at_key 显式 key；②场景/语句组 params 比较对无 PartialEq 的参数（&dyn Scene 等）不可见变化，关键语句需 ctx.key(scene_key) 强制 Enter——双栏 pane 级动画实验（2026-08-30）因此三处受限回退为场景级过渡 |
| 10 | 预测性返回：NavigationEvent + SeekableTransitionState.seekTo(progress) 手势跟手、取消/完成回放、predictivePopTransitionSpec(swipeEdge)（Android 默认 scaleOut(0.7)+spring fadeIn） | 无 |

### P2 外围
- 共享元素过渡（sharedTransitionScope + SharedEntryInSceneNavEntryDecorator）、sizeTransform、contentAlignment
- 多 back stack 拼接（rememberDecoratedNavEntries 多实例 + entries 相加——winia NavBackStack
  为普通值天然多实例，拼接显示按需）
- NavDisplay 只看 Scene.metadata 决定 ContentTransform；过渡期间同 entry 多 scene 只渲染
  z 最高者（LocalEntriesToExcludeFromCurrentScene 去重）

### 🔀 设计取舍不同（保留 winia 行为）
- **空栈**：Nav3 `require(isNotEmpty)`；winia 允许空栈渲染空（桌面友好）
- 整体替换 `State<Vec<K>>` vs SnapshotStateList 细粒度通知
- 声明式 NavTransitionSpec/原语 vs Kotlin 闭包 ContentTransform（语言能力差异）
- 过渡期输入屏蔽层（winia 命中测试不计 graphics_layer 位移的兜底——Compose 无此问题）
- 共享 tween 300ms vs Nav3 默认 tween(700)
