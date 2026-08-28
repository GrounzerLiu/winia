//! Navigation3 风格导航 demo——状态驱动的 back stack + NavDisplay 投影。
//!
//! 展示：
//! - 类型安全路由（Route enum 实现 NavKey）
//! - NavBackStack：push/pop（导航 = 改列表；Clone 共享内部 State）
//! - NavDisplay：entry_provider 路由 → 内容（SinglePane——渲染栈顶）
//! - SceneStrategy：SinglePane（默认）/ ListDetail（双栏）切换
//! - Crossfade 过渡：导航切换时交叉淡化
//!
//! 运行：cargo run -p winia --example nav_demo --features debug-server

use winia::prelude::*;
use winia::nav::{ListDetailStrategy, NavBackStack, NavDisplay, NavEntry, NavKey, SceneStrategy, SinglePaneStrategy};

/// 类型安全路由（对标 Nav3 的 NavKey + @Serializable——winia 无序列化要求）
#[derive(Clone, PartialEq, Eq, Debug)]
enum Route {
    Home,
    Detail(u64),
}

#[composable]
fn nav_demo(ctx: &mut ComposeCtx) {
    // NavBackStack 是 Clone（内部 State<Vec> 共享）——可传给按钮闭包触发导航
    let back_stack = ctx.remember(|| NavBackStack::<Route>::with_initial(Route::Home)).get();
    // 双栏模式开关（ListDetail 策略 vs SinglePane）
    let list_detail = ctx.remember(|| false);
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
                                let bs = bs.clone();
                                Button::text()
                                    .on_click(move || { bs.pop(); })
                                    .build(ctx, |ctx| Text::new("返回").build(ctx));
                            });
                    })
                }
            })
            // Scene 策略：ListDetail 双栏（宽屏列表+详情）或 SinglePane
            .scene_strategy(if list_detail.get() {
                Box::new(ListDetailStrategy) as Box<dyn SceneStrategy<Route>>
            } else {
                Box::new(SinglePaneStrategy) as Box<dyn SceneStrategy<Route>>
            })
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
