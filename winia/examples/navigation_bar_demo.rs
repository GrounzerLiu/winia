//! NavigationBar item 布局变体演示（M3 规范）：
//! "In compact windows, navigation bars use vertical items.
//!  In medium windows, navigation bars should use horizontal items."
//! https://m3.material.io/components/navigation-bar/specs
//!
//! 上方：垂直 item（图标在上、label 在下，56x32 圆形指示器）
//! 下方：水平 item（图标在左、label 在右，40 高胶囊指示器横向包裹整组）

use winia::prelude::*;

// Material 图标（24dp 视口经典路径）
const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
const PERSON_PATH: &str = "M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z";

const DESTINATIONS: [(&str, &str); 4] = [
    ("Home", HOME_PATH),
    ("Search", SEARCH_PATH),
    ("Favorites", FAVORITE_PATH),
    ("Profile", PERSON_PATH),
];

#[composable]
fn navigation_bar_demo(ctx: &mut ComposeCtx) {
    let v_selected = ctx.remember(|| 0usize);
    let h_selected = ctx.remember(|| 0usize);

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Ltr, ctx, |ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new("Compact windows - vertical items")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
                // 垂直 item（默认布局——紧凑窗口）
                let vs = v_selected.clone();
                NavigationBar::new(move |ctx| {
                    for index in 0..DESTINATIONS.len() {
                        let (name, path) = DESTINATIONS[index];
                        NavigationBarItem::new(vs.get() == index, move |ctx| {
                            Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                        })
                        .label(move |ctx| Text::new(name).build(ctx))
                        .on_click({ let vs = vs.clone(); move || vs.set(index) })
                        .build(ctx);
                    }
                })
                .build(ctx);

                Spacer::vertical(24.0);

                Text::new("Medium windows - horizontal items")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
                // 水平 item（中等窗口——图标在左、label 在右）
                let hs = h_selected.clone();
                NavigationBar::new(move |ctx| {
                    for index in 0..DESTINATIONS.len() {
                        let (name, path) = DESTINATIONS[index];
                        NavigationBarItem::new(hs.get() == index, move |ctx| {
                            Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                        })
                        .label(move |ctx| Text::new(name).build(ctx))
                        .layout(NavigationBarItemLayout::Horizontal)
                        .on_click({ let hs = hs.clone(); move || hs.set(index) })
                        .build(ctx);
                    }
                })
                .build(ctx);

                Text::new(format!(
                    "vertical tab: {} | horizontal tab: {}",
                    v_selected.get(),
                    h_selected.get()
                ))
                .modifier(Modifier::new().padding(16.0))
                .build(ctx);
            });
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(720.0, 420.0).title("Navigation Bar Demo").build(ctx, navigation_bar_demo);
    });
}
