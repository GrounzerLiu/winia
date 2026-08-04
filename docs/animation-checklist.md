# 动画系统完善 Check List

> 用途：给后来者或压缩上下文后的实现者——照此逐步完善动画系统。
> 分支：`animation-improve`（基于 `text-field` @ `4edd48d`）
> 目标：对齐 Jetpack Compose 动画 API 体系（值动画 / 内容动画 / 无限动画 / 多属性协同 / 低层控制）
> 现状（2026-08 评估）：三层 API 已有骨架（animate_*_as_state / update_transition / Animatable）+ 无限动画 + 31 种插值器（已全部接入 Tween/Keyframes）+ graphics_layer 绘制层变换——**缺口集中在"内容动画家族"与若干 API 变体**（详见阶段 0）。

---

## 阶段 0：当前状态（已完成，勿重做）

**已有**（`winia/src/animation.rs` 1052 行 + `animation/interpolator.rs` 1258 行 + `examples/animation_demo.rs` 7 节）：

- **高层**：`ComposeCtx::animate_float_as_state` / `animate_color_as_state` / `animate_dp_as_state` / `animate_offset_as_state` / `animate_size_as_state`（`composer.rs` 275-307）
- **中层**：`update_transition(target, spec, label)` + `Transition::animate_float`（仅 float）
- **低层**：`Animatable<T>`（`new`/`animate_to`/`update`/`snap_to`——帧驱动非协程）+ `push_animatable` / `push_animatable_color`
- **无限**：`remember_infinite_transition` + `InfiniteTransition::animate_float` / `animate_color`（CAM16-UCS 插值）+ `RepeatMode::{Restart, Reverse}` + `dispose`
- **Spec**：`AnimationSpec::{Spring, Tween, Keyframes, Repeatable, Snap}` + `SpringSpec`（bouncy 等）+ `TweenSpec` + `KeyframesSpec` + `RepeatableSpec`
- **插值器**：31 种（`interpolator.rs`——Linear 手动 + 30 种宏生成表驱动曲线：EaseIn/Out/InOut × Quad/Quart/Quint/Sine/Expo/Circ/Back/Elastic/Bounce + Cubic；trait + 批量宏，全部 Box<dyn Interpolator> 接入 Tween/Keyframes）
- **绘制层**：`Modifier::graphics_layer(GraphicsLayerParams)`（scale/alpha/rotation/translation——只触发重绘不触发布局）+ 派生值（DerivedValue）渲染期求值
- **帧驱动**：`app.rs` AboutToWait → `update_animations()`（全局注册表 ACTIVE_ANIMATIONS）
- **颜色动画**：f32 与 Color 分离注册表；Color 用 material-color-utilities blend/CAM16-UCS
- **demo**：7 节（spring 宽度 / as_state 平滑 / updateTransition 偏移 / 颜色 tween / 无限循环 / keyframes / dp）

**已知遗留**（前会话记录）：
- ~~动画启动帧闪烁/树塌缩~~——已修（composition-separation 8548718 等）
- ~~无限动画失效~~——已修（事件循环唤醒 + AboutToWait 兜底）
- 动画期间是否全量重组：动画 State 变化 → 依赖组件局部 dirty——粒度已局部化（demo 每节独立 `#[composable]`）；绘制层动画（graphics_layer/派生值）零重组

---

## 阶段 1：API 补齐（小改动、快收益）—— ✅ 已完成（166 tests）

**目标**：把 `animate*AsState` / Transition / Spec 的变体补齐到 Compose 同级。

### 步骤

