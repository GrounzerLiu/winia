//! UI test for `examples/lazy_item_animation_demo.rs` (fixture scenario `lazy_item_animation`):
//! placement, fade-in and fade-out on a real `LazyColumn`, asserted off the debug server rather than
//! by looking at pixels.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;

/// The item count the status line reports. The text arrives wrapped as "text(12 items | …)", so the
/// digits are taken from the END of the part before " items".
fn count(app: &mut UiTest) -> u32 {
    info(app)
        .split(" items")
        .next()
        .and_then(|head| head.rsplit(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Polls the animation report for a line that matches, so a spring that settles in a few hundred
/// milliseconds is caught before it is gone. `extra` narrows the line further (e.g. to an OFFSET
/// animation, which carries `components=2`).
fn saw_animation(app: &mut UiTest, needle: &str, extra: Option<&str>) -> bool {
    for _ in 0..8 {
        let lines = app.animations();
        if lines
            .iter()
            .any(|line| line.contains(needle) && extra.is_none_or(|extra| line.contains(extra)))
        {
            return true;
        }
        app.refresh();
    }
    false
}

/// The status line the demo prints: item count, the spec mode, whether fades are on, and the scroll
/// offset. The tree dump can lag one frame behind an action, so this retries instead of returning an
/// empty string that a caller would read as "0 items".
fn info(app: &mut UiTest) -> String {
    for _ in 0..6 {
        if let Some(text) = app
            .all_texts()
            .into_iter()
            .find(|text| text.contains(" items | spec "))
        {
            return text;
        }
        app.refresh();
    }
    String::new()
}

fn click_tag(app: &mut UiTest, tag: &str) {
    let (x, y, w, h) = app.expect_tag(tag);
    app.click(x + w / 2.0, y + h / 2.0);
}

#[test]
fn lazy_item_animation_animates_inserts_reorders_and_removals() {
    let mut app = UiTest::launch("lazy_item_animation");

    app.expect_text("Lazy item animation");
    app.expect_tag("lazy-anim-list");
    app.expect_eventually_timeout("the list reports itself", Duration::from_secs(2), |app| {
        app.refresh();
        info(app).contains("12 items") && info(app).contains("spec default")
    });

    // TAP a card first, while the list is still at the top: the debug tree reports an item in the
    // LIST's content coordinates, and those equal screen coordinates only at offset 0. A tap anywhere
    // later would have to have the scroll offset subtracted, which the report does not give per item.
    let first = app.expect_tag("lazy-anim-item-0");
    app.click(first.0 + first.2 / 2.0, first.1 + first.3 / 2.0);
    app.expect_eventually_timeout("the tapped card is gone", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("lazy-anim-item-0").is_none()
    });
    assert_eq!(count(&mut app), 11, "and the data lost that key: {:?}", info(&mut app));

    // INSERT in the MIDDLE: the rows below open a gap and animate into it, and the new key fades in.
    // (An insert at the HEAD is silent only while the list is SCROLLED, where the viewport moves with
    // the rows; at the top it reveals the new row and animates the rows down. Both cases are pinned in
    // the lazy suite, and a middle insert is the unambiguous placement case here.)
    let below_before = app.expect_tag("lazy-anim-item-6").1;
    click_tag(&mut app, "lazy-anim-insert-middle");
    app.refresh();
    assert!(
        info(&mut app).contains("12 items"),
        "the list grew back: {:?}",
        info(&mut app)
    );
    app.expect_tag("lazy-anim-item-12");
    let below_after = app.expect_tag("lazy-anim-item-6").1;
    assert!(
        below_after > below_before,
        "the rows below the insert moved down: {below_after} vs {below_before}"
    );
    // `components=2` narrows this to an OFFSET animation (x and y lanes), i.e. an item's placement:
    // the shell's own animations are scalars with their own 180/500 ms tweens, so a bare
    // `spring(stiffness=400` could otherwise be satisfied by something that is not the item.
    assert!(
        saw_animation(&mut app, "spring(stiffness=400", Some("components=2")),
        "the default contract places items with a StiffnessMediumLow spring"
    );

    // SHUFFLE: same keys, new slots — pure placement, nothing appears or disappears.
    click_tag(&mut app, "lazy-anim-shuffle");
    app.refresh();
    assert!(info(&mut app).contains("12 items"), "a shuffle changes no key count");
    assert!(
        saw_animation(&mut app, "spring(", Some("components=2")),
        "a reorder animates the placement"
    );

    // REMOVE from the middle: the key leaves the data, so its captured frame fades out. The ghost is
    // a Skia picture with no tag of its own, so what this asserts is the DATA change (count); the
    // picture itself is covered by the render tests.
    click_tag(&mut app, "lazy-anim-remove-middle");
    app.refresh();
    assert!(
        info(&mut app).contains("11 items"),
        "the list shrank: {:?}",
        info(&mut app)
    );

    // The SLOW contract: a bouncier placement spring and 450 ms tweens for the fades, so the two
    // channels are distinguishable in the animation report.
    click_tag(&mut app, "lazy-anim-spec"); // default -> placement
    click_tag(&mut app, "lazy-anim-spec"); // placement -> slow
    app.refresh();
    assert!(
        info(&mut app).contains("spec slow") && info(&mut app).contains("fade on"),
        "the spec cycled to the slow contract: {:?}",
        info(&mut app)
    );
    click_tag(&mut app, "lazy-anim-insert-middle");
    app.refresh();
    assert!(
        saw_animation(&mut app, "tween(450ms)", None),
        "the slow contract fades with a 450 ms tween"
    );
    assert!(
        saw_animation(&mut app, "spring(stiffness=180", Some("components=2")),
        "and moves with its own spring"
    );

    // PLACEMENT ONLY: the fades are switched off. The demo's own fade curve is the only
    // `tween(450ms)` in the app, so its absence is the observable half of "fades off" — a global
    // "no tween at all" would be wrong, the shell runs its own 180/500 ms tweens.
    click_tag(&mut app, "lazy-anim-spec"); // slow -> default
    click_tag(&mut app, "lazy-anim-spec"); // default -> placement
    app.refresh();
    assert!(
        info(&mut app).contains("spec placement") && info(&mut app).contains("fade off"),
        "the spec cycled to placement only: {:?}",
        info(&mut app)
    );
    click_tag(&mut app, "lazy-anim-insert-middle");
    app.refresh();
    let animations = app.animations();
    assert!(!animations.is_empty(), "the animation report answered: {animations:?}");
    assert!(
        !animations.iter().any(|line| line.contains("tween(450ms)")),
        "without_fade_in/out leaves no item fade running: {animations:?}"
    );
    assert!(
        animations
            .iter()
            .any(|line| line.contains("spring(stiffness=400") && line.contains("components=2")),
        "the placement still animates: {animations:?}"
    );
}
