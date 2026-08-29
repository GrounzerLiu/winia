# winia 导航层（Navigation3 风格）

> 对标 Jetpack Navigation 3（androidx.navigation3，2025-11 稳定 1.0）的状态驱动导航设计，
> 在 winia 的命令式组合架构下的适配实现。
> 实现位置：`winia/src/nav.rs`（`NavKey` / `NavBackStack` / `NavEntry` / `NavDisplay`）

## 一、Navigation3 源码核心设计（研究结论）

来源：AOSP `navigation3/`（androidx-main 分支，稀疏克隆研究）。

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

- 单元测试（nav.rs，10 个）：
  - `back_stack_push_pop_works`、`back_stack_remove_last_and_clear`（back stack 操作）
  - `nav_display_renders_top_entry`（Home → push Detail7 → pop 回 Home，含过渡动画推进）
  - `list_detail_scene_renders_both_entries`（ListDetail 双栏：栈 2 项时 list+detail 同渲染；pop 回 1 项退化为 SinglePane）
  - `remember_state_decorator_preserves_entry_state`（状态池：覆盖返回保持 + pop 清理双场景）
  - `transition_midframe_renders_both_pages`（过渡中间帧：新旧两页同树，完成后旧页移除）
  - `pop_slideout_does_not_repollute_entry_state_pool`（滑出层 draining 只读——池不被重污染）
  - `transition_spec_fade_and_none`（默认 fade 中间帧；`Spec::none()` 瞬时切换）
  - `transition_shared_axis_midframe`（shared_axis + `SlideOffset::Px` 路径）
  - `transition_specs_selected_per_direction`（push 用 transition_spec / pop 用 pop_transition_spec）
- 真实 demo（`examples/nav_demo.rs`，debug-server 实操）：
  - Home → 点「打开 Detail 42」→ `[Home, Detail(42)]` + Detail 页面 ✓
  - Detail → 点「返回」→ `[Home]` + Home 页面 ✓
  - **ListDetail 模式**：双栏渲染（Home 左栏 x=16 / Detail 右栏 x=243）✓
  - **模式切换**：SinglePane ↔ ListDetail 实时生效（Detail 从右栏 x=243 变单栏 x=16）✓
  - 多次导航循环稳定 ✓；动画埋点日志确认 push/pop 方向、完成清理、静置归零 ✓
- 完整 `cargo test -p winia --lib` 650 项通过（1 个 animated_size 时序敏感 flaky 与 nav 无关）

## 五、后续扩展方向

1. [x] **SceneStrategy trait**：`ScenePlan`（Single/ListDetail/Empty）+ `SceneStrategy` trait +
   `SinglePaneStrategy`/`ListDetailStrategy`——`NavDisplay::scene_strategy(Box<dyn>)` 支持运行时切换
2. [x] **过渡动画**：NavTransition 双页过渡——`transition_spec` / `pop_transition_spec`
   （对标 Nav3 transitionSpec / popTransitionSpec 的 ContentTransform：`NavEnter`/`NavExit`
   原语成对组合，`SlideOffset` 比例/dp 表达位移，默认 fade）——Android 全幅滑动、
   M3 shared-axis 等形态经原语组合表达（`horizontal_slide()` / `shared_axis()` 便捷构造）
3. [x] **NavEntryDecorator / 状态保持**：`NavEntryDecorator` trait（on_pop 广播 + wrap 链式）+
   `RememberStateDecorator` + **`remember_entry_state` 状态池**（对标 Nav3 SaveableStateHolder）
4. [ ] **过渡规格补全**：自定义时长/曲线（对标各原语的 animationSpec 参数）、
   per-entry metadata 覆盖、predictivePopTransitionSpec（预测性返回）
5. [ ] **deep link**：back stack 恢复/合成（对标 Nav3 的 NavBackStackSerializer）
6. [ ] **生命周期装饰器**：`BackStackAwareDecorator` 目前为占位（Nav3 BackStackAwareLifecycle—
   栈内 RESUMED/栈外 CREATED 语义）

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
完整测试 650 项通过。
