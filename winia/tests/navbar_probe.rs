//! 临时探针：scaffold_demo 结构下 bottom bar 区域是否被内容侵入
use winia::core::composer::{ComposeCtx, Composer};
use winia::layout::Constraints;
use winia::modifier::{Modifier, ScrollState};
use winia::render;
use winia::ui::{
    Icon, NavigationBar, NavigationBarItem, NAVIGATION_BAR_ICON_SIZE, Scaffold, Text,
    ThemeColors, TopAppBar, TopAppBarScrollBehavior, TopAppBarState, WiniaTheme,
};

const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";

#[test]
fn probe_bar_region_bleed() {
    let theme = ThemeColors::default_light();
    let sc = theme.surface_container;
    let scroll_state = ScrollState::new();
    let app_bar_state = winia::ui::TopAppBarState::new(TOP_BAR_H);
    let behavior = TopAppBarScrollBehavior::new(scroll_state.clone(), TOP_BAR_H);
    let nested = TopAppBarScrollBehavior::enter_always(app_bar_state.clone(), TOP_BAR_H);
    let conn = nested
        .nested_scroll_connection_with_scroll(scroll_state.clone())
        .unwrap();

    let scroll_for_content = scroll_state.clone();
    let conn_for_content = conn.clone();
    let behavior_for_top = behavior.clone();

    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_and_direction(
            theme.clone(),
            winia::layout::LayoutDirection::Ltr,
            ctx,
            |ctx| {
                let behavior = behavior_for_top.clone();
                let scroll = scroll_for_content.clone();
                let conn = conn_for_content.clone();
                Scaffold::new(move |ctx, _p| {
                    let scroll = scroll.clone();
                    let conn = conn.clone();
                    winia::ui::Column::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_size()
                                .vertical_scroll(scroll)
                                .nested_scroll(conn),
                        )
                        .build(ctx, |ctx| {
                            for i in 0..30 {
                                Text::new(format!("Content item {i}"))
                                    .modifier(
                                        Modifier::new().padding(16.0).fill_max_width(),
                                    )
                                    .build(ctx);
                            }
                        });
                })
                .top_bar(move |ctx| {
                    TopAppBar::new(|ctx| Text::new("Scaffold demo").build(ctx))
                        .scroll_behavior(behavior.clone())
                        .build(ctx);
                })
                .bottom_bar(|ctx| {
                    NavigationBar::new(|ctx| {
                        for i in 0..3 {
                            NavigationBarItem::new(
                                i == 0,
                                |ctx| {
                                    Icon::svg_path(HOME_PATH)
                                        .size(winia::ui::NAVIGATION_BAR_ICON_SIZE)
                                        .build(ctx);
                                },
                            )
                            .label(move |ctx| Text::new("Tab").build(ctx))
                            .on_click(|| {})
                            .build(ctx);
                        }
                    })
                    .build(ctx);
                })
                .build(ctx);
            },
        );
    }));
    composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

    // 滚动到中部：渲染期直接读 ScrollState 最新 offset，无需重组
    scroll_state.offset.set(300.0);
    composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

    let mut surface = skia_safe::surfaces::raster_n32_premul((360, 640)).unwrap();
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().unwrap();
    render::render(composer.arena_nodes(), root, surface.canvas());

    let mut px = |x: i32, y: i32| -> (u8, u8, u8, u8) {
        let mut p = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut p, 4, (x, y));
        (p[0], p[1], p[2], p[3])
    };

    println!("surface_container = {:?}", (sc.r, sc.g, sc.b, sc.a));
    println!("bar 左下 (5,635)    = {:?}", px(5, 635));
    println!("bar 中部 (180,600)  = {:?}", px(180, 600));
    println!("内容区底部 (5,555)  = {:?}", px(5, 555));
    println!("内容区 (5,500)      = {:?}", px(5, 500));

    // bar 区域（y 561..639）逐行扫描非容器色像素（排除 item 图标/label 列区域）
    let mut bleed_rows = Vec::new();
    for y in 561..640 {
        let mut odd = 0;
        for x in 0..360 {
            let p = px(x, y);
            if (p.0, p.1, p.2) != (sc.r, sc.g, sc.b) {
                odd += 1;
            }
        }
        if odd > 60 {
            bleed_rows.push((y, odd));
        }
    }
    println!("异常行（非容器色 >60px）: {:?}", bleed_rows);
}

static TOP_BAR_H: f32 = 64.0;
