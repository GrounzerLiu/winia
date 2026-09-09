//! TabRow 组件演示（固定等分，Primary/Secondary 风格）。
//! 对标 M3 PrimaryTabRow / SecondaryTabRow。

use letclone::clone;
use winia::prelude::*;

#[composable]
fn tab_row_demo(ctx: &mut ComposeCtx) {
    let sel = ctx.remember(|| 0usize);
    let sel2 = ctx.remember(|| 1usize);
    let secondary = ctx.remember(|| false);
    // RTL/LTR 切换（对齐 icon_demo / scaffold_demo 模式）
    let rtl = ctx.remember(|| false);
    let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
    // ScrollableTabRow：独立滚动状态（remember 创建的 ScrollState）
    let scroll_sel = ctx.remember(|| 6usize);
    let scroll_state = ctx.remember(|| ScrollState::new()).get();
    // 外层垂直滚动（内容较多——500px 窗口放不下全部组件）
    let outer_scroll = ctx.remember(|| ScrollState::new()).get();

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size().vertical_scroll(outer_scroll))
            .build(ctx, |ctx| {
                // Title + RTL/Secondary 切换行
                Row::new()
                    .modifier(Modifier::new().padding(16.0))
                    .spacing(12.0)
                    .alignment(Alignment::Center)
                    .build(ctx, |ctx| {
                        Text::new(if rtl.get() { "TabRow Demo（RTL）" } else { "TabRow Demo" })
                            .build(ctx);

                        // RTL/LTR 切换——方向变化由 ScrollableTabRow 内部检测
                        //（last_dir 重置 last_selected → 下帧自动重新居中选中 tab）
                        Button::new()
                            .on_click({ clone!(rtl); move || rtl.update(|v| *v = !*v) })
                            .build(ctx, |ctx| {
                                Text::new(if rtl.get() { "LTR" } else { "RTL" }).font_size(12.0).build(ctx);
                            });

                        // Primary/Secondary 切换
                        Button::new()
                            .on_click({ clone!(secondary); move || secondary.update(|v| *v = !*v) })
                            .build(ctx, |ctx| {
                                if secondary.get() {
                                    Text::new("Switch to Primary").font_size(12.0).build(ctx);
                                } else {
                                    Text::new("Switch to Secondary").font_size(12.0).build(ctx);
                                }
                            });
                    });

                Spacer::vertical(8.0);

                // ── TabRow (Primary or Secondary) ──
                let mut row = TabRow::new(sel.get(), {
                    clone!(sel);
                    move |ctx| {
                    // Tab 1: text only
                    Tab::new({ clone!(sel); sel.get() == 0 }, { clone!(sel); move || sel.set(0) })
                        .text(|ctx| Text::new("Tab A").build(ctx))
                        .build(ctx);

                    // Tab 2: text + icon (使用真实 Material Symbols Outlined star)
                    Tab::new({ clone!(sel); sel.get() == 1 }, { clone!(sel); move || sel.set(1) })
                        .text(|ctx| Text::new("Tab B").build(ctx))
                        .icon(|ctx| {
                            Icon::new(IconSource::svg(
                                r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143ZM233-120l65-281L80-590l288-25 112-265 112 265 288 25-218 189 65 281-247-149-247 149Zm247-350Z"/></svg>"#,
                            ))
                            .size(24.0)
                            .build(ctx);
                        })
                        .build(ctx);

                    // Tab 3: leading icon（icon 左 + 8dp + text 右，48dp 高）
                    Tab::new({ clone!(sel); sel.get() == 2 }, { clone!(sel); move || sel.set(2) })
                        .leading_icon()
                        .icon(|ctx| {
                            Icon::new(IconSource::svg(
                                r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143ZM233-120l65-281L80-590l288-25 112-265 112 265 288 25-218 189 65 281-247-149-247 149Zm247-350Z"/></svg>"#,
                            ))
                            .size(24.0)
                            .build(ctx);
                        })
                        .text(|ctx| Text::new("Tab Three").build(ctx))
                        .build(ctx);
                }
                });
                if secondary.get() {
                    row = row.secondary();
                }
                row.build(ctx);

                Spacer::vertical(8.0);

                // Content area
                let content = match sel.get() {
                    0 => "Content for Tab A — 这是 Tab A 的内容区。",
                    1 => "Content for Tab B — 这是 Tab B 的内容区，Tab B 带图标。",
                    _ => "Content for Tab Three — 这是 Tab Three 的内容区。",
                };
                Text::new(content)
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);

                Divider::horizontal().build(ctx);
                Spacer::vertical(8.0);

                // ── Second TabRow: Secondary (fixed) ──
                Text::new("SecondaryTabRow (固定)")
                    .modifier(Modifier::new().padding(8.0))
                    .build(ctx);

                let s2 = sel2.clone();
                TabRow::new(s2.get(), {
                    clone!(s2);
                    move |ctx| {
                    Tab::new({ clone!(s2); s2.get() == 0 }, { clone!(s2); move || s2.set(0) })
                        .text(|ctx| Text::new("One").build(ctx))
                        .build(ctx);

                    Tab::new({ clone!(s2); s2.get() == 1 }, { clone!(s2); move || s2.set(1) })
                        .text(|ctx| Text::new("Two").build(ctx))
                        .build(ctx);

                    Tab::new({ clone!(s2); s2.get() == 2 }, { clone!(s2); move || s2.set(2) })
                        .text(|ctx| Text::new("Three").build(ctx))
                        .build(ctx);
                }
                })
                .secondary()
                .build(ctx);

                Spacer::vertical(8.0);
                Divider::horizontal().build(ctx);
                Spacer::vertical(8.0);

                // ── ScrollableTabRow：多 tab 撑出滚动区（选中居中滚动）──
                Text::new("ScrollableTabRow (可滚动，选中自动居中)")
                    .modifier(Modifier::new().padding(8.0))
                    .build(ctx);

                ScrollableTabRow::new(scroll_sel.get(), {
                    clone!(scroll_sel);
                    move |ctx| {
                    for i in 0..12 {
                        let label = format!("Tab {}", i + 1);
                        Tab::new({ clone!(scroll_sel); scroll_sel.get() == i }, { clone!(scroll_sel); move || scroll_sel.set(i) })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                }
                })
                .scroll_state(scroll_state.clone())
                .build(ctx);

                Text::new(if rtl.get() { "→ 点击左侧 tab 观察自动滚动 →" } else { "← 点击右侧 tab 观察自动滚动 →" })
                    .modifier(Modifier::new().padding(8.0))
                    .build(ctx);
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        Window::new().size(500.0, 500.0).title("TabRow Demo").build(ctx, tab_row_demo);
    });
}