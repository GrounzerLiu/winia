use std::time::Duration;
use crate::shared::{SharedDerived, SharedDerivedBool, SharedSource};
use crate::ui::item::{Children, ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX, SetCustomProp};
use crate::ui::{Alignment, HorizontalAlignment, Item, Orientation};
use clonelet::clone;
use proc_macro::ItemProps;
use crate::animation::AnimationExt;
use crate::animation::interpolator::{EaseInCirc, EaseOutCirc};
use crate::core::next_id;
use crate::exclude_target;

#[derive(ItemProps)]
pub struct AnimatedVisibilityProps {
    pub item_props: ItemProps,
    pub alignment: SharedDerived<Alignment>,
    #[constructor]
    pub animated_visible: SharedDerivedBool,
    pub animation_duration: SharedDerived<Duration>,
    #[not_shared]
    pub on_visible: SharedSource<Option<Box<dyn FnMut() + Send>>>,
    #[not_shared]
    pub on_invisible: SharedSource<Option<Box<dyn FnMut() + Send>>>,
    pub width_scale: SharedDerived<f32>,
    pub height_scale: SharedDerived<f32>,
}

impl AnimatedVisibilityProps {
    pub fn new(
        item_props: ItemProps,
        animated_visible: impl Into<SharedDerivedBool>,
    ) -> Self {
        let opacity = SharedSource::new(1.0);
        let width_scale = SharedSource::new(1.0);
        let height_scale = SharedSource::new(1.0);
        let on_visible_f: Box<dyn FnMut() + Send> = Box::new({
            clone!(
                width_scale,
                height_scale,
                opacity,
            );
            move || {
                width_scale.set(1.0);
                height_scale.set(1.0);
                opacity.set(1.0);
            }
        });
        let on_invisible_f: Box<dyn FnMut() + Send> = Box::new({
            clone!(
                width_scale,
                height_scale,
                opacity,
            );
            move || {
                width_scale.set(0.0);
                height_scale.set(0.0);
                opacity.set(0.0);
            }
        });
        let on_visible: SharedSource<Option<Box<dyn FnMut() + Send>>> = SharedSource::new(Some(on_visible_f));
        let on_invisible: SharedSource<Option<Box<dyn FnMut() + Send>>> = SharedSource::new(Some(on_invisible_f));

        let animated_visible = animated_visible.into();

        animated_visible.subscribe(
            next_id(),
            {
                let mut last_visible = animated_visible.get();
                let e = item_props.window_context.event_loop_proxy().clone();
                clone!(
                    on_visible,
                    on_invisible,
                    animated_visible,
                );
                move || {
                    let visible = animated_visible.get();
                    if visible != last_visible {
                        last_visible = visible;
                        e.animate(exclude_target!())
                         .transformation({
                             clone!(
                                    on_visible,
                                    on_invisible,
                                );
                             move || {
                                 if visible {
                                     if let Some(mut cb) = on_visible.lock().as_mut() {
                                         cb();
                                     }
                                 } else {
                                     if let Some(mut cb) = on_invisible.lock().as_mut() {
                                         cb();
                                     }
                                 }
                             }
                         })
                         .interpolator(
                             if visible {
                                 Box::new(EaseOutCirc::new())
                             } else {
                                 Box::new(EaseInCirc::new())
                             }
                         )
                         .duration(Duration::from_millis(300))
                         .start();
                    }
                }
            },
        );
        Self {
            item_props,
            alignment: Alignment::top_start().into(),
            animated_visible,
            animation_duration: Duration::from_millis(300).into(),
            on_visible,
            on_invisible,
            width_scale: width_scale.into(),
            height_scale: height_scale.into(),
        }.name("AnimatedVisibility")
         .opacity(&opacity)
    }

    pub fn on_visible(
        mut self,
        on_visible: impl FnMut() + Send + 'static,
    ) -> Self {
        let on_visible_box: Box<dyn FnMut() + Send> = Box::new(on_visible);
        self.on_visible.set(Some(on_visible_box));
        self
    }

