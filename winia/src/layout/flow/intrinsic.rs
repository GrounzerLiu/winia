//! Intrinsic measurement for the flow containers.
//!
//! Compose answers the four intrinsic queries from the children's intrinsic sizes, not by running
//! the normal measure pass: the main axis is folded line by line with the same spacing/max-items/
//! max-lines rules, while the cross axis is the sum of the line heights. This is the pure-data
//! counterpart of `measure_flow`; it never places anything.

use super::FlowOverflow;
use crate::layout::{IntrinsicCtx, IntrinsicQuery};

fn main_query(horizontal: bool, min: bool) -> IntrinsicQuery {
    match (horizontal, min) {
        (true, true) => IntrinsicQuery::MinWidth,
        (true, false) => IntrinsicQuery::MaxWidth,
        (false, true) => IntrinsicQuery::MinHeight,
        (false, false) => IntrinsicQuery::MaxHeight,
    }
}

fn cross_query(horizontal: bool, min: bool) -> IntrinsicQuery {
    match (horizontal, min) {
        (true, true) => IntrinsicQuery::MinHeight,
        (true, false) => IntrinsicQuery::MaxHeight,
        (false, true) => IntrinsicQuery::MinWidth,
        (false, false) => IntrinsicQuery::MaxWidth,
    }
}

fn child_sizes(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    min: bool,
    other: f32,
) -> (Vec<f32>, Vec<f32>) {
    let main = main_query(horizontal, min);
    let cross = cross_query(horizontal, min);
    let mut main_sizes = Vec::with_capacity(children.len());
    let mut cross_sizes = Vec::with_capacity(children.len());
    for &child in children {
        let m = ctx.child_intrinsic(child, main, other);
        let c = ctx.child_intrinsic(child, cross, m);
        main_sizes.push(m);
        cross_sizes.push(c);
    }
    (main_sizes, cross_sizes)
}

fn max_main_without_wrapping(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    other: f32,
    spacing: f32,
    max_items: usize,
    max_lines: usize,
) -> f32 {
    if children.is_empty() || max_lines == 0 {
        return 0.0;
    }
    let query = main_query(horizontal, false);
    let cap = max_items.max(1);
    let mut best = 0.0f32;
    let mut line = 0.0f32;
    let mut count = 0usize;
    for (i, &child) in children.iter().enumerate() {
        if count > 0 {
            line += spacing;
        }
        line += ctx.child_intrinsic(child, query, other);
        count += 1;
        if count >= cap || i + 1 == children.len() {
            best = best.max(line);
            line = 0.0;
            count = 0;
        }
    }
    best
}

/// The cross extent when the flow is broken at `main_available`, using precomputed intrinsic
/// sizes. `clip` applies max_lines/overflow; `SpaceBetween`-style distribution is irrelevant to
/// an intrinsic size.
fn cross_size(
    main_sizes: &[f32],
    cross_sizes: &[f32],
    main_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    clip: bool,
) -> f32 {
    if main_sizes.is_empty() || max_rows == 0 {
        return 0.0;
    }
    let cap = max_items.max(1);
    let row_cap = if clip { max_rows } else { usize::MAX };
    let mut total = 0.0f32;
    let mut line_cross = 0.0f32;
    let mut remaining = main_available;
    let mut count = 0usize;
    let mut rows = 1usize;
    for i in 0..main_sizes.len() {
        let need = if count > 0 { main_spacing + main_sizes[i] } else { main_sizes[i] };
        let wrap = count > 0 && (count >= cap || remaining < need);
        if wrap {
            if rows >= row_cap {
                break;
            }
            total += line_cross + cross_spacing;
            rows += 1;
            line_cross = 0.0;
            remaining = main_available;
            count = 0;
        }
        remaining -= need;
        line_cross = line_cross.max(cross_sizes[i]);
        count += 1;
    }
    total + line_cross
}

fn cross_intrinsic(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    min: bool,
    main_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    overflow: FlowOverflow,
) -> f32 {
    let (main_sizes, cross_sizes) = child_sizes(ctx, children, horizontal, min, main_available);
    cross_size(
        &main_sizes,
        &cross_sizes,
        main_available,
        main_spacing,
        cross_spacing,
        max_rows,
        max_items,
        overflow != FlowOverflow::Visible,
    )
}

/// Minimum main-axis size that keeps the flow within `cross_available`.
#[allow(clippy::too_many_arguments)]
fn min_main_intrinsic(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    cross_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    overflow: FlowOverflow,
) -> f32 {
    if children.is_empty() || max_rows == 0 {
        return 0.0;
    }
    let (main_sizes, cross_sizes) = child_sizes(ctx, children, horizontal, true, cross_available);
    let low = main_sizes.iter().fold(0.0f32, |a, &b| a.max(b));
    let high = main_sizes.iter().sum::<f32>()
        + main_spacing * (children.len() as f32 - 1.0).max(0.0);
    if cross_available >= f32::MAX {
        return low;
    }
    let clip = overflow != FlowOverflow::Visible;
    let mut lo = low;
    let mut hi = high;
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let used = cross_size(
            &main_sizes,
            &cross_sizes,
            mid,
            main_spacing,
            cross_spacing,
            max_rows,
            max_items,
            clip,
        );
        if used > cross_available {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

pub(crate) fn min_main(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    cross_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    overflow: FlowOverflow,
) -> f32 {
    min_main_intrinsic(
        ctx,
        children,
        horizontal,
        cross_available,
        main_spacing,
        cross_spacing,
        max_rows,
        max_items,
        overflow,
    )
}

pub(crate) fn max_main(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    other: f32,
    spacing: f32,
    max_items: usize,
    max_lines: usize,
) -> f32 {
    max_main_without_wrapping(ctx, children, horizontal, other, spacing, max_items, max_lines)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn min_cross(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    main_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    overflow: FlowOverflow,
) -> f32 {
    cross_intrinsic(
        ctx,
        children,
        horizontal,
        true,
        main_available,
        main_spacing,
        cross_spacing,
        max_rows,
        max_items,
        overflow,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn max_cross(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    horizontal: bool,
    main_available: f32,
    main_spacing: f32,
    cross_spacing: f32,
    max_rows: usize,
    max_items: usize,
    overflow: FlowOverflow,
) -> f32 {
    cross_intrinsic(
        ctx,
        children,
        horizontal,
        false,
        main_available,
        main_spacing,
        cross_spacing,
        max_rows,
        max_items,
        overflow,
    )
}
