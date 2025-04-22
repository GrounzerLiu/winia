use crate::core::generate_id;
use crate::exclude_target;
use crate::shared::{
    Gettable, Settable, Shared, SharedBool, SharedColor, SharedF32, SharedSize, SharedText,
};
use crate::ui::animation::AnimationExt;
use crate::ui::app::WindowContext;
use crate::ui::component::{RectangleExt, TextExt};
use crate::ui::item::{Alignment, Size};
use crate::ui::layout::{AlignSelf, ColumnExt, StackExt};
use crate::ui::theme::color;
use crate::ui::Item;
use clonelet::clone;
use proc_macro::item;
use std::time::Duration;

struct FilledTextFieldProperty {}

#[item(input_text: impl Into<SharedText>)]
pub struct FilledTextField {
    item: Item,
    property: Shared<FilledTextFieldProperty>,
}

impl FilledTextField {
    pub fn new(w: &WindowContext, input_text: impl Into<SharedText>) -> Self {
        let input_text = input_text.into();
        let property = Shared::from(FilledTextFieldProperty {});
        let label_text_min_height = SharedF32::from(24);
        let label_text_font_size = SharedF32::from(16);
        // let input_text_min_height = SharedF32::from(24);
        let input_text_font_size = SharedF32::from(16);
        let input_text_height = SharedSize::from(Size::Fixed(0.0));
        let input_text_focused = SharedBool::from(false);
        
        let active_indicator_height = SharedSize::from_dynamic(
            [input_text_focused.as_ref().into()].into(),
            {
                clone!(
                    input_text_focused
                );
                move || {
                    if input_text_focused.get() {
                        Size::Fixed(2.0)
                    } else {
                        Size::Fixed(1.0)
                    }
                }
            }
        );
        let theme = w.theme().clone();
        let active_indicator_color = SharedColor::from_dynamic(
            [theme.as_ref().into(), input_text_focused.as_ref().into()].into(),
            {
                clone!(
                    theme,
                    input_text_focused
                );
                move || {
                    if input_text_focused.get() {
                        theme
                            .lock()
                            .get_color(color::PRIMARY)
                            .unwrap()
                    } else {
                        theme
                            .lock()
                            .get_color(color::ON_SURFACE_VARIANT)
                            .unwrap()
                    }
                }
            }
        );
        
        let event_loop_proxy = w.event_loop_proxy();
        input_text_focused.add_specific_observer(generate_id(), {
            clone!(
                event_loop_proxy,
                label_text_min_height,
                label_text_font_size,
                input_text_height,
                input_text
            );
            move |focused| {
                if !input_text.lock().is_empty() && !*focused {
                    return;
                }
                if *focused {
                    event_loop_proxy
                        .animate(exclude_target!())
                        .transformation({
                            clone!(
                                label_text_min_height,
                                label_text_font_size,
                                input_text_height
                            );
                            move || {
                                label_text_min_height.set(16.0);
                                label_text_font_size.set(12.0);
                                input_text_height.set(Size::Auto);
                            }
                        })
                        .duration(Duration::from_millis(500))
                        .start();
                } else {
                    event_loop_proxy
                        .animate(exclude_target!())
                        .transformation({
                            clone!(
                                label_text_min_height,
                                label_text_font_size,
                                input_text_height
                            );
                            move || {
                                label_text_min_height.set(24.0);
                                label_text_font_size.set(16.0);
                                input_text_height.set(Size::Fixed(0.0));
                            }
                        })
                        .duration(Duration::from_millis(500))
                        .start();
                }
            }
        });
        let item = w
            .stack(
                w.column(
                    w.text("Label text")
                        .font_size(&label_text_font_size)
                        .editable(false)
                        .item()
                        .align_content(Alignment::CenterStart)
                        .focusable(false)
                        .focused_when_clicked(false)
                        .min_height(&label_text_min_height)
                        .on_click({
                            let input_text_focused = input_text_focused.clone();
                            move |_| {
                                if !input_text_focused.get() {
                                    input_text_focused.set(true);
                                }
                            }
                        })
                        + w.text(&input_text)
                            .editable(true)
                            .font_size(&input_text_font_size)
                            .item()
                            .width(Size::Fill)
                            .focused(&input_text_focused)
                            .align_content(Alignment::CenterStart)
                            .height(&input_text_height), // .min_height(&input_text_min_height)
                ).item(),
            )
            .item()
            .align_content(Alignment::CenterStart)
            .min_height(56)
            .width(220)
            .padding_start(16)
            .padding_end(16)
            .padding_top(8)
            .padding_bottom(8)
            .background(
                w.stack(
                    w.rectangle(SharedColor::from_dynamic([w.theme().into()].into(), {
                        let theme = w.theme().clone();
                        move || {
                            theme
                                .lock()
                                .get_color(color::SURFACE_CONTAINER_HIGHEST)
                                .unwrap()
                        }
                    }))
                    .radius_top_start(4.0)
                    .radius_top_end(4.0)
                    .item().size(Size::Fill, Size::Fill) 
                    +w.rectangle(active_indicator_color).item().size(Size::Fill, active_indicator_height).align_self(Alignment::BottomCenter)
                ).item()
            );
        Self { item, property }
    }
}
