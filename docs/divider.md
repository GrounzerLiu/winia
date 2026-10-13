# Divider 组件（material3 对齐）

> 分支：`divider`（从 v2 分出）
> 对标：material3 `HorizontalDivider` / `VerticalDivider`（Compose androidx-main Divider.kt + M3 token v0_117）
> M3 规格：https://m3.material.io/components/divider/specs

## 1. API

```rust
// 水平分隔线（对标 HorizontalDivider：fillMaxWidth × thickness）
Divider::horizontal()
    .thickness(f32)                  // 线宽 dp，默认 1（DividerTokens.Thickness）
    .color(Color)                    // 默认 OutlineVariant
    .modifier(Modifier)              // 可加 padding 实现 inset/middle-inset 变体
    .build(ctx);

// 垂直分隔线（对标 VerticalDivider：thickness × fillMaxHeight）
Divider::vertical()...

// 常量
DIVIDER_THICKNESS: f32 = 1.0;   // 默认厚度
DIVIDER_HAIRLINE: f32 = NAN;    // sentinel: one DEVICE pixel (Compose's Dp.Hairline)
```

## 2. 设计要点（对齐项）

### 2.1 Token（DividerTokens）

- **Color** = `OutlineVariant`（light ≈ #CAC4D0 / dark ≈ #49454F）
- **Thickness** = 1dp

### 2.2 尺寸（M3 Measurements）

| 变体 | 值 | 实现方式 |
| --- | --- | --- |
| Full-width | 100% | 默认（fillMaxWidth） |
| Inset | 左 16dp / 右 0dp | `.modifier(padding_start(16.0))` |
| Middle-inset | 左右各 16dp | `.modifier(padding_horizontal(16.0))` |
| 与 supporting-text 间距 | 4dp | 用户布局 |

组件本身不内置 inset 参数（Compose 同——仅 modifier 支持）。

### 2.4 已知框架限制

- **CustomDraw 用节点 rect**：winia 的 `Modifier::draw()` 闭包收到的是节点 rect（含
  padding 区域），不感知 padding。Divider 组件内部读取 `get_padding_sides()` 并内缩
  绘制 rect（对齐 DrawIcon 语义）——用户 modifier 的 padding（inset/middle-inset）
  正确生效；
- **VerticalDivider 在无高度 Row 中塌陷**：Row 高度内容驱动（无固定高度）时，
  `fill_max_height` 因父约束无界而跳过，高度为 0。需显式给高度
  （`.modifier(Modifier::new().height(20.0))`）与兄弟文本对齐——Compose 同样依赖
  父容器有确定高度。
- **发丝线的设备行吸附假设变换是轴对齐的**：`draw_divider_line` 的吸附用
  `canvas.local_to_device_as_3x3()` 的 `scale_x/scale_y` 与 `translate_x/y`
  （`divider.rs:247-252`），在缩放+平移下正确；winia 另有 `rotation_z/x/y`
  （`modifier.rs:1666`、`:1684`），旋转之后"设备行"不再平行于本地 y 轴，吸附会落到错误
  轴上（最多半像素偏差）。分数平移的情况已经在三条用例里覆盖（`:527`、`:555`、`:587`），
  旋转没有。

### 2.3 绘制

- The line is stroked on the canvas, centred on the thickness (`y = thickness / 2`), which
  keeps a real thickness's stroke inside its own box at any density;
- `DIVIDER_HAIRLINE` (the NaN sentinel, or an explicit 0) strokes with width 0 — Skia's DEVICE
  hairline, one pixel whatever the canvas transform. That is Compose's mechanism too: the
  current `HorizontalDivider` strokes with `thickness.toPx()`, and `Dp.Hairline` is `Dp(0f)`.
  It is centred on a device ROW rather than on the node's edge, so it is fully covered instead
  of split across two rows at half alpha, and that centring is snapped to the row its ideal
  position falls in — in DEVICE space, so a fractional canvas translation (an overlay at a
  fractional position, a layer mid-slide) is accounted for. This makes winia's hairline
  CRISPER than Compose's, which draws on the box's edge and does not snap; it is not parity and
  is not claimed as such.
- DEVIATION: a hairline occupies ONE LOGICAL px of layout. Compose asks for
  `height(Dp.Hairline)` — zero height — and paints anyway, because its `Canvas` is not gated on
  the element having size; winia's `render_pass1` returns early for `w <= 0.0 || h <= 0.0`, so a
  zero-height divider paints NOTHING (measured). One logical px is the smallest extent that
  still draws.
- `DividerNode` 具名绘制（exp/divider-node 迁移，原 `.draw` 匿名闭包 ×2，
  `pub(crate)`）：`{vertical/thickness/color/pad_s/pad_t/pad_e/pad_b}`——
  padding 内缩解耦为四个静态 f32（build 期 `get_padding_sides()` 快照；
  inset 变体为静态值，动态 padding 快照一次，存 Modifier 进 node 涉 Debug/key）；
  `node_key` 全参数 `to_bits`（同 payload NaN key 相等）。
  **Skip 判定读的是 `Modifier::param_eq`**（`modifier.rs:3397-3412`：node 轨道按
  `node_key_of` 字符串比、元素按值比），而 `build` 让哨兵只进**绘制**厚度、布局厚度仍取
  1.0（`divider.rs:108`），所以发丝线的整条链在两个实例间比较**相等**、组照常 Skip。
  曾经的注释（以及一轮审计）写"Hairline 无 Skip 收益"，那是把 `ctx.changed` 的
  `PartialEq` 当成了判定路径——`build` 丢弃了 `changed` 的返回值（`:90-92`），它不决定
  任何事。真正的陷阱在别处：若把 NaN 也当成**布局**尺寸（`Size{height: Static(NaN)}`），
  元素比较里 `NaN != NaN` 会让链自身不相等，Skip 才会失效。
  `a_hairline_dividers_chain_compares_equal_so_it_can_skip` 把这一正一反都钉住了。

## 3. 与 Compose 的差异

- Compose 旧 `Divider` 名已废弃改名 `HorizontalDivider`；winia 用
  `Divider::horizontal()/vertical()` 双构造（无历史包袱）。
- `Dp.Hairline` is expressed in winia as the `f32::NAN` sentinel — `Divider::thickness` takes an `f32`, so the sentinel rides in the thickness rather than in a `Dp` (winia has one; it is in the prelude).
- **负厚度静默变成发丝线**：`divider.rs:108` 与 `:208` 的判定是"NaN 或 <= 0"，所以
  `.thickness(-2.0)` 既不会报错也不会画 2 逻辑像素的线，而是画发丝线。Compose 的 `Dp` 在
  `height`/`width` 上不接受负值（构造期就抛），winia 选择宽容处理；文档这一条以前只写了 0
  与哨兵，没写负数。
- 无无障碍语义（winia 语义系统未覆盖，同其它组件）。

## 4. 运行与测试

```bash
cargo run -p winia --example divider_demo
cargo test -p winia --lib components::divider
```

测试覆盖：默认值（1dp/OutlineVariant）、水平/垂直像素渲染、自定义颜色+厚度、
Hairline 单像素、padding 内缩、Hairline 链可比较相等（可 Skip，含 NaN 布局尺寸的反例）；
DrawNode 迁移：node_key 全覆盖（含 Hairline NaN 语义）、双路像素对照
（横线 padding 内缩 + 垂直线逐字节）。
