//! UI coverage for the grid demo — its own file, driven like `ui_lazy_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `grid`), so this drives the same UI a human sees with
//! `cargo run -p winia --example grid_demo`.
//!
//! The demo's cells request `ContentAlignment::STRETCH` explicitly (the default is Compose's
//! TopStart), so a tag's rect is the resolved track — exactly what these assertions read.

#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

/// Fixed tracks, weighted tracks, a span, dense flow and a cell alignment — read off the real window.
#[test]
fn grid_demo_lays_out_its_tracks() {
    let mut app = UiTest::launch("grid");
    app.expect_text("Grid");

    // 1. Fixed 80 dp columns with an 8 dp gap: the second cell is one track + one gap to the right, the
    //    fourth child starts the second row, and a stretched cell is exactly one track wide.
    let f0 = app.expect_tag("grid-fixed-0");
    let f1 = app.expect_tag("grid-fixed-1");
    let f3 = app.expect_tag("grid-fixed-3");
    assert!(
        (f1.0 - f0.0 - 88.0).abs() <= 1.0,
        "80 + 8 between the first two columns: {f0:?} {f1:?}"
    );
    assert!(
        (f3.1 - f0.1 - 48.0).abs() <= 1.0,
        "the fourth child starts a row 40 + 8 lower: {f0:?} {f3:?}"
    );
    assert!(
        (f0.2 - 80.0).abs() <= 1.0,
        "the default cell alignment stretches the child to its track: {f0:?}"
    );

    // 2. A fixed 90 dp track then two weights in a 1:2 ratio.
    let w0 = app.expect_tag("grid-weight-0");
    let w1 = app.expect_tag("grid-weight-1");
    let w2 = app.expect_tag("grid-weight-2");
    assert!(
        (w1.0 - w0.0 - 98.0).abs() <= 1.0,
        "90 + 8 between the fixed track and the first weighted one: {w0:?} {w1:?}"
    );
    assert!(
        (w2.2 - 2.0 * w1.2).abs() <= 2.0,
        "weight 2 is twice as wide as weight 1: {w1:?} {w2:?}"
    );

    // 3. A child spanning two 70 dp columns is as wide as both plus the gap between them.
    let span = app.expect_tag("grid-span-2col");
    assert!(
        (span.2 - (2.0 * 70.0 + 6.0)).abs() <= 1.0,
        "70 + 6 + 70 wide: {span:?}"
    );

    // 4. Dense flow: the two spanning children drop to consecutive rows and the single cell comes back
    //    to fill (row 0, column 2).
    let d0 = app.expect_tag("grid-dense-0");
    let d1 = app.expect_tag("grid-dense-1");
    let d2 = app.expect_tag("grid-dense-2");
    assert!(
        (d1.1 - d0.1 - 36.0).abs() <= 1.0,
        "the second spanning child starts the next row (30 + 6): {d0:?} {d1:?}"
    );
    assert!(
        (d2.0 - (d0.0 + 2.0 * 76.0)).abs() <= 1.0,
        "the third child fills the hole in row 0, column 2: {d0:?} {d2:?}"
    );
    assert!(
        (d2.1 - d0.1).abs() <= 1.0,
        "and it is on the same row as the first child: {d0:?} {d2:?}"
    );

    // 5. A centred child keeps its own size instead of stretching.
    let centered = app.expect_tag("grid-align-center");
    assert!(
        (centered.2 - 30.0).abs() <= 1.0 && (centered.3 - 30.0).abs() <= 1.0,
        "a centred cell keeps the child's size: {centered:?}"
    );
}
