//! Regression tests for the flex layout review.
//!
//! Four behaviours are pinned here, each one measured wrong before the fix:
//!
//! - a weighted child must not explode when the main axis is unbounded. winia has TWO unbounded
//!   sentinels — `Constraints::UNBOUNDED` uses `f32::INFINITY`, and a scroll container writes
//!   `f32::MAX` for its content (`layout/node/measure/engine.rs`). `f32::MAX` is finite, so the
//!   weight allocation used to hand the child a 3.4e38 share and drag the whole container with it;
//! - `SpaceBetween` with a single child packs it at the start, not the centre;
//! - a non-positive weight means "no weight" and the child measures like any other sibling;
//! - a container whose content overflows reports the CONTENT size, not the incoming maximum.

use winia::layout::{Arrangement, Column, Constraints, LayoutDirection, Row};
use winia::prelude::*;
use winia::runtime::composer::Composer;

fn fixed(w: f32, h: f32) -> Modifier {
    Modifier::new().size(w, h)
}

/// A column of [fixed 20-high sibling, weighted child] measured under `max_h`.
fn weighted_column(max_h: f32) -> (f32, f32) {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("hi").modifier(fixed(50.0, 20.0)).build(ctx);
            Text::new("weighted")
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 200.0, 0.0, max_h));
    let root = composer.layout_root().unwrap();
    let weighted = composer.arena_nodes()[root.children[1]].measured_size.height;
    (root.measured_size.height, weighted)
}

/// `f32::MAX` is what a scroll container hands its content. A weighted child must keep the zero
/// share Compose gives it on an unbounded axis, not `f32::MAX`.
#[test]
fn a_weighted_child_on_the_f32_max_axis_keeps_the_axis_content_sized() {
    let (column, weighted) = weighted_column(f32::MAX);
    assert_eq!(column, 20.0, "the column is its content, not 3.4e38");
    assert_eq!(weighted, 0.0, "an unbounded main axis gives a weighted child nothing");
}

/// The other sentinel, `f32::INFINITY`, already behaved; pin it so the two stay together.
#[test]
fn a_weighted_child_on_the_infinite_axis_keeps_the_axis_content_sized() {
    let (column, weighted) = weighted_column(f32::INFINITY);
    assert_eq!(column, 20.0);
    assert_eq!(weighted, 0.0);
}

/// A bounded axis still allocates the whole remainder to the weighted child.
#[test]
fn a_weighted_child_still_fills_a_bounded_axis() {
    let (column, weighted) = weighted_column(300.0);
    assert_eq!(column, 300.0);
    assert_eq!(weighted, 280.0, "300 less the fixed sibling");
}

/// The end-to-end shape the bug was reachable through: a weighted child inside the content of a
/// `vertical_scroll` Column. The scroll container itself keeps its viewport height.
#[test]
fn a_weighted_child_in_a_scrolling_column_does_not_inflate_the_content() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_height().vertical_scroll(ScrollState::new()))
            .build(ctx, |ctx| {
                Column::new().build(ctx, |ctx| {
                    Text::new("hi").modifier(fixed(50.0, 20.0)).build(ctx);
                    Text::new("weighted")
                        .modifier(Modifier::new().layout_weight(1.0))
                        .build(ctx);
                });
            });
    }));
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 300.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(root.measured_size.height, 300.0, "the scroll container keeps its viewport");
    let content = composer.layout_root().unwrap().children[0];
    assert_eq!(
        composer.arena_nodes()[content].measured_size.height, 20.0,
        "the scrolling content is its content"
    );
}

/// The same through a horizontal scroll, where the encoded sentinel is `max_width`.
#[test]
fn a_weighted_child_in_a_horizontally_scrolling_row_does_not_inflate_the_content() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Row::new()
            .modifier(Modifier::new().fill_max_width().horizontal_scroll(ScrollState::new()))
            .build(ctx, |ctx| {
                Row::new().build(ctx, |ctx| {
                    Text::new("hi").modifier(fixed(20.0, 50.0)).build(ctx);
                    Text::new("weighted")
                        .modifier(Modifier::new().layout_weight(1.0))
                        .build(ctx);
                });
            });
    }));
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(root.measured_size.width, 300.0, "the scroll container keeps its viewport");
    let content = composer.layout_root().unwrap().children[0];
    assert_eq!(
        composer.arena_nodes()[content].measured_size.width, 20.0,
        "the scrolling content is its content"
    );
}

/// Compose's `placeSpaceBetween` divides by `maxOf(size.lastIndex, 1)`, so one child has one gap
/// and sits at the start. Centring it was winia's own rule.
#[test]
fn space_between_with_one_child_packs_it_at_the_start() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new()
            .arrangement(Arrangement::SpaceBetween)
            .build(ctx, |ctx| {
                Text::new("only").modifier(fixed(50.0, 20.0)).build(ctx);
            });
    }));
    composer.layout(Constraints::new(0.0, 100.0, 100.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(composer.arena_nodes()[root.children[0]].position.y, 0.0);
}

/// Under RTL the same child mirrors to the end edge, which is Compose's reverseInput branch.
#[test]
fn space_between_with_one_child_mirrors_to_the_end_under_rtl() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Row::new()
            .arrangement(Arrangement::SpaceBetween)
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .build(ctx, |ctx| {
                Text::new("only").modifier(fixed(20.0, 50.0)).build(ctx);
            });
    }));
    composer.layout(Constraints::new(100.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(composer.arena_nodes()[root.children[0]].position.x, 80.0);
}

/// Compose rejects `weight <= 0` at the modifier; its measure pass treats it as unweighted. winia
/// accepts the value, so it has to measure the child instead of collapsing it into a zero slot.
#[test]
fn a_zero_weight_child_is_measured_like_an_unweighted_one() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a")
                .modifier(Modifier::new().layout_weight(0.0).size(50.0, 100.0))
                .build(ctx);
            Text::new("b").modifier(fixed(50.0, 50.0)).build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 300.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(composer.arena_nodes()[root.children[0]].measured_size.height, 100.0);
    assert_eq!(root.measured_size.height, 150.0);
}

