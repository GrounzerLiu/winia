# 动画系统分析与设计

## 一、Jetpack Compose 动画架构

### 三层 API 体系

| 层级 | API | 适用场景 |
|------|-----|---------|
| 高层 | `animate*AsState`, `AnimatedVisibility`, `Crossfade`, `animateContentSize` | 常见动画模式，开箱即用 |
| 中层 | `updateTransition`, `rememberInfiniteTransition` | 多属性协同动画、无限循环 |
| 底层 | `Animatable`, `TargetBasedAnimation` | 完全控制动画播放 |

### 核心机制：物理引擎驱动的 Spring

Compose 默认动画不是补间（tween），而是**物理弹簧模拟**（spring）：

```
// 半隐式欧拉积分
val k = stiffness * -(lastDisplacement - finalPosition)  // F = -kx
val c = dampingRatio * 2 * sqrt(stiffness)                // 阻尼系数
val force = k - c * lastVelocity                           // 净力
lastVelocity += force / mass * timeStep
lastDisplacement += lastVelocity * timeStep
```

- **有记忆**：每帧依赖上一帧速度，不是无状态百分比
- **可中断**：目标变化时立即重算，不重启
- **三种阻尼**：`<1` 超调振荡、`=1` 临界最快、`>1` 过阻尼迟钝

### 帧驱动管线

```
VSync → Choreographer → withFrameNanos → 物理积分 → MutableState 写入
→ Snapshot 通知重组 → Composable 重绘
```

- `Animatable` 持有 `AnimationState(当前值, 速度)`
- `TargetBasedAnimation` 用 `AnimationSpec` 计算每帧值
- 结果写入 `MutableState` → `Snapshot` 自动追踪依赖 → 触发重组

### Transition：多属性协同

`updateTransition` 用一个 `targetState` 控制所有子动画共享同一帧回调，避免多个独立 `animate*AsState` 不同步。

### AnimationSpec 类型

| 类型 | 描述 |
|------|------|
| `spring(dampingRatio, stiffness)` | 物理弹簧，**默认** |
| `tween(duration, easing)` | 补间，可指定插值器 |
| `keyframes { ... }` | 关键帧序列 |
| `repeatable/repeatableInfinite` | 重复播放 |
| `snap` | 瞬时跳转 |

---

## 二、D:\winia 动画架构

### 两套体系并存

**A. 布局动画（LayoutAnimation）** — 用于 UI 属性插值
- 创建：`ctx.animate(include_target!(...)).duration(300).transformation(|| {...}).start()`
- 进度：`start_time + duration`
- 插值器：`EaseOutCirc`, `EaseInCirc` 等
- 驱动：`EventLoopProxy` 发事件 + `request_redraw()`

**B. 共享值动画（Shared Animation）** — 独立数值动画
- `Tween<T>` — 补间，from→to，Duration+Interpolator
- `Spring<T>` — 物理弹簧，解析解（三种阻尼），固定 dt=1/60
- `Keyframes<T>` — 关键帧序列
- `Repeat<T>` — 循环/往返重复
- `SharedAnimation<T>` — 包装为 `Box<dyn Animation<T>>`，自动写回 `SharedSource`

### 与 Compose 的关键差异

| 维度 | Compose | D:\winia |
|------|---------|----------|
| 默认 spec | `spring()` | 无默认 |
| 架构 | `Animatable` + `Snapshot` + 重组 | `SharedSource` + `LayoutAnimation` |
| 帧驱动 | `MonotonicFrameClock` → `Choreographer` | `request_redraw()` |
| 多属性同步 | `updateTransition` 统一帧回调 | 独立 anim 加入列表统一 update |
| 值类型 | `AnimationVector` + `TwoWayConverter` | `f32` / `Color` / 泛型 |
| 可中断 | 随时 `animateTo` 新目标 | `animate_to(target)` |

---

## 三、本项目动画系统设计方案

### 核心原则

1. **与现有 State 系统集成**：动画值写入 `State<T>` → 自动触发重组
2. **物理弹簧为主，补间为辅**：默认 `SpringSpec`，可选 `TweenSpec`
3. **单帧回调管线**：在 `AboutToWait` / `RedrawRequested` 中统一更新活跃动画
4. **无外部依赖**：用 `std::time::Instant`，无需额外定时器

### API 设计（对齐 Compose）

```rust
// 高层：animateFloatAsState
let alpha = ctx.animate_float_as_state(
    1.0,                                // 目标值
    SpringSpec::default(),              // 动画规格
);
// alpha.get() 随时间从旧值平滑过渡到 1.0

// 中层：updateTransition
let transition = ctx.update_transition(current_page, |page| {
    let offset_x = transition.animate_dp(page_to_offset(page));
    let alpha = transition.animate_float(page_to_alpha(page));
});

// 底层：Animatable
let mut anim = ctx.animatable(0.0);
anim.animate_to(100.0, SpringSpec::bouncy());
```

### 模块结构

```
winia/src/animation/
├── mod.rs          // 导出 + AnimationSpec 枚举
├── spec.rs         // SpringSpec, TweenSpec, KeyframeSpec
├── animatable.rs   // Animatable<T> — 底层值动画
├── animate_as_state.rs  // animate_float_as_state 等高层 API
├── transition.rs   // updateTransition
└── clock.rs        // AnimationClock — 帧时间源
```

### 与现有系统集成

1. **State 绑定**：`Animatable` 内部持有 `State<T>`，`update()` 时写 `State::set()`
2. **帧驱动**：在 `app.rs` 的 `AboutToWait` 或 `RedrawRequested` 中遍历活跃 `Animatable`，调用 `update()`
3. **Spring 物理模拟**：用半隐式欧拉积分，纯 Rust 实现

### Spring 物理模拟

```rust
pub struct SpringSimulation {
    displacement: f32,      // 当前位移（距目标）
    velocity: f32,          // 当前速度
    stiffness: f32,         // 刚度（默认 1500.0）
    damping_ratio: f32,     // 阻尼比（默认 1.0 临界）
    mass: f32,              // 质量（默认 1.0）
    threshold: f32,         // 停止阈值（默认 0.01）
    // 固定 dt=1/60, accumulator 模式，最多 10 子步
}

impl SpringSimulation {
    pub fn update(dt: Duration) -> f32;
    pub fn is_done(&self) -> bool;
    pub fn animate_to(&mut self, target: f32);
}
```

### 实现顺序

1. `AnimationSpec` + `SpringSimulation`
2. `Animatable<T>` + 帧驱动集成
3. `animate_float_as_state`（基于 Animatable + State）
4. `updateTransition`
5. `AnimatedVisibility`
