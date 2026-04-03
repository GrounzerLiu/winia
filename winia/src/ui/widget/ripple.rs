use crate::app::WindowContext;
use crate::core::next_id;
use crate::shared::{SharedBool, SharedDerivedColor, SharedDerivedF32, SharedF32, SharedSource, TweenSpec};
use crate::theme::color;
use crate::ui::item::{Children, ItemKind, ItemProps, ItemUpdater};
use crate::ui::{Color, Item, SetColor};
use letclone::clone;
use parking_lot::Mutex;
use proc_macro::ItemProps;
use skia_safe::{Paint, Path};
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use winit::event::{ButtonSource, ElementState};
use crate::event::{ItemEvent, PointerButton};

#[derive(ItemProps)]
pub struct RippleProps {
    pub item_props: ItemProps,
    pub color: SharedDerivedColor,
    pub background_opacity: SharedDerivedF32,
    pub ripple_opacity: SharedDerivedF32,
}

impl RippleProps {
    pub fn new(item_props: ItemProps) -> Self {
        let primary_color = item_props
            .window_context
            .theme()
            .lock()
            .get_color(color::PRIMARY)
            .map_or(Color::BLACK, |c| *c);
        Self {
            item_props,
            color: primary_color.into(),
            background_opacity: 0.08.into(),
            ripple_opacity: 0.1.into(),
        }.name("Ripple")
    }
}

pub fn ripple(props: RippleProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        Children::new(),
    )
}

struct Layer {
    pub pointer: ButtonSource,
    pub is_finished: SharedBool,
    pub center: (f32, f32),
    pub progress: SharedF32,
    pub opacity: SharedF32,
}

fn subscribe_redraw(
    window_context: &WindowContext,
    need_redraw: &Arc<Mutex<ItemUpdater>>,
    shared: &SharedF32,
) {
    let event_loop_proxy = window_context.event_loop_proxy().clone();
    let need_redraw = need_redraw.clone();
    shared.subscribe(next_id(), move || {
        event_loop_proxy.request_update_layout();
        need_redraw.lock().request_update();
    });
}

fn item_event(props: &RippleProps) -> ItemEvent {
    let layers: SharedSource<Vec<Layer>> = vec![].into();
    let background_opacity = SharedF32::new(0.0);
    subscribe_redraw(
        &props.window_context,
        &props.item_updater,
        &background_opacity,
    );
    ItemEvent::new()
        .set_draw({
            clone!(
                layers,
                props.color,
                background_opacity,
                //props.foreground_opacity
            );
            move |item, canvas| {
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                let color = color.get();
                let background_opacity = background_opacity.get();
                paint.set_any_color(color.with_a_f(background_opacity));
                let current_frame = item.current_frame();
                let clip_radius =
                    (current_frame.width().powi(2) + current_frame.height().powi(2)).sqrt() / 2.0;
                canvas.clip_path(
                    &Path::circle(
                        (
                            current_frame.x() + current_frame.width() / 2.0,
                            current_frame.y() + current_frame.height() / 2.0,
                        ),
                        clip_radius,
                        None,
                    ),
                    None,
                    true,
                );

                let x = current_frame.x();
                let y = current_frame.y();
                let width = current_frame.width();
                let height = current_frame.height();
                let center_x = x + width / 2.0;
                let center_y = y + height / 2.0;
                let radius = (width.powi(2) + height.powi(2)).sqrt();
                canvas.draw_circle((center_x, center_y), radius / 2.0, &paint);

                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                let mut layers = layers.lock();
                layers.retain(|layer| !layer.is_finished.get());
                for layer in layers.iter() {
                    let opacity = layer.opacity.get();
                    let progress = layer.progress.get();
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    let color = color.with_a_f(opacity);
                    let radius = radius * progress;
                    paint.set_any_color(color);

                    let (center_x, center_y) = layer.center;
                    let center = (center_x + current_frame.x(), center_y + current_frame.y());
                    canvas.draw_circle(center, radius, &paint);
                }
            }
        })
        .set_hover_changed({
            clone!(background_opacity);
            let defual_background_opacity = props.background_opacity.clone();
            move |item, is_hovered| {
                // background_opacity.get_animation().lock().deref().if_some(|animation| animation.stop());
                {
                    let animation = background_opacity.get_animation();
                    if let Some(animation) = animation.lock().deref() {
                        animation.stop();
                    }
                }
                // background_opacity
                //     .animation_to_f32(if is_hovered { defual_background_opacity.get() } else { 0.0 })
                //     .duration(Duration::from_millis(500))
                //     .start(&item.window_context().event_loop_proxy());
                background_opacity.animate_to(
                    if is_hovered {
                        defual_background_opacity.get()
                    } else {
                        0.0
                    },
                    TweenSpec::new().duration(Duration::from_millis(500)),
                    item.event_loop_proxy()
                )
            }
        })
        .set_pointer_button({
            clone!(layers, props.window_context, props.item_updater, layers, props.ripple_opacity);
            move |item, pointer_button: &PointerButton| {
                match pointer_button.state {
                    ElementState::Pressed => {
                        let progress = SharedF32::new(0.0);
                        let opacity = SharedF32::new(ripple_opacity.get());
                        subscribe_redraw(&window_context, &item_updater, &progress);
                        subscribe_redraw(&window_context, &item_updater, &opacity);
                        // progress
                        //     .animation_to_f32(1.0)
                        //     .duration(Duration::from_millis(500))
                        //     .start(&item.window_context().event_loop_proxy());
                        progress.animate_to(
                            1.0,
                            TweenSpec::new().duration(Duration::from_millis(500)),
                            &item.props().event_loop_proxy(),
                        );
                        let current_frame = item.current_frame();
                        let layer = Layer {
                            pointer: pointer_button.button.clone(),
                            is_finished: false.into(),
                            center: (pointer_button.x - current_frame.x(), pointer_button.y - current_frame.y()),
                            progress,
                            opacity,
                        };
                        layers.lock().push(layer);
                        false
                    }
                    // PointerState::Moved => {()}
                    ElementState::Released => {
                        let mut layers = layers.lock();
                        for layer in layers.iter_mut() {
                            if layer.pointer != pointer_button.button {
                                println!("layer.pointer: {:?}, pointer_button.button: {:?}", layer.pointer, pointer_button.button);
                                continue;
                            }
                            let is_finished = layer.is_finished.clone();
                            // if let Some(animation) = layer.progress.get_animation()
                            //     && !animation.is_finished()
                            // {
                            //     let opacity = layer.opacity.clone();
                            //     let event_loop_proxy =
                            //         item.props().window_context.event_loop_proxy().clone();
                            //     animation.on_finish(move || {
                            //         let is_finished = is_finished.clone();
                            //         opacity
                            //             .animation_to_f32(0.0)
                            //             .duration(Duration::from_millis(300))
                            //             .on_finish(move || {
                            //                 is_finished.set(true);
                            //             })
                            //             .start(&event_loop_proxy);
                            //     });
                            //     continue;
                            // }
                            // layer
                            //     .opacity
                            //     .animation_to_f32(0.0)
                            //     .duration(Duration::from_millis(300))
                            //     .on_finish(move || {
                            //         is_finished.set(true);
                            //     })
                            //     .start(&item.props().window_context.event_loop_proxy());
                            layer.opacity.animate_to(
                                0.0,
                                TweenSpec::new().duration(Duration::from_millis(300))
                                    .on_finish(move || {
                                        is_finished.set(true);
                                    }),
                                &item.props().event_loop_proxy(),
                            );
                        }
                        false
                    }
                }
            }
        })
}
