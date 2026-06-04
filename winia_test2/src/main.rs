
mod button_test;
mod label_test;
mod ripple_test;
mod scroll_area_test;
mod text_field_test;
mod slider_test;
mod animation_test;
mod video_player_test;
mod animated_content_test;
mod rectangle_test;
mod shape_test;
mod loading_indicator_test;
mod badge_test;
mod icon_test;
mod navigation_test;
mod lazy_list_test;

use crate::button_test::button_test;
use crate::label_test::label_test;
use crate::ripple_test::ripple_test;
use crate::scroll_area_test::scroll_area_test;
use std::thread;
use std::time::Duration;
use letclone::clone;
use unicode_segmentation::UnicodeSegmentation;
use winia::animation::AnimationExt;
use winia::app::{run_app, App, WindowAttributes, WindowContext};
use winia::icon::Outlined;
use winia::{closure_use, exclude_target, With};
use winia::shared::{SharedBool, SharedColor, SharedSize, SharedSource, SharedString, SharedText, SharedWVec};
use winia::text::{AddTextAttribute, StyledText, TextShadow};
use winia::theme::material_theme;
use winia::ui::button::ButtonPropsTrait;
use winia::ui::item::{FocusRequesterTrait, Padding};
use winia::ui::{button, flex, icon, image, label, nav_display, rectangle, ripple, scroll_area, stack, AlignSelf, Alignment, Color, ColumnPropsTrait, DefaultScrollAreaProps, IconPropsTrait, ImagePropsTrait, Item, LabelPropsTrait, NavDisplayPropsTrait, NavKey, Radius, RectanglePropsTrait, RipplePropsTrait, RowPropsTrait, Size, StackPropsTrait, TextFieldPropsTrait};
use crate::animated_content_test::animated_content_test;
use crate::animation_test::animation_test;
use crate::badge_test::badge_test;
use crate::icon_test::icon_test;
use crate::loading_indicator_test::loading_indicator_test;
use crate::navigation_test::navigation_test;
use crate::rectangle_test::rectangle_test;
use crate::shape_test::shape_test;
use crate::slider_test::slider_test;
use crate::text_field_test::text_field_test;
use crate::video_player_test::video_player_test;

fn main() {
    run_app(App::new(
        main_ui,
        WindowAttributes::default()
            .title("Winia Test")
            // .preferred_size(100.0, 100.0),
    ));
}

fn main_ui(w: &WindowContext) -> Item {
    let back_stack: SharedWVec<Box<dyn NavKey>> =
        vec![Box::new(TestRoute::Home) as Box<dyn NavKey>].into();
    nav_display(
        w.nav_display_props(back_stack.clone(), {
            let w = w.clone();
            move |key| build_test_route(&w, key, &back_stack)
        })
        .size(Size::Fill, Size::Fill)
        .alignment(Alignment::top_start()),
    )
}

#[derive(Clone)]
enum TestRoute {
    Home,
    AnimatedContent,
    Animation,
    Badge,
    Button,
    Icon,
    Label,
    LoadingIndicator,
    MainUi2,
    Navigation,
    Rectangle,
    Ripple,
    ScrollArea,
    Shape,
    Slider,
    TextField,
    VideoPlayer,
    LazyList,
}

impl NavKey for TestRoute {
    fn nav_key(&self) -> &'static str {
        match self {
            TestRoute::Home => "home",
            TestRoute::AnimatedContent => "animated_content",
            TestRoute::Animation => "animation",
            TestRoute::Badge => "badge",
            TestRoute::Button => "button",
            TestRoute::Icon => "icon",
            TestRoute::Label => "label",
            TestRoute::LoadingIndicator => "loading_indicator",
            TestRoute::MainUi2 => "main_ui_2",
            TestRoute::Navigation => "navigation",
            TestRoute::Rectangle => "rectangle",
            TestRoute::Ripple => "ripple",
            TestRoute::ScrollArea => "scroll_area",
            TestRoute::Shape => "shape",
            TestRoute::Slider => "slider",
            TestRoute::TextField => "text_field",
            TestRoute::VideoPlayer => "video_player",
            TestRoute::LazyList => "lazy_list",
        }
    }
}

type TestBuilder = fn(&WindowContext) -> Item;

