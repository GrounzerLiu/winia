# Scrollbar 滚动条（桌面 CMP 对齐）

> 状态：已合入 `v2`（`5d589f8` 大合并 + grab `237a8ac` + followup `081de0d` +
> lazy-pulse `3d7827c` + lazy-scrollbar `0061b81`）
> 对标：Compose Multiplatform 桌面 `VerticalScrollbar` / `HorizontalScrollbar`
> + M3 `Modifier.nonInteractiveScrollbar`（`Scrollbar.kt`，2026 androidx-main，
> 浏览器取证；几何公式同社区 gist `drawScrollbar`）
> 实现：`winia/src/ui/scrollbar.rs`（`ScrollbarNode` + 三组件）；
> demo：`winia/examples/scrollbar_demo.rs`（垂直拖 thumb + 常显开关 + 水平条 + Lazy 列）。

## 1. API

```rust
// 垂直：与滚动容器并排等高（Row 内），ScrollState 共享
Row::new().build(ctx, |ctx| {
    Column::new()
        .modifier(Modifier::new().fill_max_height().layout_weight(1.0)
            .vertical_scroll(scroll.clone()))
        .build(ctx, content);
    VerticalScrollbar::new(scroll.clone())
        .thickness(6.0)              // 默认 SCROLLBAR_THICKNESS
        .thumb_color(color)          // 默认 outline 70%
        .track_color(color)          // 默认透明
        .thumb_min_length(24.0)      // 默认 SCROLLBAR_THUMB_MIN_LENGTH
        .always_show(true)           // 默认仅滚动中显示
        .build(ctx);
});

// 水平：对称（fill_max_width + horizontal_scroll 容器上下叠）
HorizontalScrollbar::new(scroll.clone()).always_show(true).build(ctx);

// Lazy：与 LazyColumn/LazyRow 并排，LazyListState 共享（锚点模型的 offset
// 与 ScrollState 同像素语义；fling_limit/pulse 由测量期回写；scrolling 通道
// lazy 侧无——fade 靠 offset/脉冲，拖 thumb 显示正常）
Row::new().build(ctx, |ctx| {
    LazyColumn::new()
        .state(list_state.clone())
        .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
        .items_plain(50, content)
        .build(ctx);
    LazyScrollbar::new(list_state)
        .always_show(false)
        .build(ctx);
});

// Lazy 横向：LazyRow 下叠 HorizontalLazyScrollbar（reverse 同 HorizontalScrollbar）
LazyRow::new()
    .state(row_state.clone())
    .modifier(Modifier::new().fill_max_width())
    .items_plain(30, content)
    .build(ctx);
HorizontalLazyScrollbar::new(row_state).always_show(true).build(ctx);
```

## 2. 语义（对标项）

- **三要素**：offset（`ScrollState.offset`，读注册依赖）+ viewport（自身约束，
  `on_size_changed` 回写，首帧 0 不画次帧显示）+ content（= `fling_limit` + viewport）。
- **thumb 几何**（M3 同）：`thumb = clamp(track * viewport/content, min, track*0.9)`，
  `thumb_offset = (scroll/max) * (track - thumb)`；content <= viewport 时不画；
  track < min 时不画。`track = viewport - 2*inset`（inset 2dp）。
- **拖 thumb**：`on_drag_start` 记 grab = 按下点(track 内) - 当时 thumb 顶部
  （钳到 `[0, thumb]`，track 空白处按下则边缘跟到光标），`on_drag` 用
  `target = 光标 - grab` 反推 offset（CMP 抓取偏移保持——thumb 跟手不跳；
  旧中心对齐大 thumb 首帧跳变）。拖拽开始抢占（`cancel_fling` + 置滚动中，
  松手/取消清——否则列表侧惯性 fling 的 `update_animations` 写回 offset
  导致拖不动；cancel 防失焦残留置位）。
- **显示**：`always_show` / 滚动中 / hover / 拖拽中 / offset 变化脉冲 /
  边界 pulse（`scroll_pulse`）+ fade 动画。
  hover 显示（`hoverable` + interaction 源）解决隐藏态无从下手拖；
  拖拽中显示解决松手即消失（`emit_drag_start/end` 维持 `dragged`）；
  wheel/程序化滚动不置 `is_scroll_in_progress`（分发层只 cancel+置 false，
  拖拽路径才置 true），故滚动检测用 offset 变化脉冲（`last_scroll` 记上帧
  offset，本帧不同即正在滚）——`scrolling` 恒 false 时仍能点亮。
