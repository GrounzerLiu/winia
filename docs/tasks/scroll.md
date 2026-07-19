# 任务：滚动支持 (Scroll Modifier)

> 对标 Jetpack Compose `Modifier.verticalScroll()` / `Modifier.horizontalScroll()`  
> 状态：设计中

---

## 一、Compose 方案分析

```kotlin
val state = rememberScrollState()
Column(Modifier.verticalScroll(state)) {
    // 1000 行内容...
}
```

**核心机制**：

```
verticalScroll(state)
  ├── clipToBounds()              # 裁剪可见区域
  ├── layout { child, constraints ->
  │       child.measure(constraints.copy(maxHeight = Infinity))  # 无限高度
  │       layout(width, constraints.maxHeight) {
  │           child.place(0, -state.value)                       # 偏移子节点
  │       }
  │   }
  └── scrollable(direction, state) # 滚轮事件 → state.value
```

**关键点**：
1. `ScrollState` 不是布局组件，是 `Modifier` 链的一部分
2. 测量时给子节点无限约束（让其自然撑开）
3. 布局时通过 `place(0, -offset)` 实现视觉滚动
4. 滚轮事件通过 `scrollable` modifier 驱动 `state.value`
5. `clipToBounds` 确保超出的内容不可见

---

## 二、Winia v2 实现方案

### 2.1 新增类型

```rust
// 不需要单独的 ScrollState 类型——直接用 State<f32>

fn counter_ui(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| State::new(0.0f32));
    
    Column::new()
        .modifier(Modifier::new()
            .size(400.0, 200.0)        // 可视区域 400×200
            .vertical_scroll(scroll_y)
        )
        .build(ctx, |ctx| {
            for i in 0..50 {
                Text::new(format!("Line {}", i)).build(ctx);
            }
        });
}
```

### 2.2 Modifier 新增

| 元素 | 作用 |
|------|------|
| `ModifierElement::VerticalScroll { state }` | 垂直滚动，绑定 `State<f32>` |
| `ModifierElement::HorizontalScroll { state }` | 水平滚动 |

### 2.3 Layout 改动

`measure_node` 中遇到 `VerticalScroll`：
- 给子节点 `height: Infinity` 约束
- 自身高度 = `constraints.max_height`（父给的高度）
- 子节点自然撑开到实际内容高度

### 2.4 Render 改动

`render_node` 中遇到 `VerticalScroll`：
1. `canvas.save()`
2. `canvas.clip_rect(visible_bounds)` — 裁剪可视区域
3. `canvas.translate(0.0, -offset)` — 偏移内容
4. 递归渲染子节点
5. `canvas.restore()`

### 2.5 事件处理

`app.rs` 中新增 `WindowEvent::MouseWheel` 处理：
1. 命中测试找到目标节点
2. 遍历 modifier 找 `VerticalScroll` / `HorizontalScroll`
3. 更新 `state.set(new_offset)`（clamp 到 [0, content_height - visible_height]）
4. `request_redraw()`

### 2.6 步骤

| 步骤 | 内容 |
|------|------|
| 1 | `ModifierElement` 加 `VerticalScroll` / `HorizontalScroll` + Modifier 方法 |
| 2 | `measure_node` 处理 scroll 约束 |
| 3 | `render.rs` 处理 scroll clip + translate |
| 4 | `app.rs` 处理 `MouseWheel` → 命中 + 更新 offset |
| 5 | Counter 示例：50 行文本 + 滚动 |
| 6 | 编译测试 |

---

## 三、预计问题

| 风险 | 缓解 |
|------|------|
| `measure_node` 改动影响现有布局 | 仅叶子和容器节点，scroll 在容器前拦截 |
| 滚轮 delta 单位不确定 | 用 `LineDelta` 乘以固定行高 |
| Scroll 内容高度计算 | 子节点自身 measured_height 已知 |
| 多个嵌套 scroll | 仅处理最内层命中节点 |

---

## 四、进度

- [x] Step 1: Modifier 元素 + 方法
- [x] Step 2: Layout 约束
- [x] Step 3: Render clip + translate
- [x] Step 4: MouseWheel 事件
- [x] Step 5: Counter 示例
- [x] Step 6: 测试 + 文档

## 五、实现总结

**用法**：
```rust
let scroll_y = ctx.remember(|| State::new(0.0f32));
Column::new()
    .modifier(Modifier::new().size(200.0, 150.0).vertical_scroll(scroll_y))
    .build(ctx, |ctx| {
        for i in 0..30 { Text::new(format!("Line {}", i)).build(ctx); }
    });
```

**已修改文件**：
- `modifier.rs` — `VerticalScroll`/`HorizontalScroll` 元素 + `vertical_scroll()`/`horizontal_scroll()` 方法
- `layout/column.rs` — `measure_node` 给 scroll 节点子节点无限约束
- `render.rs` — scroll 节点 clip + translate，`render_pass1_simple` 供 Phase 2 使用
- `app.rs` — `MouseWheel` 事件 → `apply_scroll_delta` → 更新 offset + request_redraw
- `examples/counter.rs` — 30 行文字 + 滚动演示
