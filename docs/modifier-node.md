# Modifier Node（开放扩展点，已合入 `v2`）

> 状态：已合入 `v2`（`4ca3aea`，fast-forward 自 `17962ee`，9 提交）。
> 真实迁移累计 12 节点（2026-09）：Slider 轨道（首个）→ ProgressIndicator ×4 →
> Divider ×1 → wavy ×4 + loading ×1 → Scrollbar ×1（`ScrollbarNode`，三组件
> Vertical/Horizontal/Lazy 共用同一节点）。生产代码 `.draw()` 匿名闭包已清零
> （残留仅测试双路对照 + 非 Modifier 的 `sw.draw`/`paragraph.draw`）。
> 目标：把 Modifier 从"封闭枚举"变成
> "开放节点"——第三方不改核心即可实现自定义行为。
>
> 背景：`ModifierElement` 是 `pub(crate)` 封闭枚举（`modifier.rs`），50+ 变体，
> 所有行为散在 `measure_node` / `render_pass1` / `hit_test` / `app.rs` 四处 match。
> 加一种新行为 = 改枚举 + 改四处 + 全量编译，外部作者写不出自定义行为。

## 一、设计：双轨并存

```text
Modifier { elements: Vec<ModifierElement>,   // 旧轨道：封闭枚举，内部实现
           nodes: Vec<ModifierNode> }        // 新轨道：开放节点，第三方扩展
```

- 旧 builder（`background/clickable/on_pointer_event/…`）照旧 push 枚举，行为零变化。
- 新扩展走 node：`draw_node / click_node / pointer_node / key_node / layout_node` 五个 builder。
- `then` 合并双轨（`elements.extend + nodes.extend`）。
- `param_eq`（Skip 判定）：枚举按现有规则 + node 按 `node_key()` 序列比较。
- 优先级原则：**枚举优先，node 回退**。同节点同阶段枚举消费（返回 true/命中）
  则 node 不再收到。保证迁移期旧行为稳定。
- `debug` 树：`describe_modifier` 追加 `node(<key>)` 条目，`t` 命令可见。

## 二、五种节点（Draw/Click/Pointer/Key/Layout-A型）

### DrawNode（绘制）——对标 Compose DrawModifierNode

```rust
pub trait DrawNode: Debug + Send + Sync {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect);
    fn node_key(&self) -> String { type_name::<Self>().into() }  // 默认；含参 MUST 覆盖
}
```

- 位置：`render_pass1` 枚举链走完后统一绘制（P1-1 修后与注释一致；此前 node
  在前、枚举在后，混用即被盖——已修，回归测试 `node_track_draw_after_enum_background`）。
  ⚠ 链序未保留：图标/图片等枚举内容绘制也在 node 之前（精确链序等 DrawWrapNode，§四.3）。

- 位置：`render_pass1` 背景层（枚举链走完后统一绘制，与 Background 同层语义）。
- 复用：`render::draw_background_for_node(canvas, rect, color, shape)` 公开，
  自定义背景无需复刻绘制逻辑。
- 示例：`modifier.rs` 测试 `TestBgNode`（Background 的 node 等价物）。

### ClickNode（点击）——对标 Compose clickable 语义

```rust
pub trait ClickNode: Debug + Send + Sync {
    fn on_click(&self);
    fn interaction(&self) -> Option<MutableInteractionSource> { None }
    fn node_key(&self) -> String { type_name::<Self>().into() }  // 默认；含参 MUST 覆盖
}
```

- 单首语义（P1-2）：每 Modifier 仅首个 ClickNode 生效（与枚举 `on_click()`
  取首个 Clickable 一致）；`node_clicks()` 迭代器保留供未来多播（暂未启用）。

- 接线：`fire_click_along_path`（主树+overlay 共用）、press 波纹两处
  （`clickable_interaction().cloned().or(node_click_interaction())`）、
  键盘 Enter/Space 激活、debug `c` 点击。共 5 处，均为"枚举无命中时回退"。
- 注意：`clickable_interaction()` 返回 `&` 引用，node 在 `Arc<dyn>` 内给不出
  `&`——保留原函数，另加克隆版 `node_click_interaction()`。不要试图统一签名。
- 示例：测试 `TestClickNode`（计数器，点击两次 = 2）。

### PointerNode（原始指针）——对标 Compose PointerInputModifierNode

