//! Chips 演示——M3 四种变体（Assist/Filter/Input/Suggestion）
//!
//! 运行：`cargo run -p winia --example chip_demo`

use winia::prelude::*;
use winia::ui::Chip;

fn section_title(ctx: &mut ComposeCtx, title: &str) {
    Text::new(title)
        .font_size(14.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_vertical(8.0))
        .build(ctx);
}

#[composable]
fn chip_ui(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            // ── AssistChip ──
            section_title(ctx, "1. AssistChip（辅助操作）");
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Chip::assist(|ctx| { Text::new("Add to calendar").build(ctx); }, || {})
                    .build(ctx);
                Chip::assist(|ctx| { Text::new("Get directions").build(ctx); }, || {})
                    .leading_icon(|ctx| {
                        Icon::new(IconSource::svg_path("M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"))
                            .tint(WiniaTheme::colors().primary)
                            .build(ctx);
                    })
                    .build(ctx);
                Chip::assist(|ctx| { Text::new("Disabled").build(ctx); }, || {})
                    .enabled(false)
                    .build(ctx);
            });

            // ── FilterChip（selected 切换）──
            section_title(ctx, "2. FilterChip（筛选——点击切换选中）");
            let sel1 = ctx.remember(|| true);
            let sel2 = ctx.remember(|| false);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                // ⚠ get() 必须在 Row content 闭包内（注册到 Row scope）——
                // 在 Row 外取值 → 依赖注册到 Column scope → 点击后 Column 重跑
                // 但 Row Skip（参数未变）→ content 不重跑 → chip 不重建
                let sel1v = sel1.get();
                Chip::filter(sel1v, |ctx| { Text::new("Breakfast").build(ctx); }, {
                    let s = sel1.clone();
                    move || s.update(|v| *v = !*v)
                })
                .build(ctx);
                let sel2v = sel2.get();
                Chip::filter(sel2v, |ctx| { Text::new("Lunch").build(ctx); }, {
                    let s = sel2.clone();
                    move || s.update(|v| *v = !*v)
                })
                .leading_icon({
                    let sel2v = sel2v;
                    move |ctx| {
                        // 选中态显示对勾（M3 惯例：selected 时 leading icon = checkmark）
                        if sel2v {
                            Icon::new(IconSource::svg_path("M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"))
                                .tint(WiniaTheme::colors().on_secondary_container)
                                .build(ctx);
                        }
                    }
                })
                .build(ctx);
                Chip::filter(false, |ctx| { Text::new("Dinner (disabled)").build(ctx); }, || {})
                    .enabled(false)
                    .build(ctx);
            });

            // ── InputChip（selected 切换 + 关闭按钮）──
            section_title(ctx, "3. InputChip（输入信息——trailing 关闭按钮）");
            let in1 = ctx.remember(|| true);
            let in2 = ctx.remember(|| false);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                let in1v = in1.get();
                Chip::input(in1v, |ctx| { Text::new("Chris").build(ctx); }, {
                    let s = in1.clone();
                    move || s.update(|v| *v = !*v)
                })
                .trailing_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"))
                        .tint(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                })
                .build(ctx);
                let in2v = in2.get();
                Chip::input(in2v, |ctx| { Text::new("Add a contact").build(ctx); }, {
                    let s = in2.clone();
                    move || s.update(|v| *v = !*v)
                })
                .leading_icon(|ctx| {
                    Icon::new(IconSource::svg_path("M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"))
                        .tint(WiniaTheme::colors().primary)
                        .build(ctx);
                })
                .build(ctx);
            });

            // ── SuggestionChip ──
            section_title(ctx, "4. SuggestionChip（建议）");
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Chip::suggestion(|ctx| { Text::new("See all").build(ctx); }, || {})
                    .build(ctx);
                Chip::suggestion(|ctx| { Text::new("View playlist").build(ctx); }, || {})
                    .icon(|ctx| {
                        Icon::new(IconSource::svg_path("M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"))
                            .tint(WiniaTheme::colors().primary)
                            .build(ctx);
                    })
                    .build(ctx);
            });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        winia::ui::theme::WiniaTheme::auto(ctx, |ctx| {
            winia::ui::window::Window::new()
                .size(520.0, 400.0)
                .title("Chips Demo")
                .build(ctx, chip_ui);
        });
    });
}
