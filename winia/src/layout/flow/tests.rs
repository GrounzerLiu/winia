//! Tests for FlowRow and FlowColumn: measurement, placement, RTL and composition.

use super::*;
use crate::layout::{Alignment, Arrangement, Constraints, LayoutDirection, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};

fn make_leaf(width: f32, height: f32) -> LayoutNode {
    use crate::modifier::Modifier;
    LayoutNode::leaf(Modifier::new().size(width, height))
}

/// A long text inside a 100-wide FlowRow must be measured against the container width, so the
/// paragraph wraps instead of reporting its unwrapped width and painting outside the flow.
#[test]
fn flow_row_measures_children_with_the_container_width() {
    use crate::components::text::Text;
    use crate::modifier::Modifier;

    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            FlowRow::new()
                .modifier(Modifier::new().width(100.0))
                .build(ctx, |ctx| {
                    Text::new("the quick brown fox jumps over the lazy dog and keeps running")
                        .font_size(14.0)
                        .build(ctx);
                });
        });
    });
    composer.layout(Constraints::new(0.0, 100.0, 0.0, 1000.0));
    let nodes = composer.arena_nodes();
    let text = nodes
        .iter()
        .find(|n| n.has_text_content)
        .expect("the Text leaf was composed");
    assert!(
        text.measured_size.width <= 100.0 + 0.01,
        "a child measured with an unbounded main axis reports {}px, wider than the 100px container",
        text.measured_size.width
    );
    assert!(
        text.measured_size.height > 20.0,
        "the long text should wrap onto more than one line, got {}px tall",
        text.measured_size.height
    );
}

/// A fixed-height FlowRow stops at its cross-axis maximum. The fourth 30px-tall item would start a
/// second line at y = 30 in a 35px-tall container, so Compose's default Clip overflow does not place
/// it at all; neither may winia (otherwise it paints over the content below the flow).
#[test]
fn flow_row_does_not_place_lines_past_the_cross_maximum() {
    let policy = FlowRowLayout::new();
    let mut nodes = vec![
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, 35.0),
    );
    assert_eq!(size.height, 30.0, "only the first line fits in 35px");
    assert_eq!(placements[3].size, Size::ZERO, "the overflowing item is not placed");
    assert_eq!(placements[3].position, Offset::new(0.0, 0.0));
}

/// Cross-axis clipping is per item, not per line: two 20px items fit a 35px cross axis even when a
/// third item in the same line is 40px tall. Compose keeps the two short items and drops the tall
/// one; a line-granular guard would drop the whole line.
#[test]
fn flow_row_keeps_short_items_before_a_tall_one_on_the_same_line() {
    let policy = FlowRowLayout::new();
    let mut nodes = vec![
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        LayoutNode::leaf(crate::modifier::Modifier::new().required_size(30.0, 40.0)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, 35.0),
    );
    assert_eq!(size.height, 20.0, "the two 20px items define the visible line");
    assert_eq!(size.width, 60.0, "the dropped 30px item is not part of the line extent");
    assert_eq!(placements[0].size, Size::new(30.0, 20.0));
    assert_eq!(placements[1].size, Size::new(30.0, 20.0));
    assert_eq!(placements[2].size, Size::ZERO, "the 40px item does not fit");
}

/// A first line taller than the cross axis makes the flow empty, as Compose's Clip overflow does
/// not place any item it cannot fit.
#[test]
fn flow_row_drops_a_first_line_that_cannot_fit() {
    let policy = FlowRowLayout::new();
    let mut nodes = vec![LayoutNode::leaf(crate::modifier::Modifier::new().required_size(30.0, 40.0))];
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &[0],
        Constraints::new(0.0, 100.0, 0.0, 35.0),
    );
    assert_eq!(size.height, 0.0);
    assert_eq!(placements[0].size, Size::ZERO);
}

