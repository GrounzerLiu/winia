//! TabRow 组件演示（固定等分，Primary/Secondary 风格）。
//! 对标 M3 PrimaryTabRow / SecondaryTabRow。

use winia::prelude::*;

#[composable]
fn tab_row_demo(ctx: &mut ComposeCtx) {
    let sel = ctx.remember(|| 0usize);
    let sel2 = ctx.remember(|| 1usize);
    let secondary = ctx.remember(|| false);

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Ltr, ctx, |ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                // Title
                Text::new("TabRow Demo")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);

                // Toggle secondary button
                let secondary_click = secondary.clone();
                Button::new()
                    .on_click(move || secondary_click.update(|v| *v = !*v))
                    .build(ctx, |ctx| {
                        if secondary.get() {
                            Text::new("Switch to Primary").build(ctx);
                        } else {
                            Text::new("Switch to Secondary").build(ctx);
                        }
                    });

                Spacer::vertical(16.0);

                // ── TabRow (Primary or Secondary) ──
                let sel_clone = sel.clone();
                let mut row = TabRow::new(sel.get(), move |ctx| {
                    // Tab 1: text only
                    let s = sel_clone.clone();
                    Tab::new(s.get() == 0, move || s.set(0))
                        .text(|ctx| Text::new("Tab A").build(ctx))
                        .build(ctx);

                    // Tab 2: text + icon
                    let s = sel_clone.clone();
                    Tab::new(s.get() == 1, move || s.set(1))
                        .text(|ctx| Text::new("Tab B").build(ctx))
                        .icon(|ctx| {
                            // Use a simple SVG path icon
                            Icon::svg_path("M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5")
                                .size(24.0)
                                .build(ctx);
                        })
                        .build(ctx);

                    // Tab 3: text only (longer)
                    let s = sel_clone.clone();
                    Tab::new(s.get() == 2, move || s.set(2))
                        .text(|ctx| Text::new("Tab Three").build(ctx))
                        .build(ctx);
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
                TabRow::new(sel2.get(), move |ctx| {
                    let s = s2.clone();
                    Tab::new(s.get() == 0, move || s.set(0))
                        .text(|ctx| Text::new("One").build(ctx))
                        .build(ctx);

                    let s = s2.clone();
                    Tab::new(s.get() == 1, move || s.set(1))
                        .text(|ctx| Text::new("Two").build(ctx))
                        .build(ctx);

                    let s = s2.clone();
                    Tab::new(s.get() == 2, move || s.set(2))
                        .text(|ctx| Text::new("Three").build(ctx))
                        .build(ctx);
                })
                .secondary()
                .build(ctx);
            });
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new().size(500.0, 500.0).title("TabRow Demo").build(ctx, tab_row_demo);
    });
}