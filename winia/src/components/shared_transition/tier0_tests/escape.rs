use super::*;

#[test]
fn mid_flight_cancel_restores_the_natural_size() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    list_layout_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                } else {
                    detail_layout_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    let fid = *composer.shared_flights.keys().next().expect("flight id");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut composer);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let marked = marked_in(&composer)[0];
    assert_eq!(
        composer.arena_nodes()[marked].content_box(),
        crate::unit::Size::new(210.0, 130.0),
        "mid-flight the content box is the animated size"
    );

    composer.cancel_flight(fid, "test_teardown");
    assert!(composer.shared_flights.is_empty(), "cancelled");
    // No compose: only the teardown seed can drive the re-measure.
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let nodes = composer.arena_nodes();
    assert!(
        nodes[marked].flight_measure.is_none(),
        "the override is dropped"
    );
    assert_eq!(
        nodes[marked].content_box(),
        crate::unit::Size::new(300.0, 200.0),
        "the natural size comes back on the next pass"
    );
    assert_eq!(
        nodes[marked].measured_size,
        crate::unit::Size::new(300.0, 200.0),
        "…and the parent sees it too"
    );
    crate::animation::clear_all_animations();
}