```rust
pub trait PointerNode: Debug + Send + Sync {
    fn on_pre(&self, _ev: &PointerEvent) -> bool { false }    // 隧道 外→内
    fn on_event(&self, _ev: &PointerEvent) -> bool { false }  // 冒泡 内→外
    fn node_key(&self) -> String { type_name::<Self>().into() }  // 默认；含参 MUST 覆盖
}
```

- 接线：`dispatch_ptr_event` 隧道/冒泡各加 3 行（枚举先行，消费即停，坐标同源）。
- ⚠ 手势路由隔离（P0-1）：PointerNode **不参与** `has_gesture()`——手势目标选择
  只认 TapOn/DragOn 枚举（装饰性 node 纳入即偷外层 tap，已修+回归测试
  `pointer_node_does_not_steal_outer_tap_target`）。PointerNode 只收原始
  down/move/up；要手势回调请同时挂对应枚举。
- 坐标：node 拿到的 `ev.position` 与枚举 handler 同一本地坐标（含 scroll 扣除）。

### KeyNode（键盘）——对标 Compose KeyInputModifierNode

```rust
pub trait KeyNode: Debug + Send + Sync {
    fn on_pre(&self, _ev: &KbEvent) -> bool { false }    // 隧道 root→focused
    fn on_event(&self, _ev: &KbEvent) -> bool { false }  // 冒泡 focused→root
    fn node_key(&self) -> String { type_name::<Self>().into() }  // 默认；含参 MUST 覆盖
}
```

- 接线：`dispatch_key_to_focus` Preview/Bubble 各加 3 行。
- 测试用真实 PerWindow + Composer 组树（端到端，非裸节点）。

### LayoutNode-A型（约束变换）——对标 Compose LayoutModifier

```rust
pub trait LayoutNode: Debug + Send + Sync {
    fn transform(&self, inner: Constraints) -> Constraints;
    fn node_key(&self) -> String { type_name::<Self>().into() }  // 默认；含参 MUST 覆盖
}
```

- 插入点：`measure_node` resolved_size 之后串行（min/required 可覆盖 node）。
- 纯度 MUST（P1-6）：`transform` MUST 为“key 参数 + State::get”纯函数——
  禁读外部可变；常量折叠按 incoming 缓存，非纯即 stale。动态值在内 `get`
  注册布局依赖（免 compose 重测）。B 型完全接管搁置（见 §五）。

### node_key MUST 规范（P1-3，各 trait rustdoc 同文）

- 含参节点 MUST 覆盖，纳入**全部静态视觉参数**；回写通道（State 回写）与
  瞬态动画值（逐帧 progress/alpha）MUST NOT 进 key；回调字段视为相同。
- 默认实现返回 TypeId 名（无参节点够用；含参忘覆盖则静默 Skip 误命中）。

## 三、第三方用法（照抄形状）

```rust
use winia::modifier::{DrawNode, Modifier, Shape, Color};

#[derive(Debug)]
struct MyBadge { color: Color }

impl DrawNode for MyBadge {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        winia::render::draw_background_for_node(canvas, rect, &self.color, &Shape::Circle);
    }
    fn node_key(&self) -> String {
        format!("mybadge:{:?}", self.color)  // 参数变化 → Enter（必须含全部参数）
    }
}

// 使用：
Modifier::new().size(48.0, 48.0).draw_node(MyBadge { color })
```

`node_key` 必须包含**全部静态视觉参数**（颜色/形状/开关/值），漏写即 Skip
误命中（旧值残留）。回写通道与瞬态动画值（逐帧 progress/alpha）必须排除——
渲染期 `peek` 直读，进 key 则动画每帧 Enter（P1-2）。闭包/回调类字段与枚举
同例——视为相同（回调重建不触发 Enter），如需回调变化刷新，把版本号/
布尔开关纳入 key。完整 MUST 规范见 §二末。

## 四、已知限制（故意没做）

1. ~~无 LayoutNode~~ → ✅ 已落地（A 型约束变换，见 §五）。
2. ~~无 onAttach/onDetach~~ → ✅ 约定已定：**不需要**。node 不持有组合期 State，
   状态由 build 内 `remember` 创建后 clone 进 node（State 是 Arc）。状态生命
   周期跟槽走，节点移除随槽回收。绘制期 `get` 不注册依赖（render 已出依赖帧，
   与枚举 Background color_fn 完全一致）——状态驱动靠 build 期 `get` 注册
   compose/layout 依赖 + 绘制期 `peek` 求值（实测 `node_track_stateful_draw_follows_state`）。
   想在 node 里 `get` 注册是误解，不要开这个口子。
