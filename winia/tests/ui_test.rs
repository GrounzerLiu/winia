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

    let last = app.expect_tag("item-3");
    let ghost_point = (last.0 + last.2 / 2.0, last.1 + last.3 / 2.0);
    app.click_tag("item-3");
    let removed = app.animations();
    let pixel = app
        .pixel_at_logical(ghost_point.0, ghost_point.1)
        .expect("the frame should be readable after removing an item");
    assert!(
        pixel.2.saturating_sub(pixel.0) > 8,
        "the tapped card should still be painted as a fading ghost: {pixel:?}"
    );
    assert!(
        removed.iter().any(|line| line.contains("tween(2000ms)")),
        "the tapped card should keep fading out: {removed:?}"
    );
    app.expect_eventually_timeout("the tapped card is removed", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("item-3").is_none()
    });
}
