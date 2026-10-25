//! UI coverage for the lazy-list layout demo — its own file, driven like `ui_box_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `lazy_layout`), so this drives the same UI a human
//! sees with `cargo run -p winia --example lazy_layout_demo`.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;
use winia::layout::LayoutDirection;

/// The three cases the review fixed, read off the real window: a mirrored main axis, a mirrored
/// cross axis, and an unbounded cross axis that must not leak the `f32::MAX` sentinel.
#[test]
fn lazy_layout_demo_mirrors_and_stays_bounded() {
    let mut app = UiTest::launch("lazy_layout");

    app.expect_text("Lazy Layout");
    app.expect_tag("top-app-bar-title");

    // 1. RTL row: item 0 at the row's RIGHT edge, the items running leftward, the cross axis untouched.
    let row = app.expect_tag("lazy-rtl-row");
    let i0 = app.expect_tag("lazy-rtl-item-0");
    let i1 = app.expect_tag("lazy-rtl-item-1");
    let i3 = app.expect_tag("lazy-rtl-item-3");
    assert!(
        ((i0.0 + i0.2) - (row.0 + row.2)).abs() < 2.0,
        "item 0 sits at the row's right edge: {i0:?} in {row:?}"
    );
    assert!(
        i0.0 > i1.0 && i1.0 > i3.0,
        "and the items run leftward: {i0:?} {i1:?} {i3:?}"
    );
    assert!(
        (i0.1 - row.1).abs() < 2.0,
        "the cross axis is not mirrored: {i0:?} in {row:?}"
    );
    assert!((i0.2 - 100.0).abs() < 1.0, "the item keeps its 100 dp width: {i0:?}");

    // 2. RTL column: the main axis still runs downward, the cross axis mirrors, so a 120 dp item
    //    hugs the right edge, 16 dp (the start padding) in.
    let column = app.expect_tag("lazy-rtl-column");
    let c0 = app.expect_tag("lazy-rtl-col-item-0");
    let c1 = app.expect_tag("lazy-rtl-col-item-1");
    assert!(
        ((c0.0 + c0.2) - (column.0 + column.2 - 16.0)).abs() < 2.0,
        "the item's right edge is 16 dp in from the column's right edge: {c0:?} in {column:?}"
    );
    assert!(
        (c0.1 - column.1).abs() < 2.0 && c1.1 > c0.1,
        "and the items still stack downward: {c0:?} {c1:?} in {column:?}"
    );
    assert!((c0.2 - 120.0).abs() < 1.0, "the 120 dp item is not stretched: {c0:?}");

    // 3. Unbounded cross axis: the column inside a horizontal scroll reports 50 + 6 + 4 dp. The
    //    sentinel would come out as 3.4e38 here, which is what the bound below rejects.
    let clip = app.expect_tag("lazy-unbounded-clip");
    let unbounded = app.expect_tag("lazy-unbounded-column");
    let u0 = app.expect_tag("lazy-unbounded-item-0");
    assert!(
        unbounded.2.is_finite() && unbounded.2 < 200.0,
        "the lazy column's width is its widest item, not the f32::MAX sentinel: {unbounded:?}"
    );
    assert!(
        (unbounded.2 - 60.0).abs() < 2.0,
        "50 dp of item plus 6 + 4 of cross padding: {unbounded:?}"
    );
    assert!(
        unbounded.2 <= clip.2 + 1.0,
        "and it fits inside the clipped strip: {unbounded:?} in {clip:?}"
    );
    assert!((u0.2 - 50.0).abs() < 1.0, "the item keeps its 50 dp width: {u0:?}");

    // 4. The LTR control: index 0 at the left edge, ascending.
    let ltr_row = app.expect_tag("lazy-ltr-row");
    let l0 = app.expect_tag("lazy-ltr-item-0");
    let l1 = app.expect_tag("lazy-ltr-item-1");
    assert!(
        (l0.0 - ltr_row.0).abs() < 2.0,
        "item 0 sits at the row's left edge in LTR: {l0:?} in {ltr_row:?}"
    );
    assert!(l1.0 > l0.0, "and the items run rightward: {l0:?} {l1:?}");

    // 5. The AMBIENT route, not just the modifier: flipping the window's direction mirrors the LTR
    //    row too, which is the path an OS-level RTL switch takes.
    app.set_layout_direction(LayoutDirection::Rtl);
    app.expect_eventually_timeout("the LTR row mirrors with the window direction", Duration::from_secs(2), |app| {
        app.refresh();
        match (app.find_tag("lazy-ltr-row"), app.find_tag("lazy-ltr-item-0")) {
            (Some(row), Some(item)) => ((item.0 + item.2) - (row.0 + row.2)).abs() < 2.0,
            _ => false,
        }
    });
    app.clear_layout_direction();
}
