//! Shared pieces of the lazy-list demos: the card chrome, a tile, the palette, a toolbar button.
//!
//! Both `lazy_layout_demo` and `lazy_ops_demo` include this module (`#[path = "common/lazy_kit.rs"]`),
//! and so does the `fixture_all` dispatcher that turns them into UI-test scenarios — the same
//! arrangement `common/settings.rs` uses.

use winia::prelude::*;

pub(crate) const ITEMS: usize = 200;

pub(crate) fn card(ctx: &mut ComposeCtx, tag: &'static str, title: &str, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .test_tag(tag)
                .background(Color::from_argb(14, 60, 90, 140), Shape::rounded(8.0))
                .padding(10.0),
        )
        .spacing(6.0)
        .build(ctx, |ctx| {
            Text::new(title)
                .modifier(Modifier::new().test_tag(format!("{tag}-title")))
                .font_size(14.0)
                .build(ctx);
            content(ctx);
        });
}

pub(crate) fn tile(ctx: &mut ComposeCtx, tag: String, label: String, w: f32, h: f32, color: Color) {
    Stack::new()
        .content_alignment(ContentAlignment::CENTER)
        .modifier(
            Modifier::new()
                .size(w, h)
                .test_tag(tag)
                .background(color, Shape::rounded(6.0)),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).color(Color::WHITE).build(ctx);
        });
}

// `Color::from_argb` is not a `const fn`, so the palette is a function, not constants.
/// A reader announces a collection by the name the caller gives it; the component only declares the
/// item count ("RTL row, list, 200 items"). A container with no name of its own gets UIA's fallback,
/// which concatenates the children's names into "0 1 2 3 …".
pub(crate) fn list_name(label: &str) -> Modifier {
    Modifier::new().semantics(SemanticsConfig::new().content_description(label))
}

pub(crate) fn palette(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 90, 160, 220)
    } else {
        Color::from_argb(255, 120, 200, 140)
    }
}

pub(crate) fn accent(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 220, 150, 90)
    } else {
        Color::from_argb(255, 200, 110, 60)
    }
}

pub(crate) const INITIAL: usize = 50;
pub(crate) const STEP: usize = 5;
/// Pages in the pager row.
pub(crate) const PAGES: usize = 5;

pub(crate) fn action(ctx: &mut ComposeCtx, tag: &'static str, label: &str, on_click: impl Fn() + Send + Sync + 'static) {
    Button::new()
        .modifier(Modifier::new().test_tag(tag))
        .on_click(on_click)
        .build(ctx, |ctx| Text::new(label).font_size(12.0).build(ctx));
}

