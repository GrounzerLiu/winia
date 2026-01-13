#![feature(ergonomic_clones)]
mod button_test;
mod label_test;
mod ripple_test;
mod scroll_area_test;
mod text_field_test;
mod slider_test;

use crate::button_test::button_test;
use crate::label_test::label_test;
use crate::ripple_test::ripple_test;
use crate::scroll_area_test::scroll_area_test;
use log::debug;
use std::thread;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;
use winia::animation::AnimationExt;
use winia::app::{App, WindowAttributes, WindowContext, run_app};
use winia::{clone, exclude_target, With};
use winia::shared::{SharedBool, SharedColor, SharedSize, SharedSource, SharedString, SharedText, SharedWVec};
use winia::skia_safe::textlayout::{ParagraphStyle, RectHeightStyle, RectWidthStyle, TextStyle};
use winia::text::{AddTextAttribute, StyledText, TextShadow};
use winia::theme::material_theme;
use winia::ui::button::ButtonPropsTrait;
use winia::ui::item::{FocusRequesterTrait, Padding};
use winia::ui::{AlignSelf, Alignment, Color, ColumnPropsTrait, DefaultScrollAreaProps, ImagePropsTrait, Item, LabelPropsTrait, Radius, RectanglePropsTrait, RipplePropsTrait, RowPropsTrait, Size, StackPropsTrait, button, flex, image, label, rectangle, ripple, scroll_area, stack, text_field, TextFieldPropsTrait};
use crate::text_field_test::text_field_test;

fn main() {
    run_app(App::new(
        main_ui,
        WindowAttributes::default()
            .title("Winia Test")
            .preferred_size(100.0, 100.0),
    ));
}

fn main_ui(w: &WindowContext) -> Item {
    // let r_tests:Vec<(&'static str, fn(&WindowContext) -> Item)> = vec![
    //     ("Scroll Area Test", scroll_area_test),
    //     ("Button Test", button_test),
    // ];
    // let tests = SharedWVec::from(r_tests);
    scroll_area(
        w.vertical_scroll_props(),
        flex(
            w.column_props(),
            vec![
                list_item("Button Test", button_test, w),
                list_item("Label Test", label_test, w),
                list_item("Main UI 2", main_ui2, w),
                list_item("Ripple Test", ripple_test, w),
                list_item("Scroll Area Test", scroll_area_test, w),
                list_item("Slider Test", slider_test::slider_test, w),
                list_item("Text Field Test", text_field_test, w),
            ],
        ),
    )
}

fn list_item(name: &'static str, ui_fn: fn(&WindowContext) -> Item, w: &WindowContext) -> Item {
    let ui_fn = SharedSource::new(ui_fn);
    let w = w.clone();
    stack(
        w.stack_props()
            .size(Size::Fill, Size::Auto)
            .background(ripple(w.ripple_props()))
            .on_click({
                use |_| {
                    w.event_loop_proxy().new_window(
                        {
                            use |w| {
                                let ui_fn_locked = ui_fn.lock();
                                ui_fn_locked(w)
                            }
                        },
                        WindowAttributes::default()
                            .title(name)
                            .preferred_size(800.0, 600.0),
                    );
                }
            }),
        label(
            w.label_props(name)
                .padding(Padding::all(16.0))
                .size(Size::Fill, Size::Auto)
                .color(Color::WHITE),
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
                            use |_| {
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
                            use |_| {
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
                            use |_| {
                                let maximized = SharedBool::new(false);
                                let title = SharedString::from("Window");
                                w.event_loop_proxy().new_window(
                                    use |w: &WindowContext| {
                                        let input = SharedText::from(title.get());
                                        flex(
                                            w.column_props(),
                                            button(
                                                w.button_props()
                                                 .on_click({
                                                     use |_| {
                                                         maximized.set(!maximized.get());
                                                     }
                                                 }),
                                                |props,_| label(props.text("Button"))
                                            ) +
                                            button(
                                                w.button_props()
                                                    .on_click(
                                                        use |_| {
                                                            title.set("111");
                                                        }
                                                    ),
                                                |props,_| label(props.text("Set Title"))
                                            )
                                        )
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
                            use |_| {
                                println!("Green clicked");
                                color.set(if b { Color::YELLOW } else { Color::GREEN });
                                if b {
                                    w.animate(exclude_target!())
                                        .transformation({
                                            use || {
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
                                            use || {
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
                            use |_| {
                                thread::spawn({
                                    use || {
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
                            use |range| {
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
                        use |_| {
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
                                    use || {
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
                            use |_| {
                                // // image_drawable.lock().set_color(Some(Color::RED));
                                // image_drawable.lock().set_width(150.0);
                                // image_drawable.lock().set_height(150.0);
                                // println!("Image clicked");
                                let current_color = color.get();
                                if let Some(c) = current_color {
                                    if c == Color::GREEN {
                                        w.animate(exclude_target!())
                                            .transformation({
                                                use || {
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
                                                use || {
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
