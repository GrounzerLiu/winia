# Loading Indicator（M3 Expressive）

> 状态：已实现（分支 `feat/loading-indicator`）。
> M3 规范：https://m3.material.io/components/loading-indicator/specs
> 参考旧版：`D:\winia\winia\src\ui\widget\loading_indicator.rs`（v1 实现）
> 前置基建：`material-shapes` crate（RoundedPolygon / Morph / MaterialShapes，已复制进 workspace）

## 1. 定位

- Loading indicator 是 **M3 Expressive** 新增组件：通过 7 个 Material 3 shape 之间的 Morph
  动画表达“短等待（<5s）”，替代 indeterminate circular progress indicator。
- 与 Progress Indicator 的区别：**不能**从 indeterminate 过渡到 determinate，也不表达进度。

## 2. API 面（对齐 M3 Default / Uncontained 配置）

```rust
// Uncontained（默认）——无容器，Primary indicator
LoadingIndicator::new().build(ctx);

// Container 模式——SecondaryContainer 圆形容器 + Primary indicator
LoadingIndicator::new().contained(true).build(ctx);

// 自定义
LoadingIndicator::new()
    .contained(true)
    .indicator_color(color)
    .container_color(color)
    .container_shape(Shape::rounded(12.0))
    .modifier(Modifier::new().size(96.0, 96.0))
    .build(ctx);
```

### 默认值

| 项 | Uncontained | Container |
| --- | --- | --- |
| indicator color | Primary | Primary |
| container color | Transparent（不绘制） | SecondaryContainer |
| container shape | Circle | Circle |
| 容器尺寸 | 48×48dp | 48×48dp |
| active indicator 尺寸 | 38dp | 38dp |
| shapes | 7 个 M3 shapes | 同左 |

### 配色说明

M3 specs 页的配图里，带容器的 Loading indicator 视觉上是 **Primary 指示器 + SecondaryContainer 容器**；
spec 的 token 表也列出了对应关系：

- `Loading indicator active indicator color` = Primary
- `Loading indicator container color` = SecondaryContainer
- `Loading indicator contained active indicator color` = OnPrimaryContainer
- `Loading indicator contained container color` = PrimaryContainer

注意：Compose/MDC 的 `ContainedLoadingIndicator` 源码默认是 **PrimaryContainer + OnPrimaryContainer**，
与 M3 specs 配图不完全一致。本项目按 M3 specs 配图，将 `.contained(true)`（Container 模式）默认配色设为
**Primary + SecondaryContainer**。如需 Compose 风格 Contained 配色，可显式传入：

```rust
LoadingIndicator::new()
    .contained(true)
    .indicator_color(theme.on_primary_container)
    .container_color(theme.primary_container)
    .build(ctx);
```

## 3. 动画实现

- **Morph 序列**：`SoftBurst → Cookie9Sided → Pentagon → Pill → Sunny → Cookie4Sided → Oval`，
  首尾回环。每个周期用 Spring(bouncy 0.6 / stiffness 200) 将 progress 0→1，周期约 650ms。
- **步进旋转**：每个 shape 切换后额外旋转目标 +90°（quarter rotation）。
- **全局旋转**：360° / 4666ms 线性无限循环（`InfiniteRepeatableSpec::restart_tween`）。
- 绘制：`LoadingIndicatorNode` 具名绘制（exp/wavy-node 迁移，原 `.draw` 匿名闭包，
  `pub(crate)`）：`Morph.to_path(progress)` → `progress_path` 缩放到 active indicator 尺寸并居中 →
  绕容器中心旋转。4 个动画值（morph_progress/index/rotation_target/global_rotation）
  peek 不进 key；静态参数（contained/shape/两色）全进 key。

## 4. 代码位置

- 组件：`winia/src/ui/loading_indicator.rs`
- 导出：`winia::ui::LoadingIndicator` + `winia::prelude::*`
- 示例：`winia/examples/loading_indicator_demo.rs`
- 依赖：`material-shapes`（workspace member，feature `shape_util`）

## 5. 测试

```bash
cargo test -p winia --lib loading_indicator
cargo run -p winia --example loading_indicator_demo
```

- 单元：默认 uncontained / Container 颜色解析、7 个 Morph、缩放因子、`progress_path` 居中。
- 像素：uncontained 只画 Primary 不画容器；Container 同时画 SecondaryContainer 容器与 Primary indicator。
- DrawNode 迁移：node_key 全覆盖（动画值不进 key）、双路像素对照（首帧确定值逐字节）。
