use std::ops::Deref;
use std::time::Duration;
use letclone::clone;
use rand::RngExt;
use winia::animation::AnimationExt;
use winia::app::WindowContext;
use winia::{closure_use, exclude_target};
use winia::animation::interpolator::EaseOutElastic;
use winia::shared::{Keyframe, KeyframesSpec, SharedBool, SharedF32, SharedSize, SpringSpec};
use winia::ui::{button, flex, label, rectangle, Color, ColumnPropsTrait, Item, LabelPropsTrait, RectanglePropsTrait, RowPropsTrait, Size};
use winia::ui::button::ButtonPropsTrait;
use winia::ui::item::Children;

pub fn animation_test(w: &WindowContext) -> Item {
    let offset_x = SharedF32::new(0.0);
    let offset_x_2 = SharedF32::new(0.0);
    let offset_x_3 = SharedF32::new(0.0);
    let children = Children::new();
    flex(
        w.column_props()
         .size(Size::Fill, Size::Fill),
        vec![
            label(
                w.label_props("Spring Animation")
                 .font_size(20.0)
            ),
            rectangle(
                w.rectangle_props(Color::RED)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
                 .offset_x(&offset_x)
                 .on_click({
                     let offset_x = offset_x.clone();
                     let w = w.clone();
                     move |_| {
                         let window_width = w.window_size().0;
                         let center_x = (window_width - 100.0) / 2.0;
                         let target_x = if offset_x.get() < center_x {
                             center_x
                         } else {
                             0.0
                         };
                         offset_x.animate_to(
                             target_x,
                             SpringSpec::new(0.5, 100.0),
                             w.event_loop_proxy()
                         )
                     }
                 })
            ),
            label(
                w.label_props("Keyframes Animation")
                 .font_size(20.0)
            ),
            rectangle(
                w.rectangle_props(Color::BLUE)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
                 .offset_x(&offset_x_2)
                 .on_click({
                     let offset_x_2 = offset_x_2.clone();
                     let w = w.clone();
                     move |_| {
                         let window_width = w.window_size().0;
                         let center_x = (window_width - 100.0) / 2.0;
                         let target_x = if offset_x_2.get() < center_x {
                             center_x
                         } else {
                             0.0
                         };
                         offset_x_2.animate_to(
                             0.0,
                             KeyframesSpec::new(vec![
                                 Keyframe::new(Duration::from_millis(0), 0.0),
                                 Keyframe::new(Duration::from_millis(500), window_width / 4.0 * 3.0 - 50.0),
                                 Keyframe::new(Duration::from_millis(1000), center_x - window_width / 4.0),
                                 Keyframe::new(Duration::from_millis(1500), center_x)
                                     .interpolator(EaseOutElastic::boxed()),
                             ]),
                             w.event_loop_proxy()
                         )
                     }
                 })
            ),
            label(
                w.label_props("Tween Animation")
                 .font_size(20.0)
            ),
            rectangle(
                w.rectangle_props(Color::GREEN)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
                 .offset_x(&offset_x_3)
                 .on_click({
                     let offset_x_3 = offset_x_3.clone();
                     let w = w.clone();
                     move |_| {
                         let window_width = w.window_size().0;
                         let center_x = (window_width - 100.0) / 2.0;
                         let target_x = if offset_x_3.get() < center_x {
                             center_x
                         } else {
                             0.0
                         };
                         offset_x_3.animate_to(
                             target_x,
                             winia::shared::TweenSpec::new()
                                 .duration(Duration::from_millis(1000))
                                 .interpolator(EaseOutElastic::boxed()),
                             w.event_loop_proxy()
                         )
                     }
                 })
            ),
            label(w.label_props("Layout Animation")),
            flex(
                w.row_props(),
                vec![
                    {
                        let size = SharedSize::from(100);
                        rectangle(
                            w.rectangle_props(Color::BLUE)
                             .name("R1")
                             .size(&size, &size)
                             .on_click({
                                 clone!(size, w);
                                 move |_| {
                                     w.animate(exclude_target!("r2"))
                                      .transformation({
                                          clone!(size);
                                          move || {
                                              if size.lock().deref() == &Size::Fixed(100.0) {
                                                  size.set(Size::Fixed(200.0));
                                              } else {
                                                  size.set(Size::Fixed(100.0));
                                              }
                                          }
                                      })
                                      .duration(Duration::from_millis(1000))
                                      .start();
                                 }
                             })
                        )
                    },
                    {
                        let size = SharedSize::from(100);
                        rectangle(
                            w.rectangle_props(Color::RED)
                             .name("R2")
                             .size(&size, &size)
                             .on_click({
                                 clone!(size, w);
                                 move |_| {
                                     w.animate(exclude_target!("r1"))
                                      .transformation({
                                          clone!(size);
                                          move || {
                                              if size.lock().deref() == &Size::Fixed(100.0) {
                                                  size.set(Size::Fixed(200.0));
                                              } else {
                                                  size.set(Size::Fixed(100.0));
                                              }
                                          }
                                      })
                                      .duration(Duration::from_millis(5000))
                                      .start();
                                 }
                             })
                        )
                    },
                ]
            ),
            button(
                w.button_props()
                 .on_click({
                     clone!(mut children, w);
                     move |_| {
                         let mut rng = rand::rng();
                         let r = rng.random_range(0..=255) as u8;
                         let g = rng.random_range(0..=255) as u8;
                         let b = rng.random_range(0..=255) as u8;
                         children.insert_item_with_animation(
                             0,
                             rectangle(
                                 w.rectangle_props(Color::from_rgb(r, g, b))
                                     .size(100, 100)
                             ),
                             w.animate(exclude_target!())
                                 .duration(Duration::from_millis(500)),
                             |frame| {
                                 let mut frame = frame.clone();
                                 // frame.scale_x = 0.0;
                                 // frame.scale_y = 0.0;
                                 frame.opacity = 0.0;
                                 frame.offset_y = frame.height;
                                 frame
                             }
                         )
                     }
                 }),
                |p, _| {
                    label(p.text("Add"))
                }
            ),
            button(
                w.button_props()
                 .on_click({
                     clone!(mut children, w);
                     move |_| {
                         let id = children.read().first().map(|item| item.id());
                         if let Some(id) = id {
                             children.remove_item_with_animation(
                                 id,
                                 w.animate(exclude_target!())
                                  .duration(Duration::from_millis(500)),
                                 |frame| {
                                     let mut frame = frame.clone();
                                     // frame.scale_x = 0.0;
                                     // frame.scale_y = 0.0;
                                     frame.opacity = 0.0;
                                     frame.offset_y = frame.height;
                                     frame
                                 }
                             )
                         }
                     }
                 }),
                |p, _| {
                    label(p.text("Remove"))
                }
            ),
            flex(
                w.row_props(),
                &children
            )
        ],
    )
}