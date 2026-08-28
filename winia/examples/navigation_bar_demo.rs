//! NavigationBar item 布局变体演示（M3 规范）：
//! "In compact windows, navigation bars use vertical items.
//!  In medium windows, navigation bars should use horizontal items."
//! https://m3.material.io/components/navigation-bar/specs
//!
//! 上方：垂直 item（图标在上、label 在下，56x32 圆形指示器）
//! 下方：水平 item（图标在左、label 在右，40 高胶囊指示器横向包裹整组）
//! 徽章：Search 挂圆点徽章、Favorites 挂计数徽章（点击 Favorites 自增，
//! 对标 androidx NavigationBarItem 图标槽内 BadgedBox 用法）

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

const SEARCH_INDEX: usize = 1;
const FAVORITES_INDEX: usize = 2;

/// 一条 NavigationBar：selected 决定选中项；favorites 驱动计数徽章。
#[composable]
fn nav_bar(ctx: &mut ComposeCtx, selected: State<usize>, favorites: State<i32>, layout: NavigationItemIconPosition) {
    let sel = selected.clone();
    let fav = favorites.clone();
    NavigationBar::new(move |ctx| {
        for index in 0..DESTINATIONS.len() {
            let (name, path) = DESTINATIONS[index];
            let fav_for_icon = fav.clone();
            let fav_for_click = fav.clone();
            let sel_for_click = sel.clone();
            NavigationBarItem::new(sel.get() == index, move |ctx| {
                // 图标槽内挂徽章（BadgedBox 测量尺寸 = 锚点尺寸——不影响胶囊推导）
                if index == SEARCH_INDEX {
                    // 圆点徽章（无 content）
                    BadgedBox::new(|ctx| { Badge::new().build(ctx); })
                        .build(ctx, |ctx| {
                            Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                        });
                } else if index == FAVORITES_INDEX {
                    // 计数徽章——get() 必须在 content 闭包**内**：依赖注册到
                    // 徽章内容组（最内层 scope），状态变化才能穿透 Badge 内部组
                    // 的 Skip 重执行内容（读在组外会数字冻结，见 badge.rs 回归测试）
                    BadgedBox::new(move |ctx| {
                        Badge::new()
                            .content({
                                let fav = fav_for_icon.clone();
                                move |ctx| {
                                    let count = fav.get();
                                    Text::new(count.to_string()).build(ctx)
                                }
                            })
                            .build(ctx);
                    })
                    .build(ctx, |ctx| {
                        Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                    });
                } else {
                    Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                }
            })
            .label(move |ctx| Text::new(name).build(ctx))
            .icon_position(layout)
            .on_click(move || {
                sel_for_click.set(index);
                if index == FAVORITES_INDEX {
                    fav_for_click.update(|value| *value += 1);
                }
            })
            .build(ctx);
        }
    })
    .build(ctx);
}

#[composable]
fn navigation_bar_demo(ctx: &mut ComposeCtx) {
    let v_selected = ctx.remember(|| 0usize);
    let h_selected = ctx.remember(|| 0usize);
    let favorites = ctx.remember(|| 3i32);

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Ltr, ctx, |ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new("Compact windows - vertical items")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
                nav_bar(ctx, v_selected.clone(), favorites.clone(), NavigationItemIconPosition::Top);

                Spacer::vertical(24.0);

                Text::new("Medium windows - horizontal items")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
                nav_bar(ctx, h_selected.clone(), favorites.clone(), NavigationItemIconPosition::Start);

                Text::new(format!(
                    "vertical tab: {} | horizontal tab: {} | favorites: {} (click to +1)",
                    v_selected.get(),
                    h_selected.get(),
                    favorites.get()
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
