//! UI fixture for Scaffold slot geometry, FAB clicks, and RTL placement.

use winia::prelude::*;

const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

#[composable]
fn scaffold_fixture(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let rtl = ctx.remember(|| false);
    let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
    let rtl_for_top = rtl.clone();
    let count_for_fab = count.clone();

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
        Scaffold::new(|ctx, _padding| {
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().fill_max_size().background(Color::from_argb(255, 245, 245, 245), Shape::Rectangle).test_tag("scaffold-content-probe"));
            ctx.end_node();
        })
        .top_bar(move |ctx| {
            TopAppBar::new(|ctx| Text::new("Scaffold fixture").build(ctx))
                .actions(move |ctx| {
                    Button::text().on_click({ let rtl = rtl_for_top.clone(); move || rtl.update(|value| *value = !*value) }).modifier(Modifier::new().test_tag("direction-toggle")).build(ctx, |ctx| Text::new("RTL").build(ctx));
                })
                .modifier(Modifier::new().test_tag("scaffold-top"))
                .build(ctx);
        })
        .bottom_bar(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(255, 230, 225, 235), Shape::Rectangle).test_tag("scaffold-bottom"));
            ctx.end_node();
        })
        .floating_action_button(move |ctx| {
            FloatingActionButton::new().on_click({ let count = count_for_fab.clone(); move || count.update(|value| *value += 1) }).modifier(Modifier::new().test_tag("scaffold-fab")).build(ctx, |ctx| Icon::svg_path(PLUS_PATH).build(ctx));
        })
        .build(ctx);

        Text::new(format!("fab-count: {}", count.get())).modifier(Modifier::new().absolute_offset(8.0, 72.0)).build(ctx);
        Text::new(format!("direction: {}", if direction == LayoutDirection::Rtl { "rtl" } else { "ltr" })).modifier(Modifier::new().absolute_offset(8.0, 92.0)).build(ctx);
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(360.0, 640.0).title("ui fixture: scaffold").build(ctx, scaffold_fixture);
    });
}
