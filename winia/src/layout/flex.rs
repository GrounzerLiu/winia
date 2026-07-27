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
    fn build_phase2(c: &Constraints, allocated: f32, stretch_cross: bool) -> Constraints;

    // ── 值构造 ──
    fn size(main: f32, cross: f32) -> Size;
    fn point(main: f32, cross: f32) -> Point;

    // ── RTL ──
    /// RTL 镜像时的容器宽度（x 轴范围）
    fn rtl_container_width(
        constraints: &Constraints,
        total_content_main: f32,
        remaining_main: f32,
        cross_size: f32,
    ) -> f32;
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
            min_width: c.min_width,
            max_width: c.max_width,
            min_height: 0.0,
            max_height: main_remaining.max(0.0),
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: if stretch_cross { c.min_width } else { 0.0 },
            max_width: c.max_width,
            min_height: allocated,
            max_height: allocated,
        }
    }

    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(cross, main) }
    #[inline] fn point(main: f32, cross: f32) -> Point { Point::new(cross, main) }

    #[inline]
    fn rtl_container_width(
        _c: &Constraints,
        _total_content_main: f32,
        _remaining_main: f32,
        cross_size: f32,
    ) -> f32 {
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
            min_height: c.min_height,
            max_height: c.max_height,
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: allocated,
            max_width: allocated,
            min_height: if stretch_cross { c.min_height } else { 0.0 },
            max_height: c.max_height,
        }
    }

    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(main, cross) }
    #[inline] fn point(main: f32, cross: f32) -> Point { Point::new(main, cross) }

    #[inline]
    fn rtl_container_width(
        constraints: &Constraints,
        total_content_main: f32,
        remaining_main: f32,
        _cross_size: f32,
    ) -> f32 {
        constraints.constrain_width(total_content_main + remaining_main)
    }
}

// ── 参数化 measure ──

/// 参数化 flex 测量。Column 和 Row 的 `MeasurePolicy::measure` 直接委托到此函数。
pub(crate) fn measure_flex<A: FlexAxis>(
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
    direction: LayoutDirection,
    children: &mut [LayoutNode],
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
    let weights: Vec<Option<f32>> = children.iter().map(|c| c.modifier.layout_weight()).collect();
    let aligns: Vec<Alignment> = children.iter()
        .map(|c| c.modifier.align_self().unwrap_or(alignment))
        .collect();
    let total_spacing = spacing * (n as f32 - 1.0).max(0.0);

    // ── Phase 1: 无 weight 子节点 ──
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    let mut total_fixed_main: f32 = 0.0;
    let mut max_cross: f32 = 0.0;
    let mut total_weight: f32 = 0.0;

    for (i, child) in children.iter_mut().enumerate() {
        if let Some(w) = weights[i] {
            total_weight += w;
            continue;
        }
        // 扣除已测节点占用的间距
        let measured_count = child_sizes[..i]
            .iter()
            .filter(|s| s.width > 0.0 || s.height > 0.0)
            .count() as f32;
        let spacing_deduct = measured_count * spacing;
        let main_remaining = A::main_max(constraints) - total_fixed_main - spacing_deduct;

        let cc = A::build_phase1(constraints, main_remaining);
        let (size, _) = measure_node(child, cc);
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
    for (i, child) in children.iter_mut().enumerate() {
        if let Some(w) = weights[i] {
            let allocated = if total_weight > 0.0 { remaining * w / total_weight } else { 0.0 };
            let stretch_cross = alignment == Alignment::Stretch || aligns[i] == Alignment::Stretch;
            let cc = A::build_phase2(constraints, allocated, stretch_cross);
            let (size, _) = measure_node(child, cc);
            max_cross = max_cross.max(A::cross_size(size));
            child_sizes[i] = size;
        }
    }

    // ── Phase 3: 尺寸与布局 ──
    let total_content_main: f32 =
        child_sizes.iter().map(|s| A::main_size(*s)).sum::<f32>() + total_spacing;

    let remaining_main = if A::main_max(constraints).is_finite() {
        (A::main_max(constraints) - total_content_main).max(0.0)
    } else {
        0.0
    };
    let gap_count = if n > 1 { n - 1 } else { 0 };
    let (spacing_extra, leading_space) = compute_spacing(arrangement, remaining_main, gap_count);
    let effective_spacing = spacing + spacing_extra;

    // 交叉轴最终尺寸
    let cross_size = if alignment == Alignment::Stretch && A::cross_max(constraints) < f32::MAX {
        A::cross_max(constraints)
    } else {
        A::constrain_cross(constraints, max_cross)
    };

    // 放置子节点
    let mut placements = Vec::with_capacity(n);
    let mut main_offset = leading_space;

    for (i, child_size) in child_sizes.iter().enumerate() {
        let align = aligns[i];
        let child_cross = if align == Alignment::Stretch { cross_size } else { A::cross_size(*child_size) };
        let child_main = A::main_size(*child_size);
        let cross_offset = match align {
            Alignment::Start => 0.0,
            Alignment::End => cross_size - child_cross,
            Alignment::Center => (cross_size - child_cross) / 2.0,
            Alignment::Stretch => 0.0,
        };
        placements.push(Placement {
            size: A::size(child_main, child_cross),
            position: A::point(main_offset, cross_offset),
        });
        main_offset += child_main + effective_spacing;
    }

    // ── RTL 镜像 ──
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(
            constraints, total_content_main, remaining_main, cross_size,
        );
        for p in &mut placements {
            p.position.x = container_x - p.position.x - p.size.width;
        }
    }

    // 最终测量尺寸
    let measured_main = match arrangement {
        Arrangement::SpaceBetween | Arrangement::SpaceAround | Arrangement::SpaceEvenly => {
            A::constrain_main(constraints, total_content_main + remaining_main)
        }
        _ => A::constrain_main(constraints, total_content_main),
    };

    (A::size(measured_main, cross_size), placements)
}
