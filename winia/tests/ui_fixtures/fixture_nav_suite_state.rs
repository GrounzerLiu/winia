//! Deterministic acceptance fixture for a navigation suite's movable icon and label payloads.
//!
//! Tagged buttons select the bar or rail explicitly; transitions are disabled so requested layout
//! text cannot masquerade as an actual move. Each icon's stored Column owns a remembered counter,
//! updated by a separate control without reading that counter in the caller. Each label's nested
//! Column renders a fresh String captured from caller State, exercising closure refresh as well as
//! retained state and node identity. The fixture binary uses production composable statement keys.

use std::sync::{Arc, Mutex};
use winia::components::navigation_suite::{NavigationSuiteScaffold, NavigationSuiteType};
use winia::prelude::*;

const ITEMS: usize = 3;

#[composable]
fn nav_suite_state_fixture(ctx: &mut ComposeCtx) {
    let layout_type = ctx.remember(|| NavigationSuiteType::ShortNavigationBarCompact);
    let caller_label = ctx.remember(|| 0usize);
    // Only handles cross this boundary: the counter reads and their dependencies stay in each icon.
    let counters = ctx
        .remember(|| Arc::new(Mutex::new(vec![None::<State<usize>>; ITEMS])))
        .get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            // Subscribe the actual caller container, so a clean outer Column cannot swallow input.
            let requested_layout = layout_type.get();
            let label_version = caller_label.get();
            Row::new()
                .modifier(Modifier::new().fill_max_width().height(40.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({
                            let layout_type = layout_type.clone();
                            move || layout_type.set(NavigationSuiteType::ShortNavigationBarCompact)
                        })
                        .modifier(Modifier::new().size(120.0, 40.0).test_tag("nav-state-show-bar"))
                        .build(ctx, |ctx| Text::new("Show bar").font_size(12.0).build(ctx));
                    Button::new()
                        .on_click({
                            let layout_type = layout_type.clone();
                            move || layout_type.set(NavigationSuiteType::WideNavigationRailCollapsed)
                        })
                        .modifier(Modifier::new().size(120.0, 40.0).test_tag("nav-state-show-rail"))
                        .build(ctx, |ctx| Text::new("Show rail").font_size(12.0).build(ctx));
                    Button::new()
                        .on_click({
                            let caller_label = caller_label.clone();
                            move || caller_label.update(|value| *value += 1)
                        })
                        .modifier(Modifier::new().size(140.0, 40.0).test_tag("nav-state-label-next"))
                        .build(ctx, |ctx| Text::new("Label +1").font_size(12.0).build(ctx));
                });
            Row::new()
                .modifier(Modifier::new().fill_max_width().height(40.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    for index in 0..ITEMS {
                        let counters = counters.clone();
                        Button::new()
                            .on_click(move || {
                                let counter = counters.lock().expect("icon counter handles")[index]
                                    .clone()
                                    .expect("the icon counter must exist before input");
                                counter.update(|value| *value += 1);
                            })
                            .modifier(
                                Modifier::new()
                                    .size(140.0, 40.0)
                                    .test_tag(format!("nav-state-increment-{index}")),
                            )
                            .build(ctx, |ctx| {
                                Text::new(format!("Icon {index} +1")).font_size(12.0).build(ctx);
                            });
                    }
                });
            NavigationSuiteScaffold::new(
                |items| {
                    for index in 0..ITEMS {
                        let counters = counters.clone();
                        let label = format!("L{index}: {label_version}");
                        items.item(
                            index == 0,
                            move |ctx| {
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .size(40.0 + index as f32 * 8.0, 28.0)
                                            .test_tag(format!("nav-state-icon-{index}")),
                                    )
                                    .build(ctx, |ctx| {
                                        // The State belongs to the payload's own stored Column.
                                        let counter = ctx.remember(|| 0usize);
                                        counters.lock().expect("icon counter handles")[index] =
                                            Some(counter.clone());
                                        Text::new(format!("I{index}: {}", counter.get()))
                                            .font_size(12.0)
                                            .modifier(Modifier::new().test_tag(format!("nav-state-count-{index}")))
                                            .build(ctx);
                                    });
                            },
                            move |ctx| {
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .size(56.0, 16.0)
                                            .test_tag(format!("nav-state-label-{index}")),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new(label.clone())
                                            .font_size(12.0)
                                            .modifier(Modifier::new().test_tag(format!("nav-state-label-text-{index}")))
                                            .build(ctx);
                                    });
                            },
                            || {},
                        );
                    }
                },
                |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_size().test_tag("nav-state-page"))
                        .build(ctx, |ctx| Text::new("Page content").font_size(14.0).build(ctx));
                },
            )
            .layout_type(requested_layout)
            .transition(false)
            .modifier(Modifier::new().layout_weight(1.0).test_tag("nav-state-suite"))
            .build(ctx);
        });
}

/// The registered `nav_suite_state` scenario starts the real event loop without timed state writes.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(500.0, 440.0)
                .title("Navigation Suite State Fixture")
                .build(ctx, |ctx| nav_suite_state_fixture(ctx));
        });
    });
}
