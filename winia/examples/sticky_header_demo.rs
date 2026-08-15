//! stickyHeader 演示（对齐 Compose foundation `stickyHeader`）
//!
//! 展示：
//! - 5 个 section：红色吸顶 header + 每节 19 个普通项（100 项懒加载）
//! - 滚动时 header 钉在视口顶，内容从它下面滑过
//! - 下一个 header 到来时把前一个推上去
//! - 钉住的 header 成为派生锚点（firstVisible = header index）
//! - content_padding(40, 40)：内容从顶部 40px 内边距开始，滚动到底时停在底部 40px 内边距处
//!
//! 运行：cargo run -p winia --example sticky_header_demo

use winia::prelude::*;

const SECTION_COUNT: u64 = 5;
const ITEMS_PER_SECTION: u64 = 19;

#[composable]
fn sticky_header_demo(ctx: &mut ComposeCtx) {
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("stickyHeader 演示（5 个 section，红色吸顶头）")
                .font_size(20.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("拖拽/滚轮滚动：header 钉在顶、内容滑过、下一个 header 推走前一个")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new(format!(
                "firstVisible = {} (offset {:.0}){}",
                state.first_visible(),
                state.offset(),
                if state.first_visible() % (1 + ITEMS_PER_SECTION as usize) == 0 {
                    " ← 钉住的 header"
                } else {
                    ""
                }
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 120, 120, 120))
            .modifier(Modifier::new().padding(8.0))
            .build(ctx);

            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    let s0 = state.clone();
                    Button::text()
                        .on_click(move || s0.scroll_to_item(0, 0.0))
                        .build(ctx, |ctx| Text::new("顶部").build(ctx));
                    let s1 = state.clone();
                    Button::text()
                        .on_click(move || s1.scroll_to_item(3 * (1 + ITEMS_PER_SECTION) as usize, 0.0))
                        .build(ctx, |ctx| Text::new("Section 3").build(ctx));
                    let s2 = state.clone();
                    Button::text()
                        .on_click(move || s2.scroll_to_item(4 * (1 + ITEMS_PER_SECTION) as usize, 0.0))
                        .build(ctx, |ctx| Text::new("Section 4").build(ctx));
                });

            let mut lb = LazyColumn::new()
                .state(state.clone())
                .content_padding(40.0, 40.0)
                .modifier(Modifier::new().fill_max_width().fill_max_height());
            for s in 0..SECTION_COUNT {
                lb = lb.sticky_header(s, move |ctx| {
                    Text::new(format!("SECTION {s}"))
                        .font_size(18.0)
                        .color(Color::WHITE)
                        .modifier(Modifier::new()
                            .padding(12.0)
                            .background(Color::from_argb(255, 0xC6, 0x28, 0x28), Shape::rounded(4.0)))
                        .build(ctx);
                });
                lb = lb.items(
                    ITEMS_PER_SECTION as usize,
                    move |i| 200 + s * ITEMS_PER_SECTION + i as u64,
                    move |ctx, i| {
                        let n = s * ITEMS_PER_SECTION + i as u64;
                        Text::new(format!("Item {n}"))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                );
            }
            lb.build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 600.0)
                .title("stickyHeader Demo")
                .build(ctx, sticky_header_demo);
        });
    });
}
