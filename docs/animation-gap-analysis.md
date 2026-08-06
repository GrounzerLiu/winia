# 动画系统差距分析（对标 Jetpack Compose）

> 分支：`animation-system`（基于 text-field 02ce48c）
> 方法：官方文档（developer.android.com/develop/ui/compose/animation/value-based 等）+ 本地代码盘点
> 目的：完整对照 Compose 动画 API 面，给出实现/优化清单

---

## 一、Compose 动画系统全景（按官方文档层次）

### 1. 高层值动画（animate\*AsState 族）
| API | 说明 |
|-----|------|
| `animateFloatAsState` | float 目标变化 → 自动平滑过渡 |
| `animateDpAsState` | Dp 值（尺寸/间距） |
| `animateColorAsState` | 颜色（含 label/finishedListener） |
| `animateIntAsState` | Int 值 |
| `animateOffsetAsState` | Offset |
| `animateSizeAsState` | Size |
| `animateRectAsState` | Rect |
| `animateBoundsAsState` | 布局边界 |
| `animateValueAsState` | 泛型（需 TwoWayConverter） |

特性：`label`（调试）、`finishedListener`（1.4+ 完成回调）。

### 2. 容器/可见性动画
| API | 说明 |
|-----|------|
| `AnimatedVisibility` | visible 切换 → enter/exit 过渡 |
| — enter/exit 过渡 | `fadeIn/fadeOut`、`slideInHorizontally/slideOutHorizontally`、`slideInVertically`、`expandVertically/shrinkVertically`、`scaleIn/scaleOut`、`togetherWith` 组合 |
| `AnimatedContent` | targetState 切换（内容交叉过渡 + sizeTransform） |
| `Crossfade` | 两内容交叉淡入淡出（简单版 AnimatedContent） |
| Scope | `AnimatedVisibilityScope`/`AnimatedContentScope`（内部读动画进度） |

### 3. 过渡（updateTransition）
| API | 说明 |
|-----|------|
| `updateTransition(targetState, label)` | 一个状态 → 多值联动动画 |
| `transition.animateFloat/animateColor/animateDp/animateSize/animateOffset/animateRect/animateValue` | 每个值的映射（targetState → 值） |
| `transition.createChildTransition` | 子过渡（嵌套） |
| `Transition.AnimatedVisibility/AnimatedContent` | 过渡驱动的可见性/内容动画（扩展） |

### 4. 无限动画
| API | 说明 |
|-----|------|
| `rememberInfiniteTransition(label)` | 无限过渡容器 |
| `animateFloat/animateColor/animateValue` | 子动画（initialValue + targetValue + infiniteRepeatable） |
| `infiniteRepeatable(animation, repeatMode)` | RepeatMode.Restart / Reverse |

### 5. 动画规格 AnimationSpec
| API | 说明 |
|-----|------|
| `tween(duration, delay, easing)` | 补间（easing：Linear/FastOutSlowIn/LinearOutSlowIn/FastOutLinearIn/Standard 系列常量） |
| `spring(dampingRatio, stiffness, visibilityThreshold)` | 物理弹簧（常量：DampingRatio.NoBouncy/MediumBouncy、Stiffness.Low/Medium/High） |
| `keyframes { }` | 关键帧 DSL（时间点 + easing） |
| `repeatable(iterations, animation, repeatMode)` | 有限重复 |
| `infiniteRepeatable` | 无限重复 |
| `snap(delay)` | 瞬时跳变 |
| 自定义 | `AnimationVector` + `TwoWayConverter` + `cubicBezier(...)`/`PathEasing` |

### 6. 低层 API（coroutine 基础）
| API | 说明 |
|-----|------|
| `Animatable` | 值持有 + `animateTo`（suspend）/`snapTo`/`animateDecay`；被打断时**从当前值+当前速度延续**；可组合外创建 |
| `AnimationState`/`TargetBasedAnimation` | 手动控制播放时间 |
| `DecayAnimation` + `exponentialDecay/splineBasedDecay` | fling 减速 |
| `withFrameNanos`/`withInfiniteAnimationFrameNanos` | 帧时钟 |

### 7. 修饰符/布局动画
| API | 说明 |
|-----|------|
| `Modifier.animateContentSize` | 尺寸变化自动动画 |
| `Modifier.animateItem`/`animateItemPlacement` | 列表项增删/移动（LazyList） |
| `Modifier.graphicsLayer { }` | scale/alpha/rotation/translation/shadow/clip/shape/blur（GraphicsLayerScope 完整属性） |
| `Modifier.drawBehind/drawWithContent` | 自定义绘制动画 |

### 8. 手势结合
- `anchoredDraggable` + `Animatable`（fling/settle 联动）
- `animateScrollAsState`（滚动动画）

