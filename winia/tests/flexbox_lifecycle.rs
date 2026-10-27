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

/// The FlexBox node is the layout root here; its children are the cells.
fn cells(composer: &Composer) -> Vec<(f32, f32, f32, f32)> {
    let Some(root) = composer.layout_root() else {
        return Vec::new();
    };
    root.children
        .clone()
        .into_iter()
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
