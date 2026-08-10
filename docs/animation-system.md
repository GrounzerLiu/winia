# 动画系统 — 实现与进度

> ⚠ 部分内容过时（"缺失"清单与"下一步计划"已完成）。实现细节以代码为准；最新差距分析见 `docs/animation-gap-analysis.md`。
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
  Spring / Tween / Keyframes / Repeatable / Snap / InfiniteRepeatableSpec
           │
帧驱动（app.rs new_events）
  update_animations() → 动画 update → State.set → notify → 重组渲染
```

## 二、已实现 ✅

### 1. 值类型（AnimatableValue trait）
- `f32` — 线性插值
- `Color` — CAM16-UCS 色彩空间插值（`material-colors::blend::cam16_ucs`）+ alpha 单独线性插值
- `Dp` / `Sp` / `Offset` / `Size` — unit 模块类型（unit.rs）

### 2. 动画规格（AnimationSpec）
| Spec | 说明 |
|------|------|
| `Spring` | 物理弹簧，半隐式欧拉积分，固定 1/60s 子步 + accumulator（≤10 步） |
| `Tween` | 补间，duration + interpolator |
| `Keyframes` | 关键帧序列 `(progress, value, interpolator)`，段内插值 |
| `Repeatable` | 重复 N 次（base 目前仅 Tween），Restart/Reverse 往返 |
| `Snap` | 瞬时跳转 |
| `InfiniteRepeatableSpec` | `restart(duration)` / `reverse(duration)` 无限循环 |

- `SpringSpec::default()`：damping=1.0、stiffness=1500、threshold=0.01
- `SpringSpec::bouncy()`：damping=0.6、threshold=0.1

### 3. 插值器（interpolator.rs）
24 种预计算 bezier 插值器：`Linear`, `EaseIn/Out/InOut` (Sine, Quad, Cubic, Quart, Quint, Expo, Circ, Back, Elastic, Bounce)

### 4. 高层 API（ComposeCtx 方法）
- `animate_float_as_state(target, spec) -> State<f32>` — 对标 animateFloatAsState
- `animate_color_as_state(target, spec) -> State<Color>` — 对标 animateColorAsState（Spring 自动降级 Tween）

### 5. 中层 API
- `update_transition(target, spec, label) -> Transition` — 对标 updateTransition
  - `transition.animate_float(ctx, target_fn, label) -> State<f32>`
- `remember_infinite_transition() -> InfiniteTransition` — 对标 rememberInfiniteTransition
  - `animate_float` / `animate_color`
  - `dispose()` — **显式**取消该作用域动画（无 Drop 兜底，见已知限制）

### 6. 底层 API
- `Animatable<T>` — `new / animate_to / update / snap_to`
- 超 5s 未收敛强制 snap（防 stiffness=0 无限 busy-loop）

### 7. 渲染支持（绘制层动画）

**统一入口**：`background` / `graphics_layer` 各只有一个方法，静态值与动画闭包自动适配
（`impl Into<BackgroundColor>` / `impl Into<GraphicsLayerSpec>`）：

```rust
// 静态（构建时固定）
.background(Color::RED, Shape::Circle)
.graphics_layer(GraphicsLayerParams { alpha: 0.5, ..Default::default() })

