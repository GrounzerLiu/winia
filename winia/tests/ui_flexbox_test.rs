//! FlexBox through a real window. The demo's controls that this test drives are wrap (which stops the
//! shrink) and justify (which packs a line to the main-end edge); direction, align and align-content
//! have no button assertions here, and grow is demonstrated by the fixed strip rather than a control.
//! Every assertion below reads measured geometry back out of the fixture's debug server, so a broken
//! FlexBox fails here rather than passing on tag presence.

#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

#[test]
fn flexbox_lays_out_both_axes() {
    let mut app = UiTest::launch("flexbox");

    // Default: Row / NoWrap / Start. Six 96 dp tiles plus five 8 dp gaps are more than the frame,
    // so CSS shrink (which defaults to 1) squeezes every tile instead of overflowing.
    let frame = app.expect_tag("fb-frame");
    let shared = (frame.2 - 5.0 * 8.0) / 6.0;
    let first = app.expect_tag("fb-tile-0");
    let second = app.expect_tag("fb-tile-1");
    assert!(
        (first.2 - shared).abs() <= 2.0,
        "shrink divides the frame between the six tiles ({shared}): {first:?}"
    );
    assert!(
        (second.0 - first.0 - first.2 - 8.0).abs() <= 1.5,
        "the 8 dp gap survives the shrink: {first:?} {second:?}"
    );
    app.refresh();
    let tiles = ["0", "1", "2", "3", "4", "5"]
        .iter()
        .filter(|index| app.find_tag(&format!("fb-tile-{index}")).is_some())
        .count();
    assert_eq!(tiles, 6, "the demo composes six tiles");

    // Wrap on: four 96 dp tiles fit one line (4 * 96 + 3 * 8 <= the frame), so they stop shrinking
    // and the remaining two start the second line one line-height plus the cross gap lower.
    app.click_tag("fb-wrap");
    app.expect_eventually("the tiles stop shrinking", |app| {
        app.refresh();
        (app.expect_tag("fb-tile-0").2 - 96.0).abs() <= 1.5
    });
    let line_one_last = app.expect_tag("fb-tile-3");
    let line_two_first = app.expect_tag("fb-tile-4");
    assert!(
        (line_one_last.1 - app.expect_tag("fb-tile-0").1).abs() <= 1.0,
        "four tiles share the first line"
    );
    assert!(
        line_two_first.1 - line_one_last.1 >= 90.0,
        "the fifth tile starts the second line: {line_one_last:?} {line_two_first:?}"
    );

    // justify-content: End packs each line against the main-end edge.
    app.click_tag("fb-justify");
    app.expect_eventually("the line is packed to the end", |app| {
        app.refresh();
        app.expect_tag("fb-tile-0").0 > 20.0
    });
    let frame = app.expect_tag("fb-frame");
    let packed = app.expect_tag("fb-tile-0");
    let last_of_line = app.expect_tag("fb-tile-3");
    assert!(
        (frame.0 + frame.2 - (last_of_line.0 + last_of_line.2)).abs() <= 2.0,
        "the last tile of the line ends at the frame's edge: {frame:?} {last_of_line:?}"
    );
    assert!(
        packed.0 > frame.0,
        "the line starts away from the frame's start: {packed:?}"
    );

    // direction: the same six tiles become a column, each keeping its cross size (this is where a tile
    // with no cross size collapsed to zero width before the demo sized its cross axis).
    app.click_tag("fb-direction");
    app.expect_eventually("the grid becomes a column", |app| {
        app.refresh();
        app.expect_tag("fb-tile-1").1 > app.expect_tag("fb-tile-0").1 + 8.0
    });
    let column_first = app.expect_tag("fb-tile-0");
    let column_second = app.expect_tag("fb-tile-1");
    assert!(
        column_first.2 > 50.0,
        "a column tile keeps its cross size: {column_first:?}"
    );
    assert!(
        (column_first.0 - column_second.0).abs() < 1.5,
        "the column tiles share one cross position: {column_first:?} {column_second:?}"
    );

    // grow shares the FREE space: the strip holds a grow-2 tile, a grow-1 tile and a fixed 60 dp one,
    // and the three fill the width between them.
    let frame = app.expect_tag("fb-grow-strip");
    let two = app.expect_tag("fb-grow-0");
    let one = app.expect_tag("fb-grow-1");
    let fixed = app.expect_tag("fb-grow-2");
    assert!(
        (fixed.2 - 60.0).abs() <= 1.5,
        "the tile without grow keeps its basis: {fixed:?}"
    );
    assert!(
        two.2 > one.2 + 20.0,
        "grow 2 takes more free space than grow 1: {two:?} {one:?}"
    );
    assert!(
        (two.2 + one.2 + fixed.2 + 2.0 * 8.0 - frame.2).abs() <= 2.0,
        "the three tiles and their gaps fill the strip: {frame:?} {two:?} {one:?} {fixed:?}"
    );
}
