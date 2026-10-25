//! UI coverage for the lazy-list layout demo — its own file, driven like `ui_box_test.rs`.
//!
//! The demo is compiled into the single fixture binary through a `#[path]` module
//! (`tests/ui_fixtures/fixture_all.rs`, scenario `lazy_layout`), so this drives the same UI a human
//! sees with `cargo run -p winia --example lazy_layout_demo`.
//!
//! The lists hold 200 entries, so this also drives real gestures: a lazy list that cannot scroll
//! never exercises its window, its anchor or the mirror translate.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;
use winia::layout::LayoutDirection;

/// The three cases the review fixed, read off the real window: a mirrored main axis, a mirrored
/// cross axis, an unbounded cross axis that must not leak the `f32::MAX` sentinel — plus a gesture on
/// each list, because "it lays out" is not what a lazy list is for.
#[test]
fn lazy_layout_demo_mirrors_scrolls_and_stays_bounded() {
    let mut app = UiTest::launch("lazy_layout");

    app.expect_text("Lazy Layout");
    app.expect_tag("top-app-bar-title");

    // 1. RTL row: item 0 is the RIGHTMOST tile at offset 0 and the items run leftward. With more
    //    items than fit, the render mirror also translates, so the assertion is on the ORDER and the
    //    tile sizes rather than on an absolute edge.
    let row = app.expect_tag("lazy-rtl-row");
    let i0 = app.expect_tag("lazy-rtl-item-0");
    let i1 = app.expect_tag("lazy-rtl-item-1");
    let i2 = app.expect_tag("lazy-rtl-item-2");
    assert!(
        i0.0 > i1.0 && i1.0 > i2.0,
        "the RTL row runs right to left: {i0:?} {i1:?} {i2:?}"
    );
    assert!(
        (i0.1 - row.1).abs() < 2.0,
        "the cross axis is not mirrored: {i0:?} in {row:?}"
    );
    assert!((i0.2 - 100.0).abs() < 1.0, "the tile keeps its 100 dp width: {i0:?}");

    // 2. RTL column: the main axis still runs downward, the cross axis mirrors, so a 120 dp item
    //    hugs the right edge, 16 dp (the start padding) in. A vertical list has no translate, so this
    //    edge is exact.
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

    // 4. The LTR control: index 0 at the left, ascending.
    let ltr_row = app.expect_tag("lazy-ltr-row");
    let l0 = app.expect_tag("lazy-ltr-item-0");
    let l1 = app.expect_tag("lazy-ltr-item-1");
    assert!(
        (l0.0 - ltr_row.0).abs() < 2.0,
        "item 0 sits at the row's left edge in LTR: {l0:?} in {ltr_row:?}"
    );
    assert!(l1.0 > l0.0, "and the items run rightward: {l0:?} {l1:?}");

    // 5. Scroll the RTL COLUMN: a drag that starts inside it must move the window — item 0 leaves,
    //    later items arrive. This is what the 200 entries are for.
    assert!(app.find_tag("lazy-rtl-col-item-0").is_some(), "item 0 starts visible");
    let col = app.expect_tag("lazy-rtl-column");
    let cx = col.0 + col.2 / 2.0;
    let cy = col.1 + col.3 / 2.0;
    app.drag(cx, cy + 40.0, cx, cy - 160.0);
    app.expect_eventually_timeout("the RTL column scrolls", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("lazy-rtl-col-item-0").is_none()
            && (2..12).any(|i| app.find_tag(&format!("lazy-rtl-col-item-{i}")).is_some())
    });

    // 6. Scroll the RTL ROW horizontally: item 0 leaves, later tiles come in from the left.
    //
    // The drag goes LEFT to RIGHT here: on a mirrored container the gesture delta is mirrored too
    // (`scroll.rs`: `raw = current + dx` when `scroll_reverse`), so a rightward drag is what advances
    // an RTL list — dragging leftward is clamped at offset 0, which is exactly what this test caught.
    let row = app.expect_tag("lazy-rtl-row");
    let ry = row.1 + row.3 / 2.0;
    assert!(app.find_tag("lazy-rtl-item-0").is_some(), "item 0 starts visible");
    app.drag(row.0 + 40.0, ry, row.0 + row.2 - 40.0, ry);
    app.expect_eventually_timeout("the RTL row scrolls", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("lazy-rtl-item-0").is_none()
            && (1..8).any(|i| app.find_tag(&format!("lazy-rtl-item-{i}")).is_some())
    });

    // 7. The AMBIENT route, not just the modifier: flipping the window's direction mirrors the row
    //    that carries no direction of its own, which is the path an OS-level RTL switch takes.
    let ltr_row_rect = app.expect_tag("lazy-ltr-row");
    let before = app.expect_tag("lazy-ltr-item-0");
    let before_next = app.expect_tag("lazy-ltr-item-1");
    assert!(before_next.0 > before.0, "LTR: item 1 is right of item 0");
    app.set_layout_direction(LayoutDirection::Rtl);
    // The ORDER is what flips: after the mirror, item 1 sits to the LEFT of item 0. Comparing
    // positions relative to each other keeps the assertion out of the render mirror's translate.
    app.expect_eventually_timeout(
        "the LTR row mirrors with the window direction",
        Duration::from_secs(2),
        |app| {
            app.refresh();
            match (app.find_tag("lazy-ltr-item-0"), app.find_tag("lazy-ltr-item-1")) {
                (Some(a), Some(b)) => b.0 < a.0,
                _ => false,
            }
        },
    );
    let ltr_row_rtl = app.expect_tag("lazy-ltr-row");
    assert!(
        (ltr_row_rtl.2 - ltr_row_rect.2).abs() < 2.0,
        "and it keeps its width across the flip"
    );
    app.clear_layout_direction();
}
