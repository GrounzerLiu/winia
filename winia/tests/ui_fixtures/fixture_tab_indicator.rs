//! UI-test fixture: a `TabRow` whose indicator is supplied by the caller (`TabRow::indicator`).
//!
//! Drives `tab_indicator_*` in `ui_test.rs`. The point is the SLOT: the caller's closure runs during
//! measurement (with the positions the row just computed), and what it draws has to land on the
//! selected tab. This is the path that had three defects, all invisible to a unit test because they
//! need a real `#[composable]` frame: the macro's statement-key injection into a TWO-parameter content
//! closure, the adopted subtree's geometry, and the child count a policy sees.
//!
//! Two things here are deliberate:
//! - the slot composes a REAL component (`Canvas`), not just a value — that is what exercises the key
//!   injection and the adoption path;
//! - the indicator carries a test tag, so the test reads its geometry out of the tree instead of
//!   looking at pixels, and a `sel N` line gives the test something to wait for.

use letclone::clone;
use winia::prelude::*;

#[composable]
fn tab_indicator_fixture(ctx: &mut ComposeCtx) {
    let sel = ctx.remember(|| 0usize);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Button::text()
                    .on_click({
                        clone!(sel);
                        move || sel.set(0)
                    })
                    .modifier(Modifier::new().test_tag("pick-first"))
                    .build(ctx, |ctx| Text::new("first").build(ctx));
                Button::text()
                    .on_click({
                        clone!(sel);
                        move || sel.set(2)
                    })
                    .modifier(Modifier::new().test_tag("pick-third"))
                    .build(ctx, |ctx| Text::new("third").build(ctx));
            });

            // What the test waits on: the selection, printed.
            Text::new(format!("sel {}", sel.get()))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            TabRow::new(sel.get(), {
                clone!(sel);
                move |ctx| {
                    for i in 0..3 {
                        let label = format!("Tab {}", i + 1);
                        Tab::new({ clone!(sel); sel.get() == i }, { clone!(sel); move || sel.set(i) })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                }
            })
            .indicator({
                clone!(sel);
                move |ctx, scope| {
                    let _ = sel.get();
                    let (x, w) = match scope.selected_position() {
                        Some(p) => (p.left + (p.width - p.content_width) / 2.0, p.content_width),
                        None => (0.0, 0.0),
                    };
                    Canvas::new()
                        .modifier(
                            Modifier::new()
                                .width(w)
                                .height(4.0)
                                .offset(x, 44.0)
                                .test_tag("custom-indicator"),
                        )
                        .build(ctx, move |ds| {
                            // Canvas coordinates: `rect()` is the node's own rect in that space.
                            let r = ds.rect();
                            ds.draw_round_rect(
                                skia_safe::Rect::from_xywh(r.left, r.top, r.width(), r.height()),
                                r.height() / 2.0,
                                Color::from_argb(255, 0x1B, 0x5E, 0x20),
                            );
                        });
                }
            })
            .build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 240.0)
                .title("Tab Indicator Fixture")
                .build(ctx, |ctx| tab_indicator_fixture(ctx));
        });
    });
}
