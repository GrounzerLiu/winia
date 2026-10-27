//! FlexBox through the real compositor: two lifecycle questions a measure-only unit test cannot
//! answer.
//!
//! 1. FlexBox::config_for resolves at MEASURE time, so a State it reads must still re-run the layout
//!    when it changes (the read registers a layout dependency, not a composition one).
//! 2. A child's own Modifier::flex(..) changing at runtime must reach the parent's measure, even
//!    though the container's own parameters did not change.

use letclone::clone;
use winia::layout::{Constraints, FlexBox, FlexBoxConfig, FlexDirection, FlexItemConfig, Stack};
use winia::prelude::*;
use winia::runtime::composer::Composer;

fn leaf(ctx: &mut ComposeCtx, width: f32, height: f32, flex: FlexItemConfig) {
    Stack::new()
        .modifier(Modifier::new().size(width, height).flex(flex))
        .build(ctx, |_| {});
}

/// Descend through single-child wrappers (a Column around the FlexBox) to the line's cells.
fn cells(composer: &Composer) -> Vec<(f32, f32, f32, f32)> {
    let Some(root) = composer.layout_root() else {
        return Vec::new();
    };
    let mut ids = root.children.clone();
    let mut guard = 0;
    while ids.len() == 1 && !composer.arena_nodes()[ids[0]].children.is_empty() && guard < 4 {
        ids = composer.arena_nodes()[ids[0]].children.clone();
        guard += 1;
    }
    ids.into_iter()
        .map(|child| {
            let node = &composer.arena_nodes()[child];
            (
                node.position.x,
                node.position.y,
                node.measured_size.width,
                node.measured_size.height,
            )
        })
        .collect()
}

#[test]
fn a_config_for_resolver_sees_a_state_change() {
    let mut composer = Composer::new();
    let handle: std::sync::Arc<std::sync::Mutex<Option<State<bool>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut frame = |composer: &mut Composer| {
        clone!(handle);
        composer.compose(winia::compose!(|ctx| {
            let tall = ctx.remember(|| State::new(false)).get();
            *handle.lock().unwrap() = Some(tall.clone());
            FlexBox::new()
                .config_for(move |_| {
                    if tall.get() {
                        FlexBoxConfig::new().direction(FlexDirection::Column)
                    } else {
                        FlexBoxConfig::new()
                    }
                })
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    leaf(ctx, 20.0, 40.0, FlexItemConfig::new());
                    leaf(ctx, 20.0, 40.0, FlexItemConfig::new());
                });
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
    };
    for _ in 0..2 {
        frame(&mut composer);
    }
    let row = cells(&composer);
    assert!(
        (row[0].0 - 0.0).abs() < 0.5 && (row[1].0 - 20.0).abs() < 0.5 && (row[1].1 - 0.0).abs() < 0.5,
        "the default direction lays the cells out in a row: {row:?}"
    );

    let tall = handle.lock().unwrap().clone().unwrap();
    tall.set(true);
    for _ in 0..2 {
        frame(&mut composer);
    }
    let column = cells(&composer);
    assert!(
        (column[0].0 - 0.0).abs() < 0.5
            && (column[1].0 - 0.0).abs() < 0.5
            && (column[1].1 - 40.0).abs() < 0.5,
        "a State read inside config_for re-runs the layout as a column: {column:?}"
    );
}

#[test]
fn a_child_flex_change_reaches_the_parent() {
    let mut composer = Composer::new();
    let handle: std::sync::Arc<std::sync::Mutex<Option<State<bool>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut frame = |composer: &mut Composer| {
        clone!(handle);
        composer.compose(winia::compose!(|ctx| {
            let grow = ctx.remember(|| State::new(false)).get();
            *handle.lock().unwrap() = Some(grow.clone());
            FlexBox::new()
                .config(FlexBoxConfig::new())
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    let factor = if grow.get() { 1.0 } else { 0.0 };
                    leaf(ctx, 50.0, 20.0, FlexItemConfig::new().grow(factor));
                    leaf(ctx, 50.0, 20.0, FlexItemConfig::new().grow(factor));
                });
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
    };
    for _ in 0..2 {
        frame(&mut composer);
    }
    let off = cells(&composer);
    assert!(
        (off[0].2 - 50.0).abs() < 0.5 && (off[1].2 - 50.0).abs() < 0.5,
        "without grow the cells keep their content width: {off:?}"
    );

    let grow = handle.lock().unwrap().clone().unwrap();
    grow.set(true);
    for _ in 0..2 {
        frame(&mut composer);
    }
    let on = cells(&composer);
    assert!(
        (on[0].2 - 150.0).abs() < 0.5 && (on[1].2 - 150.0).abs() < 0.5,
        "a changed child grow re-measures the parent: {on:?}"
    );
}

/// The reviewer's case for the resolver: the value is read in an ANCESTOR scope, so the FlexBox's own
/// parameters do not change. A new closure is not a comparable parameter, which is why a
/// resolver-bearing FlexBox takes the non-restartable container — otherwise the Skip decision would
/// keep the previous closure and the new direction would never appear.
#[test]
fn a_resolver_capturing_an_ancestor_value_still_updates() {
    let mut composer = Composer::new();
    let handle: std::sync::Arc<std::sync::Mutex<Option<State<bool>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut frame = |composer: &mut Composer| {
        clone!(handle);
        composer.compose(winia::compose!(|ctx| {
            let narrow = ctx.remember(|| State::new(false)).get();
            *handle.lock().unwrap() = Some(narrow.clone());
            Column::new().build(ctx, |ctx| {
                // Read OUTSIDE the FlexBox: this is the ancestor-scope capture the review flagged.
                let value = narrow.get();
                FlexBox::new()
                    .config_for(move |_| {
                        if value {
                            FlexBoxConfig::new().direction(FlexDirection::Column)
                        } else {
                            FlexBoxConfig::new()
                        }
                    })
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        leaf(ctx, 20.0, 40.0, FlexItemConfig::new());
                        leaf(ctx, 20.0, 40.0, FlexItemConfig::new());
                    });
            });
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
    };
    for _ in 0..2 {
        frame(&mut composer);
    }
    let row = cells(&composer);
    assert!(
        (row[1].0 - 20.0).abs() < 0.5 && (row[1].1 - 0.0).abs() < 0.5,
        "the default config lays the cells out in a row: {row:?}"
    );

    let narrow = handle.lock().unwrap().clone().unwrap();
    narrow.set(true);
    for _ in 0..2 {
        frame(&mut composer);
    }
    let column = cells(&composer);
    assert!(
        (column[1].0 - 0.0).abs() < 0.5 && (column[1].1 - 40.0).abs() < 0.5,
        "the captured value reaches the resolver through the non-restartable container: {column:?}"
    );
}
