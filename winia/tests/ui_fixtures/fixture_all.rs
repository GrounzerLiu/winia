//! Single fixture binary for the UI suite.
//!
//! The dispatcher (this file) is the test-capability entry point: a scenario is a module with a
//! `pub fn main()` and one row in `SCENARIOS`, selected as `argv[1]`. Fixture scenarios were
//! cleared for the upcoming refactor; the dispatcher remains so the harness and the one-binary
//! build model stay in place.

const SCENARIOS: &[(&str, fn())] = &[];

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
