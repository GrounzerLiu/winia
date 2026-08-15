# Wavy Progress Indicator 实现笔记

> 状态：**已实现（分支 `progress-indicator-wavy`）**。Compose 源码已通读并归档在
> `.reasonix/compose_wavy_ref/`。
> 分支计划：`progress-indicator-wavy`（从 v2 分出）。
> 前置：Morph/RoundedPolygon 基建已通过 `material-shapes` crate 落地进 workspace
> （见 [docs/loading-indicator.md](loading-indicator.md)），落地时直接复用即可。
> 实现：`winia/src/ui/wavy_progress_indicator.rs`；demo：
> `winia/examples/wavy_progress_indicator_demo.rs`。

## 1. 背景与定位

- Wavy 是 **M3 Expressive（实验性扩展）** 的 progress indicator 变体：经典 M3 配置表里标
  `--`（未提供），M3 Expressive 标 Available（m3.material.io/components/progress-indicators/specs）。
- Compose 已实现（androidx-main，2025）：公开 API 在 `WavyProgressIndicator.kt`，
  核心绘制在 `internal/LinearWavyProgressModifiers.kt` + `internal/CircularWavyProgressModifiers.kt`。
- 与 flat 版共用 token 体系；额外 token（已预留）：
  - Linear：`ActiveWaveAmplitude 3dp`、`ActiveWaveWavelength 40dp`、
    `IndeterminateActiveWaveWavelength 20dp`、`WaveHeight 10dp`
  - Circular：`ActiveWaveAmplitude 1.6dp`、`ActiveWaveWavelength 15dp`、`WaveSize 48dp`
- 规范定义（原文）：*Wavy indicators use amplitude and wavelength to determine the shape of
  the wave. Amplitude measures from the center of the resting position to the center of the
  peak. Wavelength measures the distance between two adjacent peaks. Height is the overall
  container height.*

## 2. API 面（对齐 Compose WavyProgressIndicator.kt）

```kotlin
// Linear —— determinate
LinearWavyProgressIndicator(
    progress: () -> Float,

    modifier: Modifier = Modifier,
    color = Defaults.indicatorColor,        // Primary
    trackColor = Defaults.trackColor,       // SecondaryContainer
    stroke = linearIndicatorStroke,         // Stroke(4dp, Round)——ActiveThickness
    trackStroke = linearTrackStroke,        // Stroke(4dp, Round)——TrackThickness
    gapSize = 4.dp,                         // LinearIndicatorTrackGapSize = TrackActiveSpace
    stopSize = 4.dp,                        // LinearTrackStopIndicatorSize = StopSize
    amplitude: (progress) -> Float = Defaults.indicatorAmplitude,
    wavelength: Dp = 40.dp,                 // LinearDeterminateWavelength = ActiveWaveWavelength
    waveSpeed: Dp = wavelength,             // 默认 = 波长（每秒移动一个波长）
)

// Linear —— indeterminate（无 progress/stopSize；amplitude 为固定 Float，默认 1f）
LinearWavyProgressIndicator(
    modifier, color, trackColor, stroke, trackStroke, gapSize,
    amplitude: Float = 1f,
    wavelength: Dp = 20.dp,                 // LinearIndeterminateWavelength
    waveSpeed: Dp = wavelength,
)

// Circular —— determinate（无 stopSize；容器 48dp = WaveSize）
CircularWavyProgressIndicator(
    progress, modifier, color, trackColor, stroke, trackStroke, gapSize,
    amplitude: (progress) -> Float = Defaults.indicatorAmplitude,
    wavelength: Dp = 15.dp,                 // CircularWavelength = ActiveWaveWavelength
    waveSpeed: Dp = wavelength,
)

// Circular —— indeterminate（amplitude 固定 Float = 1f，wavelength 15dp）
// 容器 48dp（CircularContainerSize = WaveSize）；indeterminate 用 Box 包裹 Spacer
// （progressSemantics 与绘制分离——TalkBack 噪音问题 b/347736702，winia 可简化）
```

### 默认值（WavyProgressIndicatorDefaults）

