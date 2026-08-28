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
| 路由标识 | `NavKey` 接口 + `@Serializable` | `NavKey` trait（`Clone + PartialEq + Eq + Debug + 'static`，空实现自动派生） | winia 无序列化要求；trait 约束即可 |
| back stack | `SnapshotStateList` 包装（细粒度 add/remove 通知） | `State<Vec<K>>` 包装（整体替换 + PartialEq 去重） | winia State 系统无细粒度列表；导航频率低，整体通知代价可接受 |
| 导航操作 | `add` / `removeLastOrNull` | `push` / `pop` / `remove_last` / `clear` / `set_stack` | 语义对齐，命名更明确 |
| 条目 | `NavEntry(key, contentKey, metadata, content)` | `NavEntry<K>(key, content: Box<dyn Fn(&mut ComposeCtx, &K)>)` | 去 metadata/contentKey（按需加）；content 为 `'static` 闭包（捕获 owned 数据） |
| 显示 | `NavDisplay(backStack, entryProvider, sceneStrategy, ...)` | `NavDisplay::new(&back_stack, entry_provider).build(ctx)` | `#[composable]` build；读 back_stack 注册依赖 → 变化自动重组 |
| Scene | Scene 可渲染多 entry（多栏） | SinglePane（渲染栈顶） | 多栏/自适应为后续扩展（SceneStrategy trait 预留） |
| 过渡动画 | AnimatedContent + transitionSpec | 未接（winia 有 AnimatedContent/Crossfade 可后续接） | 先验证状态驱动核心 |

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
}

#[composable]
pub fn build(self, ctx: &mut ComposeCtx) {
    let stack = self.back_stack.stack();   // 观察——变化触发重组
    if let Some(top) = stack.last() {       // SinglePane：渲染栈顶
        let entry = (self.entry_provider)(ctx, top);
        entry.build(ctx);
    }
}
```

## 四、验证

- 单元测试（nav.rs，4 个）：
  - `back_stack_push_pop_works`、`back_stack_remove_last_and_clear`（back stack 操作）
  - `nav_display_renders_top_entry`（Home → push Detail7 → pop 回 Home，含 Crossfade 过渡动画推进）
  - `list_detail_scene_renders_both_entries`（ListDetail 双栏：栈 2 项时 list+detail 同渲染；pop 回 1 项退化为 SinglePane）
- 真实 demo（`examples/nav_demo.rs`，debug-server 实操）：
  - Home → 点「打开 Detail 42」→ `[Home, Detail(42)]` + Detail 页面 ✓
  - Detail → 点「返回」→ `[Home]` + Home 页面 ✓
  - **ListDetail 模式**：双栏渲染（Home 左栏 x=16 / Detail 右栏 x=243）✓
  - **模式切换**：SinglePane ↔ ListDetail 实时生效（Detail 从右栏 x=243 变单栏 x=16）✓
  - 多次导航循环稳定 ✓
- 完整 `cargo test -p winia --lib` 643 项通过

## 五、后续扩展方向

1. [x] **SceneStrategy trait**：`ScenePlan`（Single/ListDetail/Empty）+ `SceneStrategy` trait +
   `SinglePaneStrategy`/`ListDetailStrategy`——`NavDisplay::scene_strategy(Box<dyn>)` 支持运行时切换
2. [x] **过渡动画**：SinglePane 接入 Crossfade（栈顶变化交叉淡化——对标 Nav3 transitionSpec 的淡化语义；
   方向性滑动过渡（push 滑入/pop 滑出）后续可扩展）
3. [ ] **NavEntryDecorator**：生命周期/状态保持（对标 rememberSaveableStateHolderNavEntryDecorator）
4. [ ] **metadata**：Scene 级/entry 级过渡配置（Nav3 的 transition 优先级链）
5. [ ] **deep link**：back stack 恢复/合成（对标 Nav3 的 NavBackStackSerializer）
6. [ ] **Scene 动画方向性**：push/pop 滑动过渡（winia AnimatedVisibility slide 可接）
