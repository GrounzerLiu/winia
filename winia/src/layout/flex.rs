//! Flex 布局 — 参数化 Column/Row 的共同逻辑
//!
//! Column (垂直主轴) 和 Row (水平主轴) 的 measure 算法结构完全一致，
//! 仅在主轴/交叉轴的坐标映射上有差异。FlexAxis trait 将差异抽象为
//! 编译期泛型参数，消除两处 ~140 行的重复代码。

use super::constraints::Constraints;
use super::node::*;

// ── FlexAxis trait ──

/// 主轴/交叉轴的抽象映射。所有方法均为关联函数，编译期单态化零开销。
pub(crate) trait FlexAxis {
    // ── 尺寸提取 ──
    fn main_size(s: Size) -> f32;
    fn cross_size(s: Size) -> f32;

    // ── 约束提取 ──
    fn main_max(c: &Constraints) -> f32;
    fn cross_max(c: &Constraints) -> f32;

    // ── 约束构造 ──
    fn constrain_main(c: &Constraints, v: f32) -> f32;
    fn constrain_cross(c: &Constraints, v: f32) -> f32;

    /// 构造 Constraints：cross 轴用完整父约束，main 轴 min=0, max=remaining
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints;
    /// 构造 Constraints：main 轴固定=allocated，cross 轴: min 由 stretch 决定, max=父约束
    ///
    /// `fill_main` is Compose's `LayoutWeightParentData.fill`: the share is an exact main-axis size
    /// when true and only a MAXIMUM when false — `createConstraints(mainAxisMin = if
    /// (parentData.fill) childMainAxisSize else 0, mainAxisMax = childMainAxisSize,
    /// isPrioritizing = true)` (`RowColumnMeasurePolicy.kt:195-207`).
    ///
    /// Two things in that call are deliberately not modelled, both checked against the source:
    ///
    /// - `isPrioritizing = true` selects `Constraints.fitPrioritizingWidth/Height` instead of a plain
    ///   `Constraints(...)`, and that function is about BIT PACKING, not layout: Compose's
    ///   `Constraints` is a value class over a `Long` with 18 bits for its larger dimension and 13 for
    ///   the smaller (262,143 and 8,191), so the main axis is granted the large budget and the cross
    ///   axis is clamped to what is left ("The width is granted as much space as it needs or caps the
    ///   size to 18 bits. The height is given the remaining space" — `ui-unit` `Constraints.kt:275-313`).
    ///   winia's `Constraints` is four `f32`s with no packing, so there is no budget to prioritise and
    ///   the flag has nothing to mean. It is not a semantics difference to align.
    /// - Compose quantises a weighted child's share to integer pixels and hands the rounding error to
    ///   the earliest children — `weightUnitSpace = remainingToTarget / totalWeight`, then
    ///   `childMainAxisSize = max(0, (weightUnitSpace * weight).fastRoundToInt() + remainderUnit)` with
    ///   `remainderUnit = remainder.sign` decremented per child (`:169-193`). winia allocates
    ///   `remaining * weight / total_weight` as an `f32` and never rounds, so its children fill the
    ///   axis exactly in logical pixels where Compose fills it exactly in device pixels. Matching the
    ///   rounding would mean moving the whole lay-out to integers, against winia's float model.
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints;

    // ── 值构造 ──
    fn size(main: f32, cross: f32) -> Size;
    fn point(main: f32, cross: f32) -> Point;

    // ── RTL ──
    /// RTL 镜像时的容器宽度（x 轴范围）——水平主轴用行自身测量宽度，
    /// 垂直主轴（Column）用交叉轴宽度
    fn rtl_container_width(measured_main: f32, cross_size: f32) -> f32;
}

// ── 实现：垂直主轴 (Column) ──

pub(crate) struct VerticalAxis;

impl FlexAxis for VerticalAxis {
    #[inline] fn main_size(s: Size) -> f32 { s.height }
    #[inline] fn cross_size(s: Size) -> f32 { s.width }

    #[inline] fn main_max(c: &Constraints) -> f32 { c.max_height }
    #[inline] fn cross_max(c: &Constraints) -> f32 { c.max_width }

    #[inline] fn constrain_main(c: &Constraints, v: f32) -> f32 { c.constrain_height(v) }
    #[inline] fn constrain_cross(c: &Constraints, v: f32) -> f32 { c.constrain_width(v) }

