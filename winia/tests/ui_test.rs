//! UI coverage for the FlowRow / FlowColumn feature demo.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module, so this test
//! drives the same UI a human sees with `cargo run --example flow_features_demo`.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;
use winia::layout::LayoutDirection;

/// The example's visible surface: wrapping, max items/lines, Clip vs Visible, cross arrangement,
/// weight + fill, baseline alignment, FlowColumn fill, and the debug layout-direction override.
#[test]
fn flow_features_demo_covers_the_flow_surface() {
    let mut app = UiTest::launch("flow_features");

    app.expect_text("Flow Features");
    app.expect_tag("flow-wrap");

    // max_items_in_row(3): seven items use three rows.
    for n in 0..7 {
        app.expect_tag(&format!("wrap-{n}"));
    }

    // Clip with max_lines(1) drops the second row; Visible keeps it.
    app.expect_tag_size("clip-3", (0.0, 0.0), 0.1);
    let visible = app.expect_tag("visible-3");
    assert!(visible.2 > 1.0 && visible.3 > 1.0, "Visible should place the fourth item: {visible:?}");

    // Cross arrangement Center: two lines of 20px in a 120px card, leading (120 - 16 - 48) / 2.
    let card = app.expect_tag("flow-cross");
    let first = app.expect_tag("cross-0");
    let second = app.expect_tag("cross-2");
    assert!((first.1 - card.1 - 36.0).abs() < 1.0, "first centered line: {first:?} in {card:?}");
    assert!((second.1 - card.1 - 64.0).abs() < 1.0, "second centered line: {second:?} in {card:?}");

    // Weight keeps its main share and fillMaxRowHeight makes it as tall as the 40px fixed item.
    let weighted = app.expect_tag("weight-fill");
    assert!((weighted.2 - 216.0).abs() < 1.0, "weighted width: {weighted:?}");
    assert!((weighted.3 - 40.0).abs() < 1.0, "weighted height: {weighted:?}");

    // Baseline alignment moves the small text down to the larger text's baseline.
    let small = app.expect_tag("baseline-small");
    let big = app.expect_tag("baseline-big");
    assert!(small.1 > big.1, "small text should sit lower for a shared baseline: {small:?} vs {big:?}");

    // A width(IntrinsicSize.Max) flow asks the intrinsic path for its natural width.
    let intrinsic = app.expect_tag("flow-intrinsic");
    assert!(intrinsic.2 > 0.0, "intrinsic flow should have a width: {intrinsic:?}");
    app.expect_tag("intrinsic-2");

    // FlowColumn fill takes the widest item's width.
    let column = app.expect_tag("column-fill");
    assert!((column.2 - 40.0).abs() < 1.0, "column fill width: {column:?}");

    // The debug direction override mirrors the layout without relying on the settings sheet.
    let before = app.expect_tag("wrap-0").0;
    app.set_layout_direction(LayoutDirection::Rtl);
    app.expect_eventually_timeout("the flow mirrors in RTL", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("wrap-0").is_some_and(|r| (r.0 - before).abs() > 1.0)
    });
    app.clear_layout_direction();
}

/// Changing the per-line cap from 3 to 2 is a real reflow: a keyed child moves to a new slot,
/// the paint offset springs back to zero, and the layout itself does not jump between frames.
#[test]
fn flow_features_demo_animates_a_reflow() {
    let mut app = UiTest::launch("flow_features");

    app.expect_text("Flow Features");
    let before = app.expect_tag("animated-2");
    app.expect_tag("animated-5");

    app.click_tag("toggle-columns");
    app.expect_text("2 columns");

    app.expect_eventually_timeout("the reflow moved the keyed card", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("animated-2")
            .is_some_and(|r| (r.0 - before.0).abs() > 1.0 || (r.1 - before.1).abs() > 1.0)
    });

    // The placement spring is registered in the animation table; this is the observable proof
    // that the card did not simply snap to its new slot.
    let running = app.animations();
    let placement = running.iter().any(|line| {
        line.contains("components=2") && line.contains("spring") && line.contains("target=0.000")
    });
    assert!(
        placement,
        "the reflow should leave a two-component placement spring: {running:?}"
    );

    app.expect_eventually_timeout("the box settles on the two-column height", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("animated-box")
            .is_some_and(|r| (r.3 - 164.0).abs() < 1.0)
    });
    let grown = app.expect_tag("animated-box");
    assert!((grown.3 - 164.0).abs() < 1.0, "the two-column box grows: {grown:?}");

    // Shrinking must animate too: the box is still taller than the three-column size right after
    // the toggle, then settles on it. A snap would report 112 immediately.
    app.click_tag("toggle-columns");
    app.expect_text("3 columns");
    let shrinking = app.expect_tag("animated-box");
    assert!(
        shrinking.3 > 112.5,
        "the box must spring down instead of snapping: {shrinking:?}"
    );
    app.expect_eventually_timeout("the box settles on the three-column height", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("animated-box")
            .is_some_and(|r| (r.3 - 112.0).abs() < 1.0)
    });
}