/// Under RTL only the placed items are mirrored; an item dropped by the cross-axis Clip must keep
/// a direction-independent placement.
#[test]
fn flow_row_rtl_does_not_mirror_omitted_items() {
    let policy = FlowRowLayout::new().direction(LayoutDirection::Rtl);
    let mut nodes = vec![
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
        make_leaf(30.0, 30.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, 35.0),
    );
    assert_eq!(placements[3].position, Offset::new(0.0, 0.0));
    assert_eq!(placements[0].position.x, 60.0, "the first item is mirrored to the right");
}

/// The same item-level cross guard works on the vertical axis: a second column that does not fit
/// the container's width is not placed.
#[test]
fn flow_column_drops_a_column_past_the_cross_maximum() {
    let policy = FlowColumnLayout::new();
    let mut nodes = vec![
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 25.0, 0.0, 70.0),
    );
    assert_eq!(size.width, 20.0, "one 20px column fits in 25px");
    assert_eq!(size.height, 60.0, "the kept column is two 30px items tall");
    assert_eq!(placements[0].size, Size::new(20.0, 30.0));
    assert_eq!(placements[1].size, Size::new(20.0, 30.0));
    assert_eq!(placements[2].size, Size::ZERO, "the second column does not fit");
}

/// The gap before a later line is part of its cross budget: two 10px lines with 10px between them
/// need 30px, so a 25px cross axis keeps only the first line.
#[test]
fn flow_row_charges_cross_spacing_before_the_cutoff() {
    let policy = FlowRowLayout::new().cross_spacing(10.0);
    let mut nodes = vec![
        make_leaf(40.0, 10.0),
        make_leaf(40.0, 10.0),
        make_leaf(40.0, 10.0),
        make_leaf(40.0, 10.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 80.0, 0.0, 25.0),
    );
    assert_eq!(size.height, 10.0, "the second line plus its gap does not fit in 25px");
    assert_eq!(placements[2].size, Size::ZERO);
}


#[test]
fn flow_row_wraps_when_out_of_space() {
    // A 100-wide constraint with wrap semantics (min = 0): 30+30+30 = 90 fits three items, so the
    // fourth wraps; the container width is the widest line, 90.
    let policy = FlowRowLayout::new();
    let mut nodes = vec![
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 90.0, "under wrap the container is as wide as the widest line, 90");
    assert_eq!(size.height, 40.0, "two lines of 20");
    assert_eq!(placements[0].position, Offset::new(0.0, 0.0));
    assert_eq!(placements[2].position, Offset::new(60.0, 0.0));
    assert_eq!(placements[3].position, Offset::new(0.0, 20.0), "the fourth item wraps");
}

#[test]
fn flow_row_fills_container_when_tight() {
    // A tight constraint (min = max = 100, fill semantics): the container is 100 wide and the line starts at its left edge.
    let policy = FlowRowLayout::new();
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 100.0, "under fill the container takes the constraint width");
    assert_eq!(placements[0].position.x, 0.0);
    assert_eq!(placements[1].position.x, 30.0);
}

#[test]
fn flow_row_single_line_when_unbounded() {
    // An unbounded main axis never wraps (it degenerates to a Row).
    let policy = FlowRowLayout::new();
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, _) = policy.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
    assert_eq!(size.width, 60.0);
    assert_eq!(size.height, 20.0);
}