---

## 二、本项目现状（代码盘点）

### 已有 ✅

**低层（帧驱动，非 coroutine）**：
- `push_animatable` / `push_animatable_color` / `push_infinite` + `update_animations()`（渲染循环同步推进）
- `remove_animation_by_state` / `has_animation_for_state` / `is_animating`（去重/生命周期）

**中层**：
- `Animatable<T>`：`animate_to`/`snap_to`/`update`——**无 animateDecay、无速度延续**（被打断从当前值重新开始，无 velocity）
- `Transition<T>`：`update_transition` + `animate_float`（**只有 float**）
- `InfiniteTransition`：`animate_float`/`animate_color`/`dispose`（**生命周期自动**——P3-2 已解决无限动画泄漏）

**高层（composer.rs:324-370）**：
- `animate_float_as_state` / `animate_color_as_state` / `animate_dp_as_state` / `animate_offset_as_state` / `animate_size_as_state`（5 个——**文档 §七 已过时，实际比文档多**）

**规格**：
- `AnimationSpec { Spring/Tween/Keyframes/Repeatable/Snap }` + `SpringSpec/TweenSpec/KeyframesSpec/RepeatableSpec/InfiniteRepeatableSpec`

**值类型**（AnimatableValue trait：lerp/to_f32/from_f32/supports_spring）：
- `f32`、`Color`（CAM16-UCS 感知均匀插值 + alpha 线性）、`Dp`/`Sp`/`Offset`/`Size`（unit.rs:201-223）

**插值器**：31 种（`Linear` + `EaseIn/Out/InOut` × Sine/Quad/Cubic/Quart/Quint/Expo/Circ/Back/Elastic/Bounce = 30）——**超过 Compose 内置常量**（Compose 仅 ~10 个 + 自定义 bezier）

**渲染/布局集成**：
- `graphics_layer`（scale_x/y、alpha、translation_x/y、rotation_z——**绘制层，不重排**，对标 GraphicsLayerScope 子集）
- **布局层动画**：P2-1 两段式依赖（`layout_deps`/`layout_dirty`）——布局属性动画写 `get()` 即生效，**无旁路**（这是 Compose 没有的直接对应物——Compose 靠 animateContentSize + Layout 动画）

### 缺失 ❌ / 部分 ⚠️

| 类别 | 项 | 状态 |
|------|-----|------|
| 高层值动画 | `animate_int_as_state` / `animate_value_as_state` / `animate_rect_as_state` / `animate_bounds_as_state` | ❌ 缺（前两个低成本） |
| 高层值动画 | `label` / `finished_listener` 参数 | ❌ 缺 |
| 容器动画 | **`AnimatedVisibility`**（enter/exit 过渡全集） | ❌ 缺（主线无——旧实验在 animation-improve 已放弃，布局层正路可做） |
| 容器动画 | `AnimatedContent` / `Crossfade` | ❌ 缺 |
| 过渡 | `Transition.animate_color/dp/size/offset/value` + `create_child_transition` + `label` | ⚠️ 只有 animate_float |
| 无限动画 | `animate_value`（泛型） | ❌ 缺（有 float/color） |
| 规格 | `cubic_bezier`/`PathEasing` 自定义 easing | ❌ 缺（插值器表是静态的） |
| 规格 | `spring` 常量（NoBouncy/MediumBouncy/Low/Medium/High） | ❌ 缺（直接数值） |
| 低层 | `animate_decay` / `DecayAnimationSpec`（fling） | ❌ 缺 |
| 低层 | `Animatable` 速度延续（打断后从当前速度继续） | ❌ 缺（Compose 语义） |
| 低层 | `with_frame_nanos` 帧时钟 API | ❌ 缺（隐藏在内） |
| 修饰符 | `animate_content_size` | ❌ 缺（布局层机制可做） |
| 修饰符 | `animate_item`（列表增删移动） | ❌ 缺（无 LazyList） |
| graphics_layer | shadow/clip/shape/blur | ❌ 缺（仅 6 个变换属性） |
| 手势 | fling/settle 联动 | ❌ 缺 |

### 已知限制（文档 §八，待修）⚠️
1. Color Spring 降级 Tween（supports_spring=false——`to_f32` 非单射，设计如此，需显式文档/警告）
2. `dt` 超 166ms 截断（掉帧后弹簧丢时间）
3. dedup 忽略 spec（同状态重复 push 时 spec 不更新）
4. 无 60fps 节流（动画每帧全量推进）
5. Repeatable 仅 Tween base（其他 spec 不能 repeat）
6. Keyframes/Tween `duration=0` 除零 NaN（5s 兜底）
7. 向量类型（Offset/Size）`to_f32` 返回范数（Spring 降级，非单射）

---

