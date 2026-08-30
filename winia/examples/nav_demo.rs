//! Navigation3 风格导航 demo——状态驱动的 back stack + NavDisplay 投影。
//!
//! 展示：
//! - 类型安全路由（Route enum 实现 NavKey）
//! - NavBackStack：push/pop（导航 = 改列表；Clone 共享内部 State）
//! - NavDisplay：entry_provider 路由 → 内容（SinglePane——渲染栈顶）
//! - SceneStrategy：SinglePane（默认）/ ListDetail（双栏）切换
//! - NavTransition 滑动过渡：push 新页右入/旧页左出，pop 反向
//!
//! 运行：cargo run -p winia --example nav_demo --features debug-server

use winia::prelude::*;
use winia::nav::{remember_entry_state, ListDetailStrategy, NavBackStack, NavDisplay, NavEntry, NavKey, NavTransitionSpec, SceneStrategy};

/// 类型安全路由（对标 Nav3 的 NavKey + @Serializable——winia 无序列化要求）
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
enum Route {
    Home,
    Detail(u64),
    Settings,
}

#[composable]
fn nav_demo(ctx: &mut ComposeCtx) {
    // NavBackStack 是 Clone（内部 State<Vec> 共享）——可传给按钮闭包触发导航
    let back_stack = ctx.remember(|| NavBackStack::<Route>::with_initial(Route::Home)).get();
    // 双栏模式开关（ListDetail 策略 vs SinglePane）
    let list_detail = ctx.remember(|| false);
    // 过渡规格循环（0=Android 滑动 1=共享轴 2=淡化(Nav3 默认) 3=无——对标
    // transitionSpec/popTransitionSpec 的常用取值）
    let spec_idx = ctx.remember(|| 0usize);
    let (slide_push, slide_pop) = NavTransitionSpec::horizontal_slide();
    let (axis_push, axis_pop) = NavTransitionSpec::shared_axis();
    let (push_spec, pop_spec) = match spec_idx.get() {
        1 => (axis_push, axis_pop),
        2 => (NavTransitionSpec::fade(), NavTransitionSpec::fade()),
        3 => (NavTransitionSpec::none(), NavTransitionSpec::none()),
        _ => (slide_push, slide_pop),
    };
    // 按钮用 clone（push/pop/切换模式）
    let home_bs = back_stack.clone();
    let detail_bs = back_stack.clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Navigation3 风格导航 demo").font_size(20.0).build(ctx);
            Text::new(format!("back-stack: {:?} ({} 项)", back_stack.stack(), back_stack.len()))
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            // 模式切换按钮
            let ld = list_detail.clone();
            Button::text()
                .on_click(move || ld.update(|v| *v = !*v))
                .build(ctx, |ctx| {
                    Text::new(if list_detail.get() { "模式: ListDetail 双栏" } else { "模式: SinglePane" }).build(ctx)
                });

            // 过渡规格循环按钮（demo 体验用——Nav3 transitionSpec 的常用取值）
            let si = spec_idx.clone();
            Button::text()
                .on_click(move || si.update(|v| *v = (*v + 1) % 4))
                .build(ctx, |ctx| {
                    Text::new(match spec_idx.get() {
                        1 => "过渡: 共享轴 (M3)",
                        2 => "过渡: 淡化 (Nav3 默认)",
                        3 => "过渡: 无 (瞬时切换)",
                        _ => "过渡: Android 滑动",
                    })
                    .build(ctx)
                });

            let home_bs = home_bs.clone();
            let detail_bs = detail_bs.clone();
            NavDisplay::new(&back_stack, move |ctx, key| match key {
                Route::Home => {
                    let bs = home_bs.clone();
                    NavEntry::new(key.clone(), move |ctx, _| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new("Home 页面").font_size(16.0).build(ctx);
                                Text::new("（列表栏——ListDetail 模式下显示在左侧）")
                                    .font_size(12.0)
                                    .color(Color::from_argb(255, 120, 120, 120))
                                    .build(ctx);
                                let bs = bs.clone();
                                Button::text()
                                    .on_click(move || bs.push(Route::Detail(42)))
                                    .build(ctx, |ctx| Text::new("打开 Detail 42").build(ctx));
                            });
                    })
                }
                Route::Detail(id) => {
                    let id = *id;
                    let bs = detail_bs.clone();
                    NavEntry::new(key.clone(), move |ctx, _| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new(format!("Detail 页面 (id={id})")).font_size(16.0).build(ctx);
                                // 状态保持演示：remember_entry_state 计数——返回本页后应保持
                                // （状态池跨导航存活——对标 Nav3 SaveableStateHolder）
                                let detail_count = remember_entry_state(|| 0i32);
                                Text::new(format!("本页状态计数: {}", detail_count.get()))
                                    .font_size(12.0)
                                    .color(Color::from_argb(255, 90, 120, 200))
                                    .build(ctx);
                                let dc = detail_count.clone();
                                Button::text()
                                    .on_click(move || dc.update(|v| *v += 1))
                                    .build(ctx, |ctx| Text::new("计数 +1").build(ctx));
                                let bs2 = bs.clone();
                                Button::text()
                                    .on_click(move || bs2.push(Route::Settings))
                                    .build(ctx, |ctx| Text::new("打开 Settings").build(ctx));
                                let bs3 = bs.clone();
                                Button::text()
                                    .on_click(move || { bs3.pop(); })
                                    .build(ctx, |ctx| Text::new("返回").build(ctx));
                            });
                    })
                }
                Route::Settings => {
                    let bs = detail_bs.clone();
                    NavEntry::new(key.clone(), move |ctx, _| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new("Settings 页面").font_size(16.0).build(ctx);
                                let bs = bs.clone();
                                Button::text()
                                    .on_click(move || { bs.pop(); })
                                    .build(ctx, |ctx| Text::new("返回 Detail").build(ctx));
                            });
                    })
                }
            })
            // Scene 策略链：ListDetail 双栏（宽屏列表+详情）或空链（SinglePane 兜底）
            .scene_strategies(if list_detail.get() {
                vec![Box::new(ListDetailStrategy) as Box<dyn SceneStrategy<Route>>]
            } else {
                Vec::new()
            })
            // 过渡规格（对标 Nav3 transitionSpec / popTransitionSpec——成对给出方向相反的规格）
            .transition_spec(push_spec)
            .pop_transition_spec(pop_spec)
            .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(600.0, 500.0)
                .title("Nav3 风格导航")
                .build(ctx, |ctx| nav_demo(ctx));
        });
    });
}