**1. Transition 补多属性变体**（`animation.rs` Transition impl）：—— ✅ 已完成
```rust
impl<T: Clone + PartialEq + 'static> Transition<T> {
    pub fn animate_float(&mut self, ctx, target_fn, label) -> State<f32>;      // ✅ 已有
    pub fn animate_color(&mut self, ctx, target_fn, label) -> State<Color>;     // ✅ 新增——走泛型 animate_value → push_animatable
    pub fn animate_dp(&mut self, ctx, target_fn, label) -> State<Dp>;           // ✅ 新增
    pub fn animate_offset(&mut self, ctx, target_fn, label) -> State<Offset>;   // ✅ 新增
}
```
- 实现：抽内部辅助 `fn animate_value<T2>(ctx, value)`（remember + push_animatable）——各变体只差目标值类型与提取闭包

**2. Int 系列 as_state**（`composer.rs`）：—— ✅ 已完成
```rust
pub fn animate_int_as_state(&mut self, target: i32, spec) -> State<i32>;                              // ✅
pub fn animate_int_offset_as_state(&mut self, target: (i32, i32), spec) -> State<(i32,i32)>;          // ✅
pub fn animate_int_size_as_state(&mut self, target: (i32, i32), spec) -> State<(i32,i32)>;            // ✅
```
- `AnimatableValue` 为 i32 / (i32, i32) 实现（lerp 四舍五入；(i32,i32) supports_spring=false）—— ✅
- 验证：demo section9——150ms 采样 int=49 offset=(78,20)（400ms 动画 37% 插值 ✓）
- 测试：`int_value_lerp_rounds` / `int_offset_lerp_rounds` / `animatable_i32_animates`

**3. `RepeatableSpec` 加 `start_offset`**（对齐 Compose repeatable 的 StartOffset）：—— ✅ 已完成
```rust
pub struct RepeatableSpec {
    pub iterations: u32,
    pub mode: RepeatMode,
    pub base: AnimationSpec,
    pub start_offset: Duration,   // ✅ 新增——首轮延迟（delay 语义），期间值停在 from
}
impl RepeatableSpec { pub fn with_start_offset(self, offset: Duration) -> Self; }
```
- `Animatable::update` Repeatable 分支：`total = base×iterations + start_offset`；`eff = elapsed - start_offset`，eff==0 时停在 from
- 测试：`repeatable_start_offset_delays_first_cycle`（延迟期断言停在 from，完成后断言终值）

**4. 泛型 `animate_value_as_state<T>`**：—— ✅ 已完成
```rust
pub fn animate_value_as_state<T: Clone + PartialEq + AnimatableValue + Send + Sync + 'static>(
    &mut self, target: T, spec: AnimationSpec,
) -> State<T>;
```
- 注意：Color 建议用 `animate_color_as_state`（专用 CAM16-UCS 注册表，避免跨列表双驱动）

**Demo**：section8（updateTransition float+color+dp 协同）/ section9（Int 系列）/ section10（Repeatable+startOffset）—— ✅

### 验证
- `cargo test --lib` 全过
- demo 补：section8（Transition 颜色+偏移协同）、section9（Int 系列）
- `cargo run -p winia --example animation_demo` 平滑无抖动

### 已知坑
- `Dp`/`Offset`/`Size` 的 `AnimatableValue` 实现（unit.rs）——确认 lerp 存在，缺则补
- Transition 各变体 target_fn 的 Fn(&T) -> 具体类型——闭包签名统一
- push_animatable 对同 state 重复 push：先 remove 再 push（现有语义）

---

## 阶段 2：AnimatedVisibility（出现/消失动画）—— ✅ 已完成

**目标**：`AnimatedVisibility(visible) { content }`——进入/退出动画；退出完成后**从组合移除**（对标 Compose）。—— ✅

### 实现（`ui/animated_visibility.rs` 120 行）

**1. 组件骨架**：—— ✅
```rust
pub struct AnimatedVisibility { visible: State<bool>, enter: EnterTransition, exit: ExitTransition }
impl AnimatedVisibility {
    pub fn new(visible: State<bool>) -> Self;      // 默认 fade_in/fade_out
    pub fn enter(self, t: EnterTransition) -> Self;
    pub fn exit(self, t: ExitTransition) -> Self;
    pub fn build(self, ctx, content: impl FnOnce(&mut ComposeCtx));
}
pub struct EnterTransition { spec, offset_y }      // fade_in() / expand_in()（20px）
pub struct ExitTransition  { spec, offset_y }      // fade_out() / shrink_out()
```