    #[inline]
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints {
        Constraints {
            // 交叉轴松 min（对齐 Compose Column：子节点不继承父 tight 宽度——
            // 否则 fill_max_size 父把 tight 传给所有子，子宽度被强制撑满）
            min_width: 0.0,
            max_width: c.max_width,
            min_height: 0.0,
            max_height: main_remaining.max(0.0),
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: if stretch_cross { c.min_width } else { 0.0 },
            max_width: c.max_width,
            min_height: if fill_main { allocated } else { 0.0 },
            max_height: allocated,
        }
    }

    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(cross, main) }
    #[inline] fn point(main: f32, cross: f32) -> Point { Point::new(cross, main) }

    #[inline]
    fn rtl_container_width(_measured_main: f32, cross_size: f32) -> f32 {
        cross_size
    }
}

// ── 实现：水平主轴 (Row) ──

pub(crate) struct HorizontalAxis;

impl FlexAxis for HorizontalAxis {
    #[inline] fn main_size(s: Size) -> f32 { s.width }
    #[inline] fn cross_size(s: Size) -> f32 { s.height }

    #[inline] fn main_max(c: &Constraints) -> f32 { c.max_width }
    #[inline] fn cross_max(c: &Constraints) -> f32 { c.max_height }

    #[inline] fn constrain_main(c: &Constraints, v: f32) -> f32 { c.constrain_width(v) }
    #[inline] fn constrain_cross(c: &Constraints, v: f32) -> f32 { c.constrain_height(v) }

    #[inline]
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: main_remaining.max(0.0),
            // 交叉轴松 min（对齐 Compose Row：子节点不继承父 tight 高度）
            min_height: 0.0,
            max_height: c.max_height,
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: if fill_main { allocated } else { 0.0 },
            max_width: allocated,
            min_height: if stretch_cross { c.min_height } else { 0.0 },
            max_height: c.max_height,
        }
    }

    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(main, cross) }
    #[inline] fn point(main: f32, cross: f32) -> Point { Point::new(main, cross) }

    #[inline]
    fn rtl_container_width(measured_main: f32, _cross_size: f32) -> f32 {
        measured_main
    }
}

// ── 参数化 measure ──