3. ~~No ordering semantics~~ → ✅ Partially resolved: **DrawWrapNode landed**
   (`exp/draw-wrap-node`, v2 2026-09). `DrawWrapNode` trait with `draw_before`
   (background layer, same slot as DrawNode) + `draw_after` (above children and
   above ripple, inside the scroll translate — same stack as ripple).
   `draw_after` receives `&Modifier` so shape inference (nearest Background/Border,
   same logic as `draw_ripple`) works unchanged. Verified by 4 pixel tests
   (`wrap_before_paints_at_background_layer` / `wrap_after_covers_background_layer` /
   `wrap_after_covers_child_text` / `wrap_after_covers_enum_ripple` incl. counter-proof
   without wrap) + 1 Skip key test (`wrap_node_key_drives_skip`).
   - Ripple migration: **still deferred, deliberately**. The mechanism is proven, but
     Ripple runs fine as an enum and migration buys zero user-visible change for
     1–2 days of load-bearing-wall work. Revisit when Ripple needs new semantics
     (e.g. new M3 ripple spec) or a real third-party indication demand appears —
     same "build when forced" rule that shelved LayoutNode-B.
4. **trait object 开销**：每节点多一次 vtable + Arc。热路径（measure）A 型 node
   仅多一次 transform 调用（MinWidth 级别，开销可忽略）；绘制/输入路径可接受；
   大规模列表待实测（未测，不要断言）。

## 五、下一步：LayoutNode 预研结论

预研对象：`measure_node`（`layout/node.rs:1382-1715`）。约束链共 8 段：

```text
incoming
 → resolved_size（Size/tighten，动态 State 求值+注册依赖）
 → min（MinWidth/Height，提升 min，受 max 夹）
 → required（RequiredSize，直接覆盖，⚠必须在 size 之后）
 → fixed（Fixed/Dp/Px 兼容分支）
 → padding（offset(pad)，子约束缩小）
 → fill（min=max，scroll 前保存 viewport）
 → scroll（viewport 回写节点字段；lazy 不改无界，普通改无界）
 → policy.measure / 叶子分支
 → 后处理：padding 回加 → scroll clamp 回视口 → aspectRatio → 缓存+上报
```

### 结论：分两型渐进，不要一次做完全接管

**A 型：约束变换（先做，低风险）**

```rust
pub trait LayoutNode: Debug + Send + Sync {
    /// 输入 incoming，输出给下一环节的约束（Compose LayoutModifier 对应物）。
    fn transform(&self, inner: Constraints) -> Constraints;
    fn node_key(&self) -> String;
}
```

- 插入点：resolved_size 之后、padding 之前（与现有链序同）。多个 A 型 node
  按挂载序串行变换。
- 可迁移：MinWidth/Height（提升 min）、RequiredSize（覆盖）、AspectRatio
  （测量后段单独处理，需第二钩子 `adjust_size(size, inner) -> Size`）。
- 常量折叠：`cached_constraints` 存的是 incoming，node 参数变化已由
  `param_eq/node_key` 触发 dirty——折叠语义天然保持，无需额外设计。
- 试点建议：PaddingNode（offset 变换 + place 期偏移，两段需配对）。

**B 型：完全接管（后做，高风险）**

```rust
fn measure(&self, children: &[usize], inner: Constraints) -> (Size, Vec<Placement>);
```

- 冲突点：① policy 归属（现在 policy 是组件传的独立参数，与 modifier 链
  分离——B 型 node 与 policy 谁优先？）；② place 职责（policy.place、
  padding 偏移、offset 镜像三段都在 measure 后，B 型接管后由谁执行？）；
  ③ scroll 副作用（viewport 回写、fling_limit、lazy 内容同步散在链中，
  B 型绕过即丢滚动语义）。
- 建议：B 型暂缓。等 A 型验证折叠/dirty 语义后，再评估是否值得。
  当前 policy 机制已够用（自定义布局走组件级 MeasurePolicy，如 BadgedBox），
  modifier 级布局的真实需求尚未被 demo 倒逼——不要为对称而做。

### 决策：A 型已验证（`6464439`），B 型搁置