**2. 延迟移除**（核心）：—— ✅
- 内部 `shown: State<bool>`（是否在组合）+ `alpha: State<f32>`（透明度）
- visible=true 且 !shown → shown=true + push 淡入（alpha 0→1）
- visible=false 且 shown → push 淡出 + **on_complete 回调置 shown=false** → 重组 → content 移除
- 动画经 graphics_layer（alpha + translation_y）渲染期求值——零重组
- 中途重入：exit 中 re-enter 自动换轨（同 state 不同 target 重定向）

**3. 动画引擎支持**：—— ✅
- `Animatable::on_complete(f)` 完成回调（update 完成 / snap / 5s 超时强制完成时消费一次）
- `start_anim` 辅助：目标已是终值直接回调（snap 语义）；非标量 Spring 降级 Tween

**4. demo**（animated_visibility_demo）：fade / expand+shrink / spring bouncy 三面板开关—— ✅

### 验证
- WS d/u 真实指针路径（逻辑坐标——scale≠1 时 c/d/u 用逻辑坐标）：
  - 退出动画 250ms 面板仍在（延迟移除）、1s 已移除（树 JSON 确认）
  - 重新进入、中途重入（exit 100ms 内 re-enter）正常
  - 点击仅 1 次 notify、动画结束后渲染停止（无空转）
- 166 tests 全过

### 顺带修复：debug-server 空闲事件滞留
- DebugEvent 在 RedrawRequested 处理——空闲窗口（无动画）wake_up 不产生 RedrawRequested
  → 模拟点击永远排队。修复：new_events 兜底加 `has_queued_events()`（同 pending 节流模式）
- 已知坑记录：**c/d/u 命令坐标是逻辑坐标**（scale=1.5 时按钮 (357,62) 逻辑 → 物理 535,93——用逻辑）；调试期间误判"点击无效"多次，实为坐标错位 + 编译失败跑旧 exe 叠加

---

## 阶段 3：Crossfade / AnimatedContent（内容切换）—— ⬜ 待做

**目标**：`Crossfade(target) { target -> content }` 交叉淡化；`AnimatedContent` 可定制过渡（后续）。

### 步骤

**1. Crossfade**：
```rust
pub struct Crossfade<T> { target: T, spec: AnimationSpec }
impl Crossfade<T> {
    pub fn build(self, ctx, content: impl FnOnce(&mut ComposeCtx, &T));
}
```
- 实现：target 变化 → 旧内容 alpha 1→0（保留一帧）+ 新内容 alpha 0→1 同时播放
- 两棵子树共存一帧（旧 fading out + 新 fading in）——**需要同时组合两个 content**
- 简单方案：内部 `prev: State<Option<T>>` + alpha State——`if alpha < 1 { prev content }` + `if alpha > 0 { new content }`

**2. AnimatedContent**（后置——依赖 Crossfade 机制 + 过渡方向）：
- `transitionSpec = { fadeIn togetherWith fadeOut }`——进出过渡组合
- 同 Crossfade 双树共存，但可指定方向（slide/fade/scale）

**3. demo**：Loading→Loaded→Error 状态切换（Crossfade 淡入淡出）

### 验证
- demo 切换平滑无跳变；快速连续切换收敛
- 树 JSON 确认过渡期双树共存、结束后旧树移除

### 已知坑
- 双树共存与 Slot 树 key 冲突：旧/新 content 需要不同 key 前缀（`ctx.key(...)`）
- 过渡期间两树都占布局（Stack 叠放）——用 Stack 或绝对定位

---

## 阶段 4：animateContentSize（尺寸变化动画）—— ⬜ 待做