fn build_test_route(
    w: &WindowContext,
    key: &Box<dyn NavKey>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
) -> Item {
    let route = (key.as_ref() as &dyn std::any::Any)
        .downcast_ref::<TestRoute>()
        .expect("main navigation only accepts TestRoute");

    match route {
        TestRoute::Home => test_list_page(w, back_stack),
        TestRoute::AnimatedContent => test_page(w, back_stack, "Animated Content Test", animated_content_test),
        TestRoute::Animation => test_page(w, back_stack, "Animation Test", animation_test),
        TestRoute::Badge => test_page(w, back_stack, "Badge Test", badge_test),
        TestRoute::Button => test_page(w, back_stack, "Button Test", button_test),
        TestRoute::Icon => test_page(w, back_stack, "Icon Test", icon_test),
        TestRoute::Label => test_page(w, back_stack, "Label Test", label_test),
        TestRoute::LoadingIndicator => test_page(w, back_stack, "Loading Indicator Test", loading_indicator_test),
        TestRoute::MainUi2 => test_page(w, back_stack, "Main UI 2", main_ui2),
        TestRoute::Navigation => test_page(w, back_stack, "Navigation Test", navigation_test),
        TestRoute::Rectangle => test_page(w, back_stack, "Rectangle Test", rectangle_test),
        TestRoute::Ripple => test_page(w, back_stack, "Ripple Test", ripple_test),
        TestRoute::ScrollArea => test_page(w, back_stack, "Scroll Area Test", scroll_area_test),
        TestRoute::Shape => test_page(w, back_stack, "Shape Test", shape_test),
        TestRoute::Slider => test_page(w, back_stack, "Slider Test", slider_test),
        TestRoute::TextField => test_page(w, back_stack, "Text Field Test", text_field_test),
        TestRoute::VideoPlayer => test_page(w, back_stack, "Video Player Test", video_player_test),
        TestRoute::LazyList => test_page(w, back_stack, "Lazy List Test", lazy_list_test::lazy_list_test),
    }
}

fn test_list_page(w: &WindowContext, back_stack: &SharedWVec<Box<dyn NavKey>>) -> Item {
    scroll_area(
        w.vertical_scroll_props(),
        flex(
            w.column_props(),
            vec![
                list_item("Animated Content Test", TestRoute::AnimatedContent, back_stack, w),
                list_item("Animation Test", TestRoute::Animation, back_stack, w),
                list_item("Badge Test", TestRoute::Badge, back_stack, w),
                list_item("Button Test", TestRoute::Button, back_stack, w),
                list_item("Icon Test", TestRoute::Icon, back_stack, w),
                list_item("Label Test", TestRoute::Label, back_stack, w),
                list_item("Loading Indicator Test", TestRoute::LoadingIndicator, back_stack, w),
                list_item("Main UI 2", TestRoute::MainUi2, back_stack, w),
                list_item("Navigation Test", TestRoute::Navigation, back_stack, w),
                list_item("Rectangle Test", TestRoute::Rectangle, back_stack, w),
                list_item("Ripple Test", TestRoute::Ripple, back_stack, w),
                list_item("Scroll Area Test", TestRoute::ScrollArea, back_stack, w),
                list_item("Shape Test", TestRoute::Shape, back_stack, w),
                list_item("Slider Test", TestRoute::Slider, back_stack, w),
                list_item("Text Field Test", TestRoute::TextField, back_stack, w),
                list_item("Video Player Test", TestRoute::VideoPlayer, back_stack, w),
                list_item("Lazy List Test", TestRoute::LazyList, back_stack, w)
            ],
        ),
    )
}

fn list_item(
    name: &'static str,
    route: TestRoute,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    w: &WindowContext,
) -> Item {
    let back_stack = back_stack.clone();
    stack(
        w.stack_props()
            .size(Size::Fill, Size::Auto)
            .background(ripple(w.ripple_props()))
            .on_click(move |_| {
                back_stack.push(Box::new(route.clone()));
            }),
        label(
            w.label_props(name)
                .padding(Padding::all(16.0))
                .size(Size::Fill, Size::Auto)
                .color(Color::WHITE),
        ),
    )
}

