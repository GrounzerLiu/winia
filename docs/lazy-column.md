# LazyColumn 组件（material3 对齐）

> 分支：`lazy-scroll`（从 v2 分出）
> 对标：Compose foundation `LazyColumn`（LazyDsl.kt / LazyListState.kt / LazyListMeasure.kt / LazyListScrollPosition.kt）

## 1. API

```rust
// 滚动状态（跨重组保持）
let state = ctx.remember(|| LazyListState::new()).get();

LazyColumn::new()
    .state(state)                       // 注入外部状态（不传则内部 remember）
    .spacing(8.0)                       // 项间距
    .modifier(Modifier)                 // 尺寸/布局（通常 fill_max_width/height）

    // 稳定 key 迭代（推荐）：key 从数据 id 提取——列表前部增删后
    // 滚动位置按 key 保持（对齐 Compose items(key=...)）
    .items_from(
        list: Arc<Vec<T>>,
        |item: &T| item.id,             // 稳定唯一 key
        |ctx, index, item| { ... },     // 项内容
    )

    // count 项 + 索引 key 工厂
    .items(count, |i: usize| i as u64, |ctx, i| { ... })
    // 无 key 项（位置即 key）
    .items_plain(count, |ctx, i| { ... })
    // 单一项 / 带 key 单一项
    .item(|ctx| { ... })
    .item_keyed(42, |ctx| { ... })
    .build(ctx);

// 读取滚动位置
state.first_visible();   // 第一个可见项索引
state.offset();          // 像素偏移

// 程序化滚动到指定索引（项顶部对齐视口；可带偏移——对齐 Compose scrollToItem）
// 锚点权威：不需要高度缓存/间距；测量期消费请求，从缓存推导像素 offset，
// 与放置共用同一 prefix 函数 → round-trip 精确（不会漂移）；越界 clamp 到末尾
state.scroll_to_item(50, 0.0);
```

## 2. 核心机制（对齐 Compose lazy 架构）

### 2.1 锚点滚动模型

- 滚动状态 = `LazyListState`：像素 offset（滚动输入通道）+ 派生锚点
  （first_visible_index / first_visible_offset——对齐 Compose 锚点模型语义）；
- 滚动输入走 winia 现有通道：`vertical_scroll` modifier + `apply_scroll_delta`
  （wheel/手势）→ offset State → 重组 → 重新计算可见范围。

### 2.2 稳定 key 与滚动位置保持（核心需求）

- `items_from` 接受 `key` 工厂（数据 id 等稳定值）；
- build 记录当前锚点项 key（`last_known_first_key`）；
- 当列表 total 变化（数据增删）时，用 key 反查新 index，重算 offset——
  前部插入/删除后原可见项保持在视口（对齐 Compose
  `LazyListScrollPosition.updateScrollPositionIfTheFirstItemWasMoved`）；
- ⚠ 仅 total 变化时校正——正常滚动时锚点变化是用户滚动结果，绝不校正
  （实测：每帧校正会把滚动拉回 0）；
- total 守卫（`known_total`）存在 **LazyListState 内部**而非组合级 remember——
  外部持有 state 跨 Composer 复用（测试/多窗口）时，组合级守卫每帧误触发
  校正把滚动拉回 0（实测：二次 render 后 offset=2000 → 0）。

### 2.3 区间内容（IntervalList）

- `item/items/items_from` 注册为区间（count + key 工厂 + 内容闭包）；
- 全局 index 经区间定位转段内局部 index（对齐 Compose LazyListIntervalContent）；
- key 查询：`key_of(global)` / `index_of_key(key)`——rebuild 时构建
  key→index HashMap，O(1) 反查（对齐 Compose NearestRangeKeyIndexMap 的
  查询语义）。

### 2.4 懒测量（组合期预估 + 测量期校正）

- **组合期**：高度缓存（实测 + 预估 48dp）预估可见范围 [start, end]，只注册
  可见项（含上下预取 4 项）；slot key 混合 item key 实现跨帧复用/回收；
- **测量期**：自定义 `LazyListPolicy` 真实测量子节点、写回高度缓存、
  以**内容坐标**放置（y = prefix 累计——滚动由框架 scroll translate 处理，
  双重偏移会导致内容滚出视口，实测）；
- **视口高**：有限约束直接回写；无穷（Column 内容驱动）回退缓存，避免振荡；
- **内容总高**：`LazyScroll` modifier 标记 + 测量回写 →
  `apply_scroll_delta` 用它计算 max_offset（框架新增 `scroll_content_height`）。
- **程序化跳转（锚点权威）**：`scroll_to_item(index, offset)` 只存跳转请求
  （对齐 Compose `requestPositionAndForgetLastKnownKey`——scroll position 就是
  锚点，测量从锚点开始组合）。build 按请求定组合锚点；测量期用**写回后的
  高度缓存**推导像素 offset，与放置/锚点解析共用同一 `prefix_height` →
  round-trip 精确：跳 500 就是 500（旧实现：空缓存预估 48px/项 vs 实测
  ~47.5px，500 项偏差累积 → 落到 505，实测 bug）。越界 clamp 在测量期。

## 3. 框架扩展（本组件新增）

| 项 | 位置 | 说明 |
| --- | --- | --- |
| `ModifierElement::LazyScroll` | modifier.rs | lazy 内容高标记 |
| `Modifier::lazy_scroll(content_height)` | modifier.rs | 便捷构造 |
| `LayoutNode.scroll_content_height` | layout/node.rs | 内容总高（lazy 滚动 clamp 用） |
| `apply_scroll_delta` 内容高支持 | app.rs | max_offset 用 lazy 内容高 |

## 4. 与 Compose 的差异

- winia 组合为命令式（build 直接注册节点），无 Compose 的 LazyLayout 测量期
  组合——用"组合期预估 + 测量期校正"两阶段模型（两帧收敛）；
- key 类型为 `u64`（Compose `Any`）；无 contentType/复用优化；
- 无 stickyHeader / LazyRow / 动画项放置（后续扩展）；
- `index_of_key` 线性扫描（大数据可优化为 HashMap 缓存）；
- 无 fling 惯性滚动（winia 手势系统未接）；
- 首帧 viewport 未知用固定窗口 2000px（测量后收敛）。

## 5. 运行与测试

```bash
cargo run -p winia --example lazy_column_demo
cargo test -p winia --lib ui::lazy_column
```

测试覆盖：IntervalList 定位/key 映射、高度缓存预估/记录、锚点转换、
可见范围预取、稳定 key 数据变化滚动保持。
