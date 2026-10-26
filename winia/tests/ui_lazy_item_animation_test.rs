//! UI test for `examples/lazy_item_animation_demo.rs` (fixture scenario `lazy_item_animation`):
//! placement, fade-in and fade-out on a real `LazyColumn`, asserted off the debug server rather than
//! by looking at pixels.

#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;

/// The status line the demo prints: item count, the spec mode and whether fades are on.
fn info(app: &mut UiTest) -> String {
    app.all_texts()
        .into_iter()
        .find(|text| text.contains(" items | spec "))
        .unwrap_or_default()
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

    // INSERT at the head: the new key fades in, every other card moves down one slot. Both channels
    // of the default contract are springs at Compose's StiffnessMediumLow (400).
    let first_before = app.expect_tag("lazy-anim-item-0");
    click_tag(&mut app, "lazy-anim-insert-head");
    app.refresh();
    assert!(
        info(&mut app).contains("13 items"),
        "the list grew: {:?}",
        info(&mut app)
    );
    let new_card = app.expect_tag("lazy-anim-item-12");
    let first_after = app.expect_tag("lazy-anim-item-0");
    assert!(
        new_card.1 <= first_before.1 + 1.0,
        "the inserted card takes the first slot: new {new_card:?} vs old first {first_before:?}"
    );
    assert!(
        first_after.1 > first_before.1,
        "and the old first card moved down: {first_after:?} vs {first_before:?}"
    );
    // `components=2` narrows this to an OFFSET animation (x and y lanes), i.e. an item's placement:
    // the shell's own animations are scalars with their own 180/500 ms tweens, so a bare
    // `spring(stiffness=400` could otherwise be satisfied by something that is not the item.
    let animations = app.animations();
    assert!(
        animations
            .iter()
            .any(|line| line.contains("spring(stiffness=400") && line.contains("components=2")),
        "the default contract places items with a StiffnessMediumLow spring: {animations:?}"
    );

    // SHUFFLE: same keys, new slots — pure placement, nothing appears or disappears.
    click_tag(&mut app, "lazy-anim-shuffle");
    app.refresh();
    assert!(info(&mut app).contains("13 items"), "a shuffle changes no key count");
    let animations = app.animations();
    assert!(
        animations
            .iter()
            .any(|line| line.contains("spring(") && line.contains("components=2")),
        "a reorder animates the placement: {animations:?}"
    );

    // REMOVE from the middle: the key leaves the data and its captured frame fades out.
    click_tag(&mut app, "lazy-anim-remove-middle");
    app.refresh();
    assert!(
        info(&mut app).contains("12 items"),
        "the list shrank: {:?}",
        info(&mut app)
    );

    // TAP a card: the clicked item is removed, so the fade-out path runs for a key the test names.
    let victim = app.expect_tag("lazy-anim-item-12");
    app.click(victim.0 + victim.2 / 2.0, victim.1 + victim.3 / 2.0);
    app.expect_eventually_timeout("the tapped card is gone", Duration::from_secs(2), |app| {
        app.refresh();
        app.find_tag("lazy-anim-item-12").is_none() && info(app).contains("11 items")
    });

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
    click_tag(&mut app, "lazy-anim-insert-head");
    app.refresh();
    let animations = app.animations();
    assert!(
        animations.iter().any(|line| line.contains("tween(450ms)")),
        "the slow contract fades with a 450 ms tween: {animations:?}"
    );
    assert!(
        animations
            .iter()
            .any(|line| line.contains("spring(stiffness=180") && line.contains("components=2")),
        "and moves with its own spring: {animations:?}"
    );

    // PLACEMENT ONLY: the fades are switched off, so no tween runs at all.
    click_tag(&mut app, "lazy-anim-spec"); // slow -> default
    click_tag(&mut app, "lazy-anim-spec"); // default -> placement
    app.refresh();
    assert!(
        info(&mut app).contains("spec placement") && info(&mut app).contains("fade off"),
        "the spec cycled to placement only: {:?}",
        info(&mut app)
    );
    click_tag(&mut app, "lazy-anim-insert-head");
    app.refresh();
    // The demo's own fade curve is the only `tween(450ms)` in the app, so its absence is the
    // observable half of "fades off". (A global "no tween at all" would be wrong: the shell's app bar
    // and buttons run their own 180/500 ms tweens, which are visible in the report.)
    let animations = app.animations();
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
