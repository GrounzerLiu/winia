//! 嵌套滚动演示——验证 §3.7（鼠标位置命中滚动目标）与 §3.8（delta/fling 的
//! target/ancestor handoff 顺序与消费语义）。
//!
//! 结构（三层）：
//! ```text
//! Window
//! └─ Column (fill)
//!    ├─ TopAppBar (large, 折叠)              ← 祖先 connection（TopAppBarNestedConnection）
//!    ├─ 状态行（各层 offset 实时显示）
//!    └─ 外层滚动区 (scroll_outer, 撑满)
//!       └─ Column
//!          ├─ 页面内容 0..8
//!          ├─ 内层固定高滚动列表 (scroll_inner, 高 180dp)
//!          │    └─ 30 项（滚到底 → 剩余 handoff 给外层）
//!          └─ 页面内容 8..20
//! ```
//!
//! 交互验证：
//! - **滚轮在内层列表上**：内层先滚；内层滚到底后，剩余 delta 经
//!   `HandoffConnection::on_post_scroll` 转给外层页面（外层继续滚、顶栏折叠）。
//! - **滚轮在页面上**（内层外区域）：只滚外层——§3.7 命中路径，两个滚动区不互相抢。
//! - **TopAppBar 折叠**：外层滚动时顶栏收缩（ExitUntilCollapsed）。
//! - 每层 offset 显示在顶部状态行，便于观察消费分配。

use letclone::clone;
use winia::prelude::*;

/// 把内层列表滚到底后的剩余 delta 转给外层 ScrollState 的 connection。
/// 对齐 §3.8 语义：`on_post_scroll(consumed_by_child, available)`——
/// child 已消费 `consumed_by_child`，还剩 `available` 待祖先接手。
#[derive(Clone)]
struct HandoffConnection {
    outer: ScrollState,
}

impl HandoffConnection {
    fn new(outer: ScrollState) -> Self {
        Self { outer }
    }
}

impl winia::nested_scroll::NestedScrollConnection for HandoffConnection {
    fn on_post_scroll(
        &self,
        _consumed_by_child: winia::nested_scroll::ScrollDelta,
        available: winia::nested_scroll::ScrollDelta,
        _source: winia::nested_scroll::NestedScrollSource,
    ) -> winia::nested_scroll::ScrollDelta {
        if available.y == 0.0 {
            return winia::nested_scroll::ScrollDelta::ZERO;
        }
        // 把剩余 delta 应用到外层 offset（负 delta = 内容上移 = offset 增加）。
        // ⚠ 外层 offset 上界无法从 ScrollState 读取（fling_limit 是 pub(crate)），
        // demo 简化为仅 clamp 下界（0）——上界由外层自身滚动逻辑天然限制
        // （外层 offset 超过内容高后渲染平移出界，但 demo 场景不会触发）。
        let current = self.outer.offset.get();
        let target = (current - available.y).max(0.0);
        let applied = current - target;
        self.outer.offset.set(target);
        // 返回实际应用的量（本 demo 无更外层，直接返回全部）
        winia::nested_scroll::ScrollDelta::new(0.0, applied)
    }

    fn on_post_fling(
        &self,
        _consumed_by_child: winia::nested_scroll::ScrollVelocity,
        available: winia::nested_scroll::ScrollVelocity,
    ) -> winia::nested_scroll::ScrollVelocity {
        // fling 撞边界：把剩余速度交给外层（触发外层惯性滚动）
        if available.y.abs() < 1.0 {
            return winia::nested_scroll::ScrollVelocity::default();
        }
        self.outer.fling(available.y);
        available
    }
}

