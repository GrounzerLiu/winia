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

    // 7. Compose's `ScrollToIndex` — the action a screen reader uses ("bring item 50 into view") —
    //    through the same queue a platform bridge uses. The first element that offers it in the
    //    window is card 1's row, so the row's own tiles prove it moved.
    app.send("si 50");
    app.expect_eventually_timeout("ScrollToIndex jumps the row", Duration::from_secs(3), |app| {
        app.refresh();
        app.find_tag("lazy-rtl-item-50").is_some() && app.find_tag("lazy-rtl-item-0").is_none()
    });

    // 8. The AMBIENT route, not just the modifier: flipping the window's direction mirrors the row
    //    that carries no direction of its own, which is the path an OS-level RTL switch takes.
    //
    // The ORDER is what flips: after the mirror, item 1 sits to the LEFT of item 0. Comparing their
    // positions to each other keeps the assertion out of the render mirror's translate. This runs
    // before the row is scrolled: after a drag both tiles are far from the viewport.
    let ltr_row_rect = app.expect_tag("lazy-ltr-row");
    let item0_ltr = app.expect_tag("lazy-ltr-item-0");
    let item1_ltr = app.expect_tag("lazy-ltr-item-1");
    assert!(item1_ltr.0 > item0_ltr.0, "LTR: item 1 is right of item 0");
    app.set_layout_direction(LayoutDirection::Rtl);
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

    // 8. `layoutInfo` is live: the LTR card prints its visible range from `state.layout_info()`, and
    //    scrolling the row must change that text — a measure-time write reaching a composable reader.
    //    `all_texts` reports each node as `text(<content>)`, so the match is on containment.
    app.expect_eventually_timeout("the layout-info label resolves", Duration::from_secs(2), |app| {
        app.refresh();
        app.all_texts().iter().any(|text| text.contains("items 0-"))
    });
    let label_before = app
        .all_texts()
        .into_iter()
        .find(|text| text.contains("items "))
        .expect("the layout-info label");
    let label_rect = app.expect_tag("lazy-ltr-info");
    assert!(label_rect.3 > 0.0, "the label has a box: {label_rect:?}");
    let ltr = app.expect_tag("lazy-ltr-row");
    let ly = ltr.1 + ltr.3 / 2.0;
    app.drag(ltr.0 + ltr.2 - 40.0, ly, ltr.0 + 40.0, ly);
    app.expect_eventually_timeout(
        "the layout-info label follows the scroll",
        Duration::from_secs(2),
        |app| {
            app.refresh();
            app.all_texts()
                .iter()
                .any(|text| text.contains("items ") && *text != label_before)
        },
    );

    // 9. Accessibility: the window's semantics tree reports the lists as COLLECTIONS of 200 items and
    //    each row's position in it — what a screen reader turns into "row 3 of 200". Read from the
    //    real window's tree, not a unit fixture.
    // The snapshot is published once per frame; the request retries until the frame that has it.
    let sem = app
        .semantics_until(Duration::from_secs(2), |value| {
            value.to_string().contains("collection")
        })
        .expect("the window's semantics tree")
        .to_string();
    // `semantics()` round-trips the JSON through serde_json, whose maps are ordered by key, so the
    // nested objects come back alphabetized.
    assert!(
        sem.contains("\"collection\":{\"columns\":0,\"rows\":200}"),
        "the column cards are 200-row collections"
    );
    assert!(
        sem.contains("\"collection\":{\"columns\":200,\"rows\":0}"),
        "the row cards are 200-column collections"
    );
    assert!(
        sem.contains("\"collectionItem\":{\"index\":0,\"span\":1}"),
        "and every row declares its position"
    );
    assert!(
        sem.contains("\"scrollToIndex\":true"),
        "and the lists offer ScrollToIndex"
    );
    assert!(
        sem.contains("\"indexForKey\":true"),
        "and IndexForKey, so a reader can find an item by key after the data changes"
    );
}
// ═══════════════════════════════════════════════════════════
// The second scenario: one list, driven by buttons
// ═══════════════════════════════════════════════════════════

/// The label the ops page prints from `layout_info` and the state's accessors.
fn ops_info(app: &mut UiTest) -> String {
    app.all_texts()
        .into_iter()
        .find(|text| text.contains("items ") && text.contains(" of "))
        .unwrap_or_default()
}

/// The last index the ops page reports as visible, parsed out of its label ("items A-B of N").
fn visible_last(app: &mut UiTest) -> usize {
    let info = ops_info(app);
    let range = info
        .split("items ")
        .nth(1)
        .and_then(|rest| rest.split(" of ").next())
        .unwrap_or("");
    range
        .split('-')
        .nth(1)
        .and_then(|last| last.trim().parse().ok())
        .unwrap_or(0)
}

fn click_tag(app: &mut UiTest, tag: &str) {
    let (x, y, w, h) = app.expect_tag(tag);
    app.click(x + w / 2.0, y + h / 2.0);
}

