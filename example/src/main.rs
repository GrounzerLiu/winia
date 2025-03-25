use material_colors::color::Argb;
use material_colors::dynamic_color::Variant;
use material_colors::theme::ThemeBuilder;
use std::ops::Add;
use std::thread;
use std::time::Duration;
use winia::shared::{Children, Gettable, Settable, Shared, SharedBool, SharedF32, SharedSize, SharedText};
use winia::skia_safe::Color;
use winia::text::StyledText;
use winia::ui::animation::{AnimationExt, Target};
use winia::ui::app::{run_app, AppContext, AppProperty};
use winia::ui::component::{ImageExt, RectangleExt, RippleExt, ScaleMode, TextExt};
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::FlexWrap::Wrap;
use winia::ui::layout::{AlignContent, AlignItems, ColumnExt, FlexDirection, FlexExt, FlexWrap, JustifyContent, RowExt, ScrollAreaExt, StackExt};
use winia::ui::{App, Item};
use winia::{exclude_target, func, include_target, shared};
use winia::ui::animation::interpolator::Linear;

// #[cfg(not(target_os = "android"))]
fn main() {
    run_app(App::new(animation_test).title("Example").preferred_size(800, 600));
    // run_app(
    //     App::new(|app, shared| {
    //         app.flex(Children::new() +
    //             app.text_block("Hello, مرحبا بك في سكيا world!").color(Color::WHITE).item()
    //         ).wrap(FlexWrap::Wrap).item()
    //     })
    //         .title("Example")
    //         .preferred_size(800, 600)
    // );
}

pub fn animation_test(app: AppContext, property: AppProperty) -> Item {
    let width = Shared::from(100.0);
    app.row(Children::new() +
        app.rectangle(Color::TRANSPARENT)
            .radius(10.0)
            .outline_width(5.0)
            .outline_color(Color::BLACK)
            .item()
            .name("rectangle")
            .width(&width)
            .height(150.0)
/*            .on_click(
                |_| {
                    let app = app.clone();
                    let width = width.clone();
                    // app.animate(exclude_target!("blue")).transformation({
                    //     let mut width = width.clone();
                    //     move || {
                    //         if width.get() == Size::Fixed(100.0) {
                    //             width.set(Size::Fixed(200.0));
                    //         } else {
                    //             width.set(Size::Fixed(100.0));
                    //         }
                    //     }
                    // }).interpolator(Box::new(Linear::new())).duration(Duration::from_millis(1000)).start();
                }
            )*/ 
            .on_click(
                func!(|app, width|, move |_| {
                    app.animate(exclude_target!("blue")).transformation(
                        func!(|width|, move || {
                            if width.get() == Size::Fixed(100.0) {
                                width.set(Size::Fixed(200.0));
                            } else {
                                width.set(Size::Fixed(100.0));
                            }
                        })
                    ).interpolator(Box::new(Linear::new())).duration(Duration::from_millis(1000)).start();
                })
            )+
        app.rectangle(Color::BLUE)
            .radius(50.0)
            .item()
            .name("blue")
            .width(100.0)
            .height(100.0)
            .elevation(3.0)
    ).name("row")
        .padding_start(16.0)
        .padding_top(16.0)
}

pub fn rectangle_test(app: AppContext, property: AppProperty) -> Item {
    let color = Shared::from(Color::RED);
    let radius = Shared::from(50.0);
    let size = SharedSize::from(Size::Fixed(100.0));
    let align = Shared::from(Alignment::TopStart);
    
    app.stack(Children::new() +
        app.rectangle(&color)
            .radius(&radius)
            .item().size(&size, &size)
            .name("rectangle")
            .foreground(app.ripple().borderless(true).item().name("ripple"))
            .on_click({
                let app = app.clone();
                let size = size.clone();
                let color = color.clone();
                let radius = radius.clone();
                let align = align.clone();
                move |_|{
                    app.animate(include_target!("rectangle")).transformation({
                        let mut size = size.clone();
                        let mut color = color.clone();
                        let mut radius = radius.clone();
                        let mut align = align.clone();
                        move ||{
                            if color.get() == Color::RED {
                                size.set(Size::Fixed(50.0));
                                color.set(Color::BLUE);
                                radius.set(0.0);
                                align.set(Alignment::Center);
                            }else {
                                size.set(Size::Fixed(100.0));
                                color.set(Color::RED);
                                radius.set(50.0);
                                align.set(Alignment::TopStart);
                            }
                        }
                    }).interpolator(
                        Box::new(Linear::new())
                    ).duration(Duration::from_millis(5000)).start()
                }
            })
    )
        .item()
        .align_content(&align)
}


