# 动画系统 — 实现与进度

> 更新日期：2026-07-31 · 分支：text-field

## 一、总体架构

对标 Jetpack Compose 三层动画 API，基于本项目 `State<T>` + 事件循环驱动。

```
用户 API 层
  animate_float_as_state / animate_color_as_state / updateTransition
  remember_infinite_transition
           │
Animatable<T> 引擎（底层）
  animate_to(target, spec) / update() / snap_to()
           │
AnimationSpec（动画规格）
  Spring / Tween / InfiniteRepeatableSpec
           │
帧驱动（app.rs new_events）
  update_animations() → 动画 update → State.set → notify → 重组渲染
```

## 二、已实现 ✅

### 1. 值类型（AnimatableValue trait）
- `f32` — 线性插值
- `Color` — **CAM16-UCS 色彩空间插值**（`material-colors::blend::cam16_ucs`，人眼感知均匀）+ **alpha 单独线性插值**（cam16_ucs 忽略 alpha）

### 2. 动画规格（AnimationSpec）
- **Spring**：物理弹簧，半隐式欧拉积分，固定 1/60s 子步 + accumulator（最多 10 步）
  - `SpringSpec::default()`：damping=1.0（临界）、stiffness=1500、threshold=0.01
  - `SpringSpec::bouncy()`：damping=0.6、threshold=0.1（约 300ms，带超调回弹）
- **Tween**：补间，`duration` + `interpolator`（函数指针）
- **InfiniteRepeatableSpec**：`restart(duration)` / `reverse(duration)`

### 3. 插值器（interpolator.rs）
24 种预计算 bezier 插值器：`Linear`, `EaseIn/Out/InOut` (Sine, Quad, Cubic, Quart, Quint, Expo, Circ, Back, Elastic, Bounce)

### 4. 高层 API（ComposeCtx 方法）
- `animate_float_as_state(target, spec) -> State<f32>` — 对标 animateFloatAsState
- `animate_color_as_state(target, spec) -> State<Color>` — 对标 animateColorAsState（Spring 自动降级 Tween）

### 5. 中层 API
- `update_transition(target, spec, label) -> Transition` — 对标 updateTransition
  - `transition.animate_float(ctx, target_fn, label) -> State<f32>`
- `remember_infinite_transition() -> InfiniteTransition` — 对标 rememberInfiniteTransition
  - `animate_float(ctx, from, to, spec) -> State<f32>` — 无限循环
  - `animate_color(ctx, from, to, spec) -> State<Color>` — 无限循环颜色
  - `dispose()` — 取消该作用域所有动画

### 6. 底层 API
- `Animatable<T>` — `new / animate_to / update / snap_to`
- 超 5s 未收敛强制 snap（防 stiffness=0 无限 busy-loop）

### 7. 渲染支持
- `Modifier::graphics_layer(GraphicsLayerParams)` — 对标 Compose graphicsLayer
  - `scale_x/y, alpha, translation_x/y, rotation_z`
  - **应用到整个节点**（background + text + children）：`render_pass1` 开头 save，结尾 restore
  - alpha 用 `canvas.save_layer_alpha_f(None, alpha)`

### 8. 帧驱动
- `update_animations()` / `is_animating()` — 三个全局列表（f32 / Color / 无限 Color）
- `app.rs new_events`：每轮事件批次推进动画 → 请求所有窗口 redraw
- 动画注册后首次 update → notify → wake_up → 下一轮 new_events 自续
- `ControlFlow::Wait` + `request_redraw` 自驱动（无 Poll/Wait 切换竞态）

## 三、关键机制

### Spring 物理
```
半隐式欧拉积分 + 固定步长 FIXED_DT=1/60 + accumulator（≤10 子步）
位移持续累积（current_displacement 存于 AnimationState）
收敛：|displacement| < threshold && |velocity| < threshold
```

### 动画去重（跨列表统一）
```
1. state.peek() == target → 跳过（已到位）
2. has_animation_for_state(sid) → 跳过（跨三列表，防双倍推进）
3. 同 state 目标不同 → 移除旧动画，创建新动画
4. 锁内只检查/收集，anim.update() 锁外执行（防死锁）
```

### 锁安全
- `push_*` / `update_animations` 持有 Mutex 时**不执行用户代码**
- `update_animations`：take → 锁外 update → 锁内按 state_id 去重 extend（新 push 优先）

### 依赖精确化
- `State::peek()` — 读取不注册依赖（动画引擎内部用）
- 动画 State 的依赖只来自真正读取它的地方 → 增量重组精确到读取 slot

### 无限动画生命周期
- `InfiniteTransition` 持有 `Arc<Mutex<Vec<u32>>>`（state_id 列表）
- `dispose()` 显式清理 + `Drop` 兜底自动清理

## 四、Demo（animation_demo.rs）

| 节 | 内容 | 动画 |
|----|------|------|
| 1 | 蓝色盒子宽度 | Spring Bouncy（50↔300px） |
| 2 | 绿色盒子透明度/宽度 | Tween 300ms |
| 3 | 橙色盒子 offset | updateTransition (Spring) |
| 4 | 颜色盒子 | animate_color_as_state（CAM16 绿↔紫 500ms） |
| 5 | 呼吸圆点 | rememberInfiniteTransition（alpha + 蓝↔红 CAM16 往返） |

## 五、单元测试（10 个）

- `spring_converges_to_target` / `spring_bouncy_overshoots_then_converges`
- `spring_reverse_animation` / `spring_dt_zero_is_safe`
- `tween_completes_within_duration`
- `infinite_float_restart_loops` / `infinite_float_reverse_oscillates`
- `infinite_color_reverse_cycles`
- `color_lerp_uses_cam16_and_preserves_alpha`
- `remove_animation_by_state_cleans_lists`

## 六、缺失（对标 Compose）❌

### 高层
| API | 用途 | 优先级 |
|-----|------|--------|
| `AnimatedVisibility` | 内容出现/消失（fade/expand/shrink） | 高 |
| `Crossfade` | 两内容交叉淡入淡出 | 中 |
| `animateContentSize` | 尺寸变化自动动画 | 中 |
| `animateDpAsState` / `animateSizeAsState` / `animateOffsetAsState` | 其他值类型 | 中 |

### 中层
- `updateTransition` 完整版（animate_dp/animate_color/animate_size）

### 底层
- `keyframes` / `repeatable` / `snap` 规格
- `Dp` / `Offset` / `Size` 值类型（需实现 AnimatableValue + lerp）

## 七、已知限制

1. **Color Spring 降级**：颜色无单一 f32 值，Spring 自动降级 Tween
2. **dt 超 166ms 截断**：掉帧后弹簧丢时间
3. **dedup 忽略 spec**：同 state+同 target 不同 spec 的重启请求被忽略
4. **动画期间无 60fps 节流**：等效 Poll，CPU 占用较高（动画时长有限，可接受）
5. **`update_animations` 三列表重复代码**：可提取统一 trait 整合

## 八、下一步计划

1. **`AnimatedVisibility`**（最常用 UI 动画，fade + 尺寸过渡，需布局层配合）
2. **`keyframes` + `repeatable`**（spec 生态补全，低成本）
3. **`Crossfade`**（两内容切换）
4. **更多值类型**（Dp/Offset/Size → animateDpAsState 等）