| 项 | 值 | 备注 |
| --- | --- | --- |
| ProgressAnimationSpec | `tween(DurationLong2, LinearEasing)` | ⚠ 与 flat 版不同！flat 是 Spring，wavy 是 tween 线性 |
| indicatorColor / trackColor | Primary / SecondaryContainer | 同 flat |
| linearIndicatorStroke | Stroke(ActiveThickness 4dp, Round) | |
| linearTrackStroke / circularTrackStroke | Stroke(TrackThickness 4dp, Round) | |
| circularIndicatorStroke | Stroke(ActiveThickness 4dp, Round) | |
| LinearDeterminateWavelength | 40dp | ActiveWaveWavelength |
| LinearIndeterminateWavelength | 20dp | IndeterminateActiveWaveWavelength |
| LinearContainerHeight / Width | 10dp / 240dp | WaveHeight / 规范宽 |
| CircularContainerSize | 48dp | WaveSize |
| LinearTrackStopIndicatorSize | 4dp | StopSize |
| indicatorAmplitude | `progress<=0.1 或 >=0.95 → 0f，否则 1f` | 两端退化为直线 |
| Increasing/DecreasingAmplitudeAnimationSpec | tween(DurationLong2, Standard) / tween(DurationLong2, EmphasizedAccelerate) | 振幅过渡 |

### winia API（builder 风格，命名贴近 Compose）

```rust
// Linear determinate
LinearWavyProgressIndicator::new(0.5)
    .color(my_color)
    .track_color(my_track)
    .stroke_width(4.0)          // active thickness
    .track_stroke_width(4.0)    // track thickness
    .gap_size(4.0)
    .stop_size(4.0)
    .amplitude_fn(|p| if p <= 0.1 || p >= 0.95 { 0.0 } else { 1.0 })
    .wavelength(40.0)
    .wave_speed(40.0)           // 默认 = wavelength
    .build(ctx);

// Linear indeterminate（amplitude 固定 Float）
LinearWavyProgressIndicator::indeterminate()
    .amplitude(1.0)
    .wavelength(20.0)
    .build(ctx);

// Circular determinate / indeterminate
CircularWavyProgressIndicator::new(0.5)
    .amplitude_fn(|_| 1.0)
    .build(ctx);
CircularWavyProgressIndicator::indeterminate().build(ctx);
```

## 3. Linear 核心绘制（internal/LinearWavyProgressModifiers.kt）

架构：`DelegatingNode` + `CacheDrawModifierNode`，绘制缓存 `LinearProgressDrawingCache`
（波路径/PathMeasure 按 size/wavelength/振幅缓存，避免每帧重建）。winia 对应：
`Modifier::draw()` 闭包 + `LinearShapesCache`（满幅波路径/长度缓存）与
`CircularShapesCache`（RoundedPolygon + `Morph::morph_match` 缓存）。

### 3.1 满幅波路径构造（updateFullPaths）

```kotlin
// stroke cap 预留宽度：max(stroke.width/2, trackStroke.width/2)
//（两端都是 Butt 或 height>width 时为 0——退化条件同 flat）
currentStrokeCapWidth = if (stroke.cap==Butt && trackStroke.cap==Butt || height>width) 0
                       else max(stroke.width/2, trackStroke.width/2)

fullProgressPath.moveTo(0f, 0f)
if (amplitude == 0f) {
    fullProgressPath.lineTo(width, 0f)          // 平面 → 直线
} else {
    // 二次贝塞尔波：半波长交替上下
    val halfWavelengthPx = wavelength / 2f
    var anchorX = halfWavelengthPx
    var controlX = halfWavelengthPx / 2f
    var controlY = height - stroke.width        // 满幅控制点高
    val widthWithExtraPhase = width + wavelength * 2  // 多画两个波长供滚动
    while (anchorX <= widthWithExtraPhase) {
        fullProgressPath.quadraticTo(controlX, controlY, anchorX, 0f)
        anchorX += halfWavelengthPx
        controlX += halfWavelengthPx
        controlY *= -1f                          // 上下交替
    }
}
fullProgressPath.translate(0f, height / 2f)    // 垂直居中
pathMeasure.setPath(fullProgressPath)
progressPathScale = pathMeasure.length / pathBounds.width
// ⚠ 二次贝塞尔波峰 = 控制点高度的一半（quadratic 特性），注释明确说明
// 控制点满幅 → 波峰实际为 (height - strokeWidth) / 2
```

