//! InteractionSource / ComponentState 演示——press/hover/focus 状态驱动 Button 与 TextField。
//!
//! 运行：`cargo run -p winia --example interaction_demo --features debug-server`
//! 交互：鼠标悬停/按下按钮观察颜色与阴影；Tab/点击聚焦 TextField 观察边框。

use winia::prelude::*;
use winia::core::composer::ComposeCtx;
use winia::composable;
use winia::modifier::{Color, Modifier, Shape};
use winia::ui::interaction::MutableInteractionSource;

#[composable]
fn interaction_ui(ctx: &mut ComposeCtx) {
    // 提升的交互源：Button 状态发射到这里，外部可读（Compose hoist 模式）
    let src = ctx.remember(|| MutableInteractionSource::new()).get();
    let state = src.state(true);
    let clicks = ctx.remember(|| 0i32);

    // TextField 的交互源（focus 状态——驱动边框色）
    let tf_src = ctx.remember(|| MutableInteractionSource::new()).get();
    let tf_focused = tf_src.is_focused();
    let field = ctx.remember(|| TextFieldValue::new(""));
    let has_error = ctx.remember(|| true);

    Column::new()
        .spacing(16.0)
        .modifier(Modifier::new().padding(24.0).fill_max_size())
        .build(ctx, |ctx| {
            Text::new("InteractionSource 演示")
                .font_size(22.0)
                .color(Color::from_argb(255, 40, 40, 40))
                .build(ctx);

            // ── 状态实时展示（读取 src 注册依赖——变化自动重组）──
            Text::new(format!(
                "状态: enabled={} pressed={} hovered={} focused={} dragged={}",
                state.enabled, state.pressed, state.hovered, state.focused, state.dragged
            ))
            .font_size(14.0)
            .color(Color::from_argb(255, 90, 90, 90))
            .build(ctx);

            // ── 提升源 + 阴影（ElevatedButton 近似：hover/press 抬高）──
            Button::new()
                .interaction_source(src.clone())
                .elevation(ButtonElevation::elevated())
                .on_click({
                    let c = clicks.clone();
                    move || c.update(|v| *v += 1)
                })
                .build(ctx, |ctx| {
                    Text::new(format!("点击 {} 次", clicks.get())).font_size(14.0).build(ctx);
                });

            // ── 内部源按钮（hover/press 自动变色——不提升也能工作）──
            Button::new()
                .on_click({
                    let c = clicks.clone();
                    move || c.update(|v| *v -= 1)
                })
                .build(ctx, |ctx| {
                    Text::new("内部源按钮 -1").font_size(14.0).build(ctx);
                });

            // ── 禁用按钮（无交互：不响应 hover/press）──
            Button::new()
                .enabled(false)
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Text::new("禁用按钮").font_size(14.0).build(ctx);
                });

            // ── TextField：focus 状态 + isError 驱动边框色 ──
            Text::new(format!("TextField focused={} error={}", tf_focused, has_error.get()))
                .font_size(14.0)
                .color(Color::from_argb(255, 90, 90, 90))
                .build(ctx);

            let theme = WiniaTheme::colors();
            let error = has_error.get();
            let border_color = if tf_focused {
                if error { theme.error } else { theme.primary }
            } else if error {
                theme.error
            } else {
                theme.outline
            };
            TextField::new(field.clone(), |_| {})
                .interaction_source(tf_src.clone())
                .is_error(error)
                .modifier(
                    Modifier::new()
                        .size(320.0, 42.0)
                        .background(
                            if tf_focused { theme.primary_container } else { theme.surface },
                            Shape::rounded(6.0),
                        )
                        .border(1.5, border_color, Shape::rounded(6.0)),
                )
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 560.0)
                .title("Interaction Demo")
                .build(ctx, interaction_ui);
        });
    });
}