/// 参数化 flex 测量。Column 和 Row 的 `MeasurePolicy::measure` 直接委托到此函数。
pub(crate) fn measure_flex<A: FlexAxis>(
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
    direction: LayoutDirection,
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    children: &[usize],
    constraints: &Constraints,
) -> (Size, Vec<Placement>) {
    let n = children.len();
    if n == 0 {
        return (
            Size::new(
                constraints.constrain_width(0.0),
                constraints.constrain_height(0.0),
            ),
            Vec::new(),
        );
    }

    // ── per-child 属性 ──
    let weights: Vec<Option<f32>> = children.iter().map(|&c| nodes[c].modifier.get_layout_weight()).collect();
    // Compose's `LayoutWeightParentData.fill`. A node with a weight but no explicit flag fills,
    // which is what `Modifier::layout_weight` has always meant here.
    let fills: Vec<bool> = children.iter()
        .map(|&c| nodes[c].modifier.get_layout_weight_fill().unwrap_or(true))
        .collect();
    let aligns: Vec<Alignment> = children.iter()
        .map(|&c| nodes[c].modifier.get_align_self().unwrap_or(alignment))
        .collect();
    let total_spacing = spacing * (n as f32 - 1.0).max(0.0);

    // ── Phase 1: 无 weight 子节点 ──
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    // Main-axis size a WEIGHTED child is placed at. The parent allocates that share
    // (Compose measures a weighted child with fixed main constraints and keeps the
    // slot), so it must NOT come from the child's measured/returned size: a flight
    // layout override replaces that return with the placeholder size the parent was
    // told, which would otherwise let a weighted hero shrink to its natural width
    // mid-flight while a trailing sibling slides over it.
    let mut allocated_main: Vec<Option<f32>> = vec![None; n];
    let mut total_fixed_main: f32 = 0.0;
    let mut max_cross: f32 = 0.0;
    let mut total_weight: f32 = 0.0;

    for (i, &c) in children.iter().enumerate() {
        if let Some(w) = weights[i] {
            total_weight += w;
            continue;
        }
        // Every preceding NON-WEIGHTED child charges its gap, whatever it measured — Compose adds
        // `spaceAfterLastNoWeight` after each of them (`RowColumnMeasurePolicy.kt:143-145`), an empty
        // one included. winia used to count only the children that measured non-zero, which handed
        // the following child more room than Compose gives it: measured, a 100 dp column with a 30 dp
        // spacing after a 0x0 child left the next child its full 100 where Compose measures it
        // against 70.
        //
        // Weighted children cannot be counted here — they have not been measured yet — and their
        // spacing is charged to the weighted allocation instead. Compose does the same: its
        // `fixedSpace` only ever sees non-weighted children, and the weighted pool subtracts
        // `arrangementSpacingTotal` for them separately (`:163-165`).
        let preceding_non_weighted = weights[..i].iter().filter(|w| w.is_none()).count() as f32;
        let spacing_deduct = preceding_non_weighted * spacing;
        let main_remaining = A::main_max(constraints) - total_fixed_main - spacing_deduct;

        let cc = A::build_phase1(constraints, main_remaining);
        let (size, _) = measure_node(nodes, policies, c, cc);
        total_fixed_main += A::main_size(size);
        max_cross = max_cross.max(A::cross_size(size));
        child_sizes[i] = size;
    }

    // 剩余空间
    let remaining = if A::main_max(constraints).is_finite() {
        (A::main_max(constraints) - total_fixed_main - total_spacing).max(0.0)
    } else {
        0.0
    };

    // ── Phase 2: weight 子节点 ──
    for (i, &c) in children.iter().enumerate() {
        if let Some(w) = weights[i] {
            let allocated = if total_weight > 0.0 { remaining * w / total_weight } else { 0.0 };
            // The allocation only wins when the parent actually HAD space to allocate (a
            // bounded main axis with a positive total weight). With an unbounded axis, or
            // weight <= 0, `allocated` is 0 while the child's own measurement is not, and
            // placing it in a 0-slot made it vanish — measured pre/post: a
            // `weight(0) + size(100)` child beside a 50px sibling went from 100@0 / 50@100
            // to 0@0 / 50@0 (overlapping, the row no longer containing its children).
            // Compose rejects `weight <= 0`; we fall back to the child's measurement.
            //
            // `fill = false` also falls back to the child's own measurement, and that is the whole
            // point of the flag: Compose measures such a child with its share as the MAXIMUM and
            // then adds the size the child asked for to `weightedSpace`, so a short child leaves the
            // container short instead of stretching it to the share.
            allocated_main[i] = if fills[i] && A::main_max(constraints).is_finite() && total_weight > 0.0 {
                Some(allocated)
            } else {
                None
            };
            let stretch_cross = alignment == Alignment::Stretch || aligns[i] == Alignment::Stretch;
            let cc = A::build_phase2(constraints, allocated, fills[i], stretch_cross);
            let (size, _) = measure_node(nodes, policies, c, cc);
            max_cross = max_cross.max(A::cross_size(size));
            child_sizes[i] = size;
        }
    }

    // ── Phase 3: 尺寸与布局 ──
    let total_content_main: f32 = child_sizes
        .iter()
        .enumerate()
        .map(|(i, s)| allocated_main[i].unwrap_or_else(|| A::main_size(*s)))
        .sum::<f32>()
        + total_spacing;

    // The arrangement spreads the space the container ACTUALLY ends up with, so the container's main size
    // is resolved first and the leftover is derived from it. Two things make that subtle:
    //
    //  - The unbounded sentinel is `f32::MAX`, which IS finite: the old `main_max().is_finite()` guard let
    //    it through, so `remaining` came out as `f32::MAX` and `Arrangement::Center` placed the child at
    //    1.7e38. Measured on a menu item's label (`pos:[12,170141173319319264429905852091742258462720]`).
    //  - A container that carries a MINIMUM (a 48dp menu item whose label is 20px) really does have space
    //    to distribute, which only shows up once the constrained size is used instead of the content size.
    //
    // The size is the CONTENT size, floored by the incoming minimum — never the maximum the parent offers.
    // Compose resolves it the same way: `mainAxisLayoutSize = max((fixedSpace + weightedSpace)
    // .fastCoerceAtLeast(0), mainAxisMin)` (`RowColumnMeasurePolicy.kt:252`), where `fixedSpace` only
    // accumulates the children measured and `mainAxisMax` is never consulted. A spreading arrangement on a
    // container with no explicit size therefore hugs its content and has no leftover to spread, which is
    // why `Row(horizontalArrangement = SpaceBetween)` needs `fillMaxWidth()` to push children apart.
    //
    // winia used to grow such a container to the maximum the parent offered
    // (`total_content_main + (main_max - total_content_main).max(0.0)`) — a deliberate convenience, "how
    // the demos spread a bar across their container". It was not worth the divergence: it made
    // `SpaceBetween` behave unlike Compose everywhere, and it silently defeated the date picker dialog's
    // `weight(1f, fill = false)` collapse by holding the dialog at its 568 dp cap. Removing it cost
    // nothing — every call site that wanted the spreading had already said so with an explicit
    // `fill_max_width` (`layout_demo`'s rows through their shared `arr_pad`, the date picker's nine
    // header/weekday/grid rows, `swipe_to_dismiss_demo`'s label row), which is the Compose idiom anyway.
    let measured_main = A::constrain_main(constraints, total_content_main);
    let remaining_main = (measured_main - total_content_main).max(0.0);
    let gap_count = if n > 1 { n - 1 } else { 0 };
    let (spacing_extra, leading_space) = compute_spacing(arrangement, remaining_main, gap_count);
    let effective_spacing = spacing + spacing_extra;

    // ── 交叉轴最终尺寸 ──
    //
    // Compose's alignment-line pass (`RowColumnMeasurePolicy.kt:228-251`) runs before the size is
    // taken: a child aligned by one of its lines contributes that line's distance from its own top
    // (`beforeCrossAxisAlignmentLine`) and everything below it (`afterCrossAxisAlignmentLine`), and
    // the cross axis must be at least the two added together so a line placed at `before` still has
    // room for the tallest child under it. `line_before` is where every line-aligned child's line
    // lands; children without a line ignore it.
    let mut line_before = 0.0f32;
    let mut line_after = 0.0f32;
    for (i, &c) in children.iter().enumerate() {
        let Some(line) = nodes[c].modifier.get_align_by() else { continue };
        let child_cross = A::cross_size(child_sizes[i]);
        let position = nodes[c].alignment_line(line);
        line_before = line_before.max(position.unwrap_or(0.0));
        // An unspecified line is treated as the whole child hanging below the line, as Compose does.
        line_after = line_after.max(child_cross - position.unwrap_or(child_cross));
    }

    let cross_size = if alignment == Alignment::Stretch && A::cross_max(constraints) < f32::MAX {
        A::cross_max(constraints)
    } else {
        A::constrain_cross(constraints, max_cross.max(line_before + line_after))
    };

    // 放置子节点
    let mut placements = Vec::with_capacity(n);
    let mut main_offset = leading_space;

    for (i, child_size) in child_sizes.iter().enumerate() {
        let align = aligns[i];
        let line = nodes[children[i]].modifier.get_align_by();
        // A line-aligned child is placed by its line, not by an edge, so it keeps its own cross size
        // even under a stretching parent — Compose's `getCrossAxisPosition` asks the line first and
        // only falls back to the parent's alignment (`Row.kt:216-231`).
        let child_cross = if align == Alignment::Stretch && line.is_none() {
            cross_size
        } else {
            A::cross_size(*child_size)
        };
        // A weighted child keeps its allocated slot (see `allocated_main`).
        let child_main = allocated_main[i].unwrap_or_else(|| A::main_size(*child_size));
        let cross_offset = match line {
            // `beforeCrossAxisAlignmentLine - alignmentLinePosition`: every line-aligned child's
            // line lands on the same cross-axis position.
            Some(line) => {
                line_before - nodes[children[i]].alignment_line(line).unwrap_or(0.0)
            }
            None => match align {
                Alignment::Start => 0.0,
                Alignment::End => cross_size - child_cross,
                Alignment::Center => (cross_size - child_cross) / 2.0,
                Alignment::Stretch => 0.0,
            },
        };
        placements.push(Placement {
            size: A::size(child_main, child_cross),
            position: A::point(main_offset, cross_offset),
        });
        main_offset += child_main + effective_spacing;
    }

    // ── RTL 镜像 ──
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(measured_main, cross_size);
        for p in &mut placements {
            p.position.x = container_x - p.position.x - p.size.width;
        }
    }

    (A::size(measured_main, cross_size), placements)
}

