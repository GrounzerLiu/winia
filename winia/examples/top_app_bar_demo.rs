//! Material 3 Large TopAppBar with shared-offset collapse behavior.

use winia::prelude::*;

const ARROW_BACK_PATH: &str = "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z";
const MORE_VERT_PATH: &str = "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";

#[composable]
fn top_app_bar_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let behavior = TopAppBarScrollBehavior::new(scroll.clone(), TOP_APP_BAR_LARGE_HEIGHT);
    let fraction = behavior.collapse_fraction();
    let height = TOP_APP_BAR_LARGE_HEIGHT - (TOP_APP_BAR_LARGE_HEIGHT - TOP_APP_BAR_HEIGHT) * fraction;

    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        TopAppBar::large(|ctx| Text::new("TopAppBar").build(ctx))
            .navigation_icon(|ctx| Icon::svg_path(ARROW_BACK_PATH).size(24.0).build(ctx))
            .actions(|ctx| Icon::svg_path(MORE_VERT_PATH).size(24.0).build(ctx))
            .subtitle(|ctx| Text::new("Scroll to collapse the shared-offset app bar").build(ctx))
            .scroll_behavior(behavior)
            .build(ctx);
        Text::new(format!("collapse fraction: {fraction:.2} | height: {height:.0}dp"))
            .font_size(12.0)
            .modifier(Modifier::new().padding(8.0))
            .build(ctx);

        Column::new()
            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(scroll))
            .build(ctx, |ctx| {
                for index in 0..40 {
                    Text::new(format!("Scrollable content {index}"))
                        .modifier(Modifier::new().padding(16.0).fill_max_width())
                        .build(ctx);
                }
            });
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new().size(520.0, 760.0).title("TopAppBar Demo").build(ctx, top_app_bar_demo);
        });
    });
}