#[test]
fn flow_row_max_items_forces_break() {
    // max_items = 2: wrap even when the items would fit.
    let policy = FlowRowLayout::new().max_items_in_row(2);
    let mut nodes = vec![
        make_leaf(10.0, 20.0),
        make_leaf(10.0, 20.0),
        make_leaf(10.0, 20.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 40.0, "two lines");
    assert_eq!(placements[2].position, Offset::new(0.0, 20.0));
}

#[test]
fn flow_row_spacing_and_cross_alignment() {
    // Main-axis spacing 8, cross spacing 10, centred within each line.
    let policy = FlowRowLayout::new()
        .main_spacing(8.0)
        .cross_spacing(10.0)
        .alignment(Alignment::Center);
    let mut nodes = vec![
        make_leaf(30.0, 10.0),
        make_leaf(30.0, 30.0), // sets the line height to 30
        make_leaf(30.0, 10.0), // wraps (30+8+30+8+30 = 106 > 100)
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(placements[0].position.x, 0.0);
    assert_eq!(placements[1].position.x, 38.0, "30+8");
    // Line height 30; a 10-tall item centred in it sits at y = 10.
    assert_eq!(placements[0].position.y, 10.0);
    assert_eq!(placements[2].position.y, 40.0, "the second line starts at y = 30 + 10");
    assert_eq!(size.height, 50.0, "30+10+10");
}

#[test]
fn flow_row_rtl_mirrors() {
    let policy = FlowRowLayout::new().direction(LayoutDirection::Rtl);
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    // wrap: the container is as wide as the content (60), the mirroring base.
    assert_eq!(size.width, 60.0);
    // LTR: 0/30; RTL mirrored about 60: 30/0.
    assert_eq!(placements[0].position.x, 30.0);
    assert_eq!(placements[1].position.x, 0.0);
}

#[test]
fn flow_column_wraps_to_next_column() {
    // A 100-tall constraint with wrap semantics (min = 0): 30+30+30 = 90 fits three items, so the
    // fourth starts a new column; the container height is the tallest column, 90.
    let policy = FlowColumnLayout::new();
    let mut nodes = vec![
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, f32::MAX, 0.0, 100.0),
    );
    assert_eq!(size.height, 90.0, "under wrap the container is as tall as the tallest column, 90");
    assert_eq!(size.width, 40.0, "two columns of 20");
    assert_eq!(placements[3].position, Offset::new(20.0, 0.0), "the fourth item wraps to a new column");
}

// Follow-ups from an independent review (P1/P2).

#[test]
fn flow_row_max_items_applies_when_unbounded() {
    // P1 regression: max_items still forces a break on an unbounded main axis (it is orthogonal to room).
    let policy = FlowRowLayout::new().max_items_in_row(2);
    let mut nodes = vec![
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
    assert_eq!(size.height, 40.0, "unbounded with max 2 gives two lines");
    assert_eq!(placements[2].position, Offset::new(0.0, 20.0));
}

#[test]
fn flow_row_max_items_zero_means_one_per_line() {
    // max_items = 0 is clamped to 1 (one item per line, a defensive rule).
    let policy = FlowRowLayout::new().max_items_in_row(0);
    let mut nodes = vec![make_leaf(10.0, 20.0), make_leaf(10.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 40.0, "two lines");
    assert_eq!(placements[1].position, Offset::new(0.0, 20.0));
}

#[test]
fn flow_row_single_oversized_item_clamps_line() {
    // A single oversized item: never wraps at the start of a line; the line width is clamped to the
    // constraint while the child keeps its natural size (visual overflow).
    let policy = FlowRowLayout::new();
    let mut nodes = vec![make_leaf(150.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 100.0, "the line width is clamped to the constraint");
    assert_eq!(placements[0].position, Offset::new(0.0, 0.0), "the oversized first item does not wrap");
    assert_eq!(placements[1].position, Offset::new(0.0, 20.0), "the next item wraps");
}

#[test]
fn flow_row_weight_is_ignored_measures_natural() {
    // P2: v1 silently ignores weight; a child carrying it is measured at its natural size.
    use crate::modifier::Modifier;
    let mut node = LayoutNode::leaf(Modifier::new().size(30.0, 20.0).layout_weight(1.0));
    let mut nodes = vec![node, make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 60.0, "weight ignored: natural sizes 30 + 30");
    assert_eq!(placements[1].position.x, 30.0);
}

#[test]
fn flow_row_align_self_overrides_container() {
    // P2: a per-child align_self overrides the container alignment.
    use crate::modifier::Modifier;
    use crate::layout::Alignment;
    let mut tall = LayoutNode::leaf(
        Modifier::new().size(30.0, 30.0).align_self(Alignment::End),
    );
    let mut nodes = vec![make_leaf(30.0, 10.0), tall];
    // make_leaf is built first and tall second, so the children order is [small, tall].
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new().alignment(Alignment::Start);
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    // Line height 30: the small item at Start sits at y = 0, the tall one at End also at y = 0
    // because it fills the line, which proves nothing. Swap the End to the small item instead:
    let mut nodes2 = vec![
        {
            let mut n = LayoutNode::leaf(
                Modifier::new().size(30.0, 10.0).align_self(Alignment::End),
            );
            n
        },
        make_leaf(30.0, 30.0),
    ];
    let children2: Vec<usize> = (0..nodes2.len()).collect();
    let (_, placements2) = policy.measure(
        &mut nodes2,
        &[],
        &children2,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(placements2[0].position.y, 20.0, "the small item at End sits on the line's bottom edge (30 - 10)");
    assert_eq!(placements2[1].position.y, 0.0);
}

#[test]
fn flow_row_center_arrangement_in_fill() {
    // P2: in-line arrangement other than Start; Center centres the line inside a fill container.
    use crate::layout::Arrangement;
    let policy = FlowRowLayout::new().arrangement(Arrangement::Center);
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    // Content 60 in a container of 100 leaves leading = 20.
    assert_eq!(placements[0].position.x, 20.0);
    assert_eq!(placements[1].position.x, 50.0);
}


use super::*;
use crate::modifier::Modifier;
use crate::runtime::composer::Composer;
use crate::theme::{ThemeColors, WiniaTheme};

/// FlowRow end to end: a content closure with four fixed leaves and a 100-wide container wraps.
#[test]
fn flow_row_composes_and_wraps() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            FlowRow::new()
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    for _ in 0..4 {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(30.0, 20.0));
                        ctx.end_node();
                    }
                });
        });
    });
    composer.layout(Constraints::new(100.0, 100.0, 0.0, 600.0));
    let root = composer.layout_root_idx().expect("root");
    let nodes = composer.arena_nodes();
    // Find the node with four children recursively (the FlowRow container; a WiniaTheme scope has
    // no node, so the depth is not assumed).
    fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
        if nodes[idx].children.len() == 4 { return Some(idx); }
        for &c in &nodes[idx].children {
            if let Some(f) = find_4(nodes, c) { return Some(f); }
        }
        None
    }
    let flow_idx = find_4(nodes, root).expect("there should be a FlowRow node with four children");
    let ys: Vec<f32> = nodes[flow_idx].children.iter()
        .map(|&c| nodes[c].position.y)
        .collect();
    assert_eq!(ys, vec![0.0, 0.0, 0.0, 20.0], "the first three stay on the first line, the fourth wraps (ys={ys:?})");
}

/// FlowColumn end to end: a 100-tall container wraps the fourth item into a new column.
#[test]
fn flow_column_composes_and_wraps() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            FlowColumn::new()
                .modifier(Modifier::new().fill_max_height())
                .build(ctx, |ctx| {
                    for _ in 0..4 {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(20.0, 30.0));
                        ctx.end_node();
                    }
                });
        });
    });
    composer.layout(Constraints::new(0.0, 600.0, 100.0, 100.0));
    let root = composer.layout_root_idx().expect("root");
    let nodes = composer.arena_nodes();
    fn find_4(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<usize> {
        if nodes[idx].children.len() == 4 { return Some(idx); }
        for &c in &nodes[idx].children {
            if let Some(f) = find_4(nodes, c) { return Some(f); }
        }
        None
    }
    let flow_idx = find_4(nodes, root).expect("there should be a FlowColumn node with four children");
    let xs: Vec<f32> = nodes[flow_idx].children.iter()
        .map(|&c| nodes[c].position.x)
        .collect();
    assert_eq!(xs, vec![0.0, 0.0, 0.0, 20.0], "the first three stay in the first column, the fourth wraps (xs={xs:?})");
}
