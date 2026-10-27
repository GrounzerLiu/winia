//! FlexBox demo — every knob live, picked with material3 SEGMENTED BUTTONS: direction, wrapping,
//! justify-content, align-items, align-content (the "lines" row) and grow.
//!
//! What to look at: with wrap off the six tiles squeeze into one line (they shrink, because CSS
//! shrink is 1 by default); pick Wrap and they fall into lines of four; justify-content distributes
//! the leftover inside each line; align-items positions each tile in its line; the "lines" row moves
//! the LINES themselves only when wrapping is on; the grow strip below fills its width from grow
//! 2 : 1 plus a fixed basis.
//!
//! Run with cargo run -p winia --example flexbox_demo.

use letclone::clone;
use std::sync::Arc;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

fn tile_color(index: usize) -> Color {
    const COLORS: [Color; 6] = [
        Color { r: 0x4C, g: 0xAF, b: 0x50, a: 255 },
        Color { r: 0x21, g: 0x96, b: 0xF3, a: 255 },
        Color { r: 0xFF, g: 0x98, b: 0x00, a: 255 },
        Color { r: 0x9C, g: 0x27, b: 0xB0, a: 255 },
        Color { r: 0x00, g: 0xBC, b: 0xD4, a: 255 },
        Color { r: 0xE9, g: 0x1E, b: 0x63, a: 255 },
    ];
    COLORS[index % COLORS.len()]
}

/// One tile of the grow strip: the basis is where it starts, grow decides how much of the free space
/// it takes.
fn grow_tile(ctx: &mut ComposeCtx, index: usize, config: FlexItemConfig) {
    Stack::new()
        .modifier(
            Modifier::new()
                .background(tile_color(index + 3), Shape::rounded(8.0))
                .test_tag(format!("fb-grow-{index}"))
                .flex(config),
        )
        .build(ctx, |_| {});
}

/// One tile: a fixed basis with a slightly different height per index, so alignment is visible.
fn tile(ctx: &mut ComposeCtx, index: usize, horizontal: bool, animate: bool) {
    // The tile sizes its CROSS axis and lets the flex basis own the main axis: a fixed main size sets
    // the min-content floor and would block shrink (CSS min-width: auto is the content's min size),
    // while a tile with NO cross size collapses to zero in a column.
    let extent = match index % 3 {
        0 => 40.0,
        1 => 64.0,
        _ => 88.0,
    };
    let modifier = if horizontal {
        Modifier::new().height(extent)
    } else {
        Modifier::new().width(96.0)
    };
    let config = FlexItemConfig::new().basis_dp(if horizontal { 96.0 } else { extent });
    // The marker must sit on the item's ROOT content, which for a flex child IS its own node.
    let mut chain = modifier
        .background(tile_color(index), Shape::rounded(8.0))
        .test_tag(format!("fb-tile-{index}"))
        .flex(config);
    if animate {
        chain = chain.animate_item();
    }
    Stack::new().modifier(chain).build(ctx, |_| {});
}

/// A labelled single-choice segmented row: one segment per value, the current one selected.
///
/// The read happens INSIDE the row's content closure on purpose: a pass-through container that reads
/// its parameters OUTSIDE (in the caller) Skips when only the selection changes, and its children keep
/// the stale highlight (docs/component-authoring.md 3.2).
fn segmented(
    ctx: &mut ComposeCtx,
    tag: &'static str,
    title: &'static str,
    options: &'static [&'static str],
    state: State<usize>,
) {
    Row::new()
        .modifier(Modifier::new().fill_max_width())
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new(title).font_size(11.0).build(ctx);
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                let selected = state.get();
                for (index, label) in options.iter().enumerate() {
                    clone!(state);
                    SegmentedButton::new(selected == index, move || state.set(index))
                        .shape(SegmentedButtonDefaults::item_shape(index, options.len()))
                        // The tag carries the selection, so a test can see the highlight move.
                        .modifier(Modifier::new().test_tag(format!(
                            "{tag}-{index}-{}",
                            if selected == index { "on" } else { "off" }
                        )))
                        .build(ctx, |ctx| Text::new(*label).font_size(10.0).build(ctx));
                }
            });
        });
}

