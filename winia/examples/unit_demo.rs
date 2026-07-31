//! Unit 类型演示 — Dp/Sp/Px/Offset/Size/Density
//!
//! 展示：
//! 1. .size(10.dp(), 20.px()) 混合单位
//! 2. Dp/Px 在 Density 下的转换效果
//! 3. Offset/Size 运算

use winia::prelude::*;
use winia::app;

fn unit_demo(ctx: &mut ComposeCtx) {
    let density = winia::unit::current_density();

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            Text::new("Unit Demo")
                .font_size(22.0)
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);

            // ── 1. Density 信息 ──
            Text::new(format!("Current density: {:.2} (scale_factor)", density.density))
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .build(ctx);
            Text::new(format!("  10.dp() = {:.0}px  20.px() = {:.0} 逻辑像素", 10.dp().to_px(density), 20.px().to_logical(density)))
                .font_size(12.0)
                .color(Color::from_argb(180, 120, 120, 120))
                .build(ctx);

            // ── 2. .size(10.dp(), 20.px()) 混合单位 ──
            Text::new("2. .size(10.dp(), 20.px())")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);

            // 10dp 宽（逻辑像素10）× 20px 高（density=2 时逻辑像素10）
            Column::new()
                .modifier(Modifier::new()
                    .size(10.dp(), 20.px())
                    .background(Color::from_argb(255, 63, 81, 181), Shape::rounded(3.0)))
                .build(ctx, |_| {});

            // ── 3. 三单位对比（density=2 时 dp 和 px 数量不同但视觉相等）──
            Text::new("3. Unit comparison (10dp vs 20px at density 2.0)")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);

            Row::new()
                .spacing(8.0)
                .build(ctx, |ctx| {
                    // 10dp 盒子
                    Column::new()
                        .modifier(Modifier::new()
                            .size(10.dp(), 20.0)
                            .background(Color::from_argb(255, 33, 150, 243), Shape::rounded(3.0)))
                        .build(ctx, |_| {});
                    Text::new("10.dp()")
                        .font_size(11.0)
                        .build(ctx);
                    // 20px 盒子（density=2 时与 10dp 等宽）
                    Column::new()
                        .modifier(Modifier::new()
                            .size(20.px(), 20.0)
                            .background(Color::from_argb(255, 255, 87, 34), Shape::rounded(3.0)))
                        .build(ctx, |_| {});
                    Text::new("20.px()")
                        .font_size(11.0)
                        .build(ctx);
                });

            // ── 4. Offset/Size 运算 ──
            Text::new("4. Offset / Size arithmetic")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);

            let offset = Offset::new(10.0, 20.0) + Offset::new(5.0, 5.0);
            let size = Size::new(100.0, 50.0);
            Text::new(format!(
                "Offset(10,20)+Offset(5,5) = ({:.0},{:.0})  Size 100x50 contains (50,25): {}",
                offset.x, offset.y, size.contains(Offset::new(50.0, 25.0))
            ))
                .font_size(12.0)
                .color(Color::from_argb(180, 120, 120, 120))
                .build(ctx);

            // ── 5. AnimatableValue（Dp/Offset 可直接动画）──
            Text::new("5. AnimatableValue for Dp/Offset/Size")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);

            let from_dp = 0.dp();
            let to_dp = 100.dp();
            let mid_dp = winia::animation::AnimatableValue::lerp(&from_dp, &to_dp, 0.5);
            let from_off = Offset::new(0.0, 0.0);
            let to_off = Offset::new(8.0, 6.0);
            let mid_off = winia::animation::AnimatableValue::lerp(&from_off, &to_off, 0.25);
            Text::new(format!(
                "Dp lerp(0dp→100dp, 0.5) = {:.0}dp   Offset lerp((0,0)→(8,6), 0.25) = ({:.1},{:.1})",
                mid_dp.value(), mid_off.x, mid_off.y
            ))
                .font_size(12.0)
                .color(Color::from_argb(180, 120, 120, 120))
                .build(ctx);

            // ── 撑满空间，演示正常布局 ──
            Column::new()
                .modifier(Modifier::new().fill_max_size())
                .build(ctx, |_| {});
        });
}

fn main() {
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 520.0)
                .title("Unit Demo")
                .build(ctx, |ctx| unit_demo(ctx));
        });
    });
}
