//! 背景模糊（BackdropBlur）演示——毛玻璃效果
//!
//! 展示：
//! - 风景图铺满窗口（本地资源 `examples/assets/landscape.jpg`）
//! - `backdrop_blur` 矩形（背景模糊/毛玻璃，无填充——纯模糊）
//! - 矩形可拖动（`on_drag` + `absolute_offset`）——拖到图上观察模糊效果
//!
//! 运行：`cargo run -p winia --example blur_demo`

use winia::prelude::*;

const LANDSCAPE_JPG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/landscape.jpg");

#[composable]
fn blur_demo(ctx: &mut ComposeCtx) {
    // 底层：风景图铺满窗口（Crop 裁剪，保持比例）
    Image::file(LANDSCAPE_JPG)
        .modifier(Modifier::new().fill_max_size())
        .content_scale(ContentScale::Crop)
        .build(ctx);

    // 毛玻璃矩形：现有组件（Stack）+ backdrop_blur modifier——
    // 任何组件都能背景模糊；纯模糊无白填充 + 拖动（on_drag + State）
    let pos = ctx.remember(|| (300.0f32, 250.0f32));
    Stack::new()
        .modifier(Modifier::new()
            .size(260.0, 170.0)
            .backdrop_blur(12.0)
            .absolute_offset(pos.get().0, pos.get().1)
            .on_drag(move |_, delta| {
                let mut p = pos.get();
                p.0 += delta.0;
                p.1 += delta.1;
                pos.set(p);
            }))
        .build(ctx, |_ctx| {});

    // 顶部提示
    Text::new("拖动毛玻璃矩形，观察背景模糊效果")
        .font_size(15.0)
        .color(Color::WHITE)
        .modifier(Modifier::new().absolute_offset(18.0, 16.0))
        .build(ctx);
}

fn main() {
    // 启动 tokio 运行时（供 debug WS server / LaunchedEffect 使用）
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(900.0, 640.0)
                .title("Blur Demo")
                .build(ctx, |ctx| {
                    Stack::new()
                        .modifier(Modifier::new().fill_max_size())
                        .build(ctx, |ctx| { blur_demo(ctx); });
                });
        });
    });
}
