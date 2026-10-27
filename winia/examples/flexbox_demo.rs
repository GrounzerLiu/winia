//! FlexBox demo — every knob live: direction, wrapping, justify-content, align-items and grow.
//!
//! What to look at: with wrap off the six tiles squeeze into one line (they shrink, because CSS
//! shrink is 1 by default); switch wrap on and they fall into lines of three; justify-content
//! distributes the leftover inside each line; align-items positions each tile in its line; grow makes
//! the tiles share the whole width instead of keeping their basis.
//!
//! Run with cargo run -p winia --example flexbox_demo.

use letclone::clone;
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
fn tile(ctx: &mut ComposeCtx, index: usize, horizontal: bool) {
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
    Stack::new()
        .modifier(
            modifier
                .background(tile_color(index), Shape::rounded(8.0))
                .test_tag(format!("fb-tile-{index}"))
                .flex(config),
        )
        .build(ctx, |_| {});
}

#[composable]
fn flexbox_demo(ctx: &mut ComposeCtx) {
    let direction = ctx.remember(|| State::new(0usize)).get();
    let wrap = ctx.remember(|| State::new(false)).get();
    let justify = ctx.remember(|| State::new(0usize)).get();
    let align = ctx.remember(|| State::new(0usize)).get();
    let align_content = ctx.remember(|| State::new(0usize)).get();

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
    let wrap_value = if wrap.get() { FlexWrap::Wrap } else { FlexWrap::NoWrap };
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

            Row::new().spacing(6.0).build(ctx, |ctx| {
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-direction"))
                    .on_click({
                        clone!(direction);
                        move || direction.set((direction.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("direction").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-wrap"))
                    .on_click({
                        clone!(wrap);
                        move || wrap.set(!wrap.get())
                    })
                    .build(ctx, |ctx| Text::new("wrap").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-justify"))
                    .on_click({
                        clone!(justify);
                        move || justify.set((justify.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("justify").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-align"))
                    .on_click({
                        clone!(align);
                        move || align.set((align.get() + 1) % 3)
                    })
                    .build(ctx, |ctx| Text::new("align").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("fb-align-content"))
                    .on_click({
                        clone!(align_content);
                        move || align_content.set((align_content.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("lines").font_size(11.0).build(ctx));
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
                    for index in 0..6 {
                        tile(ctx, index, direction_value.is_horizontal());
                    }
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
