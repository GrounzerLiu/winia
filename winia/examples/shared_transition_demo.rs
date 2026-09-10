//! Shared element transition hero demo: list ↔ detail morph.
//!
//! Deliberately large deltas on every axis so the flight is unmistakable:
//! list hero is a red circle at top-left (150x150, radius 75), detail hero is
//! a sharp blue rectangle lower-right (320x170, radius 0). Position, size,
//! aspect ratio AND corner radii all morph with a spring, crossfading 1→0 /
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
                    // Spacer pushes the detail hero down so the flight has an
                    // obvious y delta on top of the size/aspect change.
                    Column::new()
                        .modifier(Modifier::new().size(10.0, 140.0))
                        .build(ctx, |_| {});
                    // Leading spacer pushes the hero right for an obvious x delta.
                    Row::new()
                        .modifier(Modifier::new().fill_max_width())
                        .build(ctx, |ctx| {
                            Column::new()
                                .modifier(Modifier::new().size(40.0, 10.0))
                                .build(ctx, |_| {});
                            Column::new()
                                .modifier(
                                    Modifier::new()
                                        .size(320.0, 170.0)
                                        .background(Color::BLUE, Shape::Rectangle)
                                        .shared_element(
                                            scope.shared_content_state("hero"),
                                            BoundsTransform::spring(SpringSpec {
                                                stiffness: 120.0,
                                                ..SpringSpec::default()
                                            }),
                                            PlaceHolderSize::JumpCut,
                                            PathMotion::ArcBelow,
                                        ),
                                )
                                .build(ctx, |_| {});
                        });
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
                                .size(150.0, 150.0)
                                .background(Color::RED, Shape::Circle)
                                .shared_element(
                                    scope.shared_content_state("hero"),
                                    BoundsTransform::spring(SpringSpec {
                                        stiffness: 120.0,
                                        ..SpringSpec::default()
                                    }),
                                    PlaceHolderSize::JumpCut,
                                    PathMotion::ArcBelow,
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
