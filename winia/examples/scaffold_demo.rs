//! Scaffold demo with TopAppBar, NavigationBar bottom bar, scroll content, FAB and RTL toggle.

use winia::prelude::*;

const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
// Material 图标（24dp 视口经典路径）
const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
const PERSON_PATH: &str = "M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z";

#[composable]
fn scaffold_demo(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let rtl = ctx.remember(|| false);
    // NavigationBar 选中项 + alwaysShowLabel 切换
    let selected = ctx.remember(|| 0usize);
    let always_label = ctx.remember(|| true);
    let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };

    // TopAppBar nested scroll behavior：Standard 不可折叠，但滚动时 content_offset
    // 会累积，从而触发 scrolled 容器色。
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let app_bar_state = ctx.remember(|| TopAppBarState::new(TOP_APP_BAR_HEIGHT)).get();
    let behavior = TopAppBarScrollBehavior::new(scroll.clone(), TOP_APP_BAR_HEIGHT);
    let nested_behavior = TopAppBarScrollBehavior::enter_always(app_bar_state, TOP_APP_BAR_HEIGHT);
    let connection = nested_behavior.nested_scroll_connection_with_scroll(scroll.clone()).expect("nested behavior connection");

    let rtl_for_top_bar = rtl.clone();
    let always_for_top = always_label.clone();
    let count_for_fab = count.clone();
    let behavior_for_top = behavior.clone();
    let connection_for_content = connection.clone();
    let scroll_for_content = scroll.clone();
    let selected_for_bar = selected.clone();
    let always_for_bar = always_label.clone();

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
        Scaffold::new(move |ctx, _padding| {
            let scroll = scroll_for_content.clone();
            let conn = connection_for_content.clone();
            Column::new().modifier(Modifier::new().fill_max_size().vertical_scroll(scroll).nested_scroll(conn)).build(ctx, |ctx| {
                for index in 0..30 {
                    Text::new(format!("Content item {index}"))
                        .modifier(Modifier::new().padding(16.0).fill_max_width())
                        .build(ctx);
                }
            });
        })
        .top_bar(move |ctx| {
            TopAppBar::new(|ctx| Text::new("Scaffold demo").build(ctx))
                .scroll_behavior(behavior_for_top.clone())
                .actions(move |ctx| {
                    Button::text().on_click({ let rtl = rtl_for_top_bar.clone(); move || rtl.update(|value| *value = !*value) }).build(ctx, |ctx| Text::new("RTL").build(ctx));
                    Button::text().on_click({ let al = always_for_top.clone(); move || al.update(|value| *value = !*value) }).build(ctx, |ctx| Text::new("Label").build(ctx));
                })
                .build(ctx);
        })
        .bottom_bar(move |ctx| {
            let sel = selected_for_bar;
            let always = always_for_bar.get();
            NavigationBar::new(move |ctx| {
                let destinations = [
                    ("Home", HOME_PATH),
                    ("Search", SEARCH_PATH),
                    ("Favorites", FAVORITE_PATH),
                    ("Profile", PERSON_PATH),
                ];
                for (index, (name, path)) in destinations.iter().enumerate() {
                    let name = *name;
                    let path = *path;
                    let is_selected = sel.get() == index;
                    NavigationBarItem::new(is_selected, move |ctx| {
                        Icon::svg_path(path).size(NAVIGATION_BAR_ICON_SIZE).build(ctx);
                    })
                    .label(move |ctx| Text::new(name).build(ctx))
                    .always_show_label(always)
                    .on_click({ let sel = sel.clone(); move || sel.set(index) })
                    .build(ctx);
                }
            })
            .build(ctx);
        })
        .floating_action_button(move |ctx| {
            FloatingActionButton::new().on_click({ let count = count_for_fab.clone(); move || count.update(|value| *value += 1) }).build(ctx, |ctx| Icon::svg_path(PLUS_PATH).build(ctx));
        })
        .build(ctx);

        Text::new(format!("FAB clicks: {} | tab: {} | direction: {:?}", count.get(), selected.get(), direction))
            .modifier(Modifier::new().absolute_offset(16.0, 96.0))
            .build(ctx);
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(360.0, 640.0).title("Scaffold Demo").build(ctx, scaffold_demo);
    });
}
