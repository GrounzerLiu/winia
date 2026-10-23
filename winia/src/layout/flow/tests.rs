//! Tests for FlowRow and FlowColumn: measurement, placement, RTL and composition.

use super::*;
use crate::layout::{Alignment, Arrangement, Constraints, LayoutDirection, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};

fn text_modifier(content: &str) -> Modifier {
    use crate::modifier::ModifierElement;
    Modifier::new().push(ModifierElement::TextContent {
        content: content.to_string(),
        font_size: 14.0,
        color: crate::graphics::Color::from_argb(255, 0, 0, 0),
        font_weight: crate::text::FontWeight::NORMAL,
        font_style: crate::text::FontSlant::Upright,
        max_lines: usize::MAX,
        align: crate::text::TextAlign::Left,
        overflow: crate::text::TextOverflow::Clip,
        soft_wrap: true,
        letter_spacing: 0.0,
        line_height: None,
    })
}

fn text_leaf(content: &str) -> LayoutNode {
    LayoutNode::new(text_modifier(content), None)
}

fn make_leaf(width: f32, height: f32) -> LayoutNode {
    use crate::modifier::Modifier;
    LayoutNode::leaf(Modifier::new().size(width, height))
}

/// FlowRow's children can align by a text baseline, like Row/ColumnScope.alignByBaseline.
#[test]
fn flow_row_aligns_text_by_baseline() {
    use crate::components::text::Text;
    use crate::modifier::Modifier;

    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            FlowRow::new()
                .modifier(Modifier::new().width(400.0))
                .build(ctx, |ctx| {
                    Text::new("small")
                        .font_size(12.0)
                        .modifier(Modifier::new().align_by_baseline())
                        .build(ctx);
                    Text::new("BIG")
                        .font_size(28.0)
                        .modifier(Modifier::new().align_by_baseline())
                        .build(ctx);
                });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
    let nodes = composer.arena_nodes();
    let baselines: Vec<f32> = nodes
        .iter()
        .filter(|n| n.has_text_content)
        .map(|n| {
            n.position.y
                + n.alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE)
                    .expect("text publishes a baseline")
        })
        .collect();
    assert_eq!(baselines.len(), 2);
    assert!(
        (baselines[0] - baselines[1]).abs() < 0.1,
        "baseline-aligned text should share a baseline, got {baselines:?}"
    );
}

/// A published alignment line grows the line to ascent + descent, beyond the tallest child.
#[test]
fn flow_row_baseline_line_grows_the_row() {
    use crate::layout::AlignmentLine;
    use crate::modifier::Modifier;
    let line = AlignmentLine::FIRST_BASELINE;
    let mut nodes = vec![
        LayoutNode::leaf(
            Modifier::new()
                .size(30.0, 20.0)
                .align_by(line)
                .alignment_line(line, |_s: crate::unit::Size| 18.0),
        ),
        LayoutNode::leaf(
            Modifier::new()
                .size(30.0, 20.0)
                .align_by(line)
                .alignment_line(line, |_s: crate::unit::Size| 10.0),
        ),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = FlowRowLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 28.0, "18 above + 10 below is taller than the 20px children");
    assert_eq!(placements[0].position.y, 0.0);
    assert_eq!(placements[1].position.y, 8.0, "10px line sits 8px below the 18px line");
}

/// An align_by on a line the child never publishes contributes nothing, like Row/Column.
#[test]
fn flow_row_ignores_an_unpublished_baseline() {
    use crate::layout::AlignmentLine;
    use crate::modifier::Modifier;
    let line = AlignmentLine::FIRST_BASELINE;
    let mut nodes = vec![
        LayoutNode::leaf(Modifier::new().size(30.0, 40.0).align_by_baseline()),
        LayoutNode::leaf(
            Modifier::new()
                .size(30.0, 20.0)
                .align_by(line)
                .alignment_line(line, |_s: crate::unit::Size| 10.0),
        ),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, _) = FlowRowLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 40.0, "the unspecified line must not add another 40px tail");
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

/// cross_arrangement distributes the free cross space between lines; Center centres the stack.
#[test]
fn flow_row_cross_arrangement_centres_lines() {
    let policy = FlowRowLayout::new().cross_arrangement(Arrangement::Center);
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
        Constraints::new(0.0, 100.0, 100.0, 100.0),
    );
    assert_eq!(size.height, 100.0);
    assert_eq!(placements[0].position.y, 30.0, "two lines of 20 leave 60/2 above");
    assert_eq!(placements[3].position.y, 50.0, "the second line follows 20px later");
}

