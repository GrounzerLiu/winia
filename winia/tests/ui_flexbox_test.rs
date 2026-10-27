//! FlexBox through a real window. The demo picks its parameters with material3 SEGMENTED BUTTONS, so
//! each control is a direct choice rather than a cycle: the test selects Wrap, then justify End, then
//! the Column direction. align / align-content have no assertion here, and grow is demonstrated by the
//! fixed strip rather than a control.
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
    app.click_tag("fb-wrap-1-off");
    // The segment's tag carries its selection state, so this is the highlight moving, not just the
    // layout reacting.
    app.expect_eventually("the Wrap segment becomes the selected one", |app| {
        app.refresh();
        app.find_tag("fb-wrap-1-on").is_some() && app.find_tag("fb-wrap-0-on").is_none()
    });
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
    app.click_tag("fb-justify-1-off");
    app.expect_eventually("the End segment becomes the selected one", |app| {
        app.refresh();
        app.find_tag("fb-justify-1-on").is_some()
    });
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
    app.click_tag("fb-direction-1-off");
    app.expect_eventually("the Column segment becomes the selected one", |app| {
        app.refresh();
        app.find_tag("fb-direction-1-on").is_some() && app.find_tag("fb-direction-0-on").is_none()
    });
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

    // The remaining two pickers are covered for the highlight at least: a pass-through wrapper that
    // reads the state outside its content Skips and leaves the old segment lit.
    app.click_tag("fb-align-2-off");
    app.expect_eventually("the Stretch segment becomes the selected one", |app| {
        app.refresh();
        app.find_tag("fb-align-2-on").is_some() && app.find_tag("fb-align-0-on").is_none()
    });
    app.click_tag("fb-lines-2-off");
    app.expect_eventually("the Between lines segment becomes the selected one", |app| {
        app.refresh();
        app.find_tag("fb-lines-2-on").is_some()
    });

    // Item animation: the switch turns the marker on (it is off by default), and the item list is what
    // the springs animate. The ghost itself is pinned by the unit tests; here the geometry shows the
    // list changing under the container.
    // The demo is in the Column direction at this point, so the items stack down; compare against the
    // rect the first tile occupied rather than assuming an axis.
    // Six tiles to begin with, and the switch is off by default.
    let tile_count = |app: &mut UiTest| {
        (0..16u32)
            .filter(|id| app.find_tag(&format!("fb-tile-{id}")).is_some())
            .count()
    };
    app.refresh();
    assert_eq!(tile_count(&mut app), 6, "the demo starts with six tiles");
    app.click_tag("fb-animate");
    app.click_tag("fb-remove");
    app.expect_eventually("the first tile is gone", |app| {
        app.refresh();
        app.find_tag("fb-tile-0").is_none()
    });
    app.refresh();
    // The list is what the springs animate; the slot geometry depends on the justification the earlier
    // steps left behind, so only the list itself is asserted here (the spring's origin is pinned by
    // layout::flexbox's own test).
    assert_eq!(tile_count(&mut app), 5, "removing takes one item out of the list");
    assert!(
        app.find_tag("fb-tile-1").is_some(),
        "the remaining items keep their identity"
    );
    app.click_tag("fb-add");
    app.expect_eventually("adding appends a tile", |app| {
        app.refresh();
        (0..16u32).filter(|id| app.find_tag(&format!("fb-tile-{id}")).is_some()).count() == 6
    });

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

/// Reported: with item animation ON, resizing the window made the tiles disappear.
#[test]
fn the_tiles_survive_a_window_resize_with_animation_on() {
    let mut app = UiTest::launch("flexbox");
    app.click_tag("fb-animate");
    app.expect_eventually("the switch is on", |app| {
        app.refresh();
        app.find_tag("fb-tile-0").is_some()
    });
    let before: Vec<(f32, f32, f32, f32)> = (0..6u32)
        .filter_map(|id| app.find_tag(&format!("fb-tile-{id}")))
        .collect();
    assert_eq!(before.len(), 6, "six tiles before the resize");

    app.resize(520.0, 900.0);
    app.refresh();
    let after: Vec<String> = (0..6u32)
        .filter_map(|id| {
            app.find_tag(&format!("fb-tile-{id}"))
                .map(|rect| format!("{id}:{rect:?}"))
        })
        .collect();
    assert_eq!(
        after.len(),
        6,
        "the resize must keep every tile ({after:?})"
    );
}

/// Reported: with item animation ON, resizing the window made the tiles disappear. The placement
/// spring is what moves them, so a resize is exactly when a stuck or NaN channel would show: the tiles
/// must still be PAINTED at their slots once the motion settles.
#[test]
fn the_tiles_stay_painted_after_a_resize_with_animation_on() {
    let mut app = UiTest::launch("flexbox");
    app.click_tag("fb-animate");
    // Resize like a drag (many small steps), then jump wide and narrow.
    for width in (300..=440).step_by(20) {
        app.resize(width as f32, 900.0);
    }
    app.resize(900.0, 900.0);
    app.resize(320.0, 780.0);
    // The slot geometry comes from the tree; the pixels come from the frame the app actually painted.
    app.expect_eventually("every tile is painted at its slot", |app| {
        app.refresh();
        let tiles: Vec<(f32, f32, f32, f32)> = (0..6u32)
            .filter_map(|id| app.find_tag(&format!("fb-tile-{id}")))
            .collect();
        if tiles.len() != 6 {
            return false;
        }
        tiles.iter().all(|rect| {
            let centre = (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
            match app.pixel_at_logical(centre.0, centre.1) {
                // The demo's surface is near-black; a tile is a saturated palette colour.
                Some((r, g, b, a)) => a == 255 && (r as u32 + g as u32 + b as u32) > 150,
                None => false,
            }
        })
    });
}
