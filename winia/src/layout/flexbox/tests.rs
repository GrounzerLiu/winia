//! FlexBox's tests: the pipeline cases, the intrinsic walk and the wiring above it.

use super::*;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement};
use crate::layout::Constraints;
use crate::modifier::Modifier;
use crate::unit::{Offset, Size};

fn leaf(width: f32, height: f32) -> LayoutNode {
    let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
    node.measured_size = Size::new(width, height);
    node
}

fn item_leaf(width: f32, height: f32, config: FlexItemConfig) -> LayoutNode {
    let mut node = LayoutNode::leaf(Modifier::new().size(width, height).flex(config));
    node.measured_size = Size::new(width, height);
    node
}

fn run(
    config: FlexBoxConfig,
    nodes: &mut Vec<LayoutNode>,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlexBoxLayout::new(config);
    policy.measure(nodes, &[], &children, constraints)
}

fn xs(placements: &[Placement]) -> Vec<f32> {
    placements.iter().map(|p| p.position.x).collect()
}

fn ys(placements: &[Placement]) -> Vec<f32> {
    placements.iter().map(|p| p.position.y).collect()
}

fn xs_of(placements: &[Placement], indices: &[usize]) -> Vec<f32> {
    indices.iter().map(|i| placements[*i].position.x).collect()
}

#[test]
fn a_row_packs_its_items_at_the_start() {
    let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size, Size::new(150.0, 20.0), "a row is as wide as its content");
    assert_eq!(xs(&placements), vec![0.0, 50.0, 100.0]);
}

#[test]
fn the_column_gap_separates_the_items() {
    let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new().column_gap(10.0),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.width, 170.0, "two gaps are part of the line");
    assert_eq!(xs(&placements), vec![0.0, 60.0, 120.0]);
}

#[test]
fn justify_content_distributes_the_leftover() {
    let constraints = Constraints::new(300.0, 300.0, 0.0, 100.0);
    let cases: Vec<(FlexJustifyContent, Vec<f32>)> = vec![
        (FlexJustifyContent::Start, vec![0.0, 50.0, 100.0]),
        (FlexJustifyContent::End, vec![150.0, 200.0, 250.0]),
        (FlexJustifyContent::Center, vec![75.0, 125.0, 175.0]),
        (FlexJustifyContent::SpaceBetween, vec![0.0, 125.0, 250.0]),
        (FlexJustifyContent::SpaceAround, vec![25.0, 125.0, 225.0]),
        (FlexJustifyContent::SpaceEvenly, vec![37.5, 125.0, 212.5]),
    ];
    for (justify, expected) in cases {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let (_, placements) = run(
            FlexBoxConfig::new().justify_content(justify),
            &mut nodes,
            constraints,
        );
        assert_eq!(xs(&placements), expected, "justify {justify:?}");
    }
}

#[test]
fn wrap_breaks_the_line_when_the_next_item_does_not_fit() {
    let mut nodes = vec![leaf(100.0, 20.0), leaf(100.0, 20.0), leaf(100.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new().wrap(FlexWrap::Wrap),
        &mut nodes,
        Constraints::new(0.0, 250.0, 0.0, 200.0),
    );
    // Two 100 dp items fit 250, the third does not: lines of 2 and 1. The container is as wide as
    // the widest line, which is 200 — not the incoming maximum.
    assert_eq!(size, Size::new(200.0, 40.0));
    assert_eq!(xs(&placements), vec![0.0, 100.0, 0.0]);
    assert_eq!(ys(&placements), vec![0.0, 0.0, 20.0], "the third item starts line two");
}

#[test]
fn max_items_in_each_line_breaks_a_line() {
    let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 20.0), leaf(20.0, 20.0)];
    let (_, placements) = run(
        FlexBoxConfig::new()
            .wrap(FlexWrap::Wrap)
            .max_items_in_each_line(2),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 200.0),
    );
    assert_eq!(ys(&placements), vec![0.0, 0.0, 20.0]);
}

#[test]
fn grow_shares_the_free_space() {
    let mut nodes = vec![
        item_leaf(50.0, 20.0, FlexItemConfig::new().grow(1.0)),
        item_leaf(50.0, 20.0, FlexItemConfig::new().grow(1.0)),
    ];
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(300.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.width, 300.0);
    assert_eq!(
        placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
        vec![150.0, 150.0]
    );
}

