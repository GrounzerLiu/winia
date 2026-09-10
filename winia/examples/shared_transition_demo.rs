//! Shared element transition hero demo: list ↔ detail morph.
//!
//! The hero box flies between its list bounds (small, red, radius 8) and its
//! detail bounds (large, blue, radius 24) with a spring, crossfading 1→0 /
//! 0→1. Run: `cargo run -p winia --example shared_transition_demo`

use letclone::clone;
use winia::animation::SpringSpec;
use winia::prelude::*;

#[composable]
fn hero_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .build(ctx, |ctx| {
                if show_detail.get() {
                    Text::new("Detail").font_size(24.0).build(ctx);
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(320.0, 200.0)
                                .background(Color::BLUE, Shape::rounded(24.0))
                                .shared_element(
                                    scope.shared_content_state("hero"),
                                    BoundsTransform::spring(SpringSpec::default()),
                                ),
                        )
                        .build(ctx, |_| {});
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            move || show_detail.set(false)
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Back").build(ctx);
                        });
                } else {
                    Text::new("List (click Open)").font_size(24.0).build(ctx);
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(160.0, 100.0)
                                .background(Color::RED, Shape::rounded(8.0))
                                .shared_element(
                                    scope.shared_content_state("hero"),
                                    BoundsTransform::spring(SpringSpec::default()),
                                ),
                        )
                        .build(ctx, |_| {});
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            move || show_detail.set(true)
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Open").build(ctx);
                        });
                }
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("Shared Transition Hero")
                .build(ctx, |ctx| {
                    hero_demo(ctx);
                });
        });
    });
}
