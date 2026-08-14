# Progress Indicator 组件（material3 对齐）

> 分支：`progress-indicator`（从 v2 分出）
> 对标：material3 `LinearProgressIndicator` / `CircularProgressIndicator`
> （Compose androidx-main ProgressIndicator.kt + M3 token v0_7_0）
> M3 规格：https://m3.material.io/components/progress-indicators/specs

## 1. API

```rust
// Linear —— determinate
LinearProgressIndicator::new(progress: f32)     // 对标 LinearProgressIndicator(progress, ...)
    .color(Color)                                // 对标 color（默认 Primary）
    .track_color(Color)                          // 对标 trackColor（默认 SecondaryContainer）
    .stroke_cap(ProgressIndicatorStrokeCap)      // 对标 strokeCap（默认 Round）
    .gap_size(f32)                               // 对标 gapSize（默认 4dp——TrackActiveSpace）
    .draw_stop_indicator(bool)                   // 对标 drawStopIndicator（默认 true）
    .modifier(Modifier)
    .build(ctx);

// Linear —— indeterminate（无限双线动画，忽略 progress）
LinearProgressIndicator::indeterminate()
    .color(...).track_color(...)...
    .build(ctx);

// Circular —— determinate
CircularProgressIndicator::new(progress: f32)   // 对标 CircularProgressIndicator(progress, ...)
    .color(Color)
    .track_color(Color)
    .stroke_width(f32)                           // 对标 strokeWidth（默认 4dp——TrackThickness）
    .stroke_cap(ProgressIndicatorStrokeCap)
    .gap_size(f32)
    .modifier(Modifier)
    .build(ctx);

// Circular —— indeterminate（旋转 + 进度呼吸，无 track）
CircularProgressIndicator::indeterminate()...

// determinate 进度切换动画规格（Spring 无弹跳 + 极低刚度，阈值 1/1000）
let animated = ctx.animate_float_as_state(target, ProgressIndicatorDefaults::progress_animation_spec());
```

## 2. 设计要点（对齐项）

### 2.1 形态与尺寸

| 项 | Linear | Circular |
| --- | --- | --- |
| 默认尺寸 | 240×4dp（`LinearIndicatorWidth/Height`） | 40×40dp（`Size`） |
| 描边宽度 | = 组件高度（strokeWidth = size.height） | 4dp（`TrackThickness`） |
| 端点 | Round（`LinearStrokeCap`） | Round（`CircularDeterminateStrokeCap`） |
| 指示器/轨道间隙 | 4dp（`TrackActiveSpace`） | 4dp（`TrackActiveSpace`） |
| stop indicator | 末端 4dp 圆点（`StopSize`，距端 ≤6dp `StopIndicatorTrailingSpace`） | — |

### 2.2 颜色（`ProgressIndicatorTokens`）

- **指示器**（ActiveIndicatorColor）：Primary
- **轨道**（TrackColor）：SecondaryContainer
- **stop indicator**（StopColor）：Primary（与指示器同色）
- indeterminate circular 无轨道（`circularIndeterminateTrackColor = Transparent`）

### 2.3 几何细节

- **Round cap 间隙**：视觉间隙 = `gap + strokeWidth`（两端 cap 各延伸半宽）——
  `adjustedGapSize = if (cap == Butt || height > width) gap else gap + strokeWidth`；
  退化条件 `height > width` 与 Compose 一致。
- **Linear track 起点**：`progress + min(progress, gapFraction)`——进度小时 gap 收缩，
  避免 indicator 与 track 在起点重叠。
- **Linear stop indicator**：`stopSize = min(4dp, height)`，
  `stopOffset = min((height - stopSize)/2, 6dp)`，圆心在 `(w - stopSize/2 - offset, h/2)`；
  Butt cap 时画方形。
- **Circular 弧**：包围盒内缩 `strokeWidth/2`（stroke 中心线落在直径上）；
  `startAngle = 270°`（12 点方向）顺时针扫 `progress × 360°`；
  track 起点 = `startAngle + sweep + min(sweep, gapSweep)`，
  track 扫过 = `360 - sweep - 2 × min(sweep, gapSweep)`——gap 用弧长/周长换算角度。

### 2.4 Indeterminate 动画（对标 Compose 无限 keyframes）

**Linear**（周期 1750ms，`LinearAnimationDuration`）：

| 线 | delay | 时长 | easing |
| --- | --- | --- | --- |
| first line head | 0ms | 1000ms | EmphasizedAccelerate (0.3, 0, 0.8, 0.15) |
| first line tail | 250ms | 1000ms | 同上 |
| second line head | 650ms | 850ms | 同上 |
| second line tail | 900ms | 850ms | 同上 |

绘制顺序（Compose 同）：line1 前 track → line1 → 两线间 track → line2 → line2 后 track；
每段以 `head/tail` 分数开合。

**Circular**（周期 6000ms）：

| 动画 | 规格 | easing |
| --- | --- | --- |
| 全局旋转 | 0→1080° 线性 tween | Linear |
| 额外旋转 | 90° 步进 keyframes（300ms 到 90°，保持至 1500ms……4800ms 到 360°） | EmphasizedDecelerate (0.05, 0.7, 0.1, 1) |
| 进度呼吸 | 0.1→0.87→0.1 keyframes（3000ms 峰值） | Standard (0.2, 0, 0, 1) |

绘制：`rotate(global + additional)` 后画 `0°→sweep` 弧（Compose 同）。

## 3. 架构说明

- 与 Slider 相同架构：`Modifier::draw()`（CustomDraw）自定义 Canvas 绘制；
  无交互/焦点（progress indicator 不可交互）。
- 动画：`ctx.remember_infinite_transition()` + `InfiniteRepeatableSpec::restart_keyframes(...)`
  ——本组件新增框架能力：无限动画支持 keyframes 曲线（含段间 easing），
  对标 Compose `infiniteRepeatable(keyframes{...})`；组合点移除自动 dispose。
- 尺寸默认 `Modifier::size(240, 4)` / `(40, 40)`，用户 `.modifier()` 可覆盖（如 `.fill_max_width()`）。

## 4. 与 Compose 的差异

- winia 无 Dp 类型参与绘制：默认尺寸/gap 均为 dp 数值（1x 缩放），文档同。
- `ProgressIndicatorStrokeCap` 为组件自用枚举（Butt/Round），未暴露完整 skia Cap。
- 无 wavy（波形）变体——Compose 该分支尚未实现（token 已预留 WaveSize 等）。
- 无无障碍 progressBarRangeInfo（winia 语义系统未覆盖）。

## 5. 运行与测试

```bash
cargo run -p winia --example progress_indicator_demo        # 交互演示
cargo run -p winia --example progress_indicator_demo --features debug-server  # 调试
cargo test -p winia --lib progress_indicator                # 单元 + 像素测试
```

测试覆盖：gap/stop 几何换算、动画规格数值（与 Compose keyframes 对齐）、
默认色解析、Linear/Circular determinate 像素渲染、indeterminate 无 track、
height>width 退化 Butt、无限动画注册/清理。