#[test]
fn shrink_takes_space_proportionally() {
    // The basis is what shrinks: CSS fills the free space by scaling each shrink factor by the
    // item's basis (9.7 step 4.c). A leaf sized 200 dp cannot shrink at all — its MIN-CONTENT size
    // is 200 — so these items declare a larger basis than their content.
    let mut nodes = vec![
        item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(200.0)),
        item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(200.0)),
    ];
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(300.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.width, 300.0);
    assert_eq!(
        placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
        vec![150.0, 150.0]
    );
}

#[test]
fn a_percentage_basis_uses_the_container_main() {
    let mut nodes = vec![
        item_leaf(10.0, 20.0, FlexItemConfig::new().basis_percent(0.25)),
        item_leaf(10.0, 20.0, FlexItemConfig::new().basis_percent(0.25)),
    ];
    let (_, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(400.0, 400.0, 0.0, 100.0),
    );
    assert_eq!(
        placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
        vec![100.0, 100.0]
    );
}

#[test]
fn row_reverse_puts_the_first_item_at_the_end() {
    let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
    let (_, placements) = run(
        FlexBoxConfig::new().direction(FlexDirection::RowReverse),
        &mut nodes,
        Constraints::new(100.0, 100.0, 0.0, 100.0),
    );
    assert_eq!(xs(&placements), vec![50.0, 0.0]);
}

#[test]
fn rtl_mirrors_the_main_axis() {
    let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let mut policy = FlexBoxLayout::new(FlexBoxConfig::new());
    policy.direction = crate::layout::LayoutDirection::Rtl;
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, 100.0),
    );
    assert_eq!(xs(&placements), vec![50.0, 0.0]);
}

#[test]
fn align_items_centre_puts_the_item_in_the_middle_of_the_line() {
    let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 60.0)];
    let (size, placements) = run(
        FlexBoxConfig::new().align_items(FlexAlignItems::Center),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.height, 60.0, "the line is as tall as its tallest item");
    assert_eq!(ys(&placements), vec![20.0, 0.0]);
}

#[test]
fn align_items_stretch_fills_the_line_cross() {
    let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 60.0)];
    let (_, placements) = run(
        FlexBoxConfig::new().align_items(FlexAlignItems::Stretch),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(placements[0].size.height, 60.0, "stretch takes the line extent");
    assert_eq!(placements[1].size.height, 60.0);
}

#[test]
fn align_self_overrides_align_items() {
    let mut nodes = vec![
        leaf(20.0, 60.0),
        item_leaf(
            20.0,
            20.0,
            FlexItemConfig::new().align_self(FlexAlignSelf::End),
        ),
    ];
    let (_, placements) = run(
        FlexBoxConfig::new().align_items(FlexAlignItems::Start),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(ys(&placements), vec![0.0, 40.0], "the second item hugs the line's end");
}

#[test]
fn align_content_space_between_distributes_the_lines() {
    let mut nodes = vec![
        leaf(100.0, 20.0),
        leaf(100.0, 20.0),
        leaf(100.0, 20.0),
    ];
    let (size, placements) = run(
        FlexBoxConfig::new()
            .wrap(FlexWrap::Wrap)
            .align_content(FlexAlignContent::SpaceBetween),
        &mut nodes,
        Constraints::new(0.0, 150.0, 120.0, 120.0),
    );
    assert_eq!(size.height, 120.0, "the cross minimum is taken");
    // Each 100 dp item takes a line of its own (150 dp cannot hold two), so there are three lines
    // of 20: 60 dp of free space over two SpaceBetween gaps puts them at 0, 50 and 100.
    assert_eq!(ys(&placements), vec![0.0, 50.0, 100.0]);
}

#[test]
fn order_reorders_the_items () {
    let mut nodes = vec![
        item_leaf(20.0, 20.0, FlexItemConfig::new().order(2)),
        item_leaf(20.0, 20.0, FlexItemConfig::new().order(0)),
        item_leaf(20.0, 20.0, FlexItemConfig::new().order(1)),
    ];
    let (_, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    // Items are placed in order 0, 1, 2 -> children 1, 2, 0.
    assert_eq!(xs_of(&placements, &[1, 2, 0]), vec![0.0, 20.0, 40.0]);
}

#[test]
fn a_column_direction_swaps_the_axes() {
    let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new().direction(FlexDirection::Column),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 300.0),
    );
    assert_eq!(size, Size::new(50.0, 40.0));
    assert_eq!(ys(&placements), vec![0.0, 20.0]);
}