    pub fn on_invisible(
        mut self,
        on_invisible: impl FnMut() + Send + 'static,
    ) -> Self {
        let on_invisible_box: Box<dyn FnMut() + Send> = Box::new(on_invisible);
        self.on_invisible.set(Some(on_invisible_box));
        self
    }
}

pub fn animated_visibility(props: AnimatedVisibilityProps, children: impl Into<Children>) -> Item {
    if props.animated_visible.get() {
        let mut cb = props.on_visible.lock();
        if let Some(cb) = cb.as_mut() {
            cb();
        }
    } else {
        let mut cb = props.on_invisible.lock();
        if let Some(cb) = cb.as_mut() {
            cb();
        }
    }
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        children,
    )
}

fn item_event(props: &AnimatedVisibilityProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            clone!(
                props.width_scale,
                props.height_scale,
            );
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let mut max_width = 0_f32;
                let mut max_height = 0_f32;
                let mut children = item.children().lock();
                for child in children.iter_mut() {
                    let mut child_data = child.data();
                    let child_width = child_data.props().width.get();
                    let child_height = child_data.props().height.get();
                    child_data.dispatch_measure(
                        child_width.create_measure_mode(width_mode.value() - padding_h),
                        child_height.create_measure_mode(height_mode.value() - padding_v),
                    );
                    max_width = max_width.max(child_data.measure_frame.width);
                    max_height = max_height.max(child_data.measure_frame.height);
                }
                drop(children);
                match width_mode {
                    MeasureMode::Specified(width) => {
                        item.measure_frame.width = item.clamp_width(width);
                    }
                    MeasureMode::Unspecified(width) => {
                        item.measure_frame.width =
                            item.clamp_width((max_width + padding_h).min(width));
                    }
                }
                match height_mode {
                    MeasureMode::Specified(height) => {
                        item.measure_frame.height = item.clamp_height(height);
                    }
                    MeasureMode::Unspecified(height) => {
                        item.measure_frame.height =
                            item.clamp_height((max_height + padding_v).min(height));
                    }
                }
                item.measure_frame.width *= width_scale.get();
                item.measure_frame.height *= height_scale.get();
            }
        })
        .set_layout({
            clone!(
                props.padding,
                props.layout_direction,
                props.alignment,
                props.width_scale,
                props.height_scale,
            );
            move |item, width, height| {
                let width = width / width_scale.get();
                let height = height / height_scale.get();

                let padding_start = padding.start.get();
                let padding_end = padding.end.get();
                let padding_top = padding.top.get();
                let padding_bottom = padding.bottom.get();
                let layout_direction = layout_direction.get();

                let children = item.children();
                {
                    let children = children.lock();
                    for child in children.iter() {
                        let mut child_data = child.data();
                        let alignment = child_data
                            .props()
                            .get_custom::<SharedDerived<Alignment>>("align_self")
                            .cloned()
                            .unwrap_or_else(|| alignment.clone())
                            .get();

                        let measure_frame = &child_data.measure_frame;
                        let child_width = measure_frame.width;
                        let child_height = measure_frame.height;

                        let x = match alignment.horizontal() {
                            HorizontalAlignment::Start => padding_start,
                            HorizontalAlignment::Center => (width - child_width) / 2.0,
                            HorizontalAlignment::End => width - child_width - padding_end,
                        };
                        let y = match alignment.vertical() {
                            crate::ui::VerticalAlignment::Top => padding_top,
                            crate::ui::VerticalAlignment::Center => (height - child_height) / 2.0,
                            crate::ui::VerticalAlignment::Bottom => {
                                height - child_height - padding_bottom
                            }
                        };
                        child_data.dispatch_layout(
                            x.physical_x(layout_direction, width, child_width),
                            y,
                            child_width,
                            child_height,
                        );
                    }
                }
            }
        })
}
