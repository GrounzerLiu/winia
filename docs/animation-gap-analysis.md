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

### Gaps vs Compose (synced v2 2026-09 — was stale; §三 is authoritative)

| Category | Item | Status |
|------|-----|------|
| High-level value anim | `animate_int_as_state` / `animate_value_as_state` | ✅ done (P0-3) |
| High-level value anim | `animate_rect_as_state` / `animate_bounds_as_state` | ⏸️ skipped by design (no `Rect` unit type exists; bounds animate via `Offset`+`Size`, both animatable — add when a consumer needs it) |
| High-level value anim | `label` / `finished_listener` params | ⚠️ partial (`on_finish` + `push_animatable_with_done` done; `label` skipped — Transition already has it) |
| Container anim | **`AnimatedVisibility`** enter/exit set | ✅ done (params aligned + horizontal expand, P0-1) |
| Container anim | `AnimatedContent` / `Crossfade` | ✅ done (single content generation — engine limit, documented) |
| Transition | `animate_color/dp/size/offset/value` + `create_child_transition` + `label` | ✅ done (P0-2) |
| Infinite anim | `animate_value` (generic) | ❌ missing (float/color only) |
| Spec | `cubic_bezier`/`PathEasing` custom easing | ✅ done (P2-11) |
| Spec | `spring` constants (NoBouncy/MediumBouncy/Low/Medium/High) | ✅ done (P1-7) |
| Low-level | `animate_decay` / DecayAnimationSpec | ✅ done (P2-8) |
| Low-level | `Animatable` velocity continuation | ✅ done (P2-9) |
| Low-level | `with_frame_nanos` frame clock | ✅ done (P3-12) |
| Modifier | `animate_content_size` | ✅ done as `AnimatedSize` container (P1-5) |
| Modifier | `animate_item` (list add/remove/move) | ❌ missing (attempted `exp/animate-item`, dropped — measure-lerp feel was wrong; needs lookahead or layer-shift approach) |
| graphics_layer | shadow/clip/shape/blur | ✅ base wired (Skia ShadowUtils; advanced Compose parity open) |
| Gestures | fling/settle linkage | ❌ missing |

### Known limitations (open ⚠️)
1. Color Spring downgrades to Tween (supports_spring=false — `to_f32` non-injective, by design, documented + runtime warning).
2. `dt` over 166ms truncated (spring drops time after frame loss).
3. dedup ignores spec on same-state re-push (matches Compose: unchanged target never restarts) — kept.
4. No 60fps throttle (every frame pushes all animations).
5. Repeatable: Spring/Decay/nested-Repeatable rejected by assert (no cycle-length semantics).
6. Vector types (Offset/Size) `to_f32` returns norm (Spring downgrades, non-injective).
7. ExpandFromH Start/End never mirror for RTL (Compose parity backlog).
8. Transition child label inherits parent's (labels are `&'static str`; Compose synthesizes "child of …").

---

## 三、实现/优化清单（按优先级）

### P0 — 常用体验（先做）
1. **`AnimatedVisibility`** ✅ parameters aligned + horizontal expand
   (`exp/animated-visibility`, v2 2026-09): SlideOffset Fixed(48 default)/Fraction
   (Compose initialOffsetX), ExpandFrom Top/Bottom + ExpandFromH Start/End anchors
   (no RTL mirroring, backlog), scale_from/transform_origin (Compose scaleIn),
   expand_in_h/shrink_out_h; clip-to-bounds when expanding (unclipped overflow read
   as flash), anchors latched at animation start, clip/expand follow active direction.
2. **`Transition` 补全** ✅ `exp/transition-complete` (v2 2026-09):
   `animate_float/color/dp/size/offset` + generic `animate<U>` + named
   `animate_value<U>` wrapper + `create_child_transition(map)` (child target derived
   from parent target each call, same spec, label inherited — labels are `&'static str`);
   `label` was already present on `update_transition`/each animate. Tests:
   `transition_animate_value_named_wrapper` / `transition_child_follows_parent_target`
   (parent 1→2 flip, child converges 100→200).
3. **`animate_int_as_state` / `animate_value_as_state`**（泛型 AnimatableValue——低成本，机制已有）✅ 本分支
   - `animate_int_as_state(ctx, target, spec) -> State<i32>`（对标 Compose animateIntAsState——remember 保存动画 State + 每次调用比较 target，变化即 push_animatable；i32 lerp 四舍五入逐级跳变）
   - `animate_value_as_state<T: AnimatableValue>(ctx, target, spec) -> State<T>`（泛型——f32/i32/Color 等；向量类型 Spring 自动降级）
   - 测试：`animate_int_as_state_animates_to_target`（单调递增 + 精确到达）/ `animate_value_as_state_color_reaches_target`；demo：animation_demo §9

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
9. **`Animatable` 速度延续**（被打断时从当前速度继续——Compose 核心语义）✅ 本分支
   - `animate_to` 继承自身 `last_velocity`；`push_animatable` retarget 从被移除的旧动画继承（`AnimationInstance::last_velocity()` trait 方法）
   - Spring/Decay 每帧更新速度（Decay 解析式算瞬时速度）；Tween/Keyframes 无速度语义恒 0（从静止重启）
   - 测试：`retarget_inherits_velocity`（实例级）/ `push_retarget_inherits_velocity`（push 路径）