- A 型试点（MinWidthNode）：静态提升生效 + 常量折叠保持 + State 驱动免
  compose 重测，三项全过。插入点 resolved_size 之后（`layout/node.rs`）。
- B 型三冲突（policy 归属/place 职责/scroll 副作用）未解，且真实需求未被
  倒逼——不为对称而做。

## 六、测试矩阵

| 位置 | 用例 | 覆盖 |
|---|---|---|
| `modifier::node_track_tests` | draw 像素 / click 无枚举可达 / param_eq / then合并 / pointer装配+指纹 / key装配+指纹 / layout静态+折叠 / layout State驱动 / stateful绘制+依赖约定 | 9 项 |
| `app::pointer_dispatch_coord_tests` | 双轨顺序+坐标 / 枚举消费阻断 | 2 项 |
| `app::key_node_dual_track_tests` | PerWindow 端到端隧道+冒泡 | 1 项 |
| `slider::tests` | node_key 全参数覆盖 / node 像素保真 / 调试树可见（feature 门控） | 3 项 |
| 全量 | `cargo test -p winia --lib` | 727 通过（`animate_scroll_to_item` 时间敏感 flaky，见下） |

## 九、首个真实迁移：Slider 轨道（`SliderTrackNode`）

迁移对象选择标准：① 匿名闭包（无名、无 key、无调试可见性）② 参数多
（Slider 绘制参数 9 个；旧闭包 `CustomDraw` 在 `param_eq` 无分支 → 落到
`_ => false` → 恒 **Enter** 多余重跑，P1-1 更正：迁移收益是“消灭多余 Enter”
而非“恒 Skip 精度损失”）③ 有像素测试兜底。
Switch 符合①但枚举耦合深（14 处 match），Divider 太简单无代表性——Slider 居中。

### 迁移步骤（照抄清单）

1. 定义具名 struct（全部绘制参数 + 回写 State + interaction 源），`#[derive(Debug)]`，
   可见性 `pub(crate)`（P1-4：第三方照抄形状自定类型，不复用本节点；未 clamp
   值/`min>max` 未归一在 build 侧，node 内无校验）。
2. `impl DrawNode`：`draw` 内复用原绘制函数（`draw_slider` 原样保留，
   `pub(crate)` 不动）；`node_key` 纳入**静态视觉参数**（颜色/值/开关/源 id），
   回写通道（`track_width`）与瞬态动画值（`focus_alpha` 等逐帧值）不纳入——
   后者渲染期 `peek` 直读（P1-2；`focused` 经 `thumb_active` 间接覆盖）。
   `-0.0/0.0` key 不同但同画（保守多 Enter，无害，P2-3）。
3. build 侧：`.draw(closure)` → `.draw_node(Struct { … })`，闭包捕获的变量
   变成 struct 字段（编译器强制完备——漏字段即编译错，这是具名化的隐藏收益）。
4. 补三测试：key 全覆盖（含 colors/min/max/steps 变更 ne + 回写值变更 eq，
   P0-2）/ 真双路像素对照（同参 node 路 vs 旧闭包路逐字节 eq，P0-1）/
   调试树可见（`#[cfg(feature = "debug-server")]` 门控）。
5. 原绘制函数保留（node 内复用，单测/他处可调）+ 同步组件文档
   （`docs/slider.md` 架构段，P1-3）——迁移不是删除。
6. 迁移前自问（P1-6：Slider 范式未覆盖的三问，迁 Switch/Divider 前必答）：
   (a) 瞬态动画是否进 key？（进则每帧 Enter；不进需“排除 + peek 重绘”约定）
   (b) 层位是否匹配？（draw_node 统一枚举后绘制，不保留链序；内容之上型
   如 unbounded ripple 照搬即变 z-order）(c) 渲染期求值捕获了什么？
   （build 期快照改 Dynamic 语义；存 Modifier 进 node 涉 Debug/key；
   `to_bits` 遇 NaN 抖动）。

### 实测教训

1. **`source_id()` 需新加**：`MutableInteractionSource` 无公开身份（`PartialEq`
   用内部 State id 比较）。加 `pub fn source_id(&self) -> u32`（`interaction.rs`，
   取 pressed State id，同 PartialEq 语义跨 clone 稳定）。不要 `Debug` 格式化
   整个源（State 地址每帧变，key 抖动致永不 Skip）。
2. **测试要配 `DrawNode` import**：`node_key()` 是 trait 方法，测试模块需
   `use crate::modifier::DrawNode`（编译错 `E0599` 即此因）。