### 3.2 按 progress 截段（updateDrawPaths）

```kotlin
// progressFractions：determinate [0, progress]；indeterminate [tail1, head1, tail2, head2]
val barTail = startFraction * width
val barHead = endFraction * width

// 第一段：进度小时 gap 收缩（与 flat 相同的自适应）
adjustedTrackGapSize = if (barHead < capWidth) 0
                       else min(barHead - capWidth, gapSize)

// 端点 clamp 预留 cap 空间
adjustedBarHead = barHead.coerceIn(capWidth, width - capWidth)
adjustedBarTail = barTail.coerceIn(capWidth, width - capWidth)

if (abs(endFraction - startFraction) > 0) {
    val waveShift = if (amplitude != 0f) waveOffset * wavelength else 0f
    pathMeasure.getSegment(
        startDistance = (adjustedBarTail + waveShift) * progressPathScale,
        stopDistance  = (adjustedBarHead + waveShift) * progressPathScale,
        destination  = progressPathToDraw,
    )
    // 平移回原位置 + 按振幅 Y 缩放（满幅路径 → 目标振幅）
    progressPathToDraw.transform(Matrix {
        translate(if (waveShift > 0) -waveShift else 0, (1 - amplitude) * halfHeight)
        if (amplitude != 1f) scale(y = amplitude)
    })
}
```

### 3.3 track 绘制（从右往左 + gap）

```kotlin
// track 从右端开始画（nextEndTrackOffset = width - capWidth），每段 progress 之间留 gap：
val adaptiveTrackSpacing = if (activeIndicatorVisible)
                               adjustedTrackGapSize + capWidth * 2
                           else adjustedTrackGapSize
// 每段 progress 右侧：track 画到 (barHead + spacing)，然后跳过进度段继续向左
// 最终 fill 到最左端 capWidth
```

### 3.4 stop indicator（drawStopIndicator）

```kotlin
stopIndicatorSize = min(trackStroke.width, stopSize)
indicatorX = width - stopSize - (stopSize == trackStroke.width ? 0 : trackStroke.width/4)
progressX = width * progressEnd + horizontalInsets  // horizontalInsets = capWidth
// 进度逼近末端时 stop 收缩至消失（Compose 注释：constant until near end, then shrinks）
if (indicatorX <= progressX) {
    stopIndicatorSize = max(0f, stopIndicatorSize - (progressX - indicatorX))
    indicatorX = progressX
}
// Round cap → drawCircle；Butt → drawRect
```

### 3.5 waveOffset 滚动动画（updateOffsetAnimation）

```kotlin
// 仅当 waveSpeed > 0 且 wavelength > 0 时运行
val durationMillis = ((wavelength / waveSpeed) * 1000).roundToInt().coerceAtLeast(50)
val startValue = waveOffset.floatValue
Animatable(startValue).animateTo(startValue + 1f,
    infiniteRepeatable(tween(durationMillis, LinearEasing), RepeatMode.Restart)
) { waveOffset.floatValue = value % 1f }
// 语义：默认 waveSpeed = wavelength → 1 秒移动一个波长
```

### 3.6 振幅过渡（updateAmplitudeAnimation）

```kotlin
// determinate：amplitude 是 progress 的函数，变化时平滑过渡
if (target 变了 && 动画未在跑) {
    amplitudeAnimatable.animateTo(target,
        if (当前值 < target) IncreasingAmplitudeAnimationSpec  // tween(Long2, Standard)
        else                DecreasingAmplitudeAnimationSpec  // tween(Long2, EmphasizedAccelerate)
    )
}
```

### 3.7 indeterminate（复用 flat 动画）

