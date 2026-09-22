//! UI-test fixture: the expanded SearchBar's results follow the query.
//!
//! Drives `ui_test.rs`'s `search_results_follow_the_query`: the caller filters a small list from
//! `SearchBarState::query_text()` **in its own scope** (the ordinary Compose sample shape) and passes
//! the resulting `Arc` into the content lambda. Typing must narrow what the panel shows; the items
//! carry `test_tag`s so the test can ask the overlay tree for them by name.
//!
//! The panel is opened from the page (a button calls `state.open()`) instead of by tapping the pill, so
//! the test does not depend on the bar's geometry; the panel focuses its field on expansion, which is
//! what lets the debug key stream reach the query.

use letclone::clone;
use winia::prelude::*;
use std::sync::Arc;

const FRUITS: &[&str] = &["Apple", "Banana", "Blackberry", "Blueberry", "Cherry"];

fn filtered(query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    FRUITS
        .iter()
        .filter(|s| q.is_empty() || s.to_lowercase().contains(&q))
        .map(|s| s.to_string())
        .collect()
}

#[composable]
fn search_results_fixture(ctx: &mut ComposeCtx) {
    let state = ctx.remember(SearchBarState::new).get();
    let query = state.query_text();
    let items: Arc<Vec<String>> = Arc::new(filtered(&query));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new(format!("query: {query}")).build(ctx);
            Button::text()
                .on_click({
                    clone!(state);
                    move || state.open()
                })
                .modifier(Modifier::new().test_tag("open-search"))
                .build(ctx, |ctx| {
                    Text::new("Open search").build(ctx);
                });

            SearchBar::new()
                .state(state.clone())
                .placeholder(|ctx| {
                    Text::new("Search").build(ctx);
                })
                .build(ctx, {
                    clone!(items);
                    move |ctx| {
                        for item in items.iter() {
                            Text::new(item.clone())
                                .modifier(Modifier::new().test_tag(format!("item-{item}")))
                                .build(ctx);
                        }
                    }
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 520.0)
                .title("Search Results Fixture")
                .build(ctx, |ctx| search_results_fixture(ctx));
        });
    });
}