/// FlowColumn routes cross_arrangement through the same code on the horizontal cross axis.
#[test]
fn flow_column_cross_arrangement_centres_columns() {
    let policy = FlowColumnLayout::new().cross_arrangement(Arrangement::Center);
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
        Constraints::new(100.0, 100.0, 0.0, 70.0),
    );
    assert_eq!(size.width, 100.0);
    assert_eq!(placements[0].position.x, 30.0, "two 20px columns leave 60/2 on each side");
    assert_eq!(placements[2].position.x, 50.0);
}

/// SpaceBetween must spend the free cross space on the gaps, not only on a leading offset.
#[test]
fn flow_row_space_between_cross_arrangement_uses_the_gap() {
    let policy = FlowRowLayout::new().cross_arrangement(Arrangement::SpaceBetween);
    let mut nodes = vec![
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
        make_leaf(30.0, 20.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 100.0, 100.0),
    );
    assert_eq!(placements[0].position.y, 0.0);
    assert_eq!(placements[3].position.y, 80.0, "40 content + 60 free puts the second line at 80");
}

/// A single line with Center uses the gap-count-zero branch without dividing by zero.
#[test]
fn flow_row_single_line_cross_arrangement_centres() {
    let policy = FlowRowLayout::new().cross_arrangement(Arrangement::Center);
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 100.0, 100.0),
    );
    assert_eq!(placements[0].position.y, 40.0, "20 content in 100 leaves 40 above");
}

/// FlowColumn mirrors the cross arrangement in RTL; Center's leading offset is mirrored too.
#[test]
fn flow_column_rtl_cross_arrangement_mirrors() {
    let policy = FlowColumnLayout::new()
        .cross_arrangement(Arrangement::Center)
        .direction(LayoutDirection::Rtl);
    let mut nodes = vec![
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
        make_leaf(20.0, 30.0),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, 70.0),
    );
    assert_eq!(placements[0].position.x, 50.0, "first column mirrors to the right");
    assert_eq!(placements[2].position.x, 30.0, "second column mirrors to the left");
}

/// max_lines caps the row count even when the cross axis has room for more.
#[test]
fn flow_row_max_lines_caps_rows() {
    let policy = FlowRowLayout::new().max_lines(1);
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
    assert_eq!(size.height, 20.0, "only the first line is kept");
    assert_eq!(placements[3].size, Size::ZERO, "the fourth item is outside max_lines");
}

/// Compose's Visible overflow also bypasses max_lines (FlowLayoutBuildingBlocks.kt:84-90): all
/// rows remain visible.
#[test]
fn flow_row_visible_overflow_ignores_max_lines() {
    let policy = FlowRowLayout::new().max_items_in_row(3).max_lines(1).overflow(FlowOverflow::Visible);
    let mut nodes: Vec<LayoutNode> = (0..7).map(|_| make_leaf(30.0, 20.0)).collect();
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 60.0, "three rows of 20px are visible");
    assert_eq!(placements[6].size, Size::new(30.0, 20.0));
}

/// Visible overflow keeps placing rows past the cross-axis maximum; Clip would drop them.
#[test]
fn flow_row_visible_overflow_keeps_items_past_the_cross_maximum() {
    let policy = FlowRowLayout::new().overflow(FlowOverflow::Visible);
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
    assert_eq!(size.height, 35.0, "the container is still clamped to its max");
    assert_eq!(placements[3].size, Size::new(30.0, 30.0), "Visible paints the overflow");
    assert_eq!(placements[3].position.y, 30.0);
}

/// max_lines = 0 is Compose's empty flow, not one item per line. Tight constraints are used so
/// the early return (0x0) is distinguishable from the truncate path (100x0).
#[test]
fn flow_row_zero_max_lines_is_empty() {
    let policy = FlowRowLayout::new().max_lines(0);
    let mut nodes = vec![make_leaf(30.0, 20.0), make_leaf(30.0, 20.0)];
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &[0, 1],
        Constraints::new(100.0, 100.0, 0.0, 50.0),
    );
    assert_eq!(size, Size::ZERO);
    assert!(placements.iter().all(|p| p.size == Size::ZERO));
}

