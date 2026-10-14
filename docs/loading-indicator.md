# Loading Indicator（M3 Expressive）

> 状态：已实现（分支 `feat/loading-indicator`）。
> M3 规范：https://m3.material.io/components/loading-indicator/specs
> 参考旧版：`D:\winia\winia\src\ui\widget\loading_indicator.rs`（v1 实现）
> 前置基建：`material-shapes` crate（RoundedPolygon / Morph / MaterialShapes，已复制进 workspace）

## 1. 定位

- Loading indicator 是 **M3 Expressive** 新增组件：通过 7 个 Material 3 shape 之间的 Morph
  动画表达“短等待（<5s）”，替代 indeterminate circular progress indicator。
- 与 Progress Indicator 的区别：**不能**从 indeterminate 过渡到 determinate，也不表达进度。

## 2. API surface

```rust
// Uncontained (the default) — no container, the indicator is Primary
LoadingIndicator::new().build(ctx);

// Container configuration — a PrimaryContainer round container, an OnPrimaryContainer indicator
LoadingIndicator::new().contained(true).build(ctx);

// Overrides
LoadingIndicator::new()
    .contained(true)
    .indicator_color(color)
    .container_color(color)
    .container_shape(Shape::rounded(12.0))
    .modifier(Modifier::new().size(96.0, 96.0))
    .build(ctx);
```

### 默认值

| | Uncontained | Container |
| --- | --- | --- |
| indicator color | Primary | OnPrimaryContainer |
| container color | Transparent (not painted) | PrimaryContainer |
| container shape | Circle | Circle |
| container size | 48×48dp | 48×48dp |
| active indicator size | 38dp | 38dp |
| shapes | 7 M3 shapes | same |

### Colours

The token table (`LoadingIndicatorTokens.kt:24-27`) is what the component resolves:

- `ActiveIndicatorColor` = Primary — the uncontained indicator
- `ContainedActiveColor` = OnPrimaryContainer — the contained indicator
- `ContainedContainerColor` = PrimaryContainer — the contained container

The M3 spec artwork draws the contained form as a Primary indicator over a SecondaryContainer, and winia
followed that artwork until now: `.contained(true)` used to resolve to Primary + SecondaryContainer. It
follows the tokens from here, so the defaults agree with what Compose's `LoadingIndicatorDefaults`
actually resolves. Either colour can still be overridden — for instance back to the artwork pairing:

```rust
LoadingIndicator::new()
    .contained(true)
    .indicator_color(theme.primary)
    .container_color(theme.secondary_container)
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

- 组件：`winia/src/components/loading_indicator.rs`
- 导出：`winia::components::LoadingIndicator` + `winia::prelude::*`
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
