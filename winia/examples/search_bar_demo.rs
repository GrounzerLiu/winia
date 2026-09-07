//! SearchBar demo (cf. Compose Material3 SearchBar).
//!
//! - Top: fullscreen SearchBar. Tap the pill to expand, type to filter the fruit
//!   list, Enter triggers on_search (closes), tap a result to pick + close.
//! - Bottom: DockedSearchBar. Same data, results drop down under the bar.
//!
//! Run: cargo run -p winia --example search_bar_demo

use winia::prelude::*;
use std::sync::Arc;

const FRUITS: &[&str] = &[
    "Apple", "Apricot", "Avocado", "Banana", "Blackberry", "Blueberry", "Cherry", "Coconut",
    "Cranberry", "Date", "Dragonfruit", "Elderberry", "Fig", "Grape", "Grapefruit", "Guava",
    "Kiwi", "Lemon", "Lime", "Lychee", "Mango", "Melon", "Nectarine", "Orange", "Papaya",
    "Peach", "Pear", "Pineapple", "Plum", "Pomegranate", "Raspberry", "Strawberry",
    "Tangerine", "Watermelon",
];

fn filtered(query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    if q.is_empty() {
        return FRUITS.iter().map(|s| s.to_string()).collect();
    }
    FRUITS
        .iter()
        .filter(|s| s.to_lowercase().contains(&q))
        .map(|s| s.to_string())
        .collect()
}

fn search_icon(ctx: &mut ComposeCtx) {
    Icon::svg_path(SEARCH_ICON_PATH).size(24.0).build(ctx);
}

fn result_item(ctx: &mut ComposeCtx, label: String, picked: State<String>, close: State<bool>) {
    let s2 = label.clone();
    ListItem::new(move |ctx| {
        Text::new(label.clone()).build(ctx);
    })
        .on_click(move || {
            picked.set(s2.clone());
            close.set(true);
        })
        .build(ctx);
}

fn result_list(ctx: &mut ComposeCtx, items: Arc<Vec<String>>, picked: State<String>, close: State<bool>) {
    let items_clone = items.clone();
    LazyColumn::new()
        .modifier(Modifier::new().fill_max_width().fill_max_height())
        .items_from(
            items_clone,
            |s: &String| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                s.hash(&mut h);
                h.finish()
            },
            move |ctx, _i, s| {
                result_item(ctx, s.clone(), picked.clone(), close.clone());
            },
        )
        .build(ctx);
}

/// Docked dropdown list: plain Column (upstream `Column(content)` — all items
/// composed upfront so the slide-in animation reveals them coherently; a
/// LazyColumn would lazily compose the last item mid-animation and it would
/// pop in late).
fn docked_list(ctx: &mut ComposeCtx, items: Arc<Vec<String>>, picked: State<String>, close: State<bool>) {
    Column::new()
        .modifier(Modifier::new().fill_max_width())
        .build(ctx, |ctx| {
            for s in items.iter() {
                result_item(ctx, s.clone(), picked.clone(), close.clone());
            }
        });
}

#[composable]
fn search_bar_demo(ctx: &mut ComposeCtx) {
    let full_state = ctx.remember(SearchBarState::new).get();
    let docked_state = ctx.remember(SearchBarState::new).get();
    let picked = ctx.remember(|| State::new("—".to_string())).get();
    let close_full = ctx.remember(|| State::new(false)).get();
    // close_full consumed below → close fullscreen after pick
    if close_full.get() {
        full_state.close();
        close_full.set(false);
    }
    let full_query = full_state.query_text();
    let full_items: Arc<Vec<String>> = Arc::new(filtered(&full_query));
    let docked_query = docked_state.query_text();
    // Docked dropdown shows top suggestions only (plain Column — must fit;
    // upstream samples likewise show a handful of rows, not the whole list).
    let docked_items: Arc<Vec<String>> =
        Arc::new(filtered(&docked_query).into_iter().take(5).collect());

    // on_search closes (Compose convention: caller deactivates inside onSearch).
    let fs = full_state.clone();
    let ds = docked_state.clone();
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new(format!("Picked: {}", picked.get()))
                .font_size(16.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("Fullscreen SearchBar (tap pill → type → Enter/tap result)")
                .font_size(12.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            SearchBar::new()
                .state(full_state.clone())
                .on_search(move |_| fs.close())
                .placeholder(|ctx| {
                    Text::new("Search fruits…").build(ctx);
                })
                .leading_icon(search_icon)
                .build(ctx, {
                    let full_items = full_items.clone();
                    let picked = picked.clone();
                    let close_full = close_full.clone();
                    move |ctx| {
                        result_list(ctx, full_items.clone(), picked.clone(), close_full.clone());
                    }
                });
            Text::new("DockedSearchBar (dropdown under the bar)")
                .font_size(12.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            DockedSearchBar::new()
                .state(docked_state.clone())
                .dropdown_shadow_elevation(4.0)
                .on_search(move |_| ds.close())
                .placeholder(|ctx| {
                    Text::new("Search fruits…").build(ctx);
                })
                .leading_icon(search_icon)
                .build(ctx, {
                    let docked_items = docked_items.clone();
                    let picked = picked.clone();
                    let ds = docked_state.clone();
                    move |ctx| {
                        // Dropdown sizes to content (plain Column of ≤5 rows).
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                // Tapping a result picks + closes the dropdown.
                                // (remembered: a fresh State each build would drop
                                // the close flag before it is consumed.)
                                let close_docked = ctx.remember(|| State::new(false)).get();
                                let ds2 = ds.clone();
                                if close_docked.get() {
                                    ds2.close();
                                    close_docked.set(false);
                                }
                                docked_list(ctx, docked_items.clone(), picked.clone(), close_docked);
                            });
                    }
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 700.0)
                .title("SearchBar Demo")
                .build(ctx, search_bar_demo);
        });
    });
}