// ── 固有尺寸（Compose 的 IntrinsicMeasureBlocks）──
//
// Row/Column 是 Compose 里唯一不用 `MeasurePolicy` 默认近似、而是四个查询全覆写的容器：
// `weight` 需要与测量相位同一套算术（RowColumnImpl.kt:371-452）。下面两个函数就是那段的移植，
// 查询面由 row.rs / column.rs 按 IntrinsicMeasureBlocks 的对应表传入
// （RowColumnImpl.kt:261-369）。

/// Compose 的 `intrinsicMainAxisSize`（RowColumnImpl.kt:371-394）。
///
/// 无 weight 的子节点贡献自己的主轴固有尺寸；有 weight 的子节点**没有**固有尺寸——它由父节点
/// 分配一份空间——所以容器改为给"加权集合"定价：最大的 weight 单位（该子节点主轴尺寸 / 它的
/// weight）乘以总 weight。这正是 `weight(1f)` 的标签在 Column 里仍能报出有限固有高度的原因：
/// 少了这段算术，标签会把约束原样报回，容器于是报最大值而不是内容宽度
/// （菜单的两遍测量当年正是踩了这个坑，见 `docs/dropdown-menu.md` §4.2）。
pub(crate) fn flex_intrinsic_main(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    main_query: IntrinsicQuery,
    cross_axis_available: f32,
    spacing: f32,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let mut fixed_space = 0.0f32;
    let mut weight_unit_space = 0.0f32;
    let mut total_weight = 0.0f32;
    for &c in children {
        let weight = ctx.child_weight(c);
        let size = ctx.child_intrinsic(c, main_query, cross_axis_available);
        if weight > 0.0 {
            total_weight += weight;
            weight_unit_space = weight_unit_space.max(size / weight);
        } else {
            fixed_space += size;
        }
    }
    weight_unit_space * total_weight + fixed_space + spacing * (children.len() as f32 - 1.0).max(0.0)
}