/// fillMaxRowHeight stretches a short child to the tallest item in its row.
#[test]
fn flow_row_fill_max_row_height_fills_the_line() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(30.0, 40.0),
        LayoutNode::leaf(Modifier::new().size(30.0, 10.0).fill_max_row_height()),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = FlowRowLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 40.0);
    assert_eq!(placements[1].size.height, 40.0);
}

/// Weight and fill compose: the child keeps its weight share and also fills the line cross.
#[test]
fn flow_row_weight_and_fill_compose() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(30.0, 40.0),
        LayoutNode::leaf(
            Modifier::new()
                .size(30.0, 10.0)
                .layout_weight(1.0)
                .fill_max_row_height(),
        ),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = FlowRowLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.height, 40.0);
    assert_eq!(placements[1].size, Size::new(70.0, 40.0));
}

/// The fraction form takes a share of the line's tallest item.
#[test]
fn flow_row_fill_max_row_height_fraction() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(30.0, 40.0),
        LayoutNode::leaf(
            Modifier::new().size(30.0, 10.0).fill_max_row_height_fraction(0.5),
        ),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (_, placements) = FlowRowLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(placements[1].size.height, 20.0);
}

/// FlowColumn uses the column-width variant, transposed onto the horizontal cross axis.
#[test]
fn flow_column_fill_max_column_width_fills_the_column() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(40.0, 30.0),
        LayoutNode::leaf(Modifier::new().size(20.0, 30.0).fill_max_column_width()),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = FlowColumnLayout::new().measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 100.0, 100.0),
    );
    assert_eq!(size.width, 40.0);
    assert_eq!(placements[1].size.width, 40.0);
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

/// max_lines also caps columns in FlowColumn; this pins the duplicated column wiring.
#[test]
fn flow_column_max_lines_caps_columns() {
    let policy = FlowColumnLayout::new().max_lines(1);
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
        Constraints::new(0.0, 100.0, 0.0, 70.0),
    );
    assert_eq!(size.height, 60.0, "only the first column is kept");
    assert_eq!(placements[2].size, Size::ZERO, "the second column is outside max_lines");
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
fn flow_row_max_items_zero_is_empty() {
    let policy = FlowRowLayout::new().max_items_in_row(0);
    let mut nodes = vec![make_leaf(10.0, 20.0), make_leaf(10.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size, Size::ZERO);
    assert!(placements.iter().all(|p| p.size == Size::ZERO));
}

#[test]
fn flow_column_max_items_zero_is_empty() {
    let policy = FlowColumnLayout::new().max_items_in_column(0);
    let mut nodes = vec![make_leaf(10.0, 20.0), make_leaf(10.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, 100.0),
    );
    assert_eq!(size, Size::ZERO);
    assert!(placements.iter().all(|p| p.size == Size::ZERO));
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
fn flow_row_weight_uses_a_bounded_wrap_maximum() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(40.0, 20.0),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0).layout_weight(1.0)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 200.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 200.0, "the bounded maximum is the weight target");
    assert_eq!(placements[1].size.width, 160.0, "200 - 40 fixed");
    assert_eq!(placements[1].position.x, 40.0);
}

#[test]
fn flow_row_weight_fill_false_does_not_stretch_a_short_child() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(30.0, 20.0),
        LayoutNode::leaf(Modifier::new().size(30.0, 20.0).layout_weight_fill(1.0, false)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 60.0, "fill = false leaves the line at its natural size");
    assert_eq!(placements[1].size.width, 30.0);
}

#[test]
fn flow_row_two_weights_split_the_share() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        LayoutNode::leaf(Modifier::new().size(30.0, 20.0).layout_weight(1.0)),
        LayoutNode::leaf(Modifier::new().size(30.0, 20.0).layout_weight(3.0)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 100.0);
    assert_eq!(placements[0].size.width, 25.0);
    assert_eq!(placements[1].size.width, 75.0);
    assert_eq!(placements[1].position.x, 25.0);
}

#[test]
#[test]
fn flow_column_weight_fills_the_column() {
    use crate::modifier::Modifier;
    let mut nodes = vec![
        make_leaf(20.0, 30.0),
        LayoutNode::leaf(Modifier::new().size(20.0, 30.0).layout_weight(1.0)),
    ];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowColumnLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(0.0, 100.0, 100.0, 100.0),
    );
    assert_eq!(size.height, 100.0, "the weighted child fills the column");
    assert_eq!(placements[1].size.height, 70.0);
    assert_eq!(placements[1].position.y, 30.0);
}

