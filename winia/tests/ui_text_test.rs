//! UI coverage for the Text demo: what the parameters do to the MEASURED box, and what the two
//! nowrap overflows do to the PAINTED one.

#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

/// The demo's surface is near-black; a lit glyph sums well above this.
const LIT: u32 = 150;

/// The band just past a box: `count` samples `from` logical pixels to its right, on the row through
/// the box's vertical centre.
fn lit(app: &mut UiTest, x: f32, y: f32) -> bool {
    match app.pixel_at_logical(x, y) {
        Some((r, g, b, a)) => a == 255 && (r as u32 + g as u32 + b as u32) > LIT,
        None => false,
    }
}

fn lit_samples(app: &mut UiTest, rect: (f32, f32, f32, f32), count: i32) -> usize {
    let y = rect.1 + rect.3 / 2.0;
    (2..2 + count)
        .filter(|offset| lit(app, rect.0 + rect.2 + *offset as f32, y))
        .count()
}

#[test]
fn the_type_scale_and_letter_spacing_change_the_measured_box() {
    let mut app = UiTest::launch("text");
    let small = app.expect_tag("tx-scale-0");
    let medium = app.expect_tag("tx-scale-1");
    let large = app.expect_tag("tx-scale-2");
    assert!(
        small.3 < medium.3 && medium.3 < large.3,
        "14 < 18 < 22 sp: {small:?} {medium:?} {large:?}"
    );

    let tight = app.expect_tag("tx-space-0");
    let wide = app.expect_tag("tx-space-1");
    // Four letters, three gaps of 4 dp: at least 10 dp wider, and the row does not grow taller.
    assert!(
        wide.2 > tight.2 + 10.0,
        "letter spacing 4 widens the same string: {tight:?} -> {wide:?}"
    );
    assert!(
        (wide.3 - tight.3).abs() < 1.0,
        "letter spacing does not change the line height: {tight:?} {wide:?}"
    );
}

#[test]
fn max_lines_caps_the_height() {
    let mut app = UiTest::launch("text");
    let one = app.expect_tag("tx-lines-0");
    let three = app.expect_tag("tx-lines-1");
    assert_eq!(one.2, 180.0, "both boxes are 180 dp wide: {one:?}");
    assert_eq!(three.2, 180.0);
    assert!(
        one.3 < three.3,
        "max_lines(1) is shorter than max_lines(3): {one:?} {three:?}"
    );
}

#[test]
fn a_nowrap_ellipsis_stops_at_the_box_and_visible_does_not() {
    let mut app = UiTest::launch("text");
    let ellipsis = app.expect_tag("tx-nowrap-0");
    let visible = app.expect_tag("tx-nowrap-1");
    assert_eq!(ellipsis.2, 140.0, "the box is 140 dp: {ellipsis:?}");
    assert_eq!(visible.2, 140.0);

    // The gap is 24 dp wide, so 20 samples clear the next box.
    let clipped = lit_samples(&mut app, ellipsis, 20);
    let spilled = lit_samples(&mut app, visible, 20);
    assert_eq!(
        clipped, 0,
        "an ellipsis truncates the line to its box, nothing paints past it"
    );
    assert!(
        spilled > 0,
        "Visible paints past the box (the control that proves the band is really examined)"
    );
}

/// Before the render used the paragraph's drawn width for the alignment slack, a CENTRED nowrap
/// ellipsis computed `content - intrinsic` where the intrinsic width is the PRE-ellipsis one: the
/// line was shifted out of its own box (measured: 14 lit samples to the LEFT of a 300 dp box).
#[test]
fn a_centred_nowrap_ellipsis_stays_inside_its_box() {
    let mut app = UiTest::launch("text");
    let rect = app.expect_tag("tx-nowrap-center");
    assert_eq!(rect.2, 300.0, "the box is 300 dp: {rect:?}");
    assert_eq!(rect.3, 20.0, "one ellipsized line: {rect:?}");
    let y = rect.1 + rect.3 / 2.0;

    let spilled = (2..40).filter(|offset| lit(&mut app, rect.0 - *offset as f32, y)).count();
    assert_eq!(spilled, 0, "nothing paints left of the box");

    let inside = (4..60).filter(|offset| lit(&mut app, rect.0 + *offset as f32, y)).count();
    assert!(inside > 0, "the line is drawn inside the box (not just blank)");
}

/// Compose's `minLines`: the box reserves the lines even though the text is one line.
#[test]
fn min_lines_reserves_the_box_height() {
    let mut app = UiTest::launch("text");
    let one = app.expect_tag("tx-one-line");
    let three = app.expect_tag("tx-minlines");
    assert!(
        (three.3 - one.3 * 3.0).abs() < 2.0,
        "three reserved lines are three line boxes: one={one:?} three={three:?}"
    );
}