trait ToColor {
    fn to_color(&self) -> Color;
}

impl ToColor for Argb {
    fn to_color(&self) -> Color {
        Color::from_argb(self.alpha, self.red, self.green, self.blue)
    }
}

fn colors_test(app: AppContext, property: AppProperty) -> Item {
    let theme = ThemeBuilder::with_source(Argb::new(255, 255, 0, 0)).variant(Variant::Rainbow).build();
    let scheme = theme.schemes.dark;
    /*
pub struct Scheme {
    pub primary: Argb,
    pub on_primary: Argb,
    pub primary_container: Argb,
    pub on_primary_container: Argb,
    pub inverse_primary: Argb,
    pub primary_fixed: Argb,
    pub primary_fixed_dim: Argb,
    pub on_primary_fixed: Argb,
    pub on_primary_fixed_variant: Argb,
    pub secondary: Argb,
    pub on_secondary: Argb,
    pub secondary_container: Argb,
    pub on_secondary_container: Argb,
    pub secondary_fixed: Argb,
    pub secondary_fixed_dim: Argb,
    pub on_secondary_fixed: Argb,
    pub on_secondary_fixed_variant: Argb,
    pub tertiary: Argb,
    pub on_tertiary: Argb,
    pub tertiary_container: Argb,
    pub on_tertiary_container: Argb,
    pub tertiary_fixed: Argb,
    pub tertiary_fixed_dim: Argb,
    pub on_tertiary_fixed: Argb,
    pub on_tertiary_fixed_variant: Argb,
    pub error: Argb,
    pub on_error: Argb,
    pub error_container: Argb,
    pub on_error_container: Argb,
    pub surface_dim: Argb,
    pub surface: Argb,
    pub surface_tint: Argb,
    pub surface_bright: Argb,
    pub surface_container_lowest: Argb,
    pub surface_container_low: Argb,
    pub surface_container: Argb,
    pub surface_container_high: Argb,
    pub surface_container_highest: Argb,
    pub on_surface: Argb,
    pub on_surface_variant: Argb,
    pub outline: Argb,
    pub outline_variant: Argb,
    pub inverse_surface: Argb,
    pub inverse_on_surface: Argb,
    pub surface_variant: Argb,
    pub background: Argb,
    pub on_background: Argb,
    pub shadow: Argb,
    pub scrim: Argb,
}
*/
    let primary = scheme.primary.to_color();
    let on_primary = scheme.on_primary.to_color();
    let primary_container = scheme.primary_container.to_color();
    let on_primary_container = scheme.on_primary_container.to_color();
    let inverse_primary = scheme.inverse_primary.to_color();
    let primary_fixed = scheme.primary_fixed.to_color();
    let primary_fixed_dim = scheme.primary_fixed_dim.to_color();
    let on_primary_fixed = scheme.on_primary_fixed.to_color();
    let on_primary_fixed_variant = scheme.on_primary_fixed_variant.to_color();
    let secondary = scheme.secondary.to_color();
    let on_secondary = scheme.on_secondary.to_color();
    let secondary_container = scheme.secondary_container.to_color();
    let on_secondary_container = scheme.on_secondary_container.to_color();
    let secondary_fixed = scheme.secondary_fixed.to_color();
    let secondary_fixed_dim = scheme.secondary_fixed_dim.to_color();
    let on_secondary_fixed = scheme.on_secondary_fixed.to_color();
    let on_secondary_fixed_variant = scheme.on_secondary_fixed_variant.to_color();
    let tertiary = scheme.tertiary.to_color();
    let on_tertiary = scheme.on_tertiary.to_color();
    let tertiary_container = scheme.tertiary_container.to_color();
    let on_tertiary_container = scheme.on_tertiary_container.to_color();
    let tertiary_fixed = scheme.tertiary_fixed.to_color();
    let tertiary_fixed_dim = scheme.tertiary_fixed_dim.to_color();
    let on_tertiary_fixed = scheme.on_tertiary_fixed.to_color();
    let on_tertiary_fixed_variant = scheme.on_tertiary_fixed_variant.to_color();
    let error = scheme.error.to_color();
    let on_error = scheme.on_error.to_color();
    let error_container = scheme.error_container.to_color();
    let on_error_container = scheme.on_error_container.to_color();
    let surface_dim = scheme.surface_dim.to_color();
    let surface = scheme.surface.to_color();
    let surface_tint = scheme.surface_tint.to_color();
    let surface_bright = scheme.surface_bright.to_color();
    let surface_container_lowest = scheme.surface_container_lowest.to_color();
    let surface_container_low = scheme.surface_container_low.to_color();
    let surface_container = scheme.surface_container.to_color();
    let surface_container_high = scheme.surface_container_high.to_color();
    let surface_container_highest = scheme.surface_container_highest.to_color();
    let on_surface = scheme.on_surface.to_color();
    let on_surface_variant = scheme.on_surface_variant.to_color();
    let outline = scheme.outline.to_color();
    let outline_variant = scheme.outline_variant.to_color();
    let inverse_surface = scheme.inverse_surface.to_color();
    let inverse_on_surface = scheme.inverse_on_surface.to_color();
    let surface_variant = scheme.surface_variant.to_color();
    let background = scheme.background.to_color();
    let on_background = scheme.on_background.to_color();
    let shadow = scheme.shadow.to_color();
    let scrim = scheme.scrim.to_color();

    fn item(app: &AppContext, name: &str, color: Color) -> Item {
        app.column(Children::new() +
            app.text(name).color(Color::WHITE).item() +
            app.rectangle(color).radius(25.0).item().size(200.0, 50.0)
        )
    }

    app.scrollarea(Children::new()+
        app.flex(Children::new() +
            item(&app, "primary", primary) +
            item(&app, "on_primary", on_primary) +
            item(&app, "primary_container", primary_container) +
            item(&app, "on_primary_container", on_primary_container) +
            item(&app, "inverse_primary", inverse_primary) +
            item(&app, "primary_fixed", primary_fixed) +
            item(&app, "primary_fixed_dim", primary_fixed_dim) +
            item(&app, "on_primary_fixed", on_primary_fixed) +
            item(&app, "on_primary_fixed_variant", on_primary_fixed_variant) +
            item(&app, "secondary", secondary) +
            item(&app, "on_secondary", on_secondary) +
            item(&app, "secondary_container", secondary_container) +
            item(&app, "on_secondary_container", on_secondary_container) +
            item(&app, "secondary_fixed", secondary_fixed) +
            item(&app, "secondary_fixed_dim", secondary_fixed_dim) +
            item(&app, "on_secondary_fixed", on_secondary_fixed) +
            item(&app, "on_secondary_fixed_variant", on_secondary_fixed_variant) +
            item(&app, "tertiary", tertiary) +
            item(&app, "on_tertiary", on_tertiary) +
            item(&app, "tertiary_container", tertiary_container) +
            item(&app, "on_tertiary_container", on_tertiary_container) +
            item(&app, "tertiary_fixed", tertiary_fixed) +
            item(&app, "tertiary_fixed_dim", tertiary_fixed_dim) +
            item(&app, "on_tertiary_fixed", on_tertiary_fixed) +
            item(&app, "on_tertiary_fixed_variant", on_tertiary_fixed_variant) +
            item(&app, "error", error) +
            item(&app, "on_error", on_error) +
            item(&app, "error_container", error_container) +
            item(&app, "on_error_container", on_error_container) +
            item(&app, "surface_dim", surface_dim) +
            item(&app, "surface", surface) +
            item(&app, "surface_tint", surface_tint) +
            item(&app, "surface_bright", surface_bright) +
            item(&app, "surface_container_lowest", surface_container_lowest) +
            item(&app, "surface_container_low", surface_container_low) +
            item(&app, "surface_container", surface_container) +
            item(&app, "surface_container_high", surface_container_high) +
            item(&app, "surface_container_highest", surface_container_highest) +
            item(&app, "on_surface", on_surface) +
            item(&app, "on_surface_variant", on_surface_variant) +
            item(&app, "outline", outline) +
            item(&app, "outline_variant", outline_variant) +
            item(&app, "inverse_surface", inverse_surface) +
            item(&app, "inverse_on_surface", inverse_on_surface) +
            item(&app, "surface_variant", surface_variant) +
            item(&app, "background", background) +
            item(&app, "on_background", on_background) +
            item(&app, "shadow", shadow) +
            item(&app, "scrim", scrim)
        ).wrap(Wrap)
            .cross_axis_gap(10.0)
            .main_axis_gap(10.0)
            .item()
            .width(800)
    ).item()
}

