//! TextField 演示——M3 外观 + text-field-v2 容器化（label/placeholder/图标闭包）
//!
//! 运行：`cargo run -p winia --example text_field_demo`

use winia::prelude::*;
use winia::ui::{TextField, TextFieldValue};

// label 悬浮字号动画：展开 16sp（bodyLarge）↔ 悬浮 12sp（bodySmall）——
// M3 specs：label 展开 16 / 悬浮 12，progress 闭包参数驱动
fn label_size(p: f32) -> f32 {
    16.0 + (12.0 - 16.0) * p
}

#[composable]
fn text_field_ui(ctx: &mut ComposeCtx) {
    // Filled + 悬浮 label + placeholder（闭包内容）
    let name = ctx.remember(|| TextFieldValue::new(""));
    // Outlined + 前后缀 + 前后图标（闭包内容）
    let email = ctx.remember(|| TextFieldValue::new("13812345678"));
    // 错误态（is_error）
    let err = ctx.remember(|| TextFieldValue::new("bad input"));

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("TextField (Filled + label + leading icon):").font_size(14.0).build(ctx);
            TextField::new(name, |_| {})
                .filled()
                .label(|ctx, p| {
                    Text::new("Name").font_size(label_size(p.get())).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .placeholder(|ctx, alpha| {
                    let c = WiniaTheme::colors().on_surface_variant;
                    Text::new("Enter your name").font_size(16.0)
                        .color(winia::modifier::Color::from_argb((255.0 * alpha.get()) as u8, c.r, c.g, c.b))
                        .build(ctx);
                })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);

            Text::new("TextField (Outlined + prefix/suffix + trailing icon):").font_size(14.0).build(ctx);
            TextField::new(email, |_| {})
                .outlined()
                .label(|ctx, p| {
                    Text::new("Phone").font_size(label_size(p.get())).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .prefix(|ctx| {
                    Text::new("+86 ").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .suffix(|ctx| {
                    Text::new(" 🇨🇳").font_size(16.0).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .trailing_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .supporting_text("We'll never share your number")
                .build(ctx);

            Text::new("TextField (error state):").font_size(14.0).build(ctx);
            TextField::new(err, |_| {})
                .outlined()
                .label(|ctx, p| {
                    Text::new("Password").font_size(label_size(p.get())).color(WiniaTheme::colors().error).build(ctx);
                })
                .is_error(true)
                .supporting_text("Password must be at least 8 characters")
                .build(ctx);

            Text::new("TextField (password mask):").font_size(14.0).build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("secret123")), |_| {})
                .filled()
                .label(|ctx, p| {
                    Text::new("Secret").font_size(label_size(p.get())).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
                .visual_transformation(winia::ui::PasswordTransformation::default())
                .build(ctx);

            Text::new("TextField (disabled):").font_size(14.0).build(ctx);
            TextField::new(ctx.remember(|| TextFieldValue::new("Locked")), |_| {})
                .filled()
                .label(|ctx, p| {
                    Text::new("Readonly").font_size(label_size(p.get())).color(WiniaTheme::colors().on_surface_variant).build(ctx);
                })
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
                .size(460.0, 460.0)
                .title("TextField Test")
                .build(ctx, text_field_ui);
        });
    });
}
