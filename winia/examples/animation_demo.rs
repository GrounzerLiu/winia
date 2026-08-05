//! 动画系统演示
//!
//! 展示 animate_float_as_state / updateTransition / Spring / Tween
//! 动画值绑定到 visual 属性（size、background）实现可视动画。
//!
//! 每节拆为独立 `#[composable]` 函数 = 函数级组合 scope——
//! 动画 state 失效只重跑对应节函数（对标 Compose @Composable 的用户函数粒度）。

use winia::prelude::*;
use winia::animation::{AnimationSpec, SpringSpec, TweenSpec};
use winia::app;

#[composable]
fn animation_demo(ctx: &mut ComposeCtx) {
    let clicked = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            // 在最外层闭包（key 稳定处）remember scroll 状态
            let scroll_y = ctx.remember(|| ScrollState::new()).get();
            // ── 顶部固定：标题 + Toggle 按钮（不随滚动）──
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .alignment(winia::layout::Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new("Animation Demo")
                        .font_size(22.0)
                        .modifier(Modifier::new().padding_vertical(8.0))
                        .build(ctx);
                    Column::new()
                        .modifier(Modifier::new().layout_weight(1.0))
                        .build(ctx, |_| {});
                    Button::new()
                        .on_click({ let c = clicked.clone(); move || { c.update(|v| *v = !*v); } })
                        .build(ctx, |ctx| {
                            Text::new(if clicked.get() { "Reset" } else { "Animate!" })
                                .font_size(16.0)
                                .build(ctx);
                        });
                });

            // ── 可滚动内容区：每节独立 #[composable] 函数（动画只重跑对应节）──
            Column::new()
                .modifier(Modifier::new()
                    .padding_vertical(8.0)
                    .fill_max_size()
                    .vertical_scroll(scroll_y))
                .build(ctx, |ctx| {
                    section1(ctx, &clicked);
                    section2(ctx, &clicked);
                    section3(ctx, &clicked);
                    section4(ctx, &clicked);
                    section5(ctx);
                    section6(ctx, &clicked);
                    section7(ctx, &clicked);
                });
        });
}

/// 第 1 节：Spring 弹跳宽度。
#[composable]
fn section1(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("1. Spring Bouncy — box width")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .build(ctx);

    let scale = ctx.animate_float_as_state(
        if clicked.get() { 300.0 } else { 50.0 },
        AnimationSpec::Spring(SpringSpec::bouncy()),
    );
    // 布局属性动画：size 派生值在测量时求值，依赖注册到本函数 scope → 每帧重组重测平滑过渡
    Column::new()
        .modifier(Modifier::new()
            .size(&scale, 24.0)
            .background(Color::from_argb(255, 100, 149, 237), Shape::rounded(6.0))
            .padding(4.0))
        .build(ctx, |ctx| {
            Text::new(format!("{:.0}px", scale.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 255, 255, 255))
                .build(ctx);
        });
}

/// 第 2 节：Tween 颜色过渡。
#[composable]
fn section2(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("2. Tween 300ms — background color")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let alpha = ctx.animate_float_as_state(
        if clicked.get() { 0.9 } else { 0.2 },
        AnimationSpec::Tween(TweenSpec {
            duration: std::time::Duration::from_millis(300),
            interpolator: winia::animation::interpolator::linear,
        }),
    );
    // 表达式直接写（非闭包非宏非中间变量）——注册到本函数 scope
    let c = Color::from_argb((alpha.get() * 255.0) as u8, 76, 175, 80);
    Column::new()
        .modifier(Modifier::new()
            .size(&alpha * 200.0 + 50.0, 30.0)
            .background(c, Shape::rounded(6.0))
            .padding(4.0))
        .build(ctx, |ctx| {
            Text::new(format!("Alpha: {:.2}", alpha.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 255, 255, 255))
                .build(ctx);
        });
}

/// 第 3 节：updateTransition 位置偏移。
#[composable]
fn section3(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("3. updateTransition — offset")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let page = if clicked.get() { 1u8 } else { 0u8 };
    let mut t = ctx.update_transition(
        page,
        AnimationSpec::Spring(SpringSpec::default()),
        "page",
    );
    let tx = t.animate_float(ctx, |p| if *p == 0 { 0.0 } else { 80.0 }, "tx");
    let ty = t.animate_float(ctx, |p| if *p == 0 { 0.0 } else { 30.0 }, "ty");

    // 用 offset 模拟位置动画（动画值在容器闭包内读取）
    Column::new()
        .build(ctx, |ctx| {
            let x = tx.get();
            let y = ty.get();
            Column::new()
                .modifier(Modifier::new()
                    .offset(x, y)
                    .size(100.0, 30.0)
                    .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(6.0))
                    .padding(4.0))
                .build(ctx, |ctx| {
                    // Text 直接读 State（内层组注册依赖——随动画更新；
                    // 捕获值不触发内层重组——与 Compose 语义一致）
                    Text::new(format!("Page {} ({:.0},{:.0})", page, tx.get(), ty.get()))
                        .font_size(12.0)
                        .color(Color::from_argb(255, 255, 255, 255))
                        .build(ctx);
                });
        });
}

