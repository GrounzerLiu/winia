# FlowRow / FlowColumn 流式布局（foundation 对齐）

> 状态：已实现（分支 `exp/flow-layout`，从 v2 分出）
> 对标：Compose foundation `FlowRow` / `FlowColumn`
> （`FlowLayout.kt` + `FlowLayoutBuildingBlocks.kt`，1.11.4-sources.jar，
> 源码解包在 `tmp/flow-src/`——本地工作区，不进版本库）
> 实现：`winia/src/layout/flow.rs`（`measure_flow<A: FlexAxis>` + 双策略）；
> 组件：`winia/src/ui/layout_components.rs`（`FlowRow` / `FlowColumn`）；
> demo：`winia/examples/flow_demo.rs`（chip 过滤器 + max 行 + 换列）。

## 1. API

```rust
// 流式行：主轴填满即换行（chip 组/标签云）
FlowRow::new()
    .main_spacing(8.0)        // 行内主轴间距（对标 horizontalArrangement=spacedBy）
    .cross_spacing(8.0)       // 行间距（对标 verticalArrangement=spacedBy）
    .max_items_in_row(3)      // 每行上限（默认不限，对标 maxItemsInEachRow）
    .arrangement(Arrangement::Start)  // 行内主轴分配（复用 flex compute_spacing）
    .alignment(Alignment::Start)      // 行内交叉轴对齐（Top/Center/Bottom）
    .modifier(Modifier)
    .build(ctx, |ctx| { /* children */ });

// 流式列：主轴（垂直）填满即换列（参数对称）
FlowColumn::new()
    .main_spacing(8.0)
    .cross_spacing(8.0)
    .max_items_in_column(2)
    .build(ctx, |ctx| { /* children */ });
```

## 2. 语义（对标项）

- **断行**（对标 `FlowLayoutBuildingBlocks.getWrapInfo`）：行首不换、
  超 `max_items` 换、主轴剩余放不下即换。主轴无界时永不换行（退化 Row/Column）。
- **行尺寸**：行主轴 = 内容 clamp 进约束；行交叉 = 行内最高项。
- **行内**：主轴按 `arrangement` 分配剩余（`compute_spacing` 复用）；
  交叉轴按 `alignment`（per-child `align_self` 覆盖，复用 flex 语义）。
- **容器**：主轴 = 最宽行（wrap）/约束宽（fill）；交叉 = 各行高 + 行间距。
- **RTL**：终镜（复用 flex 模式：容器宽 − x − w）。
- **组件不宏化**（同 Row/Column：content 顶层 `get` 需冒泡给父）；
  `changed` 全参数声明（含 `max_items` + 方向）。

## 3. 与 Compose 的差异（v1 范围）

- 无 weight（Compose 按行内剩余二次分配，需两阶段重测——后续加）。
- 无 maxLines/overflow（情境 API：expand/collapse indicator——后续加）。
- 无 intrinsic（winia 无 intrinsic 体系）。
- 行布局尺寸取 `max(内容, min约束)`：fill 父（min=max）→ 行宽=容器宽，
  行内对齐在容器内；wrap 父（min=0）→ 行宽=内容宽。Compose 行宽恒=内容宽
  （wrap-content 下行为一致；fill 下 Compose 行内 Start 无可见差，
  Center/End/SpaceBetween 在容器内对齐——winia 同）。

## 4. 运行与测试

```bash
cargo run -p winia --example flow_demo
cargo test -p winia --lib layout::flow        # policy 层 7 项
cargo test -p winia --lib ui::layout_components  # 组合链路 2 项
```

测试覆盖：换行/永不换行（无界）/max_items 强制断行/spacing+行内居中/
RTL 镜像/换列/fill-tight 容器宽/组合链路（4 子换行换列）。