fn test_page(
    w: &WindowContext,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    title: &'static str,
    content: TestBuilder,
) -> Item {
    let back_stack = back_stack.clone();
    stack(
        w.stack_props()
            .size(Size::Fill, Size::Fill)
            .background(rectangle(w.rectangle_props(Color::from_rgb(0x12, 0x15, 0x1B)).size(Size::Fill, Size::Fill))),
        flex(
            w.column_props().size(Size::Fill, Size::Fill),
            vec![
                stack(
                    w.stack_props()
                        .size(Size::Fill, Size::Auto)
                        .height(64)
                        .background(rectangle(w.rectangle_props(Color::from_rgb(0x1B, 0x21, 0x2B)).size(Size::Fill, Size::Fill))),
                    label(
                        w.label_props(title)
                            .size(Size::Fill, Size::Fill)
                            .color(Color::WHITE),
                    ) + button(
                        w.button_props()
                            .size(48, 48)
                            .padding(Padding::all(12.0))
                            .align_self(Alignment::center_start())
                            .on_click(move |_| {
                                if back_stack.len() > 1 {
                                    back_stack.pop();
                                }
                            }),
                        move |props, _| {
                            icon(
                                w.icon_props(Outlined::ARROW_BACK)
                                    .size(24, 24)
                                    .color(Color::WHITE),
                            )
                        },
                    ),
                ),
                stack(
                    w.stack_props()
                        .size(Size::Fill, Size::Fill),
                    content(w),
                ),
            ],
        ),
    )
}