fn multi_thread_test(app: AppContext, property: AppProperty) -> Item {
    let color = Shared::from(Color::RED);
    let text = Shared::from("Hello, world!");
    app.column(Children::new()+
        app.rectangle(Color::RED)
            .color(&color)
            .item()
            .width(200.0)
            .height(200.0)
            .on_click({
                let mut color = color.clone();
                let mut text = text.clone();
                move |_| {
                    let mut color = color.clone();
                    let mut text = text.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(1));
                        if color.get() == Color::RED {
                            color.set(Color::BLUE);
                            text.set_static("I love you!".into());
                        } else {
                            color.set(Color::RED);
                            text.set_static("Hello, world!".into());
                        }
                    });
                }
            }) +
        app.image("/home/grounzer/RustroverProjects/winia/example/unnamed.png")
            .oversize_scale_mode(ScaleMode::Cover)
            .item()
            .align_content(Alignment::CenterEnd)
            .size(400, 200) +
        app.text("").text(&text).color(Color::WHITE).item()
    )
}

fn ripple_test(app: AppContext, property: AppProperty) -> Item {

    // let mut f32 = SharedF32::new(100.0);
    //
    // thread::spawn(move || {
    //     loop {
    //         thread::sleep(Duration::from_secs(1));
    //         f32.set(f32.get() + 10.0);
    //     }
    // });
    //
    let mut children = Children::new();
    for i in 0..100 {
        children = children.add(
            app.rectangle(Color::TRANSPARENT)
                .item()
                .width(Size::Fixed(30.0))
                .height(Size::Fixed(30.0))
                .foreground(app.ripple().borderless(true).item())
        )
    }

    app.scrollarea(Children::new()+
        app.flex(children)
            .wrap(FlexWrap::Wrap)
            .main_axis_gap(0.0)
            .cross_axis_gap(0.0)
            .item()
            .width(Size::Fixed(800.0))
    ).item()

    // app.stack(Children::new() +
    //     app.rectangle(Color::TRANSPARENT)
    //         .item()
    //         .width(Size::Fixed(100.0))
    //         .height(Size::Fixed(100.0))
    //         .foreground(app.ripple().borderless(true).item())
    //         .on_hover(|is_hovered| {
    //             println!("Rectangle hovered: {}", is_hovered);
    //         }) +
    //     app.rectangle(Color::TRANSPARENT)
    //         .item()
    //         .width(Size::Fixed(36.0))
    //         .height(Size::Fixed(36.0))
    //         .align_self(Alignment::Center)
    //         .foreground(app.ripple().borderless(true).item())
    //         .on_hover(|is_hovered| {
    //             println!("Rectangle hovered: {}", is_hovered);
    //         }) +
    //     app.text("text").color(Color::WHITE).item().align_self(Alignment::BottomEnd) +
    //     app.text("text").color(Color::WHITE).item().align_self(Alignment::BottomCenter)
    // ).item()
    //     .width(Size::Expanded).height(Size::Expanded)

    // app.scrollarea(Children::new()+
    //     app.stack(Children::new()+
    //         app.rectangle(Color::BLUE)
    //             .item()
    //             .width(Size::Fixed(100.0))
    //             .height(Size::Fixed(100.0))
    //             .foreground(app.ripple().borderless(true).item())
    //             .on_hover(|is_hovered|{
    //                 println!("Rectangle hovered: {}", is_hovered);
    //             }) +
    //         app.rectangle(Color::TRANSPARENT)
    //             .item()
    //             .width(Size::Fixed(36.0))
    //             .height(Size::Fixed(36.0))
    //             .align_self(Alignment::Center)
    //             .foreground(app.ripple().borderless(true).item())
    //             .on_hover(|is_hovered|{
    //                 println!("Rectangle hovered: {}", is_hovered);
    //             }) +
    //         app.text("text").color(Color::WHITE).item().align_self(Alignment::BottomEnd) +
    //         app.rectangle(Color::RED)
    //             .item()
    //             .width(Size::Fixed(100.0))
    //             .height(Size::Fixed(100.0))
    //             .foreground(app.ripple().borderless(true).item())
    //             .on_hover(|is_hovered|{
    //                 println!("Rectangle hovered: {}", is_hovered);
    //             }).align_self(Alignment::BottomCenter)
    //     ).item()
    //         .width(Size::Fixed(800.0)).height(Size::Fixed(1000.0))
    // ).item()
}