#[test]
fn an_empty_flex_box_takes_the_minimum() {
    let mut nodes: Vec<LayoutNode> = Vec::new();
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(120.0, 400.0, 60.0, 400.0),
    );
    assert_eq!(size, Size::new(120.0, 60.0));
    assert!(placements.is_empty());
}

#[test]
fn a_baseline_without_a_published_line_falls_back_to_the_item_height() {
    // A child that publishes no alignment line hangs entirely BELOW the baseline, so its baseline
    // is its own height — Compose's getBaseline fallback (:2124-2146). The two boxes then align
    // their BOTTOMS: line cross 40, the 20 dp box sits at 20.
    let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 40.0)];
    let (_, placements) = run(
        FlexBoxConfig::new().align_items(FlexAlignItems::Baseline),
        &mut nodes,
        Constraints::new(0.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(ys(&placements), vec![20.0, 0.0]);
}

/// A main-axis fill implies growth when the item declares no grow factor — Compose's
/// mainAxisFillFraction fallback (FlexBox.kt:365-377).
#[test]
fn a_filling_child_grows_into_the_free_space() {
    let mut filling = LayoutNode::leaf(Modifier::new().fill_max_width());
    filling.measured_size = Size::new(0.0, 20.0);
    let mut nodes = vec![filling, leaf(50.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(300.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.width, 300.0);
    assert!(
        (placements[0].size.width - 250.0).abs() < 0.5,
        "the filling child takes the space the other one leaves: {:?}",
        placements[0].size
    );
}

/// An item cannot grow below its min-content size: CSS 9.7 freezes it there and hands the rest to
/// the other items. Clamping without redistributing made the line WIDER than its container
/// (measured: bases 50+50 with one min-content of 200 gave 200 + 230 for a 300 dp line).
#[test]
fn a_growing_item_cannot_grow_below_its_min_content() {
    let mut nodes = vec![
        item_leaf(200.0, 20.0, FlexItemConfig::new().basis_dp(50.0).grow(1.0)),
        item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(50.0).grow(9.0)),
    ];
    let (size, placements) = run(
        FlexBoxConfig::new(),
        &mut nodes,
        Constraints::new(300.0, 300.0, 0.0, 100.0),
    );
    assert_eq!(size.width, 300.0);
    let widths: Vec<f32> = placements.iter().map(|p| p.size.width).collect();
    assert!(
        (widths[0] - 200.0).abs() < 0.5,
        "the 200 dp item stays at its min-content size: {widths:?}"
    );
    assert!(
        (widths[0] + widths[1] - 300.0).abs() < 0.5,
        "the other item takes exactly the rest, so the line is not wider than the container: {widths:?}"
    );
}

/// The cross gap is subtracted ONCE: Compose's totalLinesCrossSize is the sum of the line sizes
/// and the gap only appears in totalGap (:779-780).
#[test]
fn align_content_does_not_subtract_the_cross_gap_twice() {
    let mut nodes = vec![leaf(100.0, 20.0), leaf(100.0, 20.0), leaf(100.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new()
            .wrap(FlexWrap::Wrap)
            .align_content(FlexAlignContent::SpaceBetween)
            .row_gap(10.0),
        &mut nodes,
        Constraints::new(0.0, 150.0, 120.0, 120.0),
    );
    assert_eq!(size.height, 120.0);
    // free = 120 - 60 - 20 = 40 over two gaps -> 20 each: 0, 50, 100.
    assert_eq!(ys(&placements), vec![0.0, 50.0, 100.0]);
}

/// WrapReverse mirrors the line distribution instead of walking it off the cross extent: Compose
/// adds up the reversed lines from the free-space edge, which for SpaceBetween/SpaceAround leaves
/// the block outside the container (three 20 dp lines with 164 dp of free space in 240 landed at
/// 164 / 274 / 384). The distribution is computed in the cross-start frame and mirrored.
#[test]
fn wrap_reverse_keeps_the_lines_inside_the_cross_extent() {
    let mut nodes = vec![leaf(100.0, 20.0), leaf(100.0, 20.0), leaf(100.0, 20.0)];
    let (size, placements) = run(
        FlexBoxConfig::new()
            .wrap(FlexWrap::WrapReverse)
            .align_content(FlexAlignContent::SpaceBetween),
        &mut nodes,
        Constraints::new(0.0, 150.0, 240.0, 240.0),
    );
    assert_eq!(size.height, 240.0);
    let ys: Vec<f32> = placements.iter().map(|placement| placement.position.y).collect();
    assert!(
        (ys[0] - 220.0).abs() < 0.5 && (ys[1] - 110.0).abs() < 0.5 && (ys[2] - 0.0).abs() < 0.5,
        "the three lines span 0..240 with the first line at the cross-start (the bottom): {ys:?}"
    );
}

/// align-content: stretch grows the LINES before the stretched items are measured, so a stretched
/// item fills the grown line (Compose runs applyAlignContentStretch before measureFlexItems).
#[test]
fn align_content_stretch_grows_the_stretched_items() {
    let mut nodes = vec![leaf(200.0, 100.0), leaf(200.0, 100.0)];
    let (size, placements) = run(
        FlexBoxConfig::new()
            .wrap(FlexWrap::Wrap)
            .align_items(FlexAlignItems::Stretch)
            .align_content(FlexAlignContent::Stretch),
        &mut nodes,
        Constraints::new(0.0, 150.0, 300.0, 300.0),
    );
    assert_eq!(size.height, 300.0, "the two lines share the cross minimum");
    let heights: Vec<f32> = placements.iter().map(|p| p.size.height).collect();
    assert!(
        heights.iter().all(|height| (height - 150.0).abs() < 0.5),
        "each stretched item fills its grown line: {heights:?}"
    );
}

/// The baseline path with REAL published lines: two texts of different sizes must sit on one
/// first baseline (Compose's FlexAlignItems.Baseline, FlexBox.kt:916-968).
#[test]
fn a_baseline_line_lines_up_the_texts() {
    let _guard = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();
    let mut composer = crate::runtime::composer::Composer::new();
    composer.compose(crate::compose!(|ctx| {
        FlexBox::new()
            .config(FlexBoxConfig::new().align_items(FlexAlignItems::Baseline))
            .modifier(Modifier::new().fill_max_width())
            .build(ctx, |ctx| {
                crate::components::Text::new("small")
                    .font_size(12.0)
                    .build(ctx);
                crate::components::Text::new("large")
                    .font_size(32.0)
                    .build(ctx);
            });
    }));
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    let root = composer.layout_root().unwrap();
    let texts = root.children.clone();
    assert_eq!(texts.len(), 2, "two texts in the line");
    let nodes = composer.arena_nodes();
    let line = crate::layout::node::AlignmentLine::FIRST_BASELINE;
    let first = nodes[texts[0]].alignment_line(line).expect("small text has a baseline");
    let second = nodes[texts[1]]
        .alignment_line(line)
        .expect("large text has a baseline");
    let first_y = nodes[texts[0]].position.y + first;
    let second_y = nodes[texts[1]].position.y + second;
    assert!(
        (first_y - second_y).abs() < 0.5,
        "both texts sit on one baseline: {first_y} vs {second_y}"
    );
    assert!(
        nodes[texts[1]].position.y < nodes[texts[0]].position.y,
        "the larger text starts higher: {:?} {:?}",
        nodes[texts[0]].position,
        nodes[texts[1]].position
    );
}

/// Item animation: a marked child gets a paint channel, and a container change that MOVES it starts
/// the placement spring from where it was drawn (the same contract FlowRow has).
#[test]
fn a_marked_item_gets_a_paint_channel_and_animates_from_its_old_position() {
    use crate::layout::item_animation::ItemAnimationHost;

    let host = ItemAnimationHost::default();
    let mut nodes: Vec<LayoutNode> = (0..3)
        .map(|_| LayoutNode::leaf(Modifier::new().size(30.0, 30.0).animate_item()))
        .collect();
    let children: Vec<usize> = (0..3).collect();
    // Tight, or the container is content-sized and justify-content has nothing to distribute.
    let constraints = Constraints::new(300.0, 300.0, 0.0, 100.0);

    let start = FlexBoxLayout::new(FlexBoxConfig::new().gap(8.0)).item_animations(host.clone());
    let (_, placements) = start.measure(&mut nodes, &[], &children, constraints);
    start.place(&mut nodes, &children, &placements);
    for (index, node) in nodes.iter().enumerate() {
        let paint = node
            .item_paint
            .as_ref()
            .unwrap_or_else(|| panic!("item {index} carries a paint channel"));
        assert_eq!(
            paint.offset.peek(),
            Offset::ZERO,
            "the first placement is its own origin"
        );
    }

    // Centring the line moves every item by the free space over two: (300 - 106) / 2 = 97. The channel
    // starts at the OLD position, so the spring walks it from there to zero instead of jumping.
    let moved = FlexBoxLayout::new(
        FlexBoxConfig::new()
            .gap(8.0)
            .justify_content(FlexJustifyContent::Center),
    )
    .item_animations(host);
    let (_, placements) = moved.measure(&mut nodes, &[], &children, constraints);
    moved.place(&mut nodes, &children, &placements);
    for (index, node) in nodes.iter().enumerate() {
        let paint = node
            .item_paint
            .as_ref()
            .unwrap_or_else(|| panic!("item {index} carries a paint channel"));
        assert_eq!(
            paint.offset.peek(),
            Offset::new(-97.0, 0.0),
            "item {index} animates from where it was drawn"
        );
    }
}

/// An unmarked sibling must not be treated as a removed item: its channels are dropped, and no ghost
/// is drawn over a live child.
#[test]
fn an_unmarked_item_leaves_no_ghost_behind() {
    use crate::layout::item_animation::ItemAnimationHost;

    let _guard = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();
    let host = ItemAnimationHost::default();
    let mut nodes: Vec<LayoutNode> = (0..2)
        .map(|_| LayoutNode::leaf(Modifier::new().size(30.0, 30.0).animate_item()))
        .collect();
    let children: Vec<usize> = (0..2).collect();
    let constraints = Constraints::new(0.0, 300.0, 0.0, 100.0);
    let policy = FlexBoxLayout::new(FlexBoxConfig::new()).item_animations(host.clone());
    let (_, placements) = policy.measure(&mut nodes, &[], &children, constraints);
    policy.place(&mut nodes, &children, &placements);
    assert!(nodes[0].item_paint.is_some());

    // Strip the marker from the second child, then place again: it is still live, just not animated.
    nodes[1].modifier = Modifier::new().size(30.0, 30.0);
    let (_, placements) = policy.measure(&mut nodes, &[], &children, constraints);
    policy.place(&mut nodes, &children, &placements);
    assert!(
        nodes[1].item_paint.is_none(),
        "an unmarked child must not be painted through the host"
    );
}

/// The intrinsic walk (FlexBox.kt:1031-1230). Compose overrides all four instead of letting the
/// default approximation measure the content, because a wrapping flex measured at the query
/// constraints answers its RESOLVED line sizes rather than "how narrow can this get".
#[cfg(test)]
mod intrinsic_tests {
    use super::*;
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};

    /// Query one intrinsic of a FlexBox whose children are the given fixed sizes.
    fn intrinsic_of(
        policy: FlexBoxLayout,
        children: &[(f32, f32)],
        query: IntrinsicQuery,
        other: f32,
    ) -> f32 {
        let mut nodes = vec![LayoutNode::new(Modifier::new(), Some(0))];
        for (width, height) in children {
            nodes.push(LayoutNode::leaf(Modifier::new().size(*width, *height)));
        }
        nodes[0].children = (1..nodes.len()).collect();
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(policy)];
        intrinsic_size_of(&mut nodes, &policies, 0, query, other)
    }

    #[test]
    fn a_nowrap_row_min_width_is_the_sum_of_its_items() {
        let policy = FlexBoxLayout::new(FlexBoxConfig::new().gap(8.0));
        let width = intrinsic_of(policy, &[(40.0, 20.0); 3], IntrinsicQuery::MinWidth, 20.0);
        assert_eq!(width, 136.0, "40 + 8 + 40 + 8 + 40");
    }

    #[test]
    fn a_wrapping_row_min_width_is_its_widest_item() {
        let policy = FlexBoxLayout::new(FlexBoxConfig::new().wrap(FlexWrap::Wrap).gap(8.0));
        let width = intrinsic_of(policy, &[(40.0, 20.0); 3], IntrinsicQuery::MinWidth, 20.0);
        assert_eq!(width, 40.0, "every item can be alone on its line");
    }

    #[test]
    fn a_wrapping_row_min_height_breaks_on_the_available_width() {
        let policy = FlexBoxLayout::new(FlexBoxConfig::new().wrap(FlexWrap::Wrap));
        let height = intrinsic_of(
            policy,
            &[(40.0, 20.0), (40.0, 40.0), (40.0, 60.0)],
            IntrinsicQuery::MinHeight,
            100.0,
        );
        assert_eq!(height, 100.0, "two items fit a 100 dp width (cross 40), the third wraps (60)");
    }

    #[test]
    fn the_max_intrinsic_width_breaks_at_the_line_cap() {
        let capped = FlexBoxLayout::new(
            FlexBoxConfig::new().wrap(FlexWrap::Wrap).max_items_in_each_line(2),
        );
        assert_eq!(
            intrinsic_of(capped, &[(40.0, 20.0); 3], IntrinsicQuery::MaxWidth, 20.0),
            80.0,
            "the widest line holds the two capped items"
        );
        let uncapped = FlexBoxLayout::new(FlexBoxConfig::new().wrap(FlexWrap::Wrap));
        assert_eq!(
            intrinsic_of(uncapped, &[(40.0, 20.0); 3], IntrinsicQuery::MaxWidth, 20.0),
            120.0,
            "without a cap the preferred size is one line of everything"
        );
    }

    #[test]
    fn a_column_sums_its_min_height_and_wraps_its_min_width() {
        let column = FlexBoxConfig::new().direction(FlexDirection::Column);
        assert_eq!(
            intrinsic_of(FlexBoxLayout::new(column), &[(40.0, 20.0); 2], IntrinsicQuery::MinHeight, 100.0),
            40.0,
            "no wrap: the two heights add up"
        );
        let wrapping = FlexBoxConfig::new().direction(FlexDirection::Column).wrap(FlexWrap::Wrap);
        assert_eq!(
            intrinsic_of(FlexBoxLayout::new(wrapping), &[(40.0, 20.0); 2], IntrinsicQuery::MinWidth, 100.0),
            40.0,
            "the cross axis of a wrapping column is the widest line, one item each"
        );
    }
}

/// The wiring: a width(IntrinsicSize.Min) above a FlexBox must reach the flex policy instead of the
/// default measure-based approximation (which would answer the RESOLVED line width).
#[cfg(test)]
mod intrinsic_wiring_tests {
    use super::*;
    use crate::layout::{intrinsic_size_of, BoxLayout, IntrinsicQuery, IntrinsicSize};

    #[test]
    fn a_flex_box_under_width_intrinsic_min_answers_the_flex_intrinsic() {
        // 0 = width(IntrinsicSize.Min) wrapper, 1 = the FlexBox, 2..4 = three 40x20 tiles.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width(IntrinsicSize::Min), Some(0)),
            LayoutNode::new(Modifier::new(), Some(1)),
        ];
        for _ in 0..3 {
            nodes.push(LayoutNode::leaf(Modifier::new().size(40.0, 20.0)));
        }
        nodes[0].children = vec![1];
        nodes[1].children = vec![2, 3, 4];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![
            Box::new(BoxLayout::new()),
            Box::new(FlexBoxLayout::new(
                FlexBoxConfig::new().wrap(FlexWrap::Wrap),
            )),
        ];
        // width(IntrinsicSize.Min) answers a Max width query with the content's MINIMUM, which for a
        // wrapping flex is its widest item (40), not the 120 dp one-line sum.
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 20.0),
            40.0
        );
    }

    /// The same layout behind a policy that only forwards measure/place — i.e. with the DEFAULT
    /// intrinsic methods. This is what the FlexBox answered before the overrides existed: measuring
    /// at an unbounded main axis never wraps, so the minimum came back as the one-line sum.
    #[test]
    fn without_the_overrides_the_default_would_answer_the_one_line_sum() {
        #[derive(Debug)]
        struct Forwarding(FlexBoxLayout);

        impl MeasurePolicy for Forwarding {
            fn measure(
                &self,
                nodes: &mut Vec<LayoutNode>,
                policies: &[Box<dyn MeasurePolicy>],
                children: &[usize],
                constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                self.0.measure(nodes, policies, children, constraints)
            }

            fn place(
                &self,
                nodes: &mut Vec<LayoutNode>,
                children: &[usize],
                placements: &[Placement],
            ) {
                self.0.place(nodes, children, placements)
            }
        }

        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width(IntrinsicSize::Min), Some(0)),
            LayoutNode::new(Modifier::new(), Some(1)),
        ];
        for _ in 0..3 {
            nodes.push(LayoutNode::leaf(Modifier::new().size(40.0, 20.0)));
        }
        nodes[0].children = vec![1];
        nodes[1].children = vec![2, 3, 4];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![
            Box::new(BoxLayout::new()),
            Box::new(Forwarding(FlexBoxLayout::new(
                FlexBoxConfig::new().wrap(FlexWrap::Wrap),
            ))),
        ];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 20.0),
            120.0,
            "the default intrinsic walk lays the items out on one line"
        );
    }
}
