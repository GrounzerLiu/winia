# LazyColumn / LazyRow 组件（material3 对齐）

> 分支：`lazy-api`（从 v2 分出，含 LazyColumn/LazyRow 全部机制 + stickyHeader + contentPadding + reverseLayout + animateScrollToItem + 重复 key 检测）
> 对标：Compose foundation `LazyColumn` / `LazyRow`（LazyDsl.kt / LazyListState.kt / LazyListMeasure.kt / LazyListScrollPosition.kt）

## 1. API

```rust
// LazyColumn（垂直）——见下
// LazyRow（水平）：同一构建器、同一 LazyListState、同一套机制换主轴
LazyRow::new()
    .state(state)                       // 与 LazyColumn 可共用同一状态
    .spacing(8.0)                       // 主轴（横向）间距
    .modifier(Modifier)                 // 通常 fill_max_width/fill_max_height
    .items_from(list, |it: &T| it.id, |ctx, i, it| { ... })
    .items(count, |i| i as u64, |ctx, i| { ... })
    .items_plain(count, |ctx, i| { ... })
    .item(|ctx| { ... })
    .item_keyed(42, |ctx| { ... })
    .build(ctx);

// 滚动输入：横向拖拽 + 松手惯性 fling、横向滚轮/touchpad（dx）、
// 程序化 state.scroll_to_item(...)（锚点权威——与垂直完全相同）
```

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

    // 主轴内边距（对齐 Compose contentPadding）：内容从 before 开始放置；
    // 自然滚动边界处内容停在 padding 处（不贴视口边）；scrollToItem 后项仍贴顶；
    // max_offset = before + Σ项 + after - 视口（跳末尾时底部留出 after）
    .content_padding(40.0, 40.0)      // (before, after)，默认 (0, 0)
    // 交叉轴内边距：缩小每项约束宽 + 项从 before 处放置
    .content_padding_cross(16.0, 16.0) // (before, after)，默认 (0, 0)

    // 反向布局（对齐 Compose reverseLayout）：index 0 在视口底部/右端——
    // 聊天风格（最新消息在底部）；offset=0 显示列表开头（项 0 在视口底），
    // 滚动方向与正向一致（offset 增 = 向列表末尾）；scrollToItem 后项**底**贴视口底
    .reverse_layout(true)

    // 吸顶 header（对齐 Compose stickyHeader）：
    // 滚动时钉在视口顶、内容从它下面滑过；下一个 header 到来时把前一个
    // 推上去。key 必须全列表唯一；注册顺序不限（内部自动置顶绘制）
    .sticky_header(1001, |ctx| { ... })  // 通常：Text + background
    .build(ctx);

// 读取滚动位置
state.first_visible();   // 第一个可见项索引
state.offset();          // 像素偏移

