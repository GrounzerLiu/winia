use crate::app::WindowContext;
use crate::core::next_id;
use crate::shared::{
    Shared, SharedBool, SharedDerivedColor, SharedDerivedF32, SharedF32,
    SharedSource,
};
use crate::theme::color;
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, NeedRedraw, Pointer, PointerState};
use crate::ui::{Color, Item, SetColor};
use crate::{bind_properties, define_props};
use clonelet::clone;
use parking_lot::Mutex;
use skia_safe::{Paint, Path};
use std::sync::Arc;
use std::time::Duration;

define_props!(
    RipplePropsTrait;
    ripple_props;
    RippleProps{
        color: SharedDerivedColor,
        background_opacity: SharedDerivedF32,
        foreground_opacity: SharedDerivedF32,
    }
);

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
            background_opacity: 0.0.into(),
            foreground_opacity: 0.3.into(),
        }
    }
}

pub fn ripple(props: RippleProps) -> Item {
    let item = Item::new(
        ItemKind::Widget,
        item_event(&props),
        props.item_props,
        Shared::new_derived(vec![]),
    );
    bind_properties!(
        item,
        props.color,
        props.background_opacity,
        props.foreground_opacity
    );
    item
}

struct Layer {
    pub pointer: Pointer,
    pub is_finished: SharedBool,
    pub center: (f32, f32),
    pub degree: SharedF32,
    pub opacity: SharedF32,
}
fn subscribe(
    window_context: &WindowContext,
    need_redraw: &Arc<Mutex<NeedRedraw>>,
    shared: &SharedF32,
) {
    let event_loop_proxy = window_context.event_loop_proxy.clone();
    let need_redraw = need_redraw.clone();
    shared.subscribe(next_id(), move || {
        event_loop_proxy.request_redraw();
        need_redraw.lock().request();
    });
}
fn item_event(props: &RippleProps) -> ItemEvent {
    let layers: SharedSource<Vec<Layer>> = vec![].into();
    let background_opacity = SharedF32::new(0.0);
    subscribe(
        &props.window_context,
        &props.need_redraw,
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
                    Path::new().add_circle(
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
                    let degree = layer.degree.get();
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    let color = color.with_a_f(opacity);
                    let radius = radius * degree;
                    paint.set_any_color(color);

                    let (center_x, center_y) = layer.center;
                    let center = (center_x + current_frame.x(), center_y + current_frame.y());
                    canvas.draw_circle(center, radius, &paint);
                }
            }
        })
        .set_hover_changed({
            clone!(background_opacity);
            move |item, is_hovered| {
                if let Some(mut animation) = background_opacity.get_animation() {
                    animation.stop();
                }
                background_opacity.animation_to_f32(if is_hovered { 0.08 } else { 0.0 })
                    .duration(Duration::from_millis(500))
                    .start(&item.window_context().event_loop_proxy);
            }
        })
        .set_pointer_input({
            clone!(layers, props.window_context, props.need_redraw, layers);
            move |item, event| match event.pointer_state {
                PointerState::Started => {
                    let degree = SharedF32::new(0.0);
                    let opacity = SharedF32::new(0.1);
                    subscribe(&window_context, &need_redraw, &degree);
                    subscribe(&window_context, &need_redraw, &opacity);
                    degree
                        .animation_to_f32(1.0)
                        .duration(Duration::from_millis(500))
                        .start(&item.window_context().event_loop_proxy);
                    let current_frame = item.current_frame();
                    let layer = Layer {
                        pointer: event.pointer,
                        is_finished: false.into(),
                        center: (event.x - current_frame.x(), event.y - current_frame.y()),
                        degree,
                        opacity,
                    };
                    layers.lock().push(layer);
                    false
                }
                // PointerState::Moved => {()}
                PointerState::Ended => {
                    let mut layers = layers.lock();
                    for layer in layers.iter_mut() {
                        if layer.pointer != event.pointer {
                            continue;
                        }
                        let is_finished = layer.is_finished.clone();
                        if let Some(animation) = layer.degree.get_animation() && !animation.is_finished() {
                                let opacity = layer.opacity.clone();
                                let event_loop_proxy =
                                    item.props().window_context.event_loop_proxy.clone();
                                animation.on_finish(move || {
                                    let is_finished = is_finished.clone();
                                    opacity
                                        .animation_to_f32(0.0)
                                        .duration(Duration::from_millis(300))
                                        .on_finish(move || {
                                            is_finished.set(true);
                                        })
                                        .start(&event_loop_proxy);
                                });
                                continue;
                        }
                        layer
                            .opacity
                            .animation_to_f32(0.0)
                            .duration(Duration::from_millis(300))
                            .on_finish(move || {
                                is_finished.set(true);
                            })
                            .start(&item.props().window_context.event_loop_proxy);
                    }
                    false
                }
                _ => false, // PointerState::Cancelled => {()}
            }
        })
}
