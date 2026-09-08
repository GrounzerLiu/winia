//! Navigation3 风格导航 demo——状态驱动的 back stack + NavDisplay 投影。
//!
//! 展示：
//! - 类型安全路由（Route enum 实现 NavKey）
//! - NavBackStack：push/pop（导航 = 改列表；Clone 共享内部 State）
//! - NavDisplay：entry_provider 路由 → 内容（SinglePane——渲染栈顶）
//! - SceneStrategy：SinglePane（默认）/ ListDetail（双栏）切换
//! - NavTransition 滑动过渡：push 新页右入/旧页左出，pop 反向
//! - SceneDecoratorStrategy：scene 级装饰（顶部导航栏）
//! - ResultEventBus：Settings 选择结果返回 Detail（send/take）
//! - NavMetadata：entry 携带类型化元数据
//!
//! 运行：cargo run -p winia --example nav_demo --features debug-server

use letclone::clone;
use winia::prelude::*;
use winia::nav::{remember_entry_state, result_event_bus, ListDetailStrategy, NavBackStack, NavDisplay, NavEntry, NavKey, NavMetadata, NavTransitionSpec, SceneDecoratorStrategy, SceneStrategy};

/// 类型安全路由（对标 Nav3 的 NavKey + @Serializable——winia 无序列化要求）
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
enum Route {
    Home,
    Detail(u64),
    Settings,
    About,
}

/// Settings 页返回的结果（经 ResultEventBus send/take——对标 Nav3 result API）
#[derive(Clone, Debug, PartialEq)]
struct ThemeChoice {
    name: &'static str,
    color: Color,
}

/// entry 元数据（对标 Nav3 metadata {}——类型化附加信息）
#[derive(Clone, Debug, PartialEq)]
struct MetaInfo {
    page_id: u64,
    source: &'static str,
}

/// Scene 装饰器：顶部导航栏（对标 Nav3 SceneDecoratorStrategy——scene 级装饰）
struct TopBarDecorator;
impl SceneDecoratorStrategy<Route> for TopBarDecorator {
    fn decorate_scene(&self, scene: Box<dyn winia::nav::Scene<Route>>) -> Box<dyn winia::nav::Scene<Route>> {
        struct Decorated {
            inner: Box<dyn winia::nav::Scene<Route>>,
        }
        impl winia::nav::Scene<Route> for Decorated {
            fn scene_key(&self) -> u64 { self.inner.scene_key() }
            fn entries(&self) -> &[winia::nav::NavEntry<Route>] { self.inner.entries() }
            fn content(
                &self,
                ctx: &mut ComposeCtx,
                render_entry: &dyn Fn(&mut ComposeCtx, &winia::nav::NavEntry<Route>, bool),
            ) {
                // 装饰：顶部导航栏（红条——场景级装饰的视觉证据）
                Column::new()
                    .modifier(Modifier::new().fill_max_width().background(
                        Color::from_argb(255, 60, 60, 90),
                        Shape::RoundedRect { corner_radius: 4.0 },
                    ).padding_horizontal(8.0).padding_vertical(4.0))
                    .build(ctx, |ctx| {
                        Text::new("◤ Scene 装饰栏（SceneDecoratorStrategy）")
                            .font_size(11.0)
                            .color(Color::from_argb(255, 255, 255, 255))
                            .build(ctx);
                    });
                // 原 scene 内容
                self.inner.content(ctx, render_entry);
            }
        }
        Box::new(Decorated { inner: scene })
    }
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
    // NavBackStack is Clone (shares the inner State) — clone into button closures for navigation.
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
            Button::text()
                .on_click({ clone!(list_detail); move || list_detail.update(|v| *v = !*v) })
                .build(ctx, |ctx| {
                    Text::new(if list_detail.get() { "模式: ListDetail 双栏" } else { "模式: SinglePane" }).build(ctx)
                });

            // 过渡规格循环按钮（demo 体验用——Nav3 transitionSpec 的常用取值）
            Button::text()
                .on_click({ clone!(spec_idx); move || spec_idx.update(|v| *v = (*v + 1) % 4) })
                .build(ctx, |ctx| {
                    Text::new(match spec_idx.get() {
                        1 => "过渡: 共享轴 (M3)",
                        2 => "过渡: 淡化 (Nav3 默认)",
                        3 => "过渡: 无 (瞬时切换)",
                        _ => "过渡: Android 滑动",
                    })
                    .build(ctx)
                });