- 4 条 head/tail 无限动画**完全复用 flat 的** `linearIndeterminate*AnimationSpec`
  （1750ms 周期，delay 0/250/650/900ms，EmphasizedAccelerate）——上轮已实现 ✓
- progressFractions = [firstTail, firstHead, secondTail, secondHead]
- amplitude 是固定参数（默认 1f），无振幅函数

## 4. Circular 核心绘制（internal/CircularWavyProgressModifiers.kt）

### 4.1 形状模型：圆 ↔ 星形多边形 Morph（关键！）

```kotlin
// CircularShapes.update(size, wavelength, strokeWidth, requiresMorph)
val r = size.minDimension / 2 - strokeWidth / 2
val numVertices = max(5, round(2 * PI * r / wavelength))   // 每波长一个顶点

trackPolygon = RoundedPolygon.circle(numVertices).normalized()
activeIndicatorPolygon = RoundedPolygon.star(
    numVerticesPerRadius = numVertices,
    innerRadius = 0.75f,                                   // 谷半径 = 0.75R
    rounding = CornerRounding(radius = 0.35f, smoothing = 0.4f),   // 峰圆角
    innerRounding = CornerRounding(radius = 0.5f),                  // 谷圆角
).normalized()

// 需要振幅过渡时创建 Morph：
activeIndicatorMorph = Morph(start = trackPolygon, end = activeIndicatorPolygon)
```

### 4.2 绘制路径选择（getProgressPath）

```kotlin
// amplitude ∈ [0,1]：Morph.toPath(progress = amplitude)
//   → 0 = 纯圆（track），1 = 满星形
// 无 Morph 时（从未需要过渡）：amplitude==1 → star path；否则 circle path
// track 恒为 circle path（同顶点数 → morph 平滑）
```

### 4.3 滚动（startOffsetAnimation + updateDrawPaths）

```kotlin
// durationMillis = (wavelength / waveSpeed) * 1000 * vertexCount  // 每波长一个顶点
// waveOffset 0→1 无限动画（同 Linear 模式）

// 路径重复两圈（supportMotion=true → repeatPath），便于偏移取段：
progressPathMeasure.setPath(fullProgressPath, forceClosed = true)
progressPathLength = if (enableProgressMotion) length / 2 else length

// 取段：
val startStopShift = waveOffset * progressPathLength
progressPathMeasure.getSegment(pStart + shift, pStop + shift, progressPathToDraw)

// 然后整体绕中心旋转补偿：
val offsetAngle = waveOffset * 360 % 360
progressPathToDraw.translate(-center).transform(rotateZ(-offsetAngle)).translate(center)
```

### 4.4 track 段（同 flat gap 语义）

```kotlin
val trackGapSize = min(pStop, gapSize)
val horizontalInsets = min(pStop, capWidth)
val trackSpacing = horizontalInsets * 2 + trackGapSize
trackPathMeasure.getSegment(endProgress * trackPathLength + trackSpacing,
                             trackPathLength - trackSpacing, trackPathToDraw)
```

### 4.5 归一化与缩放（processPath）

```kotlin
// RoundedPolygon 归一化到 (0.5, 0.5) 中心 → 路径先 scale(width-stroke.width, height-stroke.width)
// 再 translate 到容器中心（scaleMatrix 缓存复用）
```

## 5. winia 移植方案

### 5.1 基建盘点

| 依赖 | winia 状态 | 说明 |
| --- | --- | --- |
| 二次贝塞尔 Path | ✓ skia `Path::quad_to` | 直接可用 |
| PathMeasure 截段 | ✓ skia `PathMeasure` | `getSegment` 语义对齐 |
| 无限滚动动画 | ✓ `InfiniteRepeatableSpec::restart_tween`（上轮新增） | waveOffset 0→1 循环 |
| 振幅过渡 | ✓ `ctx.animate_float_as_state` + TweenSpec | Standard/EmphasizedAccelerate 已有 CubicBezier |
| 缓存（避免每帧重建波路径） | 需新增 | draw 闭包内按 (size, wavelength, amplitude) 缓存 |
| RoundedPolygon（circle/star/归一化） | ✗ 无 | 需移植（Morph 前置） |
| Morph（圆↔星插值 + repeatPath + rotate pivot） | ✗ 无 | **作者已在另一台设备移植过**——复用其实现 |
| CornerRounding（radius + smoothing） | ✗ 无 | Morph 配套 |