/// Every button on the ops page does what its name says, and the page reports it: the data changes
/// under stable keys, the jump moves the anchor, the toggles show up in the same label, and the
/// locked list ignores a drag.
#[test]
fn lazy_ops_buttons_drive_the_list() {
    let mut app = UiTest::launch("lazy_ops");

    app.expect_text("Lazy Ops");
    app.expect_tag("lazy-main-list");
    app.expect_tag("lazy-main-header");

    // Baseline: 50 DATA items — plus the sticky header, which is an item of the list too, so the
    // collection reports 51 (Compose counts a stickyHeader the same way).
    app.expect_eventually_timeout("the list reports itself", Duration::from_secs(2), |app| {
        app.refresh();
        let info = ops_info(app);
        info.contains("of 51") && info.contains("first 0")
    });
    let baseline = ops_info(&mut app);
    assert!(baseline.contains("forward true"), "there is room ahead: {baseline:?}");
    assert!(baseline.contains("backward false"), "and none behind: {baseline:?}");
    // LOCK: user_scroll(false) — the container keeps CLIPPING and the gestures stop reaching it.
    click_tag(&mut app, "lazy-toggle-lock");
    app.expect_eventually_timeout("lock on", Duration::from_secs(2), |app| {
        app.refresh();
        ops_info(app).contains("lock true")
    });
    // Nothing of the list may paint below its own box. Without the clip the items ran over the next
    // card — the bug this assertion was written for, and it only shows near the head of the list,
    // where the composed window extends past the viewport. A tile is a saturated colour; the page
    // background is neutral in either theme.
    let list = app.expect_tag("lazy-main-list");
    let probe_y = list.1 + list.3 + 8.0;
    let points: Vec<(f32, f32)> = (0..5)
        .map(|i| (list.0 + 20.0 + i as f32 * (list.2 - 40.0) / 4.0, probe_y))
        .collect();
    for (x, y) in &points {
        if let Some((r, g, b, _a)) = app.pixel_at_logical(*x, *y) {
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(
                spread < 40,
                "nothing of the locked list may paint below its box: ({x},{y}) is a saturated colour ({r},{g},{b})"
            );
        }
    }
    // And a drag inside it must not move anything.
    let info_before = ops_info(&mut app);
    app.drag(
        list.0 + list.2 / 2.0,
        list.1 + list.3 * 0.75,
        list.0 + list.2 / 2.0,
        list.1 + list.3 * 0.25,
    );
    std::thread::sleep(Duration::from_millis(200));
    app.refresh();
    assert_eq!(
        ops_info(&mut app),
        info_before,
        "a locked list ignores the gesture, so nothing about it changed"
    );
    click_tag(&mut app, "lazy-toggle-lock");
    app.expect_eventually_timeout("lock off", Duration::from_secs(2), |app| {
        app.refresh();
        ops_info(app).contains("lock false")
    });

    // The window starts at the head, so the item whose key is 24 is not composed yet.
    assert!(
        app.find_tag("lazy-main-item-24").is_none(),
        "item 24 is far below the head of the list"
    );

    // JUMP: scroll_to_item asks for global index 25 — the sticky header occupies 0, so that is the
    // data item whose key is 24, and it must become composed. (The page's `first` stays 0 by design:
    // a pinned sticky header IS the first visible item, which is Compose's rule too.)
    click_tag(&mut app, "lazy-jump-25");
    app.expect_eventually_timeout("jump to 25", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("lazy-main-item-24").is_some() && visible_last(app) >= 25
    });

    // ADD at the head: the total grows by 5 and the anchor KEEPS its ITEM — its index moves (25 -> 30)
    // because five items now sit in front of it, and its KEY is what stayed the same. That is the
    // whole point of a stable key, and the tile tag proves which item is on screen.
    click_tag(&mut app, "lazy-add-head");
    app.expect_eventually_timeout("add 5 at the head", Duration::from_secs(2), |app| {
        app.refresh();
        ops_info(app).contains("of 56")
    });
    assert!(
        app.find_tag("lazy-main-item-24").is_some(),
        "the item on screen is still key 24 — the anchor followed the KEY, not the index"
    );

    // REMOVE 5: back to 51 items (50 data + the header), anchor still on key 24.
    click_tag(&mut app, "lazy-remove-head");
    app.expect_eventually_timeout("remove 5", Duration::from_secs(2), |app| {
        app.refresh();
        ops_info(app).contains("of 51") && app.find_tag("lazy-main-item-24").is_some()
    });

    // REVERSE: the label follows the toggle (the list is rebuilt mirrored).
    click_tag(&mut app, "lazy-flip-reverse");
    app.expect_eventually_timeout("reverse on", Duration::from_secs(2), |app| {
        app.refresh();
        ops_info(app).contains("reverse true")
    });


    // PAGER: fill_items + snap_paging — "next page" lands on page 2 and that page is on screen.
    app.expect_eventually_timeout("the pager reports page 1", Duration::from_secs(2), |app| {
        app.refresh();
        app.all_texts().iter().any(|text| text.contains("page 1 of 5"))
    });
    click_tag(&mut app, "lazy-next-page");
    app.expect_eventually_timeout("next page", Duration::from_secs(2), |app| {
        app.refresh();
        app.all_texts().iter().any(|text| text.contains("page 2 of 5"))
            && app.find_tag("lazy-page-1").is_some()
    });
    click_tag(&mut app, "lazy-first-page");
    app.expect_eventually_timeout("first page", Duration::from_secs(2), |app| {
        app.refresh();
        app.all_texts().iter().any(|text| text.contains("page 1 of 5"))
    });
}