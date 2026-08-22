//! NavigationRail 演示：基础侧边栏（Collapsed）+ Expressive 宽轨（收起/展开变形）

use winia::prelude::*;

const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
const PERSON_PATH: &str = "M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z";
const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

const DESTINATIONS: [(&str, &str); 4] = [
    ("Home", HOME_PATH),
    ("Search", SEARCH_PATH),
    ("Favorites", FAVORITE_PATH),
    ("Profile", PERSON_PATH),
];

#[composable]
fn navigation_rail_demo(ctx: &mut ComposeCtx) {
    let selected = ctx.remember(|| 0usize);
    let wide_state = WideNavigationRailState::new(ctx);
    let wide_sel = ctx.remember(|| 0usize);

    Row::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            // ── 基础 NavigationRail ──
            let sel_rail = selected.clone();
            NavigationRail::new(move |ctx| {
                for index in 0..DESTINATIONS.len() {
                    let (name, path) = DESTINATIONS[index];
                    NavigationRailItem::new(sel_rail.get() == index, move |ctx| {
                        Icon::svg_path(path).size(NAVIGATION_RAIL_ICON_SIZE).build(ctx);
                    })
                    .label(move |ctx| Text::new(name).build(ctx))
                    .on_click({ let s = sel_rail.clone(); move || s.set(index) })
                    .build(ctx);
                }
            })
                .header(|ctx| {
                    FloatingActionButton::new().build(ctx, |ctx| {
                        Icon::svg_path(PLUS_PATH).build(ctx);
                    });
                })
                .build(ctx);

            Column::new()
                .modifier(Modifier::new().fill_max_height().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new(format!("目的地：{}", DESTINATIONS[selected.get()].0)).build(ctx);
                });

            // ── Expressive 宽轨（点击按钮收起/展开）──
            let state_btn = wide_state.clone();
            Button::text()
                .on_click(move || state_btn.toggle())
                .build(ctx, |ctx| Text::new("Toggle").build(ctx));
            let sel_wide = wide_sel.clone();
            let state_wide = wide_state.clone();
            let progress = ctx.animate_float_as_state(
                if wide_state.is_expanded() { 1.0 } else { 0.0 },
                winia::animation::AnimationSpec::Spring(winia::animation::SpringSpec {
                    damping_ratio: 1.0, stiffness: 400.0, mass: 1.0, threshold: 0.01,
                }),
            );
            let _ = state_wide;
            WideNavigationRail::new(wide_state.clone(), move |ctx| {
                for index in 0..DESTINATIONS.len() {
                    let (name, path) = DESTINATIONS[index];
                    WideNavigationRailItem::new(
                        sel_wide.get() == index,
                        move |ctx| {
                            Icon::svg_path(path).size(NAVIGATION_RAIL_ICON_SIZE).build(ctx);
                        },
                        move |ctx| Text::new(name).build(ctx),
                    )
                    .progress(progress.clone())
                    .on_click({ let s = sel_wide.clone(); move || s.set(index) })
                    .build(ctx);
                }
            })
            .header(|ctx| {
                FloatingActionButton::new().build(ctx, |ctx| {
                    Icon::svg_path(PLUS_PATH).build(ctx);
                });
            })
            .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(900.0, 480.0).title("Navigation Rail Demo").build(ctx, navigation_rail_demo);
    });
}