3. **同帧 new 的源 id 不同**：`MutableInteractionSource::new()` 每次 id 全新——
   key 测试必须共享同一源实例，换源单独测。生产无此问题（remember 跨帧稳定）。
4. **调试树测试要 feature 门控**：`build_tree_json` 非 feature 下是空 stub，
   `#[cfg(feature = "debug-server")]` 门控测试（踩过一次才加）。
5. **node_key 里的 `{:?}` 会很长**（SliderColors 全字段展开）——调试树可读性差
   但正确性无碍。后续可给 key 加短指纹（fnv 压缩），现在先保证完备。
6. **format 占位符数**：9 参数写成 8 个 `{}` 即编译错——数清楚，最好分行写。

## 八、附：`animate_scroll_to_item_animates_offset` flaky 根因（分支外，记录）

- 现象：全量跑时偶发 `23980 vs 23966`（差 13px，阈值 5px），单跑必过。
- 根因：`step_animations_until_done`（`lazy_column.rs:1630`）用
  `sleep(20ms)` + `(v-last)<0.5 && frames>5` 判收敛——spring 尾段每帧位移
  <0.5 即停，残余 13px 未收完就 assert。机器负载高时帧间隔抖动，
  收敛判据提前触发。
- 与本分支无关（改动面未碰 spring/滚动；干净树同现象已验证）。
- ✅ 已修（v2，2026-09）：新增 `step_animations_until_target(state, target, max)`
  helper——调用方持 TEST_SERIAL（串行下 `has_animation_for_state` 无空窗误判），
  以"动画表排空 + 距目标 <5px"双条件收敛（与最终断言同阈）；`animate_scroll_to_item`
  用例切到该 helper。判据间隙：引擎 done 阈 0.01 vs 旧位移判据 0.5（50 倍），
  尾段提前触发是必然的。验证：单跑 10/10 + `lazy_column` 模块 28/28 +
  全量 730/730（`--test-threads=1`）。

## 七、提交历史（分支 `exp/modifier-node`，已删，内容全合入 v2）

1. `88a3cdc` Draw+Click 双轨基线（含 debug 树 node 条目）
2. `c013e03` PointerNode 双轨
3. `555c057` KeyNode 双轨
4. `b17cf8d` 设计文档 `modifier-node.md`
5. `6464439` LayoutNode A 型约束变换试点
6. `0fdf563` 有状态节点约定+链序结论
7. `6f6e749` Slider 轨道迁移 SliderTrackNode（首个真实迁移，§九）

## 十、后续迁移（v2，2026-09，分支已合入并删除）

| 批次 | 节点 | 提交 | 要点 |
|---|---|---|---|
| ProgressIndicator ×4 | LinearDeterminate/Indeterminate + CircularDeterminate/Indeterminate | `daba461` + review `66c3c88` | 无限动画 peek 不进 key（三问a探针）；review 补 `changed(draw_stop)` + 切换测试；5 新测试 |
| Divider ×1 | DividerNode | `4cb7f0d` | padding 内缩解耦四 f32（动态 padding 快照语义）；Hairline NaN 双语义注记；2 新测试 |
| wavy ×4 + loading ×1 | Linear/CircularWavy(In)determinate + LoadingIndicatorNode | `97e862d` | 路径缓存 `Arc<Mutex>` 不进 key（手写 Debug）；`amplitude_token` 进 key；4 新测试 |
| Scrollbar ×1 | ScrollbarNode（Vertical/Horizontal/LazyScrollbar 三组件共用） | `12e179a` + 后续 `bac42eb`/`232d3e1`/`f494263`/`30da727`/`6cf0846`/`8916eb3`/`0885cce`/`6e47a19`/`da8a58b`/`465dbea`/`392011c` | fade:State 进节点 draw 期 peek（build 快照 bug）；滚轮去盲找；offset 脉冲 + 单 effect 睡醒快照；review P0 修（clamp panic/viewport get/drag_cancel）；抽 `scrollbar_build_shared`；grab 保持；边界 pulse + reverse 镜像；Lazy 适配 |

生产代码 `.draw()` 匿名闭包清零验证：`grep -rn "\.draw(" winia/src` 残留仅
测试双路对照（7 处，故意保留）+ `sw.draw`（skiwin 后端）/ `paragraph.draw`
（skia 调用，非 Modifier）。全量 742 绿。
