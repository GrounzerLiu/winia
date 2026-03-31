use winia::ui::animated_visibility;
use rand::{Rng, RngExt};
use std::time::Duration;
use letclone::clone;
use winia::animation::interpolator::EaseOutBounce;
use winia::animation::AnimationExt;
use winia::app::WindowContext;
use winia::shared::{SharedF32, SharedSize, SharedSource};
use winia::ui::button::ButtonPropsTrait;
use winia::ui::{button, flex, label, rectangle, Color, ColumnPropsTrait, Item, RectanglePropsTrait, Size};
use winia::{closure_use, exclude_target};
use winia::ui::AnimatedVisibilityPropsTrait;

pub fn button_test(w: &WindowContext) -> Item {
    let w = w.clone();
    let m = w.window_attributes().get_maximized().clone();
    let offset_y = SharedF32::new(20.0);
    let opacity = SharedF32::new(1.0);
    let visible = SharedSource::new(true);
    flex(
        w.column_props()
         .size(Size::Fill, Size::Fill),
        button(
            w.button_props()
             .on_click({
                 clone!(w);
                 move |_| {
                     let scale = SharedF32::new(0.0);
                     // visible.set(!visible.get());

                     let size = SharedSize::new(Size::Fixed(100.0));
                     w.event_loop_proxy().add_layer(move |w, ctr| {
                         // visible.set(!visible.get());
                         let c = ctr.clone();
                         let w = w.clone();
                         let scale = scale.clone();
                         let mut rng = rand::rng();
                         let rand_color = Color::from_rgb(
                             rng.random_range(0..=255) as u8,
                             rng.random_range(0..=255) as u8,
                             rng.random_range(0..=255) as u8,
                         );
                         let size = SharedSize::new(Size::Fixed(rng.random_range(50..=200) as f32));
                         let window_size = w.window_size();
                         let rand_x = rng.random_range(200..(window_size.0 as i32 - 100)) as f32;
                         let rand_y = rng.random_range(200..(window_size.1 as i32 - 100)) as f32;
                         rectangle(
                             w.rectangle_props(rand_color)
                              .size(&size, &size)
                              .scale_x(&scale)
                              .scale_y(&scale)
                              .offset(rand_x, rand_y)
                              .on_mounted({
                                  clone!(w, scale);
                                  move |_| {
                                      w.animate(exclude_target!())
                                       .transformation({
                                           clone!(scale);
                                           move || {
                                               // size.set(Size::Fixed(100.0))
                                               scale.set(1.0);
                                           }
                                       })
                                       .duration(Duration::from_millis(1000))
                                       .interpolator(Box::new(EaseOutBounce::new()))
                                       .start();
                                  }
                              })
                              .on_click({
                                  clone!(w, scale, size);
                                  move |_| {
                                      // size.set(Size::Fixed(10.0));
                                      let c = c.clone();
                                      let w = w.clone();
                                      let size = size.clone();
                                      let scale = scale.clone();
                                      // ctr.remove();
                                      w.animate(exclude_target!())
                                       .transformation(move || {
                                           // size.set(Size::Fixed(0.0))
                                           scale.set(0.0);
                                       })
                                       .duration(Duration::from_millis(300))
                                       .on_finished(move || {
                                           c.remove();
                                       })
                                       .start();
                                  }
                              })
                         )
                     })
                 }
             }),
            |props, _| label(props.text("Button")),
        )
            + {
            animated_visibility(
                w.animated_visibility_props(&visible)
                /*                 .on_visible(move || {
                                     offset_y.set(0.0);
                                     opacity.set(1.0);
                                 })
                                 .on_invisible(move || {
                                     offset_y.set(20.0);
                                     opacity.set(0.0);
                                 })
                                 .offset_y(&offset_y)
                                 .opacity(&opacity)*/
                ,
                vec![
                    rectangle(
                        w.rectangle_props(Color::BLUE)
                         .size(Size::Fixed(200.0), Size::Fixed(100.0))
                    )
                ],
            )
        }
            + rectangle(
            w.rectangle_props(Color::RED)
             .size(Size::Fixed(50.0), Size::Fixed(50.0),
             )
        ),
    )
}