fn flex_test_ui(app: AppContext, property: AppProperty) -> Item {
    let size = SharedSize::from(Size::Fixed(150.0));
    app.flex(
        Children::new()
            + app.column(
                Children::new()
                    + app
                        .rectangle(Color::YELLOW)
                        .item()
                        .width(Size::Fixed(50.0))
                        .height(Size::Fixed(50.0))
                    + app
                        .rectangle(Color::GREEN)
                        .item()
                        .width(Size::Fixed(50.0))
                        .height(Size::Fixed(50.0))
                    + app
                        .rectangle(Color::BLUE)
                        .item()
                        .width(Size::Fixed(50.0))
                        .height(Size::Fixed(50.0))
                    + app
                        .rectangle(Color::YELLOW)
                        .item()
                        .width(Size::Fixed(50.0))
                        .height(Size::Fixed(50.0))
                    + app
                        .rectangle(Color::GREEN)
                        .item()
                        .width(Size::Fixed(50.0))
                        .height(Size::Fixed(50.0)),
            )
            + app
                .rectangle(Color::RED)
                .item()
                .height(&size)
                .width(Size::Fixed(250.0))
            + app
                .rectangle(Color::GREEN)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::BLUE)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::RED)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::GREEN)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::BLUE)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::RED)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::GREEN)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::BLUE)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::RED)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::GREEN)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::BLUE)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::RED)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::GREEN)
                .item()
                .width(&size)
                .height(&size)
            + app
                .rectangle(Color::BLUE)
                .item()
                .width(&size)
                .height(&size),
    )
    .direction(FlexDirection::Horizontal)
    .wrap(FlexWrap::Wrap)
    .justify_content(JustifyContent::Start)
    .align_items(AlignItems::Start)
    .align_content(AlignContent::Start)
    // .main_axis_gap(10.0)
    // .cross_axis_gap(10.0)
    .item()
    .on_click(func!(|app,size|, move|_|{
        if let Size::Fixed(size_value) = size.get() {
            if size_value > 150.0 {
                app.animate(Target::Exclusion(Vec::new()))
                    .transformation(func!(|size|,move|| {
                        size.set(Size::Fixed(150.0));
                    })).duration(Duration::from_millis(500)).start();
            }else {
                app.animate(Target::Exclusion(Vec::new()))
                    .transformation(func!(|size|,move|| {
                        size.set(Size::Fixed(200.0));
                    })).duration(Duration::from_millis(500)).start();
            }
        }
    }))
}