            NavDisplay::new(&back_stack, {
                clone!(home_bs, detail_bs);
                move |ctx, key| match key {
                Route::Home => {
                    NavEntry::new(key.clone(), {
                        clone!(home_bs);
                        move |ctx, _| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new("Home 页面").font_size(16.0).build(ctx);
                                Text::new("（列表栏——ListDetail 模式下显示在左侧）")
                                    .font_size(12.0)
                                    .color(Color::from_argb(255, 120, 120, 120))
                                    .build(ctx);
                                Button::text()
                                    .on_click({ clone!(home_bs); move || home_bs.push(Route::Detail(42)) })
                                    .build(ctx, |ctx| Text::new("打开 Detail 42").build(ctx));
                            });
                        }
                    })
                }
                Route::Detail(id) => {
                    let id = *id;
                    NavEntry::new(key.clone(), {
                        clone!(detail_bs);
                        move |ctx, _| {
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
                                // result API 演示：Settings 返回时读取结果（高级原语——
                                // 对标 Nav3 ResultEffect：内部自动跳过退场帧 + 状态持久化，
                                // 消费到的结果跨过渡稳定显示）
                                let received = winia::nav::result_event_bus_consume::<ThemeChoice>("theme");
                                if let Some(choice) = received.get() {
                                    Text::new(format!("← 收到结果: 主题色 = {}", choice.name))
                                        .font_size(12.0)
                                        .color(choice.color)
                                        .build(ctx);
                                }
                                Button::text()
                                    .on_click({ clone!(detail_count); move || detail_count.update(|v| *v += 1) })
                                    .build(ctx, |ctx| Text::new("计数 +1").build(ctx));
                                Button::text()
                                    .on_click({ clone!(detail_bs); move || detail_bs.push(Route::Settings) })
                                    .build(ctx, |ctx| Text::new("打开 Settings").build(ctx));
                                Button::text()
                                    .on_click({ clone!(detail_bs); move || { detail_bs.push(Route::About); } })
                                    .build(ctx, |ctx| Text::new("打开关于对话框").build(ctx));
                                Button::text()
                                    .on_click({ clone!(detail_bs); move || { detail_bs.pop(); } })
                                    .build(ctx, |ctx| Text::new("返回").build(ctx));
                            });
                        }
                    })
                    // metadata 演示（对标 Nav3 metadata {}——entry 携带类型化元数据）
                    .metadata(NavMetadata::new().with(MetaInfo { page_id: id, source: "nav_demo" }))
                }
                Route::Settings => {
                    NavEntry::new(key.clone(), {
                        clone!(detail_bs);
                        move |ctx, _| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_width())
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new("Settings 页面").font_size(16.0).build(ctx);
                                // result API 演示：选择主题色 → send 结果 + pop 返回
                                // （对标 Nav3 ResultEventBus：B 返回时带结果给 A）
                                // ⚠ result_event_bus() 是组合期 API（CompositionLocal
                                // 作用域内）——按钮回调里不能调，须组合期捕获再 move 进闭包
                                let bus = result_event_bus();
                                Text::new("选择主题色（返回时带结果）：")
                                    .font_size(12.0)
                                    .color(Color::from_argb(255, 120, 120, 120))
                                    .build(ctx);
                                for (name, color) in [
                                    ("蓝色", Color::from_argb(255, 66, 133, 244)),
                                    ("绿色", Color::from_argb(255, 52, 168, 83)),
                                    ("橙色", Color::from_argb(255, 255, 153, 0)),
                                ] {
                                    Button::text()
                                        .on_click({
                                            clone!(detail_bs, bus);
                                            move || {
                                                // 发送结果（覆盖同 key 旧值）+ 返回
                                                bus.send("theme", ThemeChoice { name, color });
                                                detail_bs.pop();
                                            }
                                        })
                                        .build(ctx, |ctx| {
                                            Text::new(format!("用 {name}")).build(ctx);
                                        });
                                }
                                Button::text()
                                    .on_click({ clone!(detail_bs); move || { detail_bs.pop(); } })
                                    .build(ctx, |ctx| Text::new("返回 Detail（不带结果）").build(ctx));
                            });
                        }
                    })
                }
                // 对话框路由（对标 Nav3 dialog() metadata + DialogSceneStrategy）：
                // 栈顶时渲染为模态覆盖层（主树不渲染其内容），dismiss = 弹栈
                Route::About => {
                    NavEntry::new(key.clone(), {
                        clone!(detail_bs);
                        move |ctx, _| {
                        // 对话框内容 wrap-content + 卡片背景（圆角白底）——
                        // overlay Center 定位按内容尺寸居中；fill_max_width 全宽
                        // 会让"居中"失效。Compose Dialog 的典型样式
                        Column::new()
                            .modifier(Modifier::new()
                                .padding(24.0)
                                .background(
                                    Color::from_argb(255, 240, 240, 245),
                                    Shape::RoundedRect { corner_radius: 12.0 },
                                ))
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                Text::new("关于对话框").font_size(16.0).build(ctx);
                                Text::new("winia Navigation3 风格导航 demo")
                                    .font_size(12.0)
                                    .color(Color::from_argb(255, 120, 120, 120))
                                    .build(ctx);
                                Button::text()
                                    .on_click({ clone!(detail_bs); move || { detail_bs.pop(); } })
                                    .build(ctx, |ctx| Text::new("关闭").build(ctx));
                            });
                        }
                    })
                    .as_dialog()
                }
            }
            })
            // Scene 策略链：ListDetail 双栏（宽屏列表+详情）或空链（SinglePane 兜底）
            .scene_strategies(if list_detail.get() {
                vec![Box::new(ListDetailStrategy) as Box<dyn SceneStrategy<Route>>]
            } else {
                Vec::new()
            })
            // Scene 装饰器（对标 Nav3 sceneDecoratorStrategies——顶部导航栏）
            .scene_decorator_strategies(vec![Box::new(TopBarDecorator)])
            // 过渡规格（对标 Nav3 transitionSpec / popTransitionSpec——成对给出方向相反的规格）
            .transition_spec(push_spec)
            .pop_transition_spec(pop_spec)
            .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(600.0, 500.0)
                .title("Nav3 风格导航")
                .build(ctx, |ctx| nav_demo(ctx));
        });
    });
}
