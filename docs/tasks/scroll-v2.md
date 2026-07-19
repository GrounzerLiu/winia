# 滚动系统重设计 — 对齐 Jetpack Compose

> 目标：完全对齐 Compose 的 `ScrollState` + `Modifier.verticalScroll` 架构  
> 任务 ID：`b10ff06f673b`

---

## 一、Compose 架构

```kotlin
val state = rememberScrollState()
Column(Modifier.verticalScroll(state)) {
    // 内容...
}
```

### 1.1 ScrollState

```kotlin
class ScrollState(initial: Int = 0) {
    var value: Int by mutableStateOf(0)           // 当前偏移
    val maxValue: Int                              // 最大偏移 = content - viewport
    val isScrollInProgress: Boolean                 // fling 等动画进行中

    suspend fun animateScrollTo(value: Int)        // 动画滚动
    suspend fun scrollTo(value: Int)               // 立即滚动
}
```

### 1.2 verticalScroll 实现路径

```
verticalScroll(state)
  │
  └── scrollableArea(state, orientation = Vertical)
        │
        ├── clipToBounds()               // Modifier 链: graphicsLayer { clip = true }
        │
        ├── scrollable(state, Vertical)  // 手势检测：滚轮 + 触摸 drag → delta → consumeScrollDelta
        │     └── dispatchRawDelta { delta ->
        │             state.value = (state.value + delta).coerceIn(0, maxValue)
        │         }
        │
        └── layout { child, constraints ->
                child.measure(constraints.copy(maxHeight = Infinity))
                layout(w, constraints.maxHeight) {
                    child.place(0, -state.value)   // ⭐ 偏移在布局阶段
                }
            }
```

### 1.3 关键设计要点

| 要点 | Compose 做法 |
|------|-------------|
| 偏移时机 | **layout place** 阶段 `child.place(0, -offset)` |
| ScrollState | `mutableStateOf` + `coerceIn` + `animateScrollTo` |
| 手势分离 | `scrollable` modifier 独立层，`consumeScrollDelta` 回调 |
| 裁剪 | `graphicsLayer { clip = true }` 或 `clipToBounds()` |
| 子节点测量 | `maxHeight = Infinity`（无限高） |
| 自身尺寸 | 取 `constraints.maxHeight`（父给的约束） |

---

## 二、Winia v2 当前状态

```
vertical_scroll(state)  ← 应用在 Modifier 链末尾
  │
  ├── measure_node: max_height = f32::MAX  ✅
  ├── render: canvas.clip_rect + translate  ⚠️ (应在 layout)
  └── apply_scroll_delta: 事件 → state.set  ⚠️ (应独立手势层)
```

差距：
1. **偏移在 render 而非 layout** — 子节点实际布局位置不变，视觉偏移通过 `canvas.translate` 实现
2. **无 ScrollState 类型** — 裸 `State<f32>`，无 `animateScrollTo`、`isScrollInProgress`
3. **手势和布局耦合** — `apply_scroll_delta` 递归树查找 scroll 节点
4. **Clip 在 render** — 不是 modifier 链的一部分

---

## 三、重设计方案

### 3.1 ScrollState

```rust
pub struct ScrollState {
    pub offset: State<f32>,          // 当前偏移（可观察）
    pub max_value: f32,              // 最大偏移 = content - viewport
    pub is_scroll_in_progress: State<bool>,
}

impl ScrollState {
    pub fn new() -> Self { ... }

    /// 立即滚动到指定位置
    pub fn scroll_to(&self, value: f32) {
        self.offset.set(value.clamp(0.0, self.max_value));
    }

    /// 动画滚到指定位置
    pub fn animate_scroll_to(&self, value: f32, duration: Duration, easing: Easing) {
        animate_to(&self.offset, value.clamp(0.0, self.max_value), duration, easing);
    }
}
```

### 3.2 Scrollable 布局修饰符（内置 Marker trait 或新 Modifier 类型）

不是输入事件层——是 **LayoutModifier**：

```rust
// 垂直滚动修饰符（LayoutModifier）
pub struct VerticalScrollLayout {
    state: ScrollState,
}

impl MeasurePolicy for VerticalScrollLayout {
    fn measure(&self, children: &mut [LayoutNode], constraints: Constraints) -> (Size, Vec<Placement>) {
        // 1. 给子节点无限高度
        let child_c = Constraints::new(
            constraints.min_width, constraints.max_width,
            0.0, f32::MAX,
        );
        let (child_size, _) = measure_node(&mut children[0], child_c);

        // 2. 自身高度 = 父给的约束
        let viewport_h = constraints.constrain_height(constraints.max_height);
        self.state.max_value = (child_size.height - viewport_h).max(0.0);

        // 3. 布局：子节点偏移 -offset
        let y = -self.state.offset.get().clamp(0.0, self.state.max_value);
        (Size::new(child_size.width, viewport_h), vec![Placement {
            size: child_size,
            position: Point::new(0.0, y),
        }])
    }
}
```

### 3.3 Render 层简化

移除 render.rs 中的 clip+translate。裁剪放到 Modifier 链中（`Modifier.clip(shape)` 已存在）。

### 3.4 手势层

`apply_scroll_delta` 不动——后续独立为 `scrollable` modifier 时再重构。

### 3.5 用法

```rust
let scroll = ctx.remember(|| ScrollState::new()).get();

Column::new()
    .modifier(Modifier::new()
        .size(200.0, 150.0)
        .vertical_scroll(scroll.clone())  // 改为 LayoutModifier
    )
    .build(ctx, |ctx| {
        for i in 0..30 { Text::new(format!("Line {}", i)).build(ctx); }
    });
```

---

## 四、实施步骤

| 步 | 内容 |
|----|------|
| 1 | 写 scroll 设计文档（本文档）|
| 2 | 实现 `ScrollState` |
| 3 | `VerticalScroll` 改为 LayoutModifier（MeasurePolicy） |
| 4 | render.rs 移除 scroll clip+translate |
| 5 | 编译测试 |
