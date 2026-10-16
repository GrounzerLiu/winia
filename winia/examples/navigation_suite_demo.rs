//! NavigationSuiteScaffold 演示：一套 items，按窗口尺寸类自动切换导航形态
//! （窄窗→底栏 / 其余→**收起**宽轨，与上游一致——展开要显式指定；拖拽窗口边缘实时切换）。
//! 底栏形态另带一个 `primary_action`（FAB 槽）：它浮在底栏上方 16dp。

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;


const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
const PERSON_PATH: &str = "M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z";
/// Material Icons `add` (the FAB's glyph).
const ADD_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

const DESTINATIONS: [(&str, &str); 4] = [
    ("Home", HOME_PATH),
    ("Search", SEARCH_PATH),
    ("Favorites", FAVORITE_PATH),
    ("Profile", PERSON_PATH),
];

#[composable]
fn navigation_suite_demo(ctx: &mut ComposeCtx) {
    let selected = ctx.remember(|| 0usize);
    let suite_state = remember_navigation_suite_scaffold_state(ctx);
    let (w, h) = window_size();
    NavigationSuiteScaffold::new(
        {
            clone!(selected);
            move |items| {
                for (index, (name, path)) in DESTINATIONS.iter().enumerate() {
                    let (name, path) = (*name, *path);
                    items.item(
                        selected.get() == index,
                        move |ctx| Icon::svg_path(path).size(24.0).build(ctx),
                        move |ctx| Text::new(name).build(ctx),
                        {
                            clone!(selected);
                            move || selected.set(index)
                        },
                    );
                }
            }
        },
        {
            let suite_state = suite_state.clone();
            clone!(selected);
            move |ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_size().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new(format!("当前：{}", DESTINATIONS[selected.get()].0)).build(ctx);
                    Text::new(format!("窗口 {w}x{h}")).build(ctx);
                    // `NavigationSuiteScaffoldState`: the whole suite slides away and the content
                    // takes the box (upstream animates the same way).
                    let visible = suite_state.is_visible();
                    Button::filled()
                        .on_click({
                            clone!(suite_state);
                            move || {
                                if suite_state.is_visible() { suite_state.hide() } else { suite_state.show() }
                            }
                        })
                        .build(ctx, |ctx| {
                            Text::new(if visible { "Hide navigation" } else { "Show navigation" }).build(ctx);
                        });
                });
        }
        },
    )
    .state(suite_state)
    // The FAB slot: upstream places it only in the BAR layout, 16dp above it (`:456-470`).
    .primary_action(|ctx| {
        FloatingActionButton::new()
            .on_click(|| {})
            .build(ctx, |ctx| {
                Icon::svg_path(ADD_PATH).size(24.0).build(ctx);
            });
    })
    .build(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        // Follow the system theme: without a theme node the window is pinned to the light default.
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(900.0, 544.0)
                .title("Navigation Suite Demo")
                .build(ctx, |ctx| {
                    settings::shell("Navigation Suite Demo", ctx, navigation_suite_demo);
                });
        });
    });
}