// 程序化滚动到指定索引（项顶部对齐视口；可带偏移——对齐 Compose scrollToItem）
// 锚点权威：不需要高度缓存/间距；测量期消费请求，从缓存推导像素 offset，
// 与放置共用同一 prefix 函数 → round-trip 精确（不会漂移）；越界 clamp 到末尾
state.scroll_to_item(50, 0.0);
// 动画滚动（对齐 Compose animateScrollToItem）：Spring（阻尼 1.0 / 刚度 200，
// 收敛 ~300-400ms）从当前 offset 平滑滚到同一目标（也 clamp 到 [0, max_offset]）
state.animate_scroll_to_item(50, 0.0);
```

> **key 唯一性**：全列表 key 必须唯一——`item_keyed` / `items` / `items_from` /
> `sticky_header` 的 key 冲突会在 build 时 panic（对齐 Compose：key 冲突抛异常）。
> 无 key 项（`item` / `items_plain`）用位置作 key，不会冲突。

## 2. 核心机制（对齐 Compose lazy 架构）

### 2.1 锚点滚动模型

- 滚动状态 = `LazyListState`：像素 offset（滚动输入通道）+ 派生锚点
  （first_visible_index / first_visible_offset——对齐 Compose 锚点模型语义）；
- 滚动输入走 winia 现有通道：`vertical_scroll` / `horizontal_scroll` modifier
  + `apply_scroll_delta`（wheel/手势，双轴：垂直吃 dy、水平吃 dx）→
  offset State → 重组 → 重新计算可见范围。

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
- **视口高**：有限约束直接回写；无穷（Column 内容驱动）回退缓存，避免振荡。
  ⚠ measure_node 对普通 scroll 容器把 max_height 改写成 f32::MAX——lazy 容器
  **跳过**该改写（policy 显式控制子约束），保留有限 max_height 拿真实视口：
  否则 clamp 的 max_offset = content_h - vh 偏大（回退 600 vs 真实 ~500），
  跳末尾滚过头、最后一项被推出视口底部（实测 bug）；
- **内容总高**：`LazyScroll` modifier 标记 + 测量回写 →
  `apply_scroll_delta` 用它计算 max_offset（框架新增 `scroll_content_height`）。
- **程序化跳转（锚点权威）**：`scroll_to_item(index, offset)` 只存跳转请求
  （对齐 Compose `requestPositionAndForgetLastKnownKey`——scroll position 就是
  锚点，测量从锚点开始组合）。build 按请求定组合锚点；测量期用**写回后的
  高度缓存**推导像素 offset，与放置/锚点解析共用同一 `prefix_height` →
  round-trip 精确：跳 500 就是 500（旧实现：空缓存预估 48px/项 vs 实测
  ~47.5px，500 项偏差累积 → 落到 505，实测 bug）。越界 clamp 在测量期。
- **惯性滚动（fling）**：`ScrollState::fling(velocity)` / `LazyListState::fling(v)`
  ——`push_fling`：指数衰减（`exponential_decay(4.2)` 对齐 Compose）+ 每帧
  clamp 到滚动极限（撞边界立即停——对齐 Compose fling 消耗完即停）。
  极限由测量期回写（lazy：`fling_limit = content_h - vh`；普通容器：
  measure_node 用子节点底部计算内容高）；拖拽滚动（按下滚动容器内容跟随
  指针，松手按最小二乘速度 fling，对齐 Compose scrollable 拖拽 + fling）；
  滚轮保持离散（不 fling）；手动输入自动取消进行中的 fling。

### 2.5 吸顶 header（stickyHeader，对齐 Compose LazyListMeasure 的 pinned 处理）

- **钉住**：header 的视口位置 `final(i) = max(C(i) - s, 0)`——滚过视口顶后
  钉在 0（内容坐标放置 = final + s，滚动 translate 负责视口位移）；
- **推出**：相邻 header 对反向约束 `final(prev) = min(final(prev), final(next) - h(prev))`
  ——下一个 header 距顶 h(prev) 内时前一个开始滑出，到位后完全推出
  （两 header 无缝相邻，视觉为合并带）；
- **绘制层级**：钉住 header 需盖住从下面滑过的内容——注册顺序把 sticky
  项排在普通项之后（子节点渲染顺序 = 注册顺序）；children↔全局 index 用
  显式 `globals[i]` 映射（不再能 start+i 位置映射）；
- **窗口包含钉住 header**：build 从锚点向下回溯最后一个 C(i) ≤ s 的 sticky
  （pin，深滚动时回溯距离 = 当前 section 长度）；再向上找 prev——仅当其
  底部仍低于视口顶（C(prev)+h(prev) > s，即处于被推出的过渡期）才纳入
  窗口；回溯提前终止（更上方不可能可见）；
- **锚点语义对齐 Compose**：钉住时 `first_visible_index = pin`、
  `first_visible_offset = 0`（policy 测量期写回——Compose 的
  firstVisibleItemIndex 就是钉住的 header）；未钉住时自然锚点；
- **组合数量**：仅窗口内项 + 钉住/过渡 header，不拉全列表（100 项列表
  深滚动组合数仍 ~50）。

### 2.6 主轴/交叉轴内边距（contentPadding，对齐 Compose contentPadding）

- **主轴 before/after**：内容从 `pad_before` 开始放置（所有 prefix 计算从
  pad_before 起算，锚点解析/header 钉住判定同样含 before）；
- **边界语义对齐 Compose**：自然滚动到边界时内容停在 padding 处（不贴
  视口边）——`max_offset = pad_before + Σ项+间距 + pad_after - viewport`；
  而 `scroll_to_item` 的 jump offset 含 before（`pad_before + prefix + 请求偏移`），
  跳转后项仍贴视口顶——padding 只在自然滚动边界生效；
- **sticky 交互**：钉住 header 钉在 `pad_before` 处（视口坐标），不是 0
  （`final(i) = max(pad_before + C(i) - s, pad_before)`）；
- **交叉轴 before/after**：每项约束宽 `max -= cb + ca`（项被压缩），项从
  `cross_before` 处放置（`A::with_cross_max` 轴无关改写约束）。
### 2.7 反向布局（reverseLayout，对齐 Compose reverseLayout）

- **布局镜像**：index 0 在主轴末端（LazyColumn = 底部）、index 最大在开头。
  实现 = 镜像坐标模型：锚点/可见范围/pin 回溯/jump/clamp 全部在"镜像坐标"
  中计算（公式与正向完全相同），仅两处转换：
  - **放置**：内容坐标 = `content_h - 镜像位置 - 项高`（sticky 同样镜像）；
  - **render 平移**：`scroll_origin = content_h - vh - offset`（而非 offset）——
    `LazyScroll` modifier 增 `reverse` 标志 → `LayoutNode.scroll_reverse` →
    render 的 scroll translate 分支；
- **offset 语义不变**：0 = 列表开头（项 0 在视口底）、max = 末尾；滚动方向
  与正向一致（滚轮/拖拽/fling/scroll_to_item 全部无需改动）；
- **scroll_to_item 镜像语义**：项**底**贴视口**底**（正向 = 项顶贴视口顶）——
  公式相同（`offset = before + prefix + 请求偏移`），语义自动镜像；
- **firstVisible 对齐 Compose**：反向时锚点 = 视口**底**的项（index 0 侧），
  `first_visible_offset` 从底部量（镜像公式与正向同构）；
- **首帧一致**：测量后把 `content_height` 同步回节点字段（measure_node 顶部
  读的是上一帧值）——否则 reverseLayout 首帧平移错误（内容整体被推出视口）。

### 2.8 动画滚动（animateScrollToItem，对齐 Compose animateScrollToItem）

- `LazyListState::animate_scroll_to_item(index, offset)`：与 `scroll_to_item`
  同一锚点权威（jump_request 增 animate 标志）——测量期消费、从写回后的缓存
  推导像素目标（`before + prefix + 偏移`，含 reverse 镜像语义），然后
  `push_animatable(offset, target, Spring)`（默认阻尼 1.0 / 刚度 200，
  收敛 ~300-400ms，与 Compose scroll 动画同级别）；
- 目标 clamp 到 `[0, max_offset]`（消费移到 clamp 之后）；动画与进行中的
  fling/动画互相替换（push_animatable 的 retarget 继承速度）。
## 3. 框架扩展（本组件新增）

| 项 | 位置 | 说明 |
| --- | --- | --- |
| `ModifierElement::LazyScroll` | modifier.rs | lazy 内容主轴尺寸标记（高/宽共用，增 `reverse` 标志） |
| `Modifier::lazy_scroll_reverse` / `is_lazy_scroll_reverse` | modifier.rs | reverseLayout 标记/查询 |
| `LayoutNode.scroll_reverse` | layout/node.rs | reverse 平移标志（measure 期回写） |
| 测量后内容高同步 | layout/node.rs | policy 测量后 content_height → 节点字段（首帧 reverse 平移正确） |
| `Modifier::lazy_scroll(content_height)` | modifier.rs | 便捷构造 |
| `LayoutNode.scroll_content_height` | layout/node.rs | 内容总高（lazy 滚动 clamp 用） |
| `LayoutNode.scroll_viewport_width/scroll_content_width` | layout/node.rs | 横向对称字段 |
| `apply_scroll_delta` 双轴 | app.rs | 垂直吃 dy、水平吃 dx；非零 delta 才消费（防 DFS 先遇节点吞轴） |
| `HorizontalScroll` 存完整 ScrollState | modifier.rs | 与 VerticalScroll 对称（offset/进度/fling 极限） |
| 横向输入 | app.rs/debug.rs | wheel dx、拖拽 x 轴样本、fling；stdin `s dx dy` |
| `ScrollState` 完整入 modifier | modifier.rs | VerticalScroll 存整个 ScrollState（offset/进度/fling 极限） |
| `push_fling` + Decay clamp | animation.rs | fling 专用：边界 clamp 撞停 |
| 拖拽滚动 + fling 触发 | app.rs | DragScroll 会话 + 速度样本 → 松手 fling |
| 非 lazy 内容高计算 | layout/node.rs | 子节点底部 → scroll_content_height + fling_limit |

## 4. 与 Compose 的差异

- winia 组合为命令式（build 直接注册节点），无 Compose 的 LazyLayout 测量期
  组合——用"组合期预估 + 测量期校正"两阶段模型（两帧收敛）；
- key 类型为 `u64`（Compose `Any`）；无 contentType/复用优化；
- stickyHeader 已实现（2.5 节）；无动画项放置（后续扩展）；reverseLayout 已实现
  （2.7 节镜像模型）；animateScrollToItem 已实现（2.8 节 spring）；LazyRow 已实现（同一
  `LazyList<A: LazyAxis>` 构建器 + 轴无关 `LazyListPolicy`——主轴抽象对齐
  Compose 同一套 LazyListMeasure 换轴）；
- `index_of_key` 已 O(1) HashMap（rebuild 构建）；
- fling 惯性滚动：拖拽 + 指数衰减 + 边界 clamp（垂直/水平均有）；
- 首帧 viewport 未知用固定窗口 2000px（测量后收敛）；
- 横向 scroll 容器实测尺寸 = 内容尺寸（非视口）——fixture 布局注意
  （滚动容器会把后续兄弟节点推出屏幕外，测试需把横向区放在前面）。

## 5. 运行与测试

```bash
cargo run -p winia --example lazy_column_demo
cargo run -p winia --example lazy_row_demo
cargo run -p winia --example sticky_header_demo
cargo run -p winia --example reverse_list_demo
cargo test -p winia --lib ui::lazy_column
# 集成（fixture 需 debug-server feature 构建）：
cargo test -p winia --features debug-server --test ui_test
```

测试覆盖：IntervalList 定位/key 映射、高度缓存预估/记录、锚点转换（含
before padding）、可见范围预取、稳定 key 数据变化滚动保持；LazyRow 像素
测试（横向只渲染可见项、横向滚动后内容变化）；stickyHeader 像素测试
（滚动钉顶 + 锚点 = pin、过渡期合并带/推出、深滚动无界回溯、组合数量
有界）；contentPadding 像素测试（内容从 before 内边距开始、末尾留出
pad_after、sticky 钉在 pad_before、交叉轴压缩项）；reverseLayout 像素测试
（offset=0 项 0 在屏幕底、滚动方向与正向一致、scroll_to_item 项底贴视口底、
越界 clamp 两端）；animateScrollToItem（spring 收敛到 scroll_to_item 同一目标）；
重复 key 检测（rebuild panic）；集成：横向滚轮（`s dx dy`）与横向拖拽 +
fling（fixture_scroll）。