fn main_ui2(w: &WindowContext) -> Item {
    let text = SharedText::new("Hello, Winia!".into());
    let max_lines = SharedSource::new(1usize);
    let color = SharedColor::new(Color::GREEN);
    let w = w.clone();
    stack(
        w.stack_props()
            // .name("Stack")
         .size(Size::Fill, Size::Fill)
         .on_click(|_| println!("Stack clicked"))
         .on_focus_changed(|state| println!("Stack focus changed: {:?}", state)),
        {
            let focus_requester_red = SharedSource::new_focus_requester();
            let focus_requester_blue = SharedSource::new_focus_requester();
            let focus_requester_green = SharedSource::new_focus_requester();
            flex(
                w.row_props(),
                rectangle(
                    w.rectangle_props(Color::RED)
                        // .name("Red Rectangle")
                     .size(100, 100)
                     .focus_requester(&focus_requester_red)
                     .on_focus_changed(|state| {
                         println!("Red focus changed: {:?}", state);
                     })
                     .on_click({
                         clone!(focus_requester_red);
                         move |_| {
                             println!("Red clicked");
                             focus_requester_red.request_focus();
                         }
                     })
                     .radius(Radius::new().all(24.0)),
                ) + rectangle(
                    w.rectangle_props(Color::YELLOW)
                        // .name("Yellow Rectangle")
                     .size(200, 100)
                     .focus_requester(&focus_requester_red)
                     .on_focus_changed(|state| {
                         println!("Yellow focus changed: {:?}", state);
                     })
                     .on_click({
                         move |_| {
                             println!("Yellow clicked");
                             focus_requester_red.request_focus();
                         }
                     })
                     .radius(Radius::fully_rounded()),
                ),
            ) + {
                let width: SharedSize = 100.into();
                let height: SharedSize = 100.into();
                let mut b = true;
                rectangle(
                    w.rectangle_props(Color::BLUE)
                        // .name("Blue Rectangle")
                     .size(&width, &height)
                     .focus_requester(&focus_requester_blue)
                     .on_focus_changed(|state| {
                         println!("Blue focus changed: {:?}", state);
                     })
                     .on_click({
                         clone!(w, focus_requester_blue, text);
                         move |_| {
                             let maximized = SharedBool::new(false);
                             let title = SharedString::from("Window");
                             w.event_loop_proxy().new_window(
                                 {
                                     clone!(title, maximized);
                                     move |w: &WindowContext| {
                                         let input = SharedText::from(title.get());
                                         flex(
                                             w.column_props(),
                                             button(
                                                 w.button_props()
                                                  .on_click({
                                                      move |_| {
                                                          maximized.set(!maximized.get());
                                                      }
                                                  }),
                                                 |props, _| label(props.text("Button")),
                                             ) +
                                                 button(
                                                     w.button_props()
                                                      .on_click(
                                                          move |_| {
                                                              title.set("111");
                                                          }
                                                      ),
                                                     |props, _| label(props.text("Set Title")),
                                                 ),
                                         )
                                     }
                                 },
                                 WindowAttributes::default()
                                     .title(&title)
                                     .preferred_size(400.0, 300.0)
                                     .maximized(&maximized),
                             );
                             println!("Blue clicked");
                             focus_requester_blue.request_focus();
                             if b {
                                 width.set(Size::Fixed(200.0));
                                 height.set(Size::Fixed(300.0))
                             } else {
                                 width.set(Size::Fixed(100.0));
                                 height.set(Size::Fixed(100.0));
                             }
                             b = !b;
                             text.set("Text 2");
                         }
                     })
                     .align_self(Alignment::top_end())
                     .radius(Radius::new().all(24.0)),
                )
            } + {
                let alignment: SharedSource<Alignment> = Alignment::bottom_end().into();
                let size: SharedSize = 100.into();
                let blur = SharedSource::new(0.0_f32);
                rectangle(
                    w.rectangle_props(Color::from_argb(100, 255, 255, 255))
                     .size(&size, &size)
                        // .name("Green Rectangle")
                        // .align_self(&alignment)
                     .blur(&blur)
                     .offset_x(50.0)
                     .enable_background_blur(true)
                        /*                        .clipped(true)
                        .clip_shape({
                            let shape: Box<dyn Fn(&Frame) -> Path> = Box::new(|frame: &Frame| {
                                let mut path = Path::new();
                                let rrect = RRect::new_rect_radii(
                                    frame.rect(),
                                    &[
                                        Vector::new(24.0, 24.0),
                                        Vector::new(24.0, 24.0),
                                        Vector::new(24.0, 24.0),
                                        Vector::new(24.0, 24.0),
                                    ],
                                );
                                path.add_rrect(
                                    &rrect,
                                    None
                                );
                                path
                            });
                            SharedDerived::new_derived(Some(shape))
                        })*/
                        // .focus_requester(&focus_requester_green)
                     .on_focus_changed(|state| {
                         println!("Green focus changed: {:?}", state);
                     })
                     .on_click({
                         let mut b = true;
                         let mut times = 0_usize;
                         clone!(w, focus_requester_green, text, max_lines, color);
                         move |_| {
                             println!("Green clicked");
                             color.set(if b { Color::YELLOW } else { Color::GREEN });
                             if b {
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(blur, alignment, text, max_lines, size, color);
                                      move || {
                                          blur.set(35.0);
                                          alignment.set(Alignment::top_start());

                                          // let mut style_text = StyledText::from("Text abc 你好 AAAAAAAAAAAA");
                                          // style_text.set_attrs(
                                          //     0..style_text.len(),
                                          //     false,
                                          //     &Vec::new()
                                          //         .bold()
                                          //         .color(Color::GREEN)
                                          //         .background_color(Color::RED)
                                          //         .height_multiple(2.5, true),
                                          // );
                                          let mut style_text = StyledText::from("");
                                          style_text
                                              .append_str("Text abc 你好 AAAAAAAAAAAA")
                                              .color(Color::GREEN)
                                              .bold()
                                              .italic()
                                              .font_size(64.0)
                                              .font_family("PingFang HK")
                                              .shadow(TextShadow::new(
                                                  Color::BLACK,
                                                  (8, 8),
                                                  4.0,
                                              ))
                                              .background_color(Color::RED)
                                              .height_multiple(2.5, true)
                                              .set();
                                          text.set(style_text);
                                          max_lines.set(usize::MAX);
                                          size.set(Size::Fixed(200.0));
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start();
                             } else {
                                 w.animate(exclude_target!())
                                  .transformation({
                                      clone!(blur, alignment, text, max_lines, size);
                                      move || {
                                          blur.set(10.0);
                                          alignment.set(Alignment::bottom_end());
                                          text.set("Text BBBBB\nBBBBBBBBB");
                                          max_lines.set(1usize);
                                          size.set(Size::Fixed(100.0));
                                      }
                                  })
                                  .duration(Duration::from_millis(500))
                                  .start();
                             }
                             b = !b;
                             focus_requester_green.request_focus();
                         }
                     })
                     .radius(Radius::new().all(24.0)),
                )
            } + {
                let selection = SharedSource::new(0usize..0usize);
                label(
                    w.label_props("Text: " + &text)
                        // .name("Text")
                        // .width(150.0)
                     .background(rectangle(w.rectangle_props(Color::BLACK)))
                     .align_self(Alignment::bottom_start())
                     .on_click({
                         move |_| {
                             thread::spawn({
                                 clone!(color);
                                 move || {
                                     thread::sleep(Duration::from_secs(1));
                                     color.set(Color::CYAN);
                                 }
                             });
                         }
                     })
                        // .text(StyledText::from("A long long long long long long long long long long long long long long long long long long long long text.").with_mut(|text| {
                        //     text.style_setter(0..6)
                        //         .placeholder(
                        //             SharedDrawable::from_file("/home/grounzer/Downloads/logo.png").unwrap().with_mut(|drawable|{
                        //                 drawable.lock().set_width(50.0);
                        //                 drawable.lock().set_height(50.0);
                        //             })
                        //         )
                        //         .placeholder_style(
                        //             PlaceholderAlignment::Middle,
                        //             TextBaseline::Alphabetic,
                        //             0.0,
                        //         )
                        //         .set();
                        // }))
                     .color(Color::WHITE)
                     .selectable(true)
                     .selection_range(&selection)
                     .on_selection_change({
                         move |range| {
                             selection.set(range.clone());
                         }
                     }), // .max_lines(&max_lines)
                    // .ellipsis("..."),
                )
            } + ripple(
                w.ripple_props()
                 .clipped(false)
                 .align_self(Alignment::center_start())
                 .size(200.0, 200.0),
            ) + button(
                w.button_props()
                 .align_self(Alignment::center_end())
                 .on_click({
                     let mut dark = true;
                     clone!(w);
                     move |_| {
                         /*                            if dark {
                             w.theme().set(
                                 material_theme(Color::RED, false)
                             );
                         } else {
                             w.theme().set(
                                 material_theme(Color::RED, true)
                             );
                         }*/
                         let w = w.clone();
                         w.animate(exclude_target!())
                          .transformation({
                              move || {
                                  let theme = if dark {
                                      material_theme(Color::RED, false)
                                  } else {
                                      material_theme(Color::RED, true)
                                  };
                                  w.theme().set(theme);
                              }
                          })
                          .duration(Duration::from_millis(300))
                          .start();
                         dark = !dark;
                     }
                 }),
                |props, _| {
                    label(
                        props.text("Button"), // .color(Color::WHITE)
                    )
                },
            ) + {
                let image_drawable =
                    SharedSource::from_file("/home/grounzer/Pictures/Grounzer Logo Light.svg")
                        .unwrap();
                let color = SharedSource::new(Some(Color::GREEN));
                let size = SharedSize::from(150.0);
                let align_self = SharedSource::new(Alignment::bottom_end());
                image(
                    w.image_props(&image_drawable)
                        // .name("Image")
                     .color(&color)
                     .size(&size, &size)
                     .align_self(&align_self)
                     .on_click({
                         move |_| {
                             // // image_drawable.lock().set_color(Some(Color::RED));
                             // image_drawable.lock().set_width(150.0);
                             // image_drawable.lock().set_height(150.0);
                             // println!("Image clicked");
                             let current_color = color.get();
                             if let Some(c) = current_color {
                                 if c == Color::GREEN {
                                     w.animate(exclude_target!())
                                      .transformation({
                                          clone!(color, size, align_self);
                                          move || {
                                              color.set(Some(Color::RED));
                                              size.set(Size::Auto);
                                              align_self.set(Alignment::center());
                                          }
                                      })
                                      .duration(Duration::from_millis(5000))
                                      .start();
                                 } else {
                                     w.animate(exclude_target!())
                                      .transformation({
                                          clone!(color, size, align_self);
                                          move || {
                                              color.set(Some(Color::GREEN));
                                              size.set(150);
                                              align_self.set(Alignment::bottom_end());
                                          }
                                      })
                                      .duration(Duration::from_millis(5000))
                                      .start();
                                 }
                             }
                         }
                     }),
                )
            }
        },
    )
}
