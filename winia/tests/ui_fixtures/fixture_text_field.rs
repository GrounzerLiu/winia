//! UI 测试 fixture：TextField 的焦点、键盘、多行和状态重组。

use winia::prelude::*;

#[composable]
fn text_field_fixture(ctx: &mut ComposeCtx) {
    let username = ctx.remember(|| TextFieldValue::new(""));
    let password = ctx.remember(|| TextFieldValue::new(""));
    let notes = ctx.remember(|| TextFieldValue::new(""));
    let error = ctx.remember(|| TextFieldValue::new(""));
    let readonly = ctx.remember(|| TextFieldValue::new("Read only"));
    let disabled = ctx.remember(|| TextFieldValue::new("Locked"));

    let username_text = username.get().text;
    let password_text = password.get().text;
    let notes_text = notes.get().text;
    let error_text = error.get().text;
    let readonly_text = readonly.get().text;
    let disabled_text = disabled.get().text;
    let password_valid = password_text.len() >= 4;
    let notes_lines = notes_text.lines().count().max(1);
    let error_required = error_text.is_empty();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("TextField interaction fixture").font_size(20.0).build(ctx);

            TextField::new(username.clone())
                .outlined()
                .label(|ctx| Text::new("Username").build(ctx))
                .placeholder(|ctx| Text::new("Enter username").build(ctx))
                .modifier(Modifier::new().test_tag("username-field"))
                .build(ctx);
            Text::new(format!("username: {username_text}")).build(ctx);

            TextField::new(password.clone())
                .outlined()
                .label(|ctx| Text::new("Password").build(ctx))
                .supporting_text(if password_valid { "password valid" } else { "password too short" })
                .visual_transformation(PasswordTransformation::new('•'))
                .modifier(Modifier::new().test_tag("password-field"))
                .build(ctx);
            Text::new(format!("password-length: {}", password_text.len())).build(ctx);
            Text::new(format!("password-status: {}", if password_valid { "valid" } else { "invalid" })).build(ctx);

            TextField::new(notes.clone())
                .filled()
                .label(|ctx| Text::new("Notes").build(ctx))
                .min_lines(3)
                .modifier(Modifier::new().test_tag("notes-field"))
                .build(ctx);
            Text::new(format!("notes-lines: {notes_lines}")).build(ctx);

            TextField::new(error.clone())
                .outlined()
                .label(|ctx| Text::new("Required field").build(ctx))
                .placeholder(|ctx| Text::new("Enter required value").build(ctx))
                .is_error(error_required)
                .supporting_text(if error_required { "required" } else { "accepted" })
                .modifier(Modifier::new().test_tag("error-field"))
                .build(ctx);
            Text::new(format!("error-status: {}", if error_required { "required" } else { "none" })).build(ctx);

            TextField::new(readonly.clone())
                .outlined()
                .read_only(true)
                .modifier(Modifier::new().test_tag("readonly-field"))
                .build(ctx);
            Text::new(format!("readonly-value: {readonly_text}")).build(ctx);

            TextField::new(disabled.clone())
                .filled()
                .enabled(false)
                .modifier(Modifier::new().test_tag("disabled-field"))
                .build(ctx);
            Text::new(format!("disabled-value: {disabled_text}")).build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(520.0, 860.0)
            .title("ui fixture: text_field")
            .build(ctx, text_field_fixture);
    });
}
