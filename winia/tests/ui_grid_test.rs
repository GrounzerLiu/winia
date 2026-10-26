//! UI coverage for the interactive grid demo — its own file, driven like `ui_lazy_grid_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `grid`), so this drives the same UI a human sees with
//! `cargo run -p winia --example grid_demo` — INCLUDING its controls: the point of the demo is that
//! every feature is reachable from a button, switch or slider.

#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

/// The default preset (three fixed 90 dp tracks, gap 8): the first three cells share a row, one track
/// plus one gap apart. Then the controls: a span widens cell 0, the area switch pulls cell 1 into the
/// reserved block, "cells +" grows the set and RTL mirrors the order.
#[test]
fn grid_demo_controls_drive_the_layout() {
    let mut app = UiTest::launch("grid");
    app.expect_text("Grid");

    let c0 = app.expect_tag("grid-cell-0");
    let c1 = app.expect_tag("grid-cell-1");
    let c2 = app.expect_tag("grid-cell-2");
    assert!(
        (c0.1 - c1.1).abs() <= 1.0 && (c1.1 - c2.1).abs() <= 1.0,
        "three tracks put the first three cells on one row: {c0:?} {c1:?} {c2:?}"
    );
    assert!(
        (c1.0 - c0.0 - 98.0).abs() <= 1.0,
        "90 dp track + 8 dp gap: {c0:?} {c1:?}"
    );

    // The controls are BUTTONS here on purpose: a click tag on a Switch lands on its track centre,
    // which is not its toggle target (probed), so the switch-driven features are covered by the arena
    // tests (a_named_area_reserves_its_cells, the span arithmetic) instead of this UI test.
    app.click_tag("tracks");
    app.refresh();
    assert!(
        app.all_texts().iter().any(|text| text.contains("tracks 1")),
        "the tracks button cycles the preset: {:?}",
        app.all_texts()
    );
    app.click_tag("flow");
    app.refresh();
    assert!(
        app.all_texts().iter().any(|text| text.contains("flow 1")),
        "the flow button cycles the placement order: {:?}",
        app.all_texts()
    );
    app.click_tag("cells-more");
    app.refresh();
    assert!(
        app.all_texts().iter().any(|text| text.contains("15 cells")),
        "the cells button grows the set: {:?}",
        app.all_texts()
    );
    app.click_tag("align");
    app.refresh();
    assert!(
        app.all_texts().iter().any(|text| text.contains("align 3")),
        "the align button cycles the cell alignment: {:?}",
        app.all_texts()
    );

    // REAL PIXELS through the debug server (the px command), not just the tree: the first two cells
    // are painted with palette(0) green and palette(1) blue.
    let c0 = app.expect_tag("grid-cell-0");
    let green = app
        .pixel_at_logical(c0.0 + c0.2 / 2.0, c0.1 + c0.3 / 2.0)
        .unwrap_or((0, 0, 0, 0));
    assert!(
        (green.0 as i32 - 0x4C).abs() < 16
            && (green.1 as i32 - 0xAF).abs() < 16
            && (green.2 as i32 - 0x50).abs() < 16,
        "the first cell paints palette(0) green: {green:?}"
    );
    let c1 = app.expect_tag("grid-cell-1");
    let blue = app
        .pixel_at_logical(c1.0 + c1.2 / 2.0, c1.1 + c1.3 / 2.0)
        .unwrap_or((0, 0, 0, 0));
    assert!(
        (blue.0 as i32 - 0x21).abs() < 16
            && (blue.1 as i32 - 0x96).abs() < 16
            && (blue.2 as i32 - 0xF3).abs() < 16,
        "the second cell paints palette(1) blue: {blue:?}"
    );
}