//! UI coverage for the box layout demo — its own file, driven the same way as `ui_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `box_layout`), so this drives the same UI a
//! human sees with `cargo run -p winia --example box_layout_demo`.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;
use winia::layout::LayoutDirection;

/// The three cases the box review fixed, read off the real window: each child's own alignment, an
/// overflowing child that the box reports instead of cropping, and `BoxWithConstraints`' alignment.
#[test]
fn box_layout_demo_places_children_per_child() {
    let mut app = UiTest::launch("box_layout");

    app.expect_text("Box Layout");
    app.expect_tag("top-app-bar-title");

    // 1. Per-child alignment inside one 180 dp frame: default top-start, centre, bottom-end.
    let frame = app.expect_tag("box-align-frame");
    let start = app.expect_tag("box-child-start");
    let center = app.expect_tag("box-child-center");
    let end = app.expect_tag("box-child-end");
    assert!(
        (start.0 - frame.0).abs() < 2.0 && (start.1 - frame.1).abs() < 2.0,
        "the default child sits at the frame's top-left: {start:?} in {frame:?}"
    );
    assert!(
        (center.0 - (frame.0 + (frame.2 - center.2) / 2.0)).abs() < 2.0
            && (center.1 - (frame.1 + (frame.3 - center.3) / 2.0)).abs() < 2.0,
        "align_self(Center) centres on both axes: {center:?} in {frame:?}"
    );
    assert!(
        (end.0 - (frame.0 + frame.2 - end.2)).abs() < 2.0
            && (end.1 - (frame.1 + frame.3 - end.3)).abs() < 2.0,
        "content_align_self(BOTTOM_END) puts the child at the bottom-right: {end:?} in {frame:?}"
    );

    // 2. The overflowing box reports its child's 500 dp width (the clip box around it stays narrow).
    let clip = app.expect_tag("box-overflow-clip");
    let overflow = app.expect_tag("box-overflow");
    assert!(
        (overflow.2 - 500.0).abs() < 1.0,
        "the box is as wide as its overflowing child: {overflow:?}"
    );
    assert!(
        clip.2 < overflow.2,
        "and the parent clip is narrower, so the overflow is visible: {clip:?} vs {overflow:?}"
    );

    // 3. BoxWithConstraints' alignment places the content inside the forced box.
    let bwc = app.expect_tag("bwc-box");
    let child = app.expect_tag("bwc-child");
    assert!(
        (child.0 - (bwc.0 + bwc.2 - child.2)).abs() < 2.0,
        "the content is at the box's trailing edge: {child:?} in {bwc:?}"
    );
    assert!(
        (child.1 - (bwc.1 + bwc.3 - child.3)).abs() < 2.0,
        "and at its bottom edge: {child:?} in {bwc:?}"
    );

    // RTL mirrors the horizontal half: bottom-end becomes bottom-left, top-start becomes top-right.
    let frame_before = app.expect_tag("box-align-frame");
    app.set_layout_direction(LayoutDirection::Rtl);
    app.expect_eventually_timeout("the box mirrors in RTL", Duration::from_secs(2), |app| {
        app.refresh();
        match (app.find_tag("box-align-frame"), app.find_tag("box-child-end")) {
            (Some(frame), Some(end)) => (end.0 - frame.0).abs() < 2.0,
            _ => false,
        }
    });
    let end_rtl = app.expect_tag("box-child-end");
    let frame_rtl = app.expect_tag("box-align-frame");
    assert!(
        (end_rtl.0 - frame_rtl.0).abs() < 2.0,
        "bottom-end mirrors to the left edge in RTL: {end_rtl:?} in {frame_rtl:?}"
    );
    assert!(
        (frame_rtl.2 - frame_before.2).abs() < 2.0,
        "the frame keeps its width across the flip"
    );
    app.clear_layout_direction();
}
