//! UI fixture for TopAppBar nested scroll consumption.

use winia::prelude::*;

#[composable]
fn nested_scroll_fixture(ctx: &mut ComposeCtx) {
    let app_bar_state = ctx.remember(|| TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT)).get();
    let behavior = TopAppBarScrollBehavior::enter_always(app_bar_state.clone(), TOP_APP_BAR_LARGE_HEIGHT);
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let connection = behavior.nested_scroll_connection_with_scroll(scroll.clone()).expect("nested behavior connection");

    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        TopAppBar::large(|ctx| Text::new("Nested scroll").build(ctx))
            .subtitle(|ctx| Text::new("TopAppBar consumes before content").build(ctx))
            .scroll_behavior(behavior)
            .modifier(Modifier::new().test_tag("nested-appbar"))
            .build(ctx);
        Text::new(format!("height-offset: {:.0}", app_bar_state.height_offset.get())).build(ctx);
        Text::new(format!("content-offset: {:.0}", app_bar_state.content_offset.get())).build(ctx);
        Text::new(format!("child-offset: {:.0}", scroll.offset.get())).build(ctx);

        Column::new()
            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(scroll).nested_scroll(connection).test_tag("nested-content"))
            .build(ctx, |ctx| {
                for index in 0..50 {
                    Text::new(format!("Nested content {index}")).modifier(Modifier::new().padding(16.0).fill_max_width()).build(ctx);
                }
            });
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(420.0, 700.0).title("ui fixture: nested_scroll").build(ctx, nested_scroll_fixture);
    });
}