/// Compose 的 `intrinsicCrossAxisSize`（RowColumnImpl.kt:396-452）。
///
/// 交叉轴答案必须先知道每个子节点的**主轴**空间：子节点的交叉尺寸是按它将占用的主轴空间定价的。
/// 无 weight 的子节点取"无界主轴固有尺寸"与容器剩余空间的较小者；有 weight 的子节点各占一个
/// weight 单位。这里的 `main_query` 是主轴的 **Max** 查询——Compose 的每个交叉轴块都拿主轴
/// `maxIntrinsic*` 配对（RowColumnImpl.kt:281-355），因为拿到更多空间的子节点也可能在交叉轴上变大。
pub(crate) fn flex_intrinsic_cross(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    main_query: IntrinsicQuery,
    cross_query: IntrinsicQuery,
    main_axis_available: f32,
    spacing: f32,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let unbounded = main_axis_available >= f32::MAX;
    let spacing_total = spacing * (children.len() as f32 - 1.0).max(0.0);
    // Compose: `fixedSpace = min((n - 1) * mainAxisSpacing, mainAxisAvailable)`。
    let mut fixed_space = if unbounded {
        spacing_total
    } else {
        spacing_total.min(main_axis_available)
    };
    let mut cross_axis_max = 0.0f32;
    let mut total_weight = 0.0f32;

    for &c in children {
        let weight = ctx.child_weight(c);
        if weight > 0.0 {
            total_weight += weight;
            continue;
        }
        // 问子节点想要多少主轴空间——但绝不会超过剩余可用空间。
        let remaining = if unbounded {
            f32::MAX
        } else {
            (main_axis_available - fixed_space).max(0.0)
        };
        let main_axis_space = ctx.child_intrinsic(c, main_query, f32::MAX).min(remaining);
        fixed_space += main_axis_space;
        cross_axis_max = cross_axis_max.max(ctx.child_intrinsic(c, cross_query, main_axis_space));
    }

    // weight=1 代表多少主轴空间（无界时 Compose 用 Infinity → 交叉轴按无界问）。
    let weight_unit_space = if total_weight == 0.0 {
        0.0
    } else if unbounded {
        f32::MAX
    } else {
        (main_axis_available - fixed_space).max(0.0) / total_weight
    };
    for &c in children {
        let weight = ctx.child_weight(c);
        if weight > 0.0 {
            let main_space = if weight_unit_space >= f32::MAX {
                f32::MAX
            } else {
                weight_unit_space * weight
            };
            cross_axis_max = cross_axis_max.max(ctx.child_intrinsic(c, cross_query, main_space));
        }
    }
    cross_axis_max
}
