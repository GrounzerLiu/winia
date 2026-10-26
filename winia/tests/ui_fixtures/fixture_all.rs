//! Single fixture binary for the UI suite.
//!
//! The dispatcher (this file) is the test-capability entry point: a scenario is a module with a
//! `pub fn main()` and one row in `SCENARIOS`, selected as `argv[1]`. Fixture scenarios were
//! cleared for the upcoming refactor; the dispatcher remains so the harness and the one-binary
//! build model stay in place.

#[path = "../../examples/flow_features_demo.rs"]
mod flow_features;
#[path = "../../examples/item_animation_demo.rs"]
mod item_animation;
#[path = "../../examples/flex_layout_demo.rs"]
mod flex_layout;
#[path = "../../examples/box_layout_demo.rs"]
mod box_layout;
#[path = "../../examples/lazy_layout_demo.rs"]
mod lazy_layout;
#[path = "../../examples/lazy_ops_demo.rs"]
mod lazy_ops_demo;
#[path = "../../examples/lazy_item_animation_demo.rs"]
mod lazy_item_animation;
#[path = "../../examples/grid_demo.rs"]
mod grid_demo;

const SCENARIOS: &[(&str, fn())] = &[
    ("flow_features", flow_features::main),
    ("item_animation", item_animation::main),
    ("flex_layout", flex_layout::main),
    ("box_layout", box_layout::main),
    ("lazy_layout", lazy_layout::main),
    ("lazy_ops", lazy_ops_demo::main),
    ("lazy_item_animation", lazy_item_animation::main),
    ("grid", grid_demo::main),
];

fn main() {
    let scenario = std::env::args().nth(1).unwrap_or_default();
    match SCENARIOS.iter().find(|(name, _)| *name == scenario) {
        Some((_, run)) => run(),
        None => {
            let names: Vec<&str> = SCENARIOS.iter().map(|(name, _)| *name).collect();
            eprintln!(
                "fixture_all: unknown or empty scenario `{scenario}`\n  usage: fixture_all <scenario>\n  scenarios: {}",
                names.join(", ")
            );
            std::process::exit(2);
        }
    }
}