#[composable]
fn nested_scroll_demo(ctx: &mut ComposeCtx) {
    let outer = ctx.remember(|| ScrollState::new()).get();
    let inner = ctx.remember(|| ScrollState::new()).get();
    // 内层列表滚到底 → 剩余量转给外层页面
    let handoff = ctx.remember(|| HandoffConnection::new(outer.clone())).get();
    // TopAppBar 折叠行为（connection 必须手动挂到滚动容器——见 docs/nested-scroll.md）
    // ⚠ top_bar_state 必须 remember：connection 捕获它（pre 阶段写 height_offset），
    // 若每次重组重建，connection 写的 state 与 UI 读的 state 脱节（折叠失效）
    let top_bar_state = ctx.remember(|| TopAppBarState::new(TOP_APP_BAR_LARGE_HEIGHT)).get();
    let behavior = TopAppBarScrollBehavior::exit_until_collapsed(top_bar_state.clone(), TOP_APP_BAR_LARGE_HEIGHT);
    // 绑定外层滚动状态：TopAppBar 消费外层滚动的 delta 来折叠/回弹
    let top_bar_conn = behavior.nested_scroll_connection_with_scroll(outer.clone()).unwrap();
    #[cfg(debug_assertions)]
    if std::env::var("WINIA_DRAG_TRACE").is_ok() {
        eprintln!("[demo] top_bar_state.h_off id={:?} conn.h_off id={:?}",
            top_bar_state.height_offset.state_id(),
            top_bar_conn.state().height_offset.state_id());
    }

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            // 顶栏
            TopAppBar::large(|ctx| Text::new("Nested Scroll Demo").build(ctx))
                .subtitle(|ctx| Text::new("内层滚到底 → 外层接棒 → 顶栏折叠").build(ctx))
                .scroll_behavior(behavior)
                .build(ctx);

            // 状态行
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding(8.0))
                .build(ctx, |ctx| {
                    Text::new(format!("outer: {:.0}  inner: {:.0}  h_off: {:.0}",
                        outer.offset.get(), inner.offset.get(),
                        top_bar_state.height_offset.get()))
                        .font_size(12.0)
                        .color(Color::from_argb(200, 40, 60, 100))
                        .build(ctx);
                    Spacer::horizontal(20.0).build(ctx);
                    Text::new("滚轮：内层优先，到底后页面滚")
                        .font_size(11.0)
                        .color(Color::from_argb(160, 120, 120, 120))
                        .build(ctx);
                });

            // 外层滚动区（挂 TopAppBar connection——滚动时顶栏折叠）
            Column::new()
                .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(outer.clone()).nested_scroll(top_bar_conn.clone()))
                .build(ctx, |ctx| {
                    // 页面顶部内容
                    for i in 0..8 {
                        Text::new(format!("页面内容 {i}"))
                            .modifier(Modifier::new().padding(10.0).fill_max_width())
                            .font_size(13.0)
                            .build(ctx);
                    }

                    // 内层固定高滚动列表
                    Column::new()
                        .modifier(Modifier::new()
                            .fill_max_width()
                            .height(180.0)
                            .padding(4.0)
                            .background(Color::from_argb(30, 60, 120, 200), Shape::rounded(8.0))
                            .vertical_scroll(inner.clone())
                            .nested_scroll(handoff.clone()))
                        .build(ctx, |ctx| {
                            Text::new("── 内层列表（30 项，滚到底 → 外层接棒）──")
                                .font_size(11.0)
                                .color(Color::from_argb(180, 40, 80, 140))
                                .modifier(Modifier::new().padding(4.0))
                                .build(ctx);
                            for i in 0..30 {
                                Text::new(format!("内层列表项 {i}"))
                                    .modifier(Modifier::new().padding(8.0).fill_max_width())
                                    .font_size(12.0)
                                    .build(ctx);
                            }
                        });

                    // 页面底部内容
                    for i in 8..20 {
                        Text::new(format!("页面内容 {i}"))
                            .modifier(Modifier::new().padding(10.0).fill_max_width())
                            .font_size(13.0)
                            .build(ctx);
                    }
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 720.0)
                .title("Nested Scroll Demo")
                .build(ctx, |ctx| {
                    nested_scroll_demo(ctx);
                });
        });
    });
}