## 三、实现/优化清单（按优先级）

### P0 — 常用体验（先做）
1. **`AnimatedVisibility`**：enter/exit 过渡（fade/slide/expand/shrink + togetherWith 组合）——**布局层动画正路**（layout_deps 测量期读动画值，无 force_remeasure 旁路）；退出动画期间子树保留（exit 完成才移除）
2. **`Transition` 补全**：`animate_color`/`animate_dp`/`animate_size`/`animate_offset`/`animate_value` + `label`
3. **`animate_int_as_state` / `animate_value_as_state`**（泛型 AnimatableValue——低成本，机制已有）

### P1 — 常见需求
4. **`Crossfade`**（两内容交叉淡入淡出——简单版 AnimatedContent）✅ `7d85252`
   - 实现为**顺序淡入淡出**（非交叉）：组合引擎单内容世代（无 Compose 双世代
     outgoing 组合）——旧内容淡出完成才切换新内容淡入；`Crossfade::new(target).build(ctx, |ctx, t| ...)`
5. **`animate_content_size`**（尺寸变化自动动画）✅ `7d85252`
   - 实现为**容器组件 `AnimatedSize`**（非 Modifier）：本框架 Modifier 是纯数据
     （构建期无组合上下文）无法内嵌 remember——容器组件在组合期创建 State（机制等价）
   - 注意：target 状态必须用 State 跨重组保留（policy 实例每次 build 重建）
6. **`finished_listener` / `label`** ✅ 部分 `7d85252`
   - `Animatable::on_finish` + `push_animatable_with_done`（完成回调）已实现
   - animate\*AsState 的 `label` 参数跳过（Transition 已有 label；调试价值低）
7. **spring 常量**（`SpringSpec::DAMPING_RATIO_*` / `STIFFNESS_*`）✅ `7d85252`

### P2 — 进阶
8. **`animate_decay` + fling**（DecayAnimationSpec：exponential_decay——拖拽/惯性滚动配合）✅ 本分支
   - `AnimationSpec::Decay(DecaySpec)` + `Animatable::animate_decay(v0, spec)` + `push_decay(state, v0, spec)` + `exponential_decay(friction)`（prelude 导出）
   - 解析式 `value(t) = from + v0/friction·(1-e^(-friction·t))`；速度 `< threshold` 时写极限值精确停靠
   - ⚠️ 仅标量语义（to_f32/from_f32 对向量类型退化为范数方向）
   - 测试：`decay_moves_and_stops_at_limit` / `decay_zero_velocity_done_immediately`；demo：`decay_demo`（fling 方块）
9. **`Animatable` 速度延续**（被打断时从当前速度继续——Compose 核心语义，当前从 0 速度重启）
   - 铺垫：`AnimationState.initial_velocity` 字段已加（Decay v0）；Decay 分支每帧更新 `last_velocity` 供打断继承
10. **`Repeatable` 支持任意 base spec**（当前仅 Tween）
11. **easing 自定义**（`cubic_bezier(p1x,p1y,p2x,p2y)`/`path_easing`——插值器表外运行时曲线）

### P3 — 基础设施/远期
12. **帧时钟 API**（`with_frame_nanos` 类——LaunchedEffect/自定义动画的帧驱动入口）
13. **graphics_layer 补属性**（shadow/clip/shape/blur——渲染层能力）
14. **`animate_item`**（列表增删/移动动画——需先有 LazyList 或简单列表容器）
15. **`AnimatedContent`**（targetState 切换 + sizeTransform）

### 缺陷修复（随上述实施顺带）
- dedup 忽略 spec（P0-2 时修）
- dt 166ms 截断（P2-9 速度延续时修）
- Keyframes duration=0 除零（P0-2 时修）
- 60fps 节流（帧驱动层）
- Color Spring 降级显式化（supports_spring 文档 + 运行时警告）

---

## 四、设计要点（实施时遵循）

1. **动画不触发重组**：动画值写入 State 用 `set_no_wake`/`peek` 读取（渲染期/测量期读），组件闭包不因动画值重跑——现有机制已验证（P2-1）
2. **布局层 vs 绘制层**：尺寸/位置动画走布局层（layout_deps——测量期读动画值，重测不重组）；视觉属性（alpha/scale/rotation/颜色）走 graphics_layer/背景闭包（绘制期读）
3. **AnimatedVisibility 的退出语义**：visible=false → 触发 exit 动画 → 动画完成才从组合树移除（对应 Compose 的 ExitTransition 完成后离开组合）——需要"延迟移除"机制（组合树保留 exit 期间）
4. **速度延续**：Animatable 内部保存上一帧 velocity，新目标从 (current, velocity) 启动（Spring 物理天然支持——compute_spring_displacement 已有 vel 参数）
