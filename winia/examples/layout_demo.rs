//! 布局演示 — Column / Row / Stack 的排列、对齐、权重功能
//!
//! 仅使用现有组件 API，不使用 ctx.start_leaf / ctx.end_node 低级模式。

use winia::prelude::*;
use winia::app;

/// 布局演示主界面（#[composable] = 函数级组合 scope）
#[composable]
fn layout_demo_ui(ctx: &mut ComposeCtx) {
    let scroll_state = ctx.remember(|| ScrollState::new()).get();

    Column::new()
                        .modifier(Modifier::new()
                            .size(Dimension::Fill, 700.0)
                            .padding(16.0)
                            .vertical_scroll(scroll_state))
                        .spacing(12.0)
                        .build(ctx, |ctx| {

                            // ── 1. Column 基础 ──
                            Text::new("■ Column — vertical stack")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Column::new()
                                .spacing(4.0)
                                .modifier(Modifier::new()
                                    .fill_max_width().height(80.0)
                                    .background(Color::from_argb(30, 0, 0, 0), Shape::rounded(4.0))
                                    .padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("Top").font_size(12.0).modifier(Modifier::new().size(Dimension::Fill, 20.0).background(Color::from_argb(180, 180, 180, 200), Shape::rounded(3.0)).padding(2.0)).build(ctx);
                                    Text::new("Middle").font_size(12.0).modifier(Modifier::new().size(Dimension::Fill, 20.0).background(Color::from_argb(180, 180, 180, 200), Shape::rounded(3.0)).padding(2.0)).build(ctx);
                                    Text::new("Bottom").font_size(12.0).modifier(Modifier::new().size(Dimension::Fill, 20.0).background(Color::from_argb(180, 180, 180, 200), Shape::rounded(3.0)).padding(2.0)).build(ctx);
                                });

                            // ── 2. Row 基础 ──
                            Text::new("■ Row — horizontal stack")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Row::new()
                                .spacing(8.0)
                                .modifier(Modifier::new()
                                    .fill_max_width().height(40.0)
                                    .background(Color::from_argb(30, 0, 0, 0), Shape::rounded(4.0))
                                    .padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("A").font_size(12.0).modifier(Modifier::new().size(60.0, 20.0).padding(2.0)).build(ctx);
                                    Text::new("B").font_size(12.0).modifier(Modifier::new().size(60.0, 20.0).padding(2.0)).build(ctx);
                                    Text::new("C").font_size(12.0).modifier(Modifier::new().size(60.0, 20.0).padding(2.0)).build(ctx);
                                });

                            // ── 3. Row Arrangement 对比 ──
                            Text::new("■ Row — Arrangement")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            let arr_bg = Color::from_argb(20, 50, 100, 200);
                            let arr_pad = Modifier::new().fill_max_width().height(30.0);
                            let item_bg = Color::from_argb(200, 180, 180, 200);

                            Text::new("  Start").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::Start)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  Center").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::Center)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  End").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::End)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  SpaceBetween").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::SpaceBetween)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  SpaceAround").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::SpaceAround)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  SpaceEvenly").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .arrangement(Arrangement::SpaceEvenly)
                                .modifier(arr_pad.clone().background(arr_bg, Shape::rounded(4.0)).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("1").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("2").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                    Text::new("3").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).background(item_bg, Shape::rounded(3.0)).padding(4.0)).build(ctx);
                                });

                            // ── 4. Column Alignment (交叉轴) ──
                            Text::new("■ Column — Alignment (cross-axis)")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Text::new("  Alignment::Start (default)").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Column::new()
                                .alignment(Alignment::Start).spacing(4.0)
                                .modifier(Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(20, 200, 100, 50), Shape::rounded(4.0)).padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("AA").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("BBBBB").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("C").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                });

                            Text::new("  Alignment::Center").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Column::new()
                                .alignment(Alignment::Center).spacing(4.0)
                                .modifier(Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(20, 200, 100, 50), Shape::rounded(4.0)).padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("AA").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("BBBBB").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("C").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                });

                            Text::new("  Alignment::End").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Column::new()
                                .alignment(Alignment::End).spacing(4.0)
                                .modifier(Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(20, 200, 100, 50), Shape::rounded(4.0)).padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("AA").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("BBBBB").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                    Text::new("C").font_size(10.0).modifier(Modifier::new().size(40.0, 20.0).padding(4.0)).build(ctx);
                                });

                            // ── 5. align_self 子节点覆盖对齐 ──
                            Text::new("■ align_self — per-child override")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Row::new()
                                .alignment(Alignment::Start).spacing(8.0)
                                .modifier(Modifier::new().fill_max_width().height(50.0).background(Color::from_argb(25, 0, 0, 0), Shape::rounded(4.0)).padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("Start").font_size(9.0).modifier(Modifier::new().size(50.0, 20.0).align_self(Alignment::Start).background(Color::from_argb(200, 100, 120, 160), Shape::rounded(4.0)).padding(2.0)).build(ctx);
                                    Text::new("Center").font_size(9.0).modifier(Modifier::new().size(50.0, 20.0).align_self(Alignment::Center).background(Color::from_argb(200, 100, 120, 160), Shape::rounded(4.0)).padding(2.0)).build(ctx);
                                    Text::new("End").font_size(9.0).modifier(Modifier::new().size(50.0, 20.0).align_self(Alignment::End).background(Color::from_argb(200, 100, 120, 160), Shape::rounded(4.0)).padding(2.0)).build(ctx);
                                    Text::new("Stretch").font_size(9.0).modifier(Modifier::new().size(50.0, 20.0).align_self(Alignment::Stretch).background(Color::from_argb(200, 100, 120, 160), Shape::rounded(4.0)).padding(2.0)).build(ctx);
                                });

                            // ── 6. layout_weight 按比例分配 ──
                            Text::new("■ layout_weight — proportional space")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Text::new("  Row with weights 1:2:1").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .spacing(6.0)
                                .modifier(Modifier::new().fill_max_width().height(36.0).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("w1").font_size(12.0).align(TextAlign::Center).modifier(Modifier::new().fill_max_height().layout_weight(1.0).background(Color::from_argb(200, 100, 180, 220), Shape::rounded(4.0)).padding(4.0)).build(ctx);
                                    Text::new("w2").font_size(12.0).align(TextAlign::Center).modifier(Modifier::new().fill_max_height().layout_weight(2.0).background(Color::from_argb(200, 220, 180, 100), Shape::rounded(4.0)).padding(4.0)).build(ctx);
                                    Text::new("w1").font_size(12.0).align(TextAlign::Center).modifier(Modifier::new().fill_max_height().layout_weight(1.0).background(Color::from_argb(200, 100, 180, 220), Shape::rounded(4.0)).padding(4.0)).build(ctx);
                                });

                            Text::new("  Row with weights 3:1").font_size(11.0).color(Color::from_argb(200, 120, 120, 120)).build(ctx);
                            Row::new()
                                .spacing(6.0)
                                .modifier(Modifier::new().fill_max_width().height(36.0).padding(4.0))
                                .build(ctx, |ctx| {
                                    Text::new("W=3").font_size(12.0).align(TextAlign::Center).modifier(Modifier::new().fill_max_height().layout_weight(3.0).background(Color::from_argb(200, 100, 180, 220), Shape::rounded(4.0)).padding(4.0)).build(ctx);
                                    Text::new("W=1").font_size(12.0).align(TextAlign::Center).modifier(Modifier::new().fill_max_height().layout_weight(1.0).background(Color::from_argb(200, 220, 180, 100), Shape::rounded(4.0)).padding(4.0)).build(ctx);
                                });

                            // ── 7. Row > Column 嵌套 + weight ──
                            Text::new("■ Nested: Row > Column")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Row::new()
                                .spacing(8.0).alignment(Alignment::Center)
                                .modifier(Modifier::new().fill_max_width().height(100.0).background(Color::from_argb(20, 0, 150, 100), Shape::rounded(6.0)).padding(8.0))
                                .build(ctx, |ctx| {
                                    Column::new().spacing(4.0)
                                        .modifier(Modifier::new().fill_max_height().layout_weight(1.0).background(Color::from_argb(40, 0, 0, 0), Shape::rounded(4.0)).padding(6.0))
                                        .build(ctx, |ctx| {
                                            Text::new("Left Column").font_size(12.0).build(ctx);
                                            Text::new("weight=1").font_size(12.0).build(ctx);
                                        });
                                    Column::new().spacing(4.0)
                                        .modifier(Modifier::new().fill_max_height().layout_weight(2.0).background(Color::from_argb(40, 0, 0, 0), Shape::rounded(4.0)).padding(6.0))
                                        .build(ctx, |ctx| {
                                            Text::new("Right Column").font_size(12.0).build(ctx);
                                            Text::new("weight=2").font_size(12.0).build(ctx);
                                        });
                                });

                            // ── 8. Stack 层叠 + Z-order ──
                            Text::new("■ Stack — z-order with offset")
                                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                                .build(ctx);

                            Stack::new()
                                .modifier(Modifier::new().size(Dimension::Fill, 60.0).background(Color::from_argb(20, 100, 100, 200), Shape::rounded(6.0)))
                                .build(ctx, |ctx| {
                                    // 底层（先 build → 先绘制）
                                    Text::new("Behind").font_size(12.0)
                                        .background(Color::from_argb(200, 220, 180, 100), Shape::rounded(4.0))
                                        .modifier(Modifier::new().size(100.0, 36.0).padding(4.0))
                                        .build(ctx);
                                    // 顶层（后 build → 后绘制，覆盖底层）
                                    Text::new("Front (offset)").font_size(12.0)
                                        .background(Color::from_argb(220, 100, 140, 200), Shape::rounded(4.0))
                                        .modifier(Modifier::new().size(120.0, 36.0).offset(40.0, 10.0).padding(4.0))
                                        .build(ctx);
                                });
                        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(winia::app_root!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 700.0)
                .title("Layout Demo")
                .build(ctx, |ctx| layout_demo_ui(ctx));
        });
    }));
}