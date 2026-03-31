use std::time::Duration;
use material_shapes::{MaterialShapes, Morph, MorphToPath, PolygonToPath};
use proc_macro::ItemProps;
use crate::animation::interpolator::{EaseOutBack, EaseOutElastic};
use crate::core::next_id;
use crate::shared::{SharedF32, SharedSource, SpringSpec, TweenSpec};
use crate::ui::{Color, Item, SetColor};
use crate::ui::item::{Children, ItemEvent, ItemKind, ItemProps};

#[derive(ItemProps)]
pub struct ShapeProps {
    pub item_props: ItemProps,
}

impl ShapeProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self { item_props }
    }
}

pub fn shape(props: ShapeProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        Children::new(),
    )
}

fn item_event(props: &ShapeProps) -> ItemEvent {
    let progress = SharedF32::new(0.0);
    let state: SharedSource<bool> = SharedSource::new(false);
    let animating = SharedSource::new(false);
    let e = props.window_context.event_loop_proxy();
    // progress.animation_to_f32(1.0)
    //     .duration(Duration::from_millis(3000))
    //     .on_finish({
    //         let progress = progress.clone();
    //         let e = e.clone();
    //         move || {
    //             progress.animation_to_f32(0.0)
    //                 .duration(Duration::from_millis(3000))
    //                 .start(&e);
    //         }
    //     })
    //     .start(e);
    progress.subscribe(
        next_id(),
        {
            let item_updater = props.item_updater.clone();
            let e = e.clone();
            move || {
                item_updater.lock().request_update();
                e.request_update_layout();
            }
        }
    );
    ItemEvent::new()
        .set_draw({
            let shape1 = MaterialShapes::flower();
            let shape2 = MaterialShapes::clam_shell();
            let morph = Morph::new(
                &shape1,
                &shape2,
            );
            let progress = progress.clone();
            let state = state.clone();
            let animating = animating.clone();
            move |item, canvas| {
                let progress = progress.get();
                let bounds: [f32; 4] = morph.calculate_bounds(None);
                // let bounds: [f32; 4] = if animating.get() {
                //     morph.calculate_bounds(None)
                // } else {
                //     if state.get() {
                //         shape2.calculate_bounds(None)
                //     } else {
                //         shape1.calculate_bounds(None)
                //     }
                // };
                // {
                    let path = morph.to_path(
                        progress,
                        0,
                        None,
                        None,
                        None,
                        None,
                    );
                // }
                // let path = if animating.get() {
                //     morph.to_path(
                //         progress,
                //         0,
                //         None,
                //         None,
                //         None,
                //         None,
                //     )
                // } else {
                //     if state.get() {
                //         shape2.to_path(
                //             0,
                //             None,
                //             None,
                //         )
                //     } else {
                //         shape1.to_path(
                //             0,
                //             None,
                //             None
                //         )
                //     }
                // };
                let target_size = 200.0;
                let target_x = 100.0;
                let target_y = 100.0;
                // canvas.save();
                // canvas.translate(translate_x, translate_y);
                // canvas.scale(scale, scale);
                let scale_x = target_size / (bounds[2] - bounds[0]);
                let scale_y = target_size / (bounds[3] - bounds[1]);
                let scale = scale_x.min(scale_y);
                let translate_x =
                    target_x - (bounds[0] + bounds[2]) / 2.0 * scale;
                let translate_y =
                    target_y - (bounds[1] + bounds[3]) / 2.0 * scale;
                canvas.save();
                canvas.translate((translate_x, translate_y));
                canvas.scale((scale, scale));
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_any_color(Color::RED);
                paint.set_style(skia_safe::paint::Style::Fill);
                canvas.draw_path(&path, &paint);
                // draw bounds
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_any_color(Color::BLUE);
                paint.set_style(skia_safe::paint::Style::Stroke);
                paint.set_stroke_width(1.0 / scale);
                let rect = skia_safe::Rect::from_xywh(
                    bounds[0],
                    bounds[1],
                    bounds[2] - bounds[0],
                    bounds[3] - bounds[1],
                );
                canvas.draw_rect(rect, &paint);
                canvas.restore();
            }
        })
        .set_click_input({
            let progress = progress.clone();
            let state = state.clone();
            let animating = animating.clone();
            let e = e.clone();
            move |item, _click_info| {
                println!("Shape clicked");
                let state_value = state.get();
                // if let Some(mut animation) = progress.get_animation() {
                //     animation.cancel()
                // }
                state.set(!state_value);
                animating.set(true);
                // progress.animation_to_f32(
                //     if state_value { 0.0 } else { 1.0 }
                // ).duration(Duration::from_millis(300))
                //         .on_finish({
                //             let animating = animating.clone();
                //             move || {
                //                 animating.set(false);
                //             }
                //         })
                //     .interpolator(EaseOutBack::boxed())
                //         .start(&e);
                progress.animate_to(
                    if state_value { 0.0 } else { 1.0 },
                    SpringSpec::new(0.8, 100.0).visibility_threshold(0.1),
                    &e
                );
                progress.subscribe(
                    next_id(),
                    {
                        let progress = progress.clone();
                        move || {
                            println!("Progress: {}", progress.get());
                        }
                    }
                );
            }
        })
}