use std::thread;
use std::time::Duration;
use clonelet::clone;
use skia_safe::Color;
use crate::core::{generate_id, get_id_by_name};
use crate::include_target;
use crate::shared::{Settable, Shared};
use crate::ui::animation::AnimationExt;
use crate::ui::animation::interpolator::EaseOutCirc;
use crate::ui::app::WindowContext;
use crate::ui::component::{RectangleExt, TextExt};
use crate::ui::item::{Alignment, Size};
use crate::ui::layout::{AlignSelf, StackExt};
use crate::ui::theme::color;
use crate::ui::animation::Target;

pub trait ToastExt {
    fn toast(&self, message: impl Into<String>);
}

impl ToastExt for WindowContext {
    fn toast(&self, message: impl Into<String>) {
        let message = message.into();
        self.event_loop_proxy().new_layer(
            |w, controller|{
                let (text_color, background_color) = {
                    let theme = w.theme();
                    let theme_lock = theme.lock();
                    let text_color = theme_lock.get_color(color::INVERSE_SURFACE).unwrap_or(Color::WHITE);
                    let background_color = theme_lock.get_color(color::INVERSE_ON_SURFACE).unwrap_or(Color::BLACK);
                    (text_color, background_color)
                };
                let id = generate_id();
                let name = format!("Toast {}", id);
                let margin_bottom = Shared::from_static(-64.0);
                let opacity = Shared::from_static(1.0);

                thread::spawn({
                    let event_loop_proxy = w.event_loop_proxy().clone();
                    clone!(opacity, name, controller);
                    move || {
                        thread::sleep(Duration::from_secs(2));
                        event_loop_proxy.animate(Target::Inclusion(
                            get_id_by_name(&name).map_or(vec![], |id| vec![id])
                        )).transformation({
                            let opacity = opacity.clone();
                            move || {
                                opacity.set(0.0);
                            }
                        }).duration(Duration::from_millis(500))
                            .interpolator(Box::new(EaseOutCirc::new()))
                            .on_finished({
                                let controller = controller.clone();
                                move || {
                                    controller.remove();
                                }
                            })
                            .start();
                    }
                });
                let item = w.stack(
                    w.text(message).editable(false)
                        .color(text_color)
                        .font_size(16)
                        .item()
                ).item().size(Size::Auto, 48)
                    .align_content(Alignment::Center)
                    .background(
                        w.rectangle(background_color).radius(16).item()
                    )
                    .align_self(Alignment::BottomCenter)
                    .name(&name)
                    .padding_start(48)
                    .padding_end(48)
                    .margin_bottom(&margin_bottom)
                    .opacity(&opacity);

                w.animate(include_target!(&name)).transformation({
                    let margin_bottom = margin_bottom.clone();
                    move ||{
                        margin_bottom.set(16.0);
                    }
                }).duration(Duration::from_millis(500))
                    .interpolator(Box::new(EaseOutCirc::new()))
                    .start();

                item
            }
        );
    }
}