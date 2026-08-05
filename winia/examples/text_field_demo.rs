//! Minimal TextField 测试

use winia::prelude::*;
use winia::ui::{TextField, TextFieldValue};
use winia::app;

#[composable]
fn text_field_ui(ctx: &mut ComposeCtx) {
    let text = ctx.remember(|| TextFieldValue::new("Type here..."));

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Below is a TextField:").font_size(14.0).build(ctx);
            TextField::new(text, |_| {}).build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 150.0)
                .title("TextField Test")
                .build(ctx, text_field_ui);
        });
    });
}