**目标**：`Modifier.animate_content_size(spec)`——内容尺寸变化时平滑过渡（不跳变）。

### 步骤

**1. modifier**（`modifier.rs`）：
```rust
pub fn animate_content_size(self, spec: AnimationSpec) -> Self
```
- 测量策略包装：外层固定测量一次——内容实际尺寸变化时，用动画 State 插值（上一尺寸 → 新尺寸）
- 实现：`MeasurePolicy` 包装——measure 子节点得新尺寸 → 动画插值（旧→新）→ 返回插值尺寸
- **绘制阶段**：布局用插值尺寸；内容绘制位置随插值（内容实际尺寸 vs 布局尺寸的 offset 补偿）

**2. 触发**：内容尺寸变化来源——文本换行/条件尺寸——State 变化 → 重测 → 测量器发现尺寸变 → 动画

### 验证
- demo：展开/收起卡片（文本多行 ↔ 单行）平滑过渡
- 过渡中重测（新目标）不跳变

### 已知坑
- 测量期动画：measure 里不能直接改 State（借用）——用全局注册表 + 测量后更新
- 布局尺寸与绘制内容的错位（动画中间态）——子内容需按插值偏移

---

## 阶段 5：animateDecay（惯性/fling）—— ⬜ 待做（中期）

**目标**：`animateDecay(initialVelocity)`——fling 惯性滚动/拖拽释放动画（对标 Compose `splineBasedDecay`）。

### 步骤

**1. 核心**：`DecayAnimation`——基于速度的衰减（spline/指数衰减），无固定目标值：
```rust
pub fn push_decay(state: State<f32>, initial_velocity: f32, friction: f32);
```
- update：`velocity *= (1 - friction * dt)`；`value += velocity * dt`；速度近零停止
- `AnimatableValue` 扩展：offset/float 版

**2. 手势集成**（后续）：scrollable/draggable 释放时调用（scroll 系统现有 `apply_scroll_delta`——fling 对接）

### 验证
- 单测：速度衰减曲线收敛、不振荡
- demo：拖拽释放惯性滚动

### 已知坑
- 无目标值——没有"到达"判定，只有速度截止
- 与 scroll 的 clamp（最大偏移）交互：衰减中撞边界停止

---

## 阶段 6：Animatable 顺序/并发动画 + 文本动画（远期）—— ⬜ 待做

**目标**：
- **顺序动画**：`anim.animate_to(a).then().animate_to(b)`——Compose 用 suspend 链，本项目帧驱动——提供 `on_finish: Box<dyn FnOnce()>` 回调链（animate_to 完成触发下一个）
- **并发**：多个 Animatable 各自 update（现有全局注册表天然并发）
- **TextMotion 类**：文本 scale/translate 动画时字形平滑（渲染层——skia 文本布局缓存与变换的交互——远期）

### 已知坑
- 回调链的所有权（Animatable 被注册表持有 + 用户持有——clone 语义）
- on_finish 中再 push 动画（递归注册——注意注册表锁）

---

## 通用验证（每阶段后）

```bash
cd /d/Projects/winia/winia
cargo test --lib                      # 162+ 全过（现有）
cargo build -p winia --example animation_demo --features debug-server
# WS 验证（demo 启动后）：
#   ws://localhost:9998 → 't' 看树（节点数稳定/动画节局部重组）
#   'c' 点按钮 → 动画平滑（无跳变/无闪帧）
#   'p' 连拍截图 → 过渡帧亮度/位置连续（无空白帧）
```

## 里程碑检查

- [x] 阶段 1：API 补齐（Transition 变体 / Int 系列 / start_offset / 泛型 as_state）
- [x] 阶段 2：AnimatedVisibility（延迟移除 + enter/exit）
- [ ] 阶段 3：Crossfade / AnimatedContent
- [ ] 阶段 4：animateContentSize
- [ ] 阶段 5：animateDecay
- [ ] 阶段 6：顺序动画 / 文本动画
