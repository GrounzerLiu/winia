use std::time::Duration;
use letclone::clone;
use winia::animation::AnimationExt;
use winia::app::WindowContext;
use winia::{exclude_target};
use winia::shared::SharedSource;
use winia::ui::{button, flex, label, loading_indicator, stack, Item, LabelPropsTrait, LoadingIndicatorPropsTrait, RowPropsTrait, StackPropsTrait};
use winia::ui::button::ButtonPropsTrait;
use winia::ui::item::Padding;

pub fn loading_indicator_test(w: &WindowContext) -> Item {
    let is_contained = SharedSource::new(true);
    flex(
        w.row_props()
         .padding(Padding::all(16.0)),
        vec![
            button(
                w.button_props()
                    .on_click({
                        clone!(is_contained, w);
                        move |_| {
                            w.animate(exclude_target!())
                                .transformation({
                                    clone!(is_contained);
                                    move || {
                                        is_contained.set(!is_contained.get());
                                    }
                                })
                                .duration(Duration::from_millis(500))
                                .start()
                        }
                    }),
                |p,_|{
                    label(w.label_props("Toggle Contained"))
                }
            ),
            loading_indicator(w),
            loading_indicator(
                w.loading_indicator_props(false)
            ),
            loading_indicator(
                w.loading_indicator_props(true)
                 .size(96, 96)
            ),
            loading_indicator(
                w.loading_indicator_props(&is_contained)
                    .size(96, 96)
            ),
            loading_indicator(
                w.loading_indicator_props(true)
                // .is_contained(false)
            ),
        ],
    )
}