fn flow_row_weight_fills_the_line() {
    use crate::modifier::Modifier;
    let mut node = LayoutNode::leaf(Modifier::new().size(30.0, 20.0).layout_weight(1.0));
    let mut nodes = vec![node, make_leaf(30.0, 20.0)];
    let children: Vec<usize> = (0..nodes.len()).collect();
    let policy = FlowRowLayout::new();
    let (size, placements) = policy.measure(
        &mut nodes,
        &[],
        &children,
        Constraints::new(100.0, 100.0, 0.0, f32::MAX),
    );
    assert_eq!(size.width, 100.0, "the weighted child fills the line");
    assert_eq!(placements[0].size.width, 70.0, "100 - 30 fixed = 70 for weight 1");
    assert_eq!(placements[1].position.x, 70.0);
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

/// A changed FlowRow parameter must re-enter its restartable group; the state read happens in
/// the caller, so only `ctx.changed` can carry the new spacing into the group.
#[test]
fn flow_row_declares_its_parameters_for_skip() {
    use std::cell::Cell;
    use std::rc::Rc;

    let spacing = crate::State::new(0.0f32);
    let runs = Rc::new(Cell::new(0usize));
    let mut composer = Composer::new();
    let mut compose = |composer: &mut Composer| {
        let runs = runs.clone();
        let spacing = spacing.clone();
        composer.compose(move |ctx| {
            FlowRow::new()
                .main_spacing(spacing.get())
                .build(ctx, move |ctx| {
                    runs.set(runs.get() + 1);
                    let key = ctx.next_key();
                    ctx.start_leaf(key, Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                });
        });
    };

    compose(&mut composer);
    compose(&mut composer);
    assert_eq!(runs.get(), 1, "an unchanged FlowRow skips its content");

    spacing.set(8.0);
    compose(&mut composer);
    assert_eq!(runs.get(), 2, "a changed main_spacing re-enters the content group");
}

/// The transposed builder declares its parameters too.
#[test]
fn flow_column_declares_its_parameters_for_skip() {
    use std::cell::Cell;
    use std::rc::Rc;

    let spacing = crate::State::new(0.0f32);
    let runs = Rc::new(Cell::new(0usize));
    let mut composer = Composer::new();
    let mut compose = |composer: &mut Composer| {
        let runs = runs.clone();
        let spacing = spacing.clone();
        composer.compose(move |ctx| {
            FlowColumn::new()
                .main_spacing(spacing.get())
                .build(ctx, move |ctx| {
                    runs.set(runs.get() + 1);
                    let key = ctx.next_key();
                    ctx.start_leaf(key, Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                });
        });
    };

    compose(&mut composer);
    compose(&mut composer);
    assert_eq!(runs.get(), 1, "an unchanged FlowColumn skips its content");

    spacing.set(8.0);
    compose(&mut composer);
    assert_eq!(runs.get(), 2, "a changed main_spacing re-enters the content group");
}

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

/// minIntrinsicWidth must show every item the caps allow, not merely fit the cross budget.
#[test]
fn flow_row_min_intrinsic_width_requires_the_capped_items() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let mut nodes = vec![LayoutNode::new(Modifier::new(), Some(0))];
    for _ in 0..5 {
        nodes.push(LayoutNode::leaf(Modifier::new().size(40.0, 20.0)));
    }
    nodes[0].children = (1..=5).collect();
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(
        FlowRowLayout::new().max_items_in_row(2).max_lines(1),
    )];
    let width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 20.0);
    assert_eq!(width, 80.0, "two items per the one-row cap must fit");
}

/// With only max_lines set, minIntrinsicWidth must leave every child visible on that line.
#[test]
fn flow_row_min_intrinsic_width_requires_all_items_on_one_line() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let mut nodes = vec![LayoutNode::new(Modifier::new(), Some(0))];
    for _ in 0..3 {
        nodes.push(LayoutNode::leaf(Modifier::new().size(40.0, 20.0)));
    }
    nodes[0].children = (1..=3).collect();
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new().max_lines(1))];
    let width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 20.0);
    assert_eq!(width, 120.0, "all three items must be shown on the single line");
}