### 5.2 建议拆分

1. **阶段 A：Linear wavy**（不依赖 Morph）——贝塞尔波 + PathMeasure + 已有动画基建，
   可完整对齐官方源码。含 determinate/indeterminate、振幅函数、stop indicator、gap 语义。
2. **阶段 B：Morph/RoundedPolygon 基建**——从作者已有移植落地到 winia（skia 路径化：
   多边形 → Path，顶点间圆角用圆弧/贝塞尔逼近）。
3. **阶段 C：Circular wavy**——Morph 之上实现圆↔星形 + 滚动旋转。

### 5.3 组件结构建议（镜像现有 progress_indicator.rs）

```rust
// winia/src/ui/wavy_progress_indicator.rs（或并入 progress_indicator.rs）
pub struct LinearWavyProgressIndicator {
    progress: f32,
    indeterminate: bool,
    modifier, color, track_color,
    stroke_width: f32,      // ActiveThickness 4dp
    track_stroke_width: f32, // TrackThickness 4dp
    gap_size: f32,
    stop_size: f32,
    amplitude: 闭包或枚举（默认 indicatorAmplitude 语义）,
    wavelength: f32,        // 默认 40 / 20
    wave_speed: f32,        // 默认 = wavelength
}
// draw 闭包：满幅波路径（按 wavelength 缓存）→ PathMeasure 截段 → 振幅缩放 → track → stop
// waveOffset：remember_infinite_transition + restart_tween
```

## 6. 测试要点

- 波几何：quadraticTo 锚点/控制点位置、波峰高度 = (h - strokeWidth)/2 断言
- PathMeasure 截段：给定 waveOffset 下 startDistance/stopDistance 与 Compose 公式一致
- progressPathScale：波浪路径长/宽 > 1 的缩放正确性
- 振幅过渡：0↔1 切换时动画规格选择（增 Standard / 减 EmphasizedAccelerate）
- stop indicator 收缩：进度逼近 1 时尺寸单调减至 0
- 像素测试：amplitude=0 时应与 flat 视觉一致（回归锚点）
- Circular：numVertices = max(5, round(2πr/λ))；amplitude=0 → 纯圆（与 flat 对比）
- 缓存失效：wavelength/size/振幅变化触发重建

## 7. TODO（作者备注）

- [x] Morph/RoundedPolygon 移植已复制进 workspace（`material-shapes` crate，见 loading-indicator）
- [x] 阶段 A：Linear wavy（分支 `progress-indicator-wavy`）
- [x] 阶段 B：Morph 基建落地（`material-shapes` 已入 workspace）
- [x] 阶段 C：Circular wavy
- [x] demo + docs + 测试（`wavy_progress_indicator_demo.rs` + 14 个单测）
- [x] review 问题修复（wave 动画按需启停、Linear/Circular 路径/Morph 缓存、锁恢复、振幅 token 跟踪）
- [x] Circular 相位稳定性：Morph 中间帧与 amplitude=1 统一走 Morph，避免波峰角度跳变
- [x] 移除依赖全局 `is_animating()` 的竞态测试（并行下会互相污染）
- [x] review → 合并 v2（已验收）

## 8. 参考

- Compose 源码（androidx-main，已归档 `.reasonix/compose_wavy_ref/`）：
  - `WavyProgressIndicator.kt`（504 行）
  - `internal/LinearWavyProgressModifiers.kt`（1145 行）
  - `internal/CircularWavyProgressModifiers.kt`（1408 行）
- M3 规范：https://m3.material.io/components/progress-indicators/specs
- 关键差异：wavy 的 ProgressAnimationSpec 是 **tween 线性**（flat 是 Spring）
