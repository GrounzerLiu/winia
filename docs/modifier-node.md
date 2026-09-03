# Modifier Node（开放扩展点，实验分支 `exp/modifier-node`）

> 状态：实验中（双轨并存，零破坏）。目标：把 Modifier 从"封闭枚举"变成
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
- 新扩展走 node：`draw_node / click_node / pointer_node / key_node` 四个 builder。
- `then` 合并双轨（`elements.extend + nodes.extend`）。
- `param_eq`（Skip 判定）：枚举按现有规则 + node 按 `node_key()` 序列比较。
- 优先级原则：**枚举优先，node 回退**。同节点同阶段枚举消费（返回 true/命中）
  则 node 不再收到。保证迁移期旧行为稳定。
- `debug` 树：`describe_modifier` 追加 `node(<key>)` 条目，`t` 命令可见。

## 二、四种节点

### DrawNode（绘制）——对标 Compose DrawModifierNode

```rust
pub trait DrawNode: Debug + Send + Sync {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect);
    fn node_key(&self) -> String;  // Skip 指纹，含参节点必须覆盖
}
```

- 位置：`render_pass1` 背景层（枚举链走完后统一绘制，与 Background 同层语义）。
- 复用：`render::draw_background_for_node(canvas, rect, color, shape)` 公开，
  自定义背景无需复刻绘制逻辑。
- 示例：`modifier.rs` 测试 `TestBgNode`（Background 的 node 等价物）。

### ClickNode（点击）——对标 Compose clickable 语义

```rust
pub trait ClickNode: Debug + Send + Sync {
    fn on_click(&self);
    fn interaction(&self) -> Option<MutableInteractionSource> { None }
    fn node_key(&self) -> String;
}
```

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
    fn node_key(&self) -> String;
}
```

- 接线：`dispatch_ptr_event` 隧道/冒泡各加 3 行（枚举先行，消费即停，坐标同源）。
- `has_gesture()` 含 PointerNode——第三方手势无需 TapOn/DragOn 枚举即可进路由。
  这是未来手势竞技场的前置条件。
- 坐标：node 拿到的 `ev.position` 与枚举 handler 同一本地坐标（含 scroll 扣除）。

### KeyNode（键盘）——对标 Compose KeyInputModifierNode

```rust
pub trait KeyNode: Debug + Send + Sync {
    fn on_pre(&self, _ev: &KbEvent) -> bool { false }    // 隧道 root→focused
    fn on_event(&self, _ev: &KbEvent) -> bool { false }  // 冒泡 focused→root
    fn node_key(&self) -> String;
}
```

- 接线：`dispatch_key_to_focus` Preview/Bubble 各加 3 行。
- 测试用真实 PerWindow + Composer 组树（端到端，非裸节点）。

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

`node_key` 必须包含**全部影响输出的参数**（颜色/形状/开关），漏写即 Skip 误命中
（旧值残留）。闭包/回调类字段与枚举同例——视为相同（回调重建不触发 Enter），
如需回调变化刷新，把版本号/布尔开关纳入 key。

## 四、已知限制（故意没做）

1. **无 LayoutNode**：`MeasurePolicy` 仍是组件级参数，未下沉到 modifier 级。
   `modifier.layout_node(...)` 需动 `measure_node` 约束解析链，单独设计（见 §五）。
2. **无 onAttach/onDetach**：node 是纯数据+行为，无状态。StatefulNode
   （内部动画/订阅）需接 `remember`，挂载点待设计。
3. **无顺序语义**：node 链统一在背景层/枚举后绘制，不保留链序交织。
   需要链序精确控制（如 drawWithContent 包裹）暂不支持。
4. **trait object 开销**：每节点多一次 vtable + Arc。热路径（measure）未用 node，
   绘制/输入路径可接受；大规模列表待实测。

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
| `modifier::node_track_tests` | draw 像素 / click 无枚举可达 / param_eq / then合并 / pointer装配+指纹 / key装配+指纹 / layout静态+折叠 / layout State驱动 | 8 项 |
| `app::pointer_dispatch_coord_tests` | 双轨顺序+坐标 / 枚举消费阻断 | 2 项 |
| `app::key_node_dual_track_tests` | PerWindow 端到端隧道+冒泡 | 1 项 |
| 全量 | `cargo test -p winia --lib` | 724 通过（1 时间敏感 flaky 重跑过） |

## 七、提交历史（分支 `exp/modifier-node`）

1. `88a3cdc` Draw+Click 双轨基线（含 debug 树 node 条目）
2. `c013e03` PointerNode 双轨
3. `555c057` KeyNode 双轨
4. `b17cf8d` 设计文档 `modifier-node.md`
5. `6464439` LayoutNode A 型约束变换试点
