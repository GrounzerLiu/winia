//! TextField 演示——M3 外观（Filled/Outlined + label + 支持文本 + 错误态）
//!
//! 运行：`cargo run -p winia --example text_field_demo`

use winia::prelude::*;
use winia::ui::{TextField, TextFieldValue};

#[composable]
fn text_field_ui(ctx: &mut ComposeCtx) {
    // Filled + 悬浮 label + placeholder
    let name = ctx.remember(|| TextFieldValue::new(""));
    // Outlined + label + 支持文本
    let email = ctx.remember(|| TextFieldValue::new(""));
    // 错误态（is_error）
    let err = ctx.remember(|| TextFieldValue::new("bad input"));

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("TextField (M3 Filled + label):").font_size(14.0).build(ctx);
            TextField::new(name, |_| {})
                .filled()
                .label("Name")
                .placeholder("Enter your name")
                .build(ctx);

            Text::new("TextField (M3 Outlined + supporting):").font_size(14.0).build(ctx);
            TextField::new(email, |_| {})
                .outlined()
                .label("Email")
                .placeholder("you@example.com")
                .supporting_text("We'll never share your email")
                .build(ctx);

            Text::new("TextField (error state):").font_size(14.0).build(ctx);
            TextField::new(err, |_| {})
                .outlined()
                .label("Password")
                .is_error(true)
                .supporting_text("Password must be at least 8 characters")
                .build(ctx);

            Text::new("TextField (password mask):").font_size(14.0).build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("secret123")), |_| {})
                .filled()
                .label("Secret")
                .visual_transformation(winia::ui::PasswordTransformation::default())
                .build(ctx);

            Text::new("TextField (disabled):").font_size(14.0).build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("Locked")), |_| {})
                .filled()
                .label("Readonly")
                .enabled(false)
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 420.0)
                .title("TextField Test")
                .build(ctx, text_field_ui);
        });
    });
}