/// The dedicated item-animation demo exercises both directions of the shared item host: an added
/// key fades in, and a removed key keeps its captured picture while it fades out.
#[test]
fn item_animation_demo_fades_added_and_removed_items() {
    let mut app = UiTest::launch("item_animation");
    app.expect_text("Item animation");

    let zero_before = app.expect_tag("item-0");
    app.click_tag("add-head");
    app.refresh();
    let added = app.animations();
    assert!(
        added.iter().any(|line| line.contains("tween(300ms)")),
        "the head item should fade in with its own curve: {added:?}"
    );
    let inserted = app.expect_tag("item-4");
    let zero_after = app.expect_tag("item-0");
    assert!(
        inserted.0 <= zero_before.0 && zero_after.0 > zero_before.0,
        "the new card must take the first slot and push the old first right: inserted {inserted:?}, old first {zero_before:?}, moved first {zero_after:?}"
    );

    app.expect_tag("item-3");
    app.click_tag("item-3");
    app.expect_eventually_timeout("the tapped card is removed", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("item-3").is_none()
    });
}

/// The flex layout demo exercises the four shapes the layout review found broken, through the
/// real window: a weighted child inside a scrolling Column, SpaceBetween with a single child, a
/// zero weight child, and a Column whose content overflows its box.
#[test]
fn flex_layout_demo_pins_the_review_cases() {
    let mut app = UiTest::launch("flex_layout");

    // The shared demo chrome is up: the top app bar and its title.
    app.expect_text("Flex Layout");
    app.expect_tag("top-app-bar-title");

    // 1. The SCROLLING CONTENT is the measured thing, not the card: the card is pinned by its own
    //    80 dp box either way, while the content Column was 3.4e38 before the fix. It holds the
    //    40 dp fixed child plus a weighted child that an unbounded axis gives nothing.
    let card = app.expect_tag("scroll-weight");
    let title = app.expect_tag("scroll-weight-title");
    let content = app.expect_tag("scroll-content");
    let fixed = app.expect_tag("scroll-fixed");
    assert!((fixed.3 - 40.0).abs() < 1.0, "the fixed child keeps its 40: {fixed:?}");
    assert!(
        (content.3 - 40.0).abs() < 1.0,
        "the scrolling content is its 40 dp child, not a 3.4e38 one: {content:?}"
    );
    // The card is padding (10 top + 10 bottom) + the title + the 6 dp spacing + the 80 dp box.
    assert!(
        (card.3 - (20.0 + title.3 + 6.0 + 80.0)).abs() < 2.0,
        "the card is padding + title + 6 + its 80 dp scroll box: {card:?}, title {title:?}"
    );

    // 2. SpaceBetween with one child packs it at the start, not the centre.
    let card = app.expect_tag("space-between-one");
    let title = app.expect_tag("space-between-one-title");
    let bar = app.expect_tag("space-between-bar");
    // padding 10 + the title's own measured height + spacing 6, then the bar at the row's start.
    let expected_y = card.1 + 10.0 + title.3 + 6.0;
    assert!(
        (bar.1 - expected_y).abs() < 2.0,
        "the single child sits at the card's start: bar {bar:?} in card {card:?}, title {title:?}"
    );

    // 3. weight(0) measures like an unweighted sibling instead of collapsing, and does not take
    //    the column: the sibling still sits right below it.
    let zero = app.expect_tag("zero-weight-child");
    let sibling = app.expect_tag("zero-sibling");
    assert!((zero.3 - 30.0).abs() < 1.0, "weight(0) keeps its 30: {zero:?}");
    assert!(
        (sibling.1 - (zero.1 + zero.3)).abs() < 2.0,
        "the sibling follows the weight(0) child: {sibling:?} after {zero:?}"
    );

    // 4. The overflowing Column reports its content (4 x 45 = 180) while its 100 dp parent clips.
    //    The row width is a control: required_size reports 120 on both sides of the fix.
    let card = app.expect_tag("overflow");
    let title = app.expect_tag("overflow-title");
    let content = app.expect_tag("overflow-content");
    let row = app.expect_tag("overflow-row");
    assert!(
        (content.3 - 180.0).abs() < 1.0,
        "the overflowing column reports its content: {content:?}"
    );
    assert!(
        (card.3 - (20.0 + title.3 + 6.0 + 100.0)).abs() < 2.0,
        "the card is padding + title + 6 + the 100 dp clip box: {card:?}, title {title:?}"
    );
    assert!((row.2 - 120.0).abs() < 1.0, "the row keeps its required 120 width: {row:?}");

    // The debug direction override still mirrors the page with the top bar in place.
    let before = app.expect_tag("space-between-bar").0;
    app.set_layout_direction(LayoutDirection::Rtl);
    app.expect_eventually_timeout("the demo mirrors in RTL", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("space-between-bar").is_some_and(|r| (r.0 - before).abs() > 1.0)
    });
    app.clear_layout_direction();
}