/// 第 4 节：animate_color_as_state 颜色过渡。
#[composable]
fn section4(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("4. animate_color_as_state (Tween 500ms)")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let bg = ctx.animate_color_as_state(
        if clicked.get() { Color::from_argb(255, 76, 175, 80) }
        else { Color::from_argb(255, 156, 39, 176) },
        AnimationSpec::Tween(TweenSpec {
            duration: std::time::Duration::from_millis(500),
            interpolator: winia::animation::interpolator::linear,
        }),
    );
    Column::new()
        .build(ctx, |ctx| {
            let c = bg.get();
            Column::new()
                .modifier(Modifier::new()
                    .size(200.0, 40.0)
                    .background(c, Shape::rounded(6.0))
                    .padding(4.0))
                .build(ctx, |ctx| {
                    // Text 直接读 State——内层组注册依赖，随动画更新
                    let cur = bg.get();
                    Text::new(format!("#{:02X}{:02X}{:02X}", cur.r, cur.g, cur.b))
                        .font_size(12.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });
        });
}

/// 第 5 节：rememberInfiniteTransition 无限循环（绘制层动画，零重组）。
#[composable]
fn section5(ctx: &mut ComposeCtx) {
    Text::new("5. rememberInfiniteTransition (Reverse)")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let mut infinite = ctx.remember_infinite_transition();
    let pulse = infinite.animate_float(
        ctx, 0.4, 1.0,
        winia::animation::InfiniteRepeatableSpec::reverse(std::time::Duration::from_millis(600)),
    );
    // 无限颜色：蓝色↔红色 CAM16 插值（绘制层动画：set_visual 不触发重组，渲染时 peek）
    let pulse_color = infinite.animate_color(
        ctx,
        Color::from_argb(255, 33, 150, 243),
        Color::from_argb(255, 255, 82, 82),
        winia::animation::InfiniteRepeatableSpec::reverse(std::time::Duration::from_millis(600)),
    );
    // 呼吸圆点：alpha + 颜色由动态闭包在渲染时求值（零重组，只重绘）
    Column::new()
        .modifier(Modifier::new()
            .size(40.0, 40.0)
            .graphics_layer({
                let pulse = pulse.clone();
                move || winia::modifier::GraphicsLayerParams {
                    alpha: pulse.peek(),
                    ..Default::default()
                }
            })
            .background({
                let pulse_color = pulse_color.clone();
                move || pulse_color.peek()
            }, Shape::Circle)
        )
        .build(ctx, |_| {});
}

/// 第 6 节：Keyframes 关键帧。
#[composable]
fn section6(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("6. Keyframes (400ms, overshoot)")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let kf = ctx.animate_float_as_state(
        if clicked.get() { 200.0 } else { 40.0 },
        AnimationSpec::Keyframes(winia::animation::KeyframesSpec::new(
            std::time::Duration::from_millis(400),
            vec![(0.0, 0.0), (0.6, 1.2), (1.0, 1.0)], // 中途 120% 超调
        )),
    );
    Column::new()
        .modifier(Modifier::new()
            .size(&kf, 24.0)
            .background(Color::from_argb(255, 0, 150, 136), Shape::rounded(6.0))
            .padding(4.0))
        .build(ctx, |_| {});
}

/// 第 7 节：animateDpAsState Dp 动画。
#[composable]
fn section7(ctx: &mut ComposeCtx, clicked: &winia::core::state::State<bool>) {
    Text::new("7. animateDpAsState (Dp)")
        .font_size(14.0)
        .color(Color::from_argb(200, 100, 100, 100))
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);

    let dp = ctx.animate_dp_as_state(
        if clicked.get() { 30.dp() } else { 100.dp() },
        AnimationSpec::Spring(winia::animation::SpringSpec::default()),
    );
    Column::new()
        .modifier(Modifier::new()
            .size(&dp, 20.0)
            .background(Color::from_argb(255, 63, 81, 181), Shape::rounded(4.0)))
        .build(ctx, |_| {});
}

fn main() {
    // 启动 tokio 运行时（供 debug WS server 使用）
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 500.0)
                .title("Animation Demo")
                .build(ctx, |ctx| animation_demo(ctx));
        });
    });
}
