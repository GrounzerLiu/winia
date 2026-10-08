//! UI-test fixture: THE acceptance case for movable content.
//!
//! The navigation suite's items are composed as movable content, one handle per item, so a shape
//! switch moves the caller's payloads instead of rebuilding them. The evidence is a counter inside the
//! ICON payload — the place a caller actually keeps per-item state — which only advances when the
//! payload's slot is created. Before movable content the round trip rebuilt every item.
//!
//! CURRENTLY IT READS `markers 6` — the divergence is STILL THERE, one re-initialization per item on
//! the first shape switch. This fixture exists so the remaining bug is measurable rather than argued
//! about: read `markers N flips M` from the semantics tree, and 3 is the number that means fixed. What
//! the trace says so far: the payload's `remember` uses the scaffold's explicit key on the first
//! composition and a statement-derived key on later ones, and the item components do NOT subcompose
//! (checked: no `subcompose` in navigation_bar.rs / navigation_rail.rs / short_navigation_bar.rs), so
//! the payload is reached by two different composition paths and therefore lands in two different
//! slots. That is what to chase next.
//!
//! Why a fixture and not a unit test: in a `#[test]` build the `remember` key falls back to the path
//! hash (production keys come from the `#[composable]` statement id), so statements that share a path
//! share one counter and the key moves when a shape composes the payloads in a different order. This
//! binary is not `cfg(test)`, so it exercises exactly the keys a real app does.
//!
//! The shape flips on its own every `SWITCH_MS` (a background thread moves a `State`, and the test's
//! `refresh()` recomposes), so the test needs no debug-server command for it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use winia::components::navigation_suite::{NavigationSuiteScaffold, NavigationSuiteType};
use winia::prelude::*;

/// How many times an item payload's `remember` initializer has run. It must stay at 3 (one per item).
static MARKERS: AtomicUsize = AtomicUsize::new(0);
/// The flip counter, published by the composition so the background thread can move it — a plain
/// atomic would change nothing on screen, because nothing would invalidate the composition.
static TICKS: OnceLock<State<u64>> = OnceLock::new();
/// How often the shape flips.
const SWITCH_MS: u64 = 350;
/// How many items the suite shows.
const ITEMS: usize = 3;

#[composable]
fn nav_suite_state_fixture(ctx: &mut ComposeCtx) {
    let ticks: State<u64> = ctx.remember(|| 0);
    let _ = TICKS.set(ticks.clone());
    // Reading it subscribes this composition to the flips, so the background thread's writes land.
    let flipped = ticks.get();
    let layout_type = if flipped % 2 == 0 {
        NavigationSuiteType::ShortNavigationBarCompact
    } else {
        NavigationSuiteType::WideNavigationRailCollapsed
    };
    Column::new()
        .modifier(Modifier::new().fill_max_size().background(Color::BLACK, Shape::Rectangle))
        .build(ctx, |ctx| {
            // Two readouts the test reads from the semantics tree: the live marker count (the
            // assertion) and which shape is showing (the proof the switch happened).
            let counts = format!("markers {} flips {}", MARKERS.load(Ordering::SeqCst), flipped);
            let shape = if flipped % 2 == 0 { "shape bar" } else { "shape rail" };
            Text::new(&counts).font_size(14.0).build(ctx);
            Text::new(shape).font_size(14.0).build(ctx);
            NavigationSuiteScaffold::new(
                |items| {
                    for index in 0..ITEMS {
                        items.item(
                            index == 0,
                            move |ctx| {
                                // The caller's own state inside the payload — what must survive.
                                let marker: State<usize> =
                                    ctx.remember(|| MARKERS.fetch_add(1, Ordering::SeqCst));
                                let _ = marker.get();
                                let w = 20.0 + index as f32 * 4.0;
                                Column::new()
                                    .modifier(Modifier::new().size(w, w))
                                    .build(ctx, |_ctx| {});
                            },
                            move |ctx| {
                                Text::new(&format!("item {index}")).font_size(12.0).build(ctx);
                            },
                            || {},
                        );
                    }
                },
                |ctx| {
                    Text::new("page").font_size(12.0).build(ctx);
                },
            )
            .layout_type(layout_type)
            .build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(SWITCH_MS));
        if let Some(ticks) = TICKS.get() {
            ticks.update(|v| *v += 1);
        }
    });
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(500.0, 300.0)
                .title("Navigation Suite State Fixture")
                .build(ctx, |ctx| nav_suite_state_fixture(ctx));
        });
    });
}