/// Intrinsic line breaking must charge main_spacing; otherwise it reports one row where measure
/// makes two (and a min width that the real layout cannot use).
#[test]
fn flow_row_intrinsics_charge_main_spacing() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0)),
    ];
    nodes[0].children = vec![1, 2];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new().main_spacing(30.0))];
    let height = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0);
    assert_eq!(height, 40.0, "40 + 30 + 40 does not fit 100, so two lines of 20");
    let width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 20.0);
    assert_eq!(width, 110.0, "the minimum width includes the 30px gap");
}

/// max_lines(0) is the empty flow for all four intrinsic queries, like measure.
#[test]
fn flow_row_zero_max_lines_intrinsics_are_empty() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0)),
    ];
    nodes[0].children = vec![1];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new().max_lines(0))];
    for query in [
        IntrinsicQuery::MinWidth,
        IntrinsicQuery::MaxWidth,
        IntrinsicQuery::MinHeight,
        IntrinsicQuery::MaxHeight,
    ] {
        assert_eq!(intrinsic_size_of(&mut nodes, &policies, 0, query, 100.0), 0.0);
    }
}

/// Weight is a measure-time allocation; intrinsic queries keep using the child's intrinsic size,
/// matching Compose's weighted IntrinsicMeasurable path.
#[test]
fn flow_row_intrinsics_ignore_weight_distribution() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    use crate::modifier::Modifier;
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0).layout_weight(3.0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0).layout_weight(1.0)),
    ];
    nodes[0].children = vec![1, 2];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new())];
    assert_eq!(
        intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0),
        80.0
    );
    assert_eq!(
        intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 100.0),
        40.0,
        "with enough height the flow can wrap, so min width is one item"
    );
}

/// max_items(0) is the empty flow for every intrinsic query too.
#[test]
fn flow_row_zero_max_items_intrinsics_are_empty() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        LayoutNode::leaf(Modifier::new().size(40.0, 20.0)),
    ];
    nodes[0].children = vec![1];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new().max_items_in_row(0))];
    for query in [
        IntrinsicQuery::MinWidth,
        IntrinsicQuery::MaxWidth,
        IntrinsicQuery::MinHeight,
        IntrinsicQuery::MaxHeight,
    ] {
        assert_eq!(intrinsic_size_of(&mut nodes, &policies, 0, query, 100.0), 0.0);
    }
}

/// FlowColumn's intrinsic methods are the transposed versions, using height as the main axis.
#[test]
fn flow_column_intrinsics_are_line_aware() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};

    let mut single = vec![text_leaf("alpha")];
    let one_height = intrinsic_size_of(&mut single, &[], 0, IntrinsicQuery::MaxHeight, 100.0);
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        text_leaf("alpha"),
        text_leaf("bravo"),
    ];
    nodes[0].children = vec![1, 2];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowColumnLayout::new())];
    let max_height = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0);
    assert!(
        max_height >= one_height * 1.9,
        "two rows should be about twice one text line (one {one_height}, got {max_height})"
    );
    let max_width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 1000.0);
    assert!(max_width > 0.0, "the column has a cross-axis width");
}

/// The intrinsic methods fold the same line rules as measure: max width is a line sum, and a
/// narrower width makes max height grow by another text line.
#[test]
fn flow_row_intrinsics_are_line_aware() {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};

    fn leaf_width(content: &str) -> f32 {
        let mut leaf = vec![text_leaf(content)];
        intrinsic_size_of(&mut leaf, &[], 0, IntrinsicQuery::MaxWidth, 100.0)
    }

    let a_width = leaf_width("alpha");
    let b_width = leaf_width("bravo");
    let mut nodes = vec![
        LayoutNode::new(Modifier::new(), Some(0)),
        text_leaf("alpha"),
        text_leaf("bravo"),
    ];
    nodes[0].children = vec![1, 2];
    let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FlowRowLayout::new())];
    let max_width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0);
    assert!(
        max_width >= a_width + b_width - 0.5,
        "the max width is the one-line sum ({a_width} + {b_width}, got {max_width})"
    );
    let one_line = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, max_width);
    let narrow = intrinsic_size_of(
        &mut nodes,
        &policies,
        0,
        IntrinsicQuery::MaxHeight,
        a_width.max(b_width),
    );
    assert!(
        narrow > one_line,
        "a width that fits one item wraps the other: {narrow} should exceed {one_line}"
    );
    let min_width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, f32::MAX);
    assert!(min_width > 0.0 && min_width < max_width, "min width is a word, not the line");
}
