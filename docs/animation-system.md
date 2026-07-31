# 动画系统 — 实现与进度

> 更新日期：2026-07-31 · 分支：text-field

## 一、总体架构

对标 Jetpack Compose 三层动画 API，基于本项目 `State<T>` + 事件循环驱动。

```
用户 API 层
  animate_float_as_state / animate_color_as_state / updateTransition
           │
Animatable<T> 引擎（底层）
  animate_to(target, spec) / update() / snap_to()
           │
AnimationSpec（动画规格）
  Spring(Tween / SpringSpec / TweenSpec
           │
帧驱动（app.rs new_events）
  update_animations() → Animatable::update → State.set → notify → 重组渲染
```

## 二、已实现 ✅

### 1. 值类型（AnimatableValue trait）
- `f32` — 线性插值
- `Color` — RGBA 四通道插值（`crate::modifier::Color`）

### 2. 动画规格（AnimationSpec）
- **Spring**：物理弹簧，半隐式欧拉积分，固定 1/60s 子步 + accumulator（最多 10 步）
  - `SpringSpec { damping_ratio, stiffness, mass, threshold }`
  - `default()`：damping=1.0（临界）、stiffness=1500、threshold=0.01
  - `bouncy()`：damping=0.6、threshold=0.1（约 300ms 收敛，带超调回弹）
- **Tween**：补间，`duration` + `interpolator`（函数指针）
  - `TweenSpec::default()`：300ms + linear

### 3. 插值器（interpolator.rs）
从 D:\winia 复制，含 **24 种**预计算 bezier 插值器：
`Linear`, `EaseIn/Out/InOut` (Sine, Quad, Cubic, Quart, Quint, Expo, Circ, Back, Elastic, Bounce)

### 4. 高层 API（ComposeCtx 方法）
- `animate_float_as_state(target, spec) -> State<f32>` — 对标 animateFloatAsState
- `animate_color_as_state(target, spec) -> State<Color>` — 对标 animateColorAsState（Spring 自动降级 Tween）

### 5. 中层 API
- `update_transition(target, spec, label) -> Transition` — 对标 updateTransition
  - `transition.animate_float(ctx, target_fn, label) -> State<f32>`

### 6. 底层 API
- `Animatable<T>` — `new / animate_to / update / snap_to`
- `push_animatable` / `push_animatable_color` — 注册到全局活跃列表

### 7. 渲染支持
- `Modifier::graphics_layer(GraphicsLayerParams)` — 对标 Compose graphicsLayer
  - `scale_x/y, alpha, translation_x/y, rotation_z`
  - 渲染时 `canvas.save → translate → scale → rotate → restore`

### 8. 帧驱动
- `update_animations()` / `is_animating()` — 全局动画列表（f32 + Color 分开）
- `app.rs new_events`：每轮事件批次推进动画 → 请求所有窗口 redraw
- 动画注册后：`push_*` 首次 `update()` → notify → wake_up → 下一轮 new_events 自续
- `ControlFlow::Wait` + `request_redraw` 自驱动（无 Poll/Wait 切换竞态）

## 三、关键机制

### Spring 物理（animation.rs compute_spring_displacement）
```
半隐式欧拉积分：
  force = -stiffness * displacement - damping_coeff * velocity
  velocity += force / mass * step
  displacement += velocity * step
固定步长：FIXED_DT = 1/60s，accumulator 最多 10 子步
位移持续累积（current_displacement 存于 AnimationState）
收敛：|displacement| < threshold && |velocity| < threshold
```

### 动画去重（push_animatable）
```
1. state.get() == target → 跳过（已到位）
2. 同 state_id + 同 target 已有动画 → 跳过（防每帧重启）
3. 同 state_id 目标不同 → 移除旧动画，创建新动画
4. 锁内只检查/收集，anim.update() 锁外执行（防死锁）
```

### 锁安全
- `push_animatable` / `update_animations` 持有 Mutex 时**不执行用户代码**
- `update_animations` 用 `std::mem::take` 取出列表 → 锁外 update → 锁内 extend

## 四、Demo（animation_demo.rs）

| 节 | 内容 | 动画 |
|----|------|------|
| 1 | 蓝色盒子宽度 | Spring Bouncy（50↔300px） |
| 2 | 绿色盒子透明度/宽度 | Tween 300ms |
| 3 | 橙色盒子 offset | updateTransition (Spring) |
| 4 | 颜色盒子 | animate_color_as_state（绿↔紫 500ms） |

## 五、单元测试（5 个）

- `spring_converges_to_target` — 临界阻尼收敛
- `spring_bouncy_overshoots_then_converges` — 欠阻尼超调后收敛
- `spring_reverse_animation` — 反向动画（防 max(EPSILON) 卡死回归）
- `spring_dt_zero_is_safe` — dt=0 无 NaN
- `tween_completes_within_duration` — 300ms 内完成

## 六、缺失（对标 Compose）❌

### 高层
| API | 用途 | 优先级 |
|-----|------|--------|
| `AnimatedVisibility` | 内容出现/消失（fade/expand/shrink） | 高 |
| `Crossfade` | 两内容交叉淡入淡出 | 中 |
| `animateContentSize` | 尺寸变化自动动画 | 中 |
| `animateDpAsState` / `animateSizeAsState` / `animateOffsetAsState` | 其他值类型 | 中 |

### 中层
| API | 用途 |
|-----|------|
| `rememberInfiniteTransition` | 无限循环动画（加载指示器） |
| `updateTransition` 完整版 | animate_dp/animate_color/animate_size |

### 底层
| API | 用途 |
|-----|------|
| `keyframes` | 关键帧动画 |
| `repeatable` / `infiniteRepeatable` | 重复/往返动画 |
| `snap` | 瞬时跳转 |

### 值类型
| 类型 | 状态 |
|------|------|
| `Dp` / `Offset` / `Size` | 需实现 AnimatableValue + lerp |
| `Rect` / `BorderRadius` | 需实现 |

## 七、已知限制

1. **Color Spring 降级**：颜色无单一 f32 值，Spring 自动降级 Tween
2. **dt 超 166ms 截断**：掉帧后弹簧丢时间（可接受）
3. **dedup 忽略 spec**：同 state+同 target 不同 spec 的重启请求被忽略
4. **`is_animating()` 无调用者**：可清理或用于后续节流
5. **动画期间无 60fps 节流**：等效 Poll，CPU 占用较高（动画时长有限，可接受）

## 八、后续计划

1. **`rememberInfiniteTransition`**（低投入高回报，加载动画基础）
2. **`AnimatedVisibility`**（最常用 UI 动画，需布局层配合）
3. **`keyframes` + `repeatable`**（spec 生态补全）
4. **`Crossfade` / `animateContentSize`**
5. **更多值类型**（Dp/Offset/Size）