fn text_test_ui(app: AppContext, property: AppProperty) -> Item {
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
";
    app.stack(
        Children::new()+
            app.rectangle(Color::RED)
                .item()
                .width(Size::Fixed(100.0))
                .height(Size::Fixed(100.0))
            // + app
            //     .text(text)
            //     .color(Color::WHITE)
            //     .item()
            //     .width(Size::Expanded),
    )
    .item()
}

fn main_ui(app: AppContext, property: AppProperty) -> Item {
    let size = SharedSize::from(100.0);
    let offset = SharedF32::from(0.0);
    let margin_start = SharedF32::from(150.0);
    let margin_top = SharedF32::from(50.0);

    // let txt_file = "/home/grounzer/Downloads/long.txt";
    // let long_text = std::fs::read_to_string(txt_file).unwrap();
    //

    let a = SharedBool::from(false);
    let b = SharedBool::from(false);

    let c = SharedBool::from(false);

    let mut f32 = SharedF32::new(100.0);
    let mut size = shared!(|f32| {
        Size::Fixed(f32.get())
    });

    let text = SharedText::from("Hello, world!");
    app.stack(Children::new() +
        app.stack(Children::new() +
            app.rectangle(Color::BLUE)
                .item().width(Size::Fixed(100.0)).height(Size::Fixed(100.0))
                // .skew_x(1.0)
                // .rotation(45.0)
                // .opacity(0.5)
                .name("blue_rect")
                .on_click(func!(|app, property, c, text|, move|_|{
                    println!("Blue rectangle clicked");
                    // property.title().set("Blue rectangle clicked".to_string());
                    // property.maximized().set(property.maximized().get().not())
                    app.animate(Target::Exclusion(Vec::new()))
                    .transformation(func!(|c, text|,move|| {
                        println!("c = {}", c.get());
                        if c.get() {
                            text.set(StyledText::from("Hello, world!"));
                            c.set(false);
                        } else {
                            text.set(StyledText::from("This is a new text,This is a "));
                            c.set(true);
                        }
            })).duration(Duration::from_millis(500)).start();
                })) +

            // app.flex(Children::new() +
            //     app.rectangle()
            //         .color(Color::RED)
            //         .item().width(&size).height(&size)
            //         .focused(&a)
            //         .on_focus(|focused| {
            //             println!("Red rectangle focused: {}", focused);
            //         })
            //         .on_click(func!(|app,f32,a|, move|_|{
            //             if a.get() {
            //                 f32.animation_to_f32(100.0)
            //                 .duration(Duration::from_secs(5))
            //                 .interpolator(winia::ui::animation::interpolator::EaseOutCirc::new())
            //                 .start(&app);
            //                 a.set(false);
            //             } else {
            //                 f32.animation_to_f32(200.0)
            //                 .duration(Duration::from_secs(5))
            //                 .interpolator(winia::ui::animation::interpolator::EaseOutCirc::new())
            //                 .start(&app);
            //                 a.set(true);
            //             }
            //         })) +
            //     app.rectangle()
            //         .color(Color::YELLOW)
            //         .item().width(Size::Fixed(50.0)).height(Size::Fixed(50.0))
            //         .focused(&b)
            //         .on_focus(|focused| {
            //             println!("Yellow rectangle focused: {}", focused);
            //         })
            //         .on_click(func!(|b|, move|_|{
            //             b.set(true);
            //         }))+
            //     app.text(&text).color(Color::RED).item()
            // )
            //     .direction(FlexDirection::Horizontal)
            //     .wrap(FlexWrap::Wrap).item()
            //     .width(Size::Fixed(400.0))+

            app.rectangle(Color::WHITE).item()
                .name("white_rect")
                .width(&size).height(&size)
                .offset_x(&offset)
                .offset_y(&offset)
                .margin_start(&margin_start)
                .margin_top(&margin_top)
                .name("red_rect")
                .enable_background_blur(true)
                .opacity(0.5)
                // .rotation(-30.0)
                // .rotation_center_x(InnerPosition::End(0.0))
                // .rotation_center_y(InnerPosition::End(0.0))
                .on_click(func!(|app, size, margin_start, margin_top, offset|, move|_|{
                    app.animate(include_target!("white_rect"))
                    .transformation(func!(|size, margin_start, margin_top, offset|,move|| {
                        if let Size::Expanded = size.get() {
                            size.set(Size::Fixed(100.0));
                            margin_start.set(150.0);
                            margin_top.set(50.0);
                            offset.set(0.0);
                        } else {
                            size.set(Size::Expanded);
                            margin_start.set(0.0);
                            margin_top.set(0.0);
                            offset.set(-50.0);
                        }
                    })).duration(Duration::from_secs(5)).start();
                }))
        )
            .item()
            .width(Size::Fixed(400.0))
            .height(Size::Fixed(400.0))
            .background(
                app.rectangle(Color::GREEN).item()
            )
    )
        .item()
        .width(Size::Expanded)
        .height(Size::Expanded)
        .padding_start(10.0)
        .padding_top(10.0)
        .name("root")
}