// 动画（渲染时每帧求值，零重组）——闭包内用 State::peek()
.background(|| pulse.peek(), Shape::Circle)
.graphics_layer(|| GraphicsLayerParams { alpha: pulse.peek(), ..Default::default() })
```

- `graphics_layer` 应用到整个节点（background + text + children）
- alpha 用 `canvas.save_layer_alpha_f(None, alpha)`

**⚠️ 使用原则**：
- 静态绘制属性 → 传值（`Color` / `GraphicsLayerParams`）
- 需要动画（值随帧变化）→ 传闭包，闭包内必须用 **`State::peek()`**（不注册依赖）
- **不要**在闭包里用 `State::get()`——会注册依赖触发整树重组，破坏零重组设计

### 7.1 绘制层 vs 布局层（Compose 分层模型）

| 属性 | 动画方式 | 触发 | 对应 API |
|------|---------|------|---------|
| 布局属性（size/offset/weight） | 值变→重组+重测 | `State::set()` → notify | `animate_float_as_state` 等 |
| 绘制层属性（alpha/scale/颜色/位移） | 值变→只重绘 | `State::set_visual()`（不 notify）+ `request_redraw` | `graphics_layer(闭包)` / `background(闭包)` |

绘制层动画链路：
```
动画引擎 update → state.set_visual(新值)   // 写值，不 notify → 零重组
→ 动画引擎 request_redraw()               // 请求重绘
→ 渲染 → background/graphics_layer 闭包执行 → peek() 读到最新值 → 绘制
```

### 8. 帧驱动
- `update_animations()` / `is_animating()` — 三个全局列表（f32 / Color / 无限 Color）
- `app.rs new_events`：每轮推进动画 → 请求所有窗口 redraw
- `ControlFlow::Wait` + `request_redraw` 自驱动

## 三、关键机制

### Spring 物理
半隐式欧拉积分 + 固定步长 1/60 + accumulator（≤10 子步），位移持续累积。

### 动画去重（跨列表统一）
```
1. state.peek() == target → 跳过
2. has_animation_for_state(sid) → 跳过（跨三列表）
3. 同 state 目标不同 → 移除旧动画，创建新动画
4. 锁内只检查/收集，anim.update() 锁外执行
```

### 依赖精确化
- `State::peek()` — 读取不注册依赖
- 动画 State 依赖只来自真正读取处 → 增量重组精确到读取 slot

### 无限动画生命周期
- `InfiniteTransition` 持有 `Arc<Mutex<Vec<u32>>>`（remember 持久化）
- 显式 `dispose()` 清理（无 Drop 兜底）

## 四、Unit 系统（unit.rs，对标 compose.ui.unit）

| 类型 | 说明 |
|------|------|
| `Dp` / `Sp` / `Px` | 强类型单位 + `.dp()` / `.sp()` / `.px()` 扩展（f32/i32/u32） |
| `TextUnit` | Sp/Px 统一（f32 默认 Sp），`to_logical_px()` |
| `Offset` / `Size` / `IntOffset` / `IntSize` | 2D 向量类型 |
| `Density` | density + font_scale（font_scale 恒 1.0，未接入系统缩放） |
| `LOCAL_DENSITY` | CompositionLocal，覆盖 compose+layout+draw 全程（含 open_window 首次） |

- `Dimension` 扩展 Dp/Px 变体 → `.size(10.dp(), 20.px())`
- `Text`/`RichText`/`TextField` 字号支持 `.sp()` / `.px()`

## 五、Demo（animation_demo.rs）

| 节 | 内容 | 动画 |
|----|------|------|
| 1 | 蓝色盒子宽度 | Spring Bouncy |
| 2 | 绿色盒子透明度/宽度 | Tween |
| 3 | 橙色盒子 offset | updateTransition |
| 4 | 颜色盒子 | animate_color_as_state（CAM16） |
| 5 | 呼吸圆点 | rememberInfiniteTransition |
| 6 | 青色盒子 | Keyframes（400ms 超调） |

另有 unit_demo.rs：Dp/Px/Offset/Size/Density/TextUnit 展示。

## 六、单元测试（15 个动画 + 7 个 unit）

动画：spring 收敛/超调/反向/dt=0、tween、keyframes 段插值/首帧边界、
repeatable 重复/Reverse 偶数次、snap、infinite float/color、color lerp、
remove_animation_by_state、keyframes/repeatable/snap。
unit：dp/sp 转换、算术、扩展语法、offset/size 运算、AnimatableValue。

## 七、缺失（对标 Compose）❌

### 高层
| API | 用途 | 优先级 |
|-----|------|--------|
| `AnimatedVisibility` | 内容出现/消失（fade/expand/shrink） | 高 |
| `animateDpAsState` / `animateSizeAsState` / `animateOffsetAsState` | unit 类型高层 API | 中（低投入） |
| `Crossfade` | 两内容交叉淡入淡出 | 中 |
| `animateContentSize` | 尺寸变化自动动画 | 中 |

### 中层
- `updateTransition` 完整版（animate_dp/animate_color/animate_size）

## 八、已知限制

1. **Color Spring 降级**：颜色无单一 f32 值，Spring 自动降级 Tween
2. **dt 超 166ms 截断**：掉帧后弹簧丢时间
3. **dedup 忽略 spec**：同 state+同 target 不同 spec 的重启请求被忽略
4. **动画期间无 60fps 节流**：等效 Poll，CPU 占用较高
5. **无限动画无 Drop 兜底**：`dispose()` 需显式调用（每次 compose drop 局部变量会误清，故不能 Drop）
6. **Repeatable 仅支持 Tween base**：其他类型 debug_assert 警告 + 回退 300ms 线性
7. **Keyframes/Tween duration=0**：除零 NaN，靠 5s 兜底完成

## 九、下一步计划

1. `animateDpAsState` / `animateOffsetAsState` / `animateSizeAsState`（unit 高层 API，低成本）
2. `AnimatedVisibility`（最常用，需布局层配合）
3. `Crossfade` / `animateContentSize`
4. `updateTransition` 完整版