#[composable]
fn flexbox_demo(ctx: &mut ComposeCtx) {
    let direction = ctx.remember(|| State::new(0usize)).get();
    let wrap = ctx.remember(|| State::new(0usize)).get();
    let justify = ctx.remember(|| State::new(0usize)).get();
    let align = ctx.remember(|| State::new(0usize)).get();
    let align_content = ctx.remember(|| State::new(0usize)).get();
    // Item animation is OFF by default: the tiles then carry no marker at all.
    let animate = ctx.remember(|| State::new(false)).get();
    // The ids are the animation identity: removing the FIRST one keeps the others' identity, so they
    // slide instead of re-entering.
    let ids = ctx.remember(|| State::new((0..6u64).collect::<Vec<u64>>())).get();

    let direction_value = match direction.get() {
        0 => FlexDirection::Row,
        1 => FlexDirection::Column,
        2 => FlexDirection::RowReverse,
        _ => FlexDirection::ColumnReverse,
    };
    let justify_value = match justify.get() {
        0 => FlexJustifyContent::Start,
        1 => FlexJustifyContent::End,
        2 => FlexJustifyContent::Center,
        _ => FlexJustifyContent::SpaceBetween,
    };
    let align_value = match align.get() {
        0 => FlexAlignItems::Start,
        1 => FlexAlignItems::Center,
        _ => FlexAlignItems::Stretch,
    };
    let wrap_value = match wrap.get() {
        0 => FlexWrap::NoWrap,
        1 => FlexWrap::Wrap,
        _ => FlexWrap::WrapReverse,
    };
    let align_content_value = match align_content.get() {
        0 => FlexAlignContent::Start,
        1 => FlexAlignContent::Center,
        2 => FlexAlignContent::SpaceBetween,
        _ => FlexAlignContent::Stretch,
    };

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new("FlexBox — CSS flex in winia").font_size(18.0).build(ctx);
            Text::new(format!(
                "{:?} | {:?} | justify {:?} | align {:?} | lines {:?}",
                direction_value,
                wrap_value,
                justify_value,
                align_value,
                align_content_value
            ))
            .font_size(11.0)
            .modifier(Modifier::new().test_tag("fb-status"))
            .build(ctx);

            // The parameters are picked with material3 segmented buttons: one row per knob, one
            // segment per value, so the current state is visible at a glance.
            segmented(
                ctx,
                "fb-direction",
                "direction",
                &["Row", "Column", "RowReverse", "ColumnReverse"],
                direction.clone(),
            );
            segmented(
                ctx,
                "fb-wrap",
                "wrap",
                &["NoWrap", "Wrap", "WrapReverse"],
                wrap.clone(),
            );
            segmented(
                ctx,
                "fb-justify",
                "justify",
                &["Start", "End", "Center", "Between"],
                justify.clone(),
            );
            segmented(
                ctx,
                "fb-align",
                "align",
                &["Start", "Center", "Stretch"],
                align.clone(),
            );
            segmented(
                ctx,
                "fb-lines",
                "lines",
                &["Start", "Center", "Between", "Stretch"],
                align_content.clone(),
            );

            // Item animation is off by default; the two buttons change the item list so the placement
            // spring and the fade are visible (adding appends, removing takes the FIRST item so the
            // rest have to slide).
            Row::new().spacing(10.0).build(ctx, |ctx| {
                let on = animate.get();
                Text::new(format!("item animation: {}", if on { "on" } else { "off" }))
                    .font_size(11.0)
                    .build(ctx);
                Switch::new(on)
                    .modifier(Modifier::new().test_tag("fb-animate"))
                    .on_checked_change({
                        clone!(animate);
                        move |checked| animate.set(checked)
                    })
                    .build(ctx, |_| {});
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-add"))
                    .on_click({
                        clone!(ids);
                        move || {
                            let mut next = ids.get();
                            let next_id = next.last().copied().unwrap_or(0) + 1;
                            next.push(next_id);
                            ids.set(next);
                        }
                    })
                    .build(ctx, |ctx| Text::new("add").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-remove"))
                    .on_click({
                        clone!(ids);
                        move || {
                            let mut next = ids.get();
                            if !next.is_empty() {
                                next.remove(0);
                                ids.set(next);
                            }
                        }
                    })
                    .build(ctx, |ctx| Text::new("remove").font_size(11.0).build(ctx));
            });

            // A sized frame: the FlexBox fills it, so justify-content and align-content have space to
            // distribute. The size and the tag ride on the FlexBox's OWN modifier — a pass-through
            // wrapper would Skip and never rebuild the flex lines.
            let config = FlexBoxConfig::new()
                .direction(direction_value)
                .wrap(wrap_value)
                .justify_content(justify_value)
                .align_items(align_value)
                .align_content(align_content_value)
                .gap(8.0);
            FlexBox::new()
                .config(config)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(320.0)
                        .test_tag("fb-frame"),
                )
                .build(ctx, |ctx| {
                    // Read the state INSIDE the container's content, or a change would dirty the
                    // demo's scope and this group would Skip with the old tiles (component-authoring 3.2).
                    let animate_on = animate.get();
                    let list = ids.get();
                    ctx.keyed_for_each(list, |id| *id, |ctx, id| {
                        tile(ctx, id as usize, direction_value.is_horizontal(), animate_on);
                    });
                });

            // grow shares the FREE space, so it needs a line with room left: three tiles whose grow
            // weights are 2 and 1 plus a fixed one sit in a strip that is not full. No state here, so
            // no wrapper problem.
            FlexBox::new()
                .config(FlexBoxConfig::new().gap(8.0).align_items(FlexAlignItems::Stretch))
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(56.0)
                        .test_tag("fb-grow-strip"),
                )
                .build(ctx, |ctx| {
                    grow_tile(ctx, 0, FlexItemConfig::new().grow(2.0).basis_dp(40.0));
                    grow_tile(ctx, 1, FlexItemConfig::new().grow(1.0).basis_dp(40.0));
                    grow_tile(ctx, 2, FlexItemConfig::new().basis_dp(60.0));
                });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 720.0)
                .title("FlexBox demo")
                .build(ctx, |ctx| {
                    settings::shell("FlexBox", ctx, flexbox_demo);
                });
        });
    });
}
