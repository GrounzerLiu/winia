//! UI coverage for the lazy grid demo — its own file, driven like `ui_lazy_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `lazy_grid`), so this drives the same UI a human sees
//! with `cargo run -p winia --example lazy_grid_demo`.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;

/// The vertical grid chunks three equal cells per line and scrolls; the horizontal one is adaptive and
/// mirrors under RTL.
#[test]
fn lazy_grids_lay_out_both_axes() {
    let mut app = UiTest::launch("lazy_grid");
    app.expect_text("Lazy Grid");

    // Vertical grid: the first three cells share a line, are equally wide and run left to right; the
    // fourth cell starts the second line.
    let v0 = app.expect_tag("vcell-0");
    let v1 = app.expect_tag("vcell-1");
    let v2 = app.expect_tag("vcell-2");
    let v3 = app.expect_tag("vcell-3");
    assert!(
        (v0.1 - v1.1).abs() <= 1.0 && (v1.1 - v2.1).abs() <= 1.0,
        "the first three cells share a line: {v0:?} {v1:?} {v2:?}"
    );
    assert!(
        v1.0 > v0.0 && v2.0 > v1.0,
        "cells run left to right: {v0:?} {v1:?} {v2:?}"
    );
    assert!(
        (v0.2 - v1.2).abs() <= 1.0,
        "the cells are equally wide: {v0:?} {v1:?}"
    );
    assert!(
        v3.1 > v0.1,
        "the fourth cell opens the second line: {v0:?} {v3:?}"
    );

    // Horizontal grid, RTL: cell 0 is the RIGHTMOST of its line and the cells run leftward.
    let h0 = app.expect_tag("hcell-0");
    let h1 = app.expect_tag("hcell-1");
    assert!(
        h0.0 > h1.0,
        "the RTL horizontal grid runs right to left: {h0:?} {h1:?}"
    );
    assert!(
        (h0.1 - h1.1).abs() <= 1.0,
        "both cells sit on the same line: {h0:?} {h1:?}"
    );

    // The vertical grid really scrolls: a drag upwards moves the first cell out of place, and the
    // scroll settles over a few frames (the drag helper sends eight move steps and a release).
    let before = app.expect_tag("vcell-0");
    let centre = (before.0 + before.2 / 2.0, before.1 + before.3 / 2.0);
    app.drag(centre.0, centre.1, centre.0, centre.1 - 180.0);
    app.expect_eventually_timeout("the vertical grid scrolls", Duration::from_secs(2), |app| {
        app.refresh();
        match app.find_tag("vcell-0") {
            Some(rect) => rect.1 < before.1 - 5.0,
            // Scrolled far enough that the first cell left the window.
            None => true,
        }
    });
}

/// The staggered grid drifts its lanes: an item goes into the SHORTER lane, not into a shared row, so
/// the lanes end up at different cursors. Read off the real window (scenario `lazy_staggered`).
#[test]
fn a_staggered_grid_drifts_its_lanes() {
    let mut app = UiTest::launch("lazy_staggered");
    app.expect_text("Staggered Grid");

    // Heights cycle 60 / 90 / 120, so items 0 and 1 open the two lanes, 2 and 3 continue them.
    let i0 = app.expect_tag("stagger-item-0");
    let i1 = app.expect_tag("stagger-item-1");
    let i2 = app.expect_tag("stagger-item-2");
    let i3 = app.expect_tag("stagger-item-3");
    assert!(
        (i0.1 - i1.1).abs() <= 1.0,
        "both lanes start at the same main offset: {i0:?} {i1:?}"
    );
    assert!(i1.0 > i0.0, "the second item opens the second lane: {i0:?} {i1:?}");
    assert!(
        (i2.0 - i0.0).abs() <= 1.0,
        "the third item continues the FIRST lane: {i0:?} {i2:?}"
    );
    assert!(i2.1 > i0.1, "below the first item: {i0:?} {i2:?}");
    assert!(
        (i3.0 - i1.0).abs() <= 1.0,
        "the fourth continues the SECOND lane: {i1:?} {i3:?}"
    );
    assert!(
        (i2.1 - i3.1).abs() > 1.0,
        "the lanes have drifted apart, which a line grid could not express: {i2:?} {i3:?}"
    );
}