- **fade**（M3 默认）：400ms delay + 250ms tween——单 `LaunchedEffect`
  `key=(fade_target, scrolling, hovered, dragged, scroll)` 包办"显示→等→藏"：
  先 tween 到 1，sleep 400ms（期间输入变化→key 变化→abort 睡眠→保持显示），
  睡满后看静态保持条件（`always_show/scrolling/hovered/dragged`）快照：
  成立保持（hover 静置不能藏），否则播到 0。`fade_state: State<f32>` 存进
  `ScrollbarNode`，绘制期 `peek` 叠加 alpha（零重组——动画引擎每帧
  `request_redraw` 驱动；**不能**在 build 期快照为 f32，peek 不注册依赖→
  fade 变化永不重组→节点残留旧值，实测 bug）。`fade` 不进 `node_key`
  （逐帧动画值，进则每帧 Enter；目标同 Slider 瞬态值零重组，手段不同：
  Slider 是 build 期 peek 烘进节点，本节点持 State、draw 期 peek）。
- **绘制**：`ScrollbarNode` 具名绘制（track 矩形 + thumb 圆角，半径 thickness/2）；
  `node_key` 含几何（offset/len 进 key——滚动即 Enter，v1 语义）。

## 3. 与 Compose 的差异（v1 范围）

- 无 RTL 镜像（垂直条恒右侧，由调用方放；M3 按 layoutDirection 放 end edge）。
- 无 LazyList 适配（已做：`LazyScrollbar` 吃 `LazyListState`——offset 同像素
  语义，fling_limit/pulse 测量期回写；scrolling 通道 lazy 侧无，fade 靠脉冲）。
- `LazyRow` 适配（已做）：`HorizontalLazyScrollbar` 吃 `LazyListState`，
  拼装语义与 `LazyScrollbar` 一致（轴为水平；`scroll_reverse` 同语义）。
- thumb 几何进 key（滚动每帧 Enter——v1 简单语义；优化方向：绘制期 peek +
  layout 依赖，参考 TabRow indicator 模式）。
- viewport 首帧 0（`on_size_changed` 次帧回写，1 帧延迟——Lazy viewport 同级；
  回写靠 `viewport_state.get()` 注册的组合依赖驱动重组，`peek` 则无订阅 stale）。
- 边界滚轮点亮（P1-3，已做）：顶/底继续滚时 offset 无变化，offset 脉冲
  检测不到，故 `ScrollState.scroll_pulse: u64` 单调计数——分发层在"命中但
  消费为 0"的 wheel 上自增，scrollbar 侧以变化为脉冲点亮 fade（M3/CMP
  "到底了"反馈同行为）。lazy 列表同样接通：`LazyListState.scroll_pulse`
  跨帧稳定，拼装 `ScrollState` 时 clone 进去（与 offset 同生命周期）。
- 横向 `scroll_reverse`（P2-3，已做）：`HorizontalScrollbar::scroll_reverse
  (Option<bool>)`，与滚动容器的 `horizontal_scroll_reverse` 同值——render 侧
  offset 语义镜像（offset 0 = 内容末端），scrollbar 几何/拖拽用同一镜像坐标
  `visual = max - scroll`（读镜像、写回镜像，对合无漂移）。垂直条无 reverse。
  `LazyScrollbar::scroll_reverse` 同语义（反向懒列表 `reverse_layout(true)` 传
  `Some(true)`）。`None` 与 `Some(false)` 等价（都直用；`Option` 造型为直透
  `is_horizontal_scroll_reversed()` 返回值）。
- 边界 pulse 按轴独立（P2-1）：顶边斜滚（垂直顶住、水平正常消费）时两轴
  各判各，不丢单轴反馈。
- 拖拽边界无点亮（P2-4 取舍）：pulse 仅 Wheel 源；拖拽顶住边界时 offset 无
  变化、`scroll_active` 为假、`scroll_pulse` 不增——fade 不亮（拖拽中手未松
  时 `dragged` 为真条本来就显示，影响仅松手瞬间）。
- 需要 tokio runtime（fade 的 `LaunchedEffect` 驱动——与 TextField blink 同约定；
  demo main 需先建 `Runtime` + `enter`，见 `scrollbar_demo.rs`）。

## 4. 运行与测试

```bash
cargo run -p winia --example scrollbar_demo
cargo test -p winia --lib ui::scrollbar
```

测试覆盖：几何隐藏/比例/min-max 钳/小 track 不 panic（P0-1）/非有限输入防腐/
拖拽往返/退化 travel/抓取保持/镜像往返/node_key（含 fade 不进 key）/
组件联动（offset.set 后 thumb key 跟随）/Lazy 联动/Lazy 反向镜像/
水平 Lazy 联动。