10. **`Repeatable` 支持任意 base spec** ✅ 本分支
    - 支持 Tween/Keyframes（有明确时长——周期内走 base 曲线，Reverse 时曲线倒放）；Snap（total=0 首次即完成）；Spring/Decay/嵌套 Repeatable 断言拒绝（无循环长度语义）
    - 测试：`repeatable_keyframes_base`（3×50ms 在 ~10 帧内完成，修复前回退 300ms 线性）/ `repeatable_snap_base`
11. **easing 自定义** ✅ 本分支
    - `CubicBezier::new(p1x,p1y,p2x,p2y)`（对标 Compose CubicBezierEasing——牛顿 + 二分求解，epsilon 1e-6）+ `PathEasing::new(points)`（对标 PathEasing——段间线性，复用 find_interval）
    - 均可 `Into<Arc<dyn Interpolator>>` → `TweenSpec::new(dur, ...)` 直接接线
    - 测试：5 个（端点/对称/单调/clamp/单点退化/接线）；interpolator_demo 扩至 32 行

### P3 — 基础设施/远期
12. **帧时钟 API**（`with_frame_nanos` 类）✅ 本分支
    - `winia::effect::with_frame_nanos()`（对标 Compose withFrameNanos——broadcast 帧时钟，渲染循环 `frame_tick()` 每帧广播纳秒时间戳；订阅者收到**下一帧**）
    - LaunchedEffect/CoroutineScope 内 `loop { let t = with_frame_nanos().await; ... }` 驱动自定义动画（dt 自算）
    - 测试：`frame_clock_ticks_and_waits`；prelude 导出
13. **graphics_layer 补属性**（shadow/clip/shape/blur——渲染层能力）
14. **`animate_item`**（列表增删/移动动画——需先有 LazyList 或简单列表容器）
15. **`AnimatedContent`** ✅ 本分支
    - `AnimatedContent<T>::new(target)` + `.animation(spec)`（fade）+ `.size_animation(spec)`（sizeTransform，默认 Spring bouncy）
    - 机制：target 变 → 旧内容淡出 → 淡出完成（progress<0.001）→ 锁定旧尺寸 → current.set → 内容重建 → 淡入；**容器尺寸 = lerp(prev_size, 新内容尺寸, progress)**（布局层 layout_dep 每帧重测）；alpha 绘制层 peek（零重组）
    - 单内容世代（无 Compose 双世代 outgoing——组合引擎限制，文档注明）
    - 测试：`animated_content_switches_with_size_transform`；demo：`animated_content_demo`

### 缺陷修复（随上述实施顺带）
- dedup 忽略 spec（P0-2 时修）
- dt 166ms 截断（P2-9 速度延续时修）
- Keyframes duration=0 除零（P0-2 时修）
- 60fps 节流（帧驱动层）
- duration=0 Tween/Keyframes 立即完成（显式 is_zero 分支 + 测试）
- dedup 同 target 不更新 spec：**与 Compose 语义一致**（animate*AsState target 未变不重启）——保留
- 同帧二次 compose 物化：8548718 守卫移除——改为现有树重建 prev 索引（Skip 复用保留子内容 + Enter 正常更新；AnimatedContent 切换帧的 B 内容不再被丢弃）
- Color Spring 降级显式化（supports_spring 文档 + 运行时警告）

---

## 四、设计要点（实施时遵循）

1. **动画不触发重组**：动画值写入 State 用 `set_no_wake`/`peek` 读取（渲染期/测量期读），组件闭包不因动画值重跑——现有机制已验证（P2-1）
2. **布局层 vs 绘制层**：尺寸/位置动画走布局层（layout_deps——测量期读动画值，重测不重组）；视觉属性（alpha/scale/rotation/颜色）走 graphics_layer/背景闭包（绘制期读）
3. **AnimatedVisibility 的退出语义**：visible=false → 触发 exit 动画 → 动画完成才从组合树移除（对应 Compose 的 ExitTransition 完成后离开组合）——需要"延迟移除"机制（组合树保留 exit 期间）
4. **速度延续**：Animatable 内部保存上一帧 velocity，新目标从 (current, velocity) 启动（Spring 物理天然支持——compute_spring_displacement 已有 vel 参数）
