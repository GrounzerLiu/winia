//! TextField 演示

use winia::prelude::*;
use winia::ui::{TextField, TextFieldValue, TextChange};
use winia::app;

fn text_field_ui(ctx: &mut ComposeCtx) {
    let text = ctx.remember(|| TextFieldValue::new("Type here..."));
    let label = ctx.remember(|| String::new());

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("TextField Demo")
                .font_size(22.0).font_weight(FontWeight::BOLD)
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            // TextField
            TextField::new(text.clone(), {
                let l = label.clone();
                move |v| { l.set(v.text); }
            })
            .modifier(Modifier::new()
                .size(300.0, 36.0)
                .padding(8.0)
                .background(Color::from_argb(40, 200, 200, 200), Shape::rounded(4.0)))
            .build(ctx);

            Text::new(format!("Value: \"{}\"", label.get()))
                .font_size(12.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 200.0)
                .title("TextField Demo")
                .build(ctx, text_field_ui);
        });
    });
}
