//! The FlexBox intrinsic walk (FlexBox.kt:1031-1230).
//!
//! Compose overrides all four intrinsic measurements instead of letting the default approximation
//! measure the content: a wrapping flex measured at the query constraints lays the items out for
//! real and answers its RESOLVED line sizes, which is not what "how narrow can this get" asks.
//!
//! The main-axis minimum sums the items when wrapping is off and takes the widest item when it is
//! on; the main-axis maximum breaks at maxItemsInEachLine; a CROSS answer simulates the line breaks
//! against the other axis and adds the lines up.

use super::config::FlexBoxConfig;
use super::policy::FlexBoxLayout;
use crate::layout::node::{IntrinsicCtx, IntrinsicQuery};
use crate::layout::Constraints;

/// A width intrinsic: the main axis when rows, the cross axis when columns. Compose resolves the
/// config with the query bound in place (:1035, :1098).
pub(super) fn width(
    policy: &FlexBoxLayout,
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    height: f32,
    min: bool,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let config = policy.resolve(Constraints::new(0.0, f32::MAX, 0.0, height));
    if config.direction.is_horizontal() {
        main_axis(ctx, children, &config, min, height)
    } else {
        cross_axis(ctx, children, &config, height, min)
    }
}

/// A height intrinsic: the main axis when columns, the cross axis when rows.
pub(super) fn height(
    policy: &FlexBoxLayout,
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    width: f32,
    min: bool,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let config = policy.resolve(Constraints::new(0.0, width, 0.0, f32::MAX));
    if config.direction.is_horizontal() {
        cross_axis(ctx, children, &config, width, min)
    } else {
        main_axis(ctx, children, &config, min, width)
    }
}

/// The MAIN-axis intrinsic (FlexBox.kt:1031-1082 for min, :1155-1185 for max). A wrapping minimum
/// is the widest single item — every item could be alone on its line; a wrapping maximum is the
/// widest line when a maxItemsInEachLine cap forces the breaks.
fn main_axis(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    config: &FlexBoxConfig,
    min: bool,
    other: f32,
) -> f32 {
    let horizontal = config.direction.is_horizontal();
    let gap = config.main_gap(horizontal);
    let query = if horizontal {
        if min { IntrinsicQuery::MinWidth } else { IntrinsicQuery::MaxWidth }
    } else if min {
        IntrinsicQuery::MinHeight
    } else {
        IntrinsicQuery::MaxHeight
    };
    if min {
        let sizes: Vec<f32> = children
            .iter()
            .map(|&child| ctx.child_intrinsic(child, query, other))
            .collect();
        if config.wrap.is_enabled() {
            sizes.iter().copied().fold(0.0f32, f32::max)
        } else {
            sizes.iter().sum::<f32>() + (children.len().saturating_sub(1) as f32) * gap
        }
    } else {
        let cap = if config.wrap.is_enabled() {
            config.max_items_in_each_line.max(1)
        } else {
            usize::MAX
        };
        let mut max_line = 0.0f32;
        let mut line = 0.0f32;
        let mut in_line = 0usize;
        for &child in children {
            let item = ctx.child_intrinsic(child, query, other);
            line += if in_line == 0 { item } else { gap + item };
            in_line += 1;
            if in_line == cap {
                max_line = max_line.max(line);
                line = 0.0;
                in_line = 0;
            }
        }
        max_line.max(line)
    }
}

/// The CROSS-axis intrinsic: simulate the line breaks on the other axis and add up each line's
/// cross extent plus the gaps (FlexBox.kt:1186-1230).
fn cross_axis(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    config: &FlexBoxConfig,
    main_available: f32,
    min: bool,
) -> f32 {
    let horizontal = config.direction.is_horizontal();
    let main_gap = config.main_gap(horizontal);
    let cross_gap = config.cross_gap(horizontal);
    let (main_query, cross_query) = if horizontal {
        (
            if min { IntrinsicQuery::MinWidth } else { IntrinsicQuery::MaxWidth },
            if min { IntrinsicQuery::MinHeight } else { IntrinsicQuery::MaxHeight },
        )
    } else {
        (
            if min { IntrinsicQuery::MinHeight } else { IntrinsicQuery::MaxHeight },
            if min { IntrinsicQuery::MinWidth } else { IntrinsicQuery::MaxWidth },
        )
    };
    let mut line_main = 0.0f32;
    let mut line_cross = 0.0f32;
    let mut total = 0.0f32;
    let mut in_line = 0usize;
    for &child in children {
        let item_main = ctx.child_intrinsic(child, main_query, f32::MAX);
        let item_cross = ctx.child_intrinsic(child, cross_query, item_main);
        let projected = if in_line == 0 {
            item_main
        } else {
            line_main + main_gap + item_main
        };
        let breaks = config.wrap.is_enabled()
            && in_line > 0
            && (in_line >= config.max_items_in_each_line || projected > main_available);
        if breaks {
            total += line_cross + cross_gap;
            line_main = item_main;
            line_cross = item_cross;
            in_line = 1;
        } else {
            line_main = projected;
            line_cross = line_cross.max(item_cross);
            in_line += 1;
        }
    }
    total + line_cross
}
