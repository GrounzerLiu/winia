//! Scaffold demo with TopAppBar, bottom surface, scroll content, FAB and RTL toggle.

use winia::prelude::*;

const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

#[composable]
fn scaffold_demo(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let rtl = ctx.remember(|| false);
    let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };

    // TopAppBar nested scroll behavior：Standard 不可折叠，但滚动时 content_offset
    // 会累积，从而触发 scrolled 容器色。
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let app_bar_state = ctx.remember(|| TopAppBarState::new(TOP_APP_BAR_HEIGHT)).get();
    // Standard app bar 的容器色直接跟随共享 ScrollState；动画 fling 每帧更新
    // offset，因此回到顶部时颜色会立即恢复，不依赖下一次手势触发 nested callback。
    let behavior = TopAppBarScrollBehavior::new(scroll.clone(), TOP_APP_BAR_HEIGHT);
    let nested_behavior = TopAppBarScrollBehavior::enter_always(app_bar_state, TOP_APP_BAR_HEIGHT);
    let connection = nested_behavior.nested_scroll_connection_with_scroll(scroll.clone()).expect("nested behavior connection");

    let rtl_for_top_bar = rtl.clone();
    let count_for_fab = count.clone();
    let behavior_for_top = behavior.clone();
    let connection_for_content = connection.clone();
    let scroll_for_content = scroll.clone();

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
                })
                .build(ctx);
        })
        .bottom_bar(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(255, 235, 230, 240), Shape::Rectangle));
            ctx.end_node();
        })
        .floating_action_button(move |ctx| {
            FloatingActionButton::new().on_click({ let count = count_for_fab.clone(); move || count.update(|value| *value += 1) }).build(ctx, |ctx| Icon::svg_path(PLUS_PATH).build(ctx));
        })
        .build(ctx);

        Text::new(format!("FAB clicks: {} | direction: {:?}", count.get(), direction))
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