/// `mainAxisLayoutSize = max(content, mainAxisMin)` never consults `mainAxisMax`: a container
/// whose content overflows reports the content and lets the parent decide what to clip.
///
/// The children have to be `required_size` — winia measures every unweighted child against the
/// REMAINING main axis (as Compose does), so a plain `size` child can never overflow it.
#[test]
fn an_overflowing_column_reports_its_content_height() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").modifier(Modifier::new().required_size(50.0, 60.0)).build(ctx);
            Text::new("b").modifier(Modifier::new().required_size(50.0, 60.0)).build(ctx);
            Text::new("c").modifier(Modifier::new().required_size(50.0, 60.0)).build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(root.measured_size.height, 180.0, "content, not the 100 the parent offered");
}

/// The cross axis follows the same rule: a child that does not wrap overflows the container rather
/// than the container pretending it fits.
#[test]
fn an_overflowing_cross_axis_reports_the_child_size() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("x").modifier(Modifier::new().required_size(150.0, 20.0)).build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(root.measured_size.width, 150.0);
}
// ── RTL ──
//
// The mirror runs AFTER the main-axis pass (`flex.rs`'s RTL block), so every rule above has to hold
// in both directions: the weighted share is allocated in LTR coordinates and mirrored, the cross
// axis mirrors Start/End, and an overflowing content size mirrors with it.

/// A weighted child keeps its share under RTL: the allocation happens before the mirror, so the
/// fixed 20 dp sibling ends up on the right and the weighted child takes the remaining 80 on the
/// left.
#[test]
fn a_weighted_child_keeps_its_share_under_rtl() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Row::new()
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .build(ctx, |ctx| {
                Text::new("fixed").modifier(fixed(20.0, 50.0)).build(ctx);
                Text::new("weighted")
                    .modifier(Modifier::new().layout_weight(1.0))
                    .build(ctx);
            });
    }));
    composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    let fixed = &composer.arena_nodes()[root.children[0]];
    let weighted = &composer.arena_nodes()[root.children[1]];
    assert_eq!(weighted.measured_size.width, 80.0, "the share, not the whole axis");
    assert_eq!(weighted.position.x, 0.0, "the weighted child starts at the left edge in RTL");
    assert_eq!(fixed.position.x, 80.0, "the fixed sibling mirrors to the right");
}

/// Cross-axis `End` mirrors too: in RTL it is the LEFT edge of a Column.
#[test]
fn rtl_mirrors_the_cross_axis_alignment() {
    let column = |direction: LayoutDirection| {
        let mut composer = Composer::new();
        composer.compose(winia::compose!(|ctx| {
            Column::new()
                .alignment(Alignment::End)
                .modifier(Modifier::new().layout_direction(direction))
                .build(ctx, |ctx| {
                    Text::new("x").modifier(fixed(20.0, 10.0)).build(ctx);
                });
        }));
        composer.layout(Constraints::new(100.0, 100.0, 0.0, 100.0));
        let root = composer.layout_root().unwrap();
        composer.arena_nodes()[root.children[0]].position.x
    };
    assert_eq!(column(LayoutDirection::Ltr), 80.0, "End is the right edge in LTR");
    assert_eq!(column(LayoutDirection::Rtl), 0.0, "and the left edge in RTL");
}

/// `SpaceBetween` with two children mirrors its gap: LTR puts them at 0 / 70, RTL at 80 / 0.
#[test]
fn space_between_two_children_mirrors_its_gap_under_rtl() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Row::new()
            .arrangement(Arrangement::SpaceBetween)
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .build(ctx, |ctx| {
                Text::new("a").modifier(fixed(20.0, 10.0)).build(ctx);
                Text::new("b").modifier(fixed(30.0, 10.0)).build(ctx);
            });
    }));
    composer.layout(Constraints::new(100.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(composer.arena_nodes()[root.children[0]].position.x, 80.0);
    assert_eq!(composer.arena_nodes()[root.children[1]].position.x, 0.0);
}

/// An overflowing RTL row reports the content width and mirrors inside it: two `required_size(70)`
/// children make a 140-wide row whose first child lands at 70.
#[test]
fn an_overflowing_rtl_row_mirrors_its_content() {
    let mut composer = Composer::new();
    composer.compose(winia::compose!(|ctx| {
        Row::new()
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .build(ctx, |ctx| {
                Text::new("a").modifier(Modifier::new().required_size(70.0, 10.0)).build(ctx);
                Text::new("b").modifier(Modifier::new().required_size(70.0, 10.0)).build(ctx);
            });
    }));
    composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
    let root = composer.layout_root().unwrap();
    assert_eq!(root.measured_size.width, 140.0);
    assert_eq!(composer.arena_nodes()[root.children[0]].position.x, 70.0);
    assert_eq!(composer.arena_nodes()[root.children[1]].position.x, 0.0);
}
