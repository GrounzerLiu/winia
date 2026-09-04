# Scrollbar 滚动条（桌面 CMP 对齐）

> 状态：已实现（分支 `exp/scrollbar`，从 v2 分出）
> 对标：Compose Multiplatform 桌面 `VerticalScrollbar` / `HorizontalScrollbar`
> + M3 `Modifier.nonInteractiveScrollbar`（`Scrollbar.kt`，2026 androidx-main，
> 浏览器取证；几何公式同社区 gist `drawScrollbar`）
> 实现：`winia/src/ui/scrollbar.rs`（`ScrollbarNode` + 双组件）；
> demo：`winia/examples/scrollbar_demo.rs`（垂直拖 thumb + 常显开关 + 水平条）。

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
```

## 2. 语义（对标项）

- **三要素**：offset（`ScrollState.offset`，读注册依赖）+ viewport（自身约束，
  `on_size_changed` 回写，首帧 0 不画次帧显示）+ content（= `fling_limit` + viewport）。
- **thumb 几何**（M3 同）：`thumb = clamp(track * viewport/content, min, track*0.9)`，
  `thumb_offset = (scroll/max) * (track - thumb)`；content <= viewport 时不画；
  track < min 时不画。`track = viewport - 2*inset`（inset 2dp）。
- **拖 thumb**：`on_drag` 本地坐标 → `offset.set(pos/(track-thumb) * max)`。
  拖拽开始抢占（`cancel_fling` + 置滚动中，松手清——否则列表侧惯性 fling
  的 `update_animations` 写回 offset 导致拖不动）。
- **显示**：`always_show` / 滚动中 / hover / 拖拽中 + fade 动画。
  hover 显示（`hoverable` + interaction 源）解决隐藏态无从下手拖；
  拖拽中显示解决松手即消失（`emit_drag_start/end` 维持 `dragged`）。
- **fade**（M3 默认）：400ms delay + 250ms tween——`LaunchedEffect(key)`
  驱动 + `push_animatable(fade_state)`；绘制期 `peek` 叠加 alpha
  （`fade_alpha` 不进 key，Slider 瞬态值同惯例）。
- **绘制**：`ScrollbarNode` 具名绘制（track 矩形 + thumb 圆角，半径 thickness/2）；
  `node_key` 含几何（offset/len 进 key——滚动即 Enter，v1 语义）。

## 3. 与 Compose 的差异（v1 范围）

- 无 RTL 镜像（垂直条恒右侧，由调用方放；M3 按 layoutDirection 放 end edge）。
- 无 LazyList 适配（`LazyListState` 锚点模型另有 first_visible——后续加）。
- thumb 几何进 key（滚动每帧 Enter——v1 简单语义；优化方向：绘制期 peek +
  layout 依赖，参考 TabRow indicator 模式）。
- viewport 首帧 0（`on_size_changed` 次帧回写，1 帧延迟——Lazy viewport 同级）。
- 需要 tokio runtime（fade 的 `LaunchedEffect` 驱动——与 TextField blink 同约定；
  demo main 需先建 `Runtime` + `enter`，见 `scrollbar_demo.rs`）。

## 4. 运行与测试

```bash
cargo run -p winia --example scrollbar_demo
cargo test -p winia --lib ui::scrollbar
```

测试覆盖：几何隐藏/比例/min-max 钳/拖拽往返/node_key/组件联动
（offset.set 后 thumb key 跟随）。
