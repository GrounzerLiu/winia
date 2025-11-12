use std::thread;
use std::time::Duration;
use winia::animation::AnimationExt;
use winia::app::{App, WindowAttributes, WindowContext, run_app};
use winia::shared::{SharedColor, SharedSize, SharedSource, SharedText};
use winia::text::{AddTextAttribute, StyledText, TextShadow};
use winia::ui::item::{FocusRequesterTrait, Frame};
use winia::ui::{AlignSelf, Alignment, Color, ImagePropsTrait, Item, LabelPropsTrait, Radius, RectanglePropsTrait, RipplePropsTrait, RowPropsTrait, Size, StackPropsTrait, flex, image, label, rectangle, ripple, stack, button, ButtonPropsTrait};
use winia::{clone, exclude_target};
use winia::skia_safe::{Path, RRect, Rect, Vector};
use winia::theme::material_theme;

fn main() {
    run_app(App::new(
        main_ui,
        WindowAttributes::default()
            .title("Winia Test")
            .preferred_size(100.0, 100.0),
    ));
}

fn main_ui(w: &WindowContext) -> Item {
    let text = SharedText::new("Hello, Winia!".into());
    let max_lines = SharedSource::new(1usize);
    let color = SharedColor::new(Color::GREEN);
    stack(
        w.stack_props()
         .name("Stack")
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
                    w.rectangle_props()
                     .name("Red Rectangle")
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
                     .color(Color::RED)
                     .radius(Radius::new().all(24.0)),
                ) + rectangle(
                    w.rectangle_props()
                     .name("Yellow Rectangle")
                     .size(200, 100)
                     .focus_requester(&focus_requester_red)
                     .on_focus_changed(|state| {
                         println!("Yellow focus changed: {:?}", state);
                     })
                     .on_click({
                         clone!(focus_requester_red);
                         move |_| {
                             println!("Yellow clicked");
                             focus_requester_red.request_focus();
                         }
                     })
                     .color(Color::YELLOW)
                     .radius(Radius::fully_rounded()),
                ),
            ) + {
                let width: SharedSize = 100.into();
                let height: SharedSize = 100.into();
                let mut b = true;
                rectangle(
                    w.rectangle_props()
                     .name("Blue Rectangle")
                     .size(&width, &height)
                     .focus_requester(&focus_requester_blue)
                     .on_focus_changed(|state| {
                         println!("Blue focus changed: {:?}", state);
                     })
                     .on_click({
                         clone!(focus_requester_blue, width, height, text, w);
                         move |_| {
                             w.event_loop_proxy().new_window(
                                 |w| {
                                     stack(
                                         w.stack_props()
                                             // .name("New Window Stack")
                                          .size(Size::Fill, Size::Fill),
                                         vec![
                                             label(
                                                 w.label_props()
                                                     // .name("New Window Label")
                                                  .text("This is a new window")
                                                  .color(Color::WHITE),
                                             ),
                                         ],
                                     )
                                 },
                                 WindowAttributes::default().preferred_size(400.0, 300.0),
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
                     .color(Color::BLUE)
                     .radius(Radius::new().all(24.0)),
                )
            } +
                {
                    let alignment: SharedSource<Alignment> = Alignment::bottom_end().into();
                    let size: SharedSize = 100.into();
                    let blur = SharedSource::new(0.0_f32);
                    rectangle(
                        w.rectangle_props()
                         .size(&size, &size)
                         .name("Green Rectangle")
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
                             clone!(
                                w,
                                focus_requester_green,
                                color,
                                text,
                                alignment,
                                max_lines,
                                size,
                                blur
                            );
                             let mut times = 0_usize;
                             move |_| {
                                 println!("Green clicked");
                                 color.set(if b { Color::YELLOW } else { Color::GREEN });
                                 if b {
                                     w.animate(exclude_target!())
                                      .transformation({
                                          clone!(alignment, text, max_lines, size, blur);
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
                                          clone!(alignment, text, max_lines, size, blur);
                                          move || {
                                              blur.set(0.0);
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
                         .color(/*&color*/Color::from_argb(100, 255, 255, 255))
                         .radius(Radius::new().all(24.0))
                    )
                } + {
                let selection = SharedSource::new(0usize..0usize);
                label(
                    w.label_props()
                     .name("Text")
                        // .width(150.0)
                     .background(rectangle(
                         w.rectangle_props().color(Color::BLACK),
                     ))
                     .align_self(Alignment::bottom_start())
                     .on_click({
                         clone!(color);
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
                     .text("Text: " + &text)
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
                         clone!(selection);
                         move |range| {
                             selection.set(range.clone());
                         }
                     })
                    // .max_lines(&max_lines)
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
                        clone!(w);
                        let mut dark = true;
                        move |_| {
                            if dark {
                                w.theme().set(
                                    material_theme(Color::RED, false)
                                );
                            } else {
                                w.theme().set(
                                    material_theme(Color::RED, true)
                                );
                            }
                            dark = !dark;
                        }
                    }),
                |props| {
                    label(
                        props
                            .text("Button")
                            // .color(Color::WHITE)
                    )
                }
            ) + {
                let image_drawable = SharedSource::from_file("/home/grounzer/Pictures/Grounzer Logo Light.svg").unwrap();
                let color = SharedSource::new(Some(Color::WHITE));
                let size = SharedSize::from(150.0);
                let align_self = SharedSource::new(Alignment::bottom_end());
                image(
                    w.image_props()
                     .drawable(&image_drawable)
                     .color(&color)
                     .size(&size, &size)
                     .align_self(&align_self)
                     .on_click({
                         clone!(color, size, align_self, w);
                         move |_| {
                             // // image_drawable.lock().set_color(Some(Color::RED));
                             // image_drawable.lock().set_width(150.0);
                             // image_drawable.lock().set_height(150.0);
                             // println!("Image clicked");
                             let current_color = color.get();
                             if let Some(c) = current_color {
                                 if c == Color::WHITE {
                                     w.animate(exclude_target!())
                                      .transformation({
                                          clone!(color, size, align_self);
                                          move || {
                                              color.set(Some(Color::RED));
                                              size.set(Size::Auto);
                                              align_self.set(Alignment::center());
                                          }
                                      })
                                      .duration(Duration::from_millis(500))
                                      .start();
                                 } else {
                                     w.animate(exclude_target!())
                                      .transformation({
                                          clone!(color, size, align_self);
                                          move || {
                                              color.set(Some(Color::WHITE));
                                              size.set(150);
                                              align_self.set(Alignment::bottom_end()
                                              );
                                          }
                                      })
                                      .duration(Duration::from_millis(500))
                                      .start();
                                 }
                             }
                         }
                     })
                )
            }
        },
    )
}
