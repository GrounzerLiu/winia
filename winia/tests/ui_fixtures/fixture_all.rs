//! Single fixture binary for the UI suite — one scenario per process, one skia link for all of them.
//!
//! The harness spawns this binary with the scenario name as `argv[1]`:
//! `UiTest::launch("overlay_focus")` runs `fixture_all.exe overlay_focus`. Each scenario still gets
//! its own PROCESS, so the isolation the suite relies on (a fresh window, a fresh `ACTIVE_ANIMATIONS`,
//! a fixture that can panic without taking the suite down) is unchanged.
//!
//! What changes is the build. A fixture used to be its own `[[bin]]`, i.e. its own full link against
//! skia plus its own debug info: measured at 18 fixtures — 538 MB of executables and 2.26 GB of PDBs,
//! and every lib change relinked all of them (cargo links in parallel, which is where the memory spike
//! came from). Here a scenario is a module of this one binary: adding one costs a module and a match
//! arm, not another 30 MB executable and 125 MB PDB.
//!
//! The `fixture_*.rs` files are ordinary modules in this crate — their `fn main` is just a function
//! this dispatcher calls (it starts the event loop and never returns).

#[path = "fixture_click.rs"]
mod click;
#[path = "fixture_dialog_dismiss.rs"]
mod dialog_dismiss;
#[path = "fixture_nest.rs"]
mod nest;
#[path = "fixture_nested_scroll.rs"]
mod nested_scroll;
#[path = "fixture_overlay.rs"]
mod overlay;
#[path = "fixture_overlay_focus.rs"]
mod overlay_focus;
#[path = "fixture_panic.rs"]
mod panic_fixture;
#[path = "fixture_popup_content.rs"]
mod popup_content;
#[path = "fixture_popup_drag.rs"]
mod popup_drag;
#[path = "fixture_popup_slide_tap.rs"]
mod popup_slide_tap;
#[path = "fixture_popup_tap.rs"]
mod popup_tap;
#[path = "fixture_range_slider.rs"]
mod range_slider;
#[path = "fixture_resize.rs"]
mod resize;
#[path = "fixture_scaffold.rs"]
mod scaffold;
#[path = "fixture_scroll.rs"]
mod scroll;
#[path = "fixture_search_results.rs"]
mod search_results;
#[path = "fixture_subwindow.rs"]
mod subwindow;
#[path = "fixture_text_field.rs"]
mod text_field;
#[path = "fixture_toggle.rs"]
mod toggle;
#[path = "fixture_top_app_bar.rs"]
mod top_app_bar;

/// Every scenario this binary can run: the name the harness passes as `argv[1]` and the module entry
/// point it dispatches to. THIS TABLE IS THE REGISTRY — a scenario missing from it cannot be launched,
/// and the usage message is generated from it, so adding a scenario is one row here plus its
/// `#[path] mod` above (nothing else to keep in sync).
///
/// One scenario per process, always: the fixture modules are separate compositions that happen to
/// share this binary, and two of them declare a `#[composable] fn ui` at the same line and column, so
/// composing two scenarios in one process would alias their scope keys.
const SCENARIOS: &[(&str, fn())] = &[
    ("click", click::main),
    ("dialog_dismiss", dialog_dismiss::main),
    ("nest", nest::main),
    ("nested_scroll", nested_scroll::main),
    ("overlay", overlay::main),
    ("overlay_focus", overlay_focus::main),
    ("panic", panic_fixture::main),
    ("popup_content", popup_content::main),
    ("popup_drag", popup_drag::main),
    ("popup_slide_tap", popup_slide_tap::main),
    ("popup_tap", popup_tap::main),
    ("range_slider", range_slider::main),
    ("resize", resize::main),
    ("scaffold", scaffold::main),
    ("scroll", scroll::main),
    ("search_results", search_results::main),
    ("subwindow", subwindow::main),
    ("text_field", text_field::main),
    ("toggle", toggle::main),
    ("top_app_bar", top_app_bar::main),
];

fn main() {
    let scenario = std::env::args().nth(1).unwrap_or_default();
    match SCENARIOS.iter().find(|(name, _)| *name == scenario) {
        Some((_, run)) => run(),
        None => {
            let names: Vec<&str> = SCENARIOS.iter().map(|(name, _)| *name).collect();
            eprintln!(
                "fixture_all: unknown scenario `{scenario}`\n  usage: fixture_all <scenario>\n  scenarios: {}",
                names.join(", ")
            );
            std::process::exit(2);
        }
    }
}
