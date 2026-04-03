use crate::app::WindowContext;
use crate::core::next_id;
use crate::shared::{RepeatCount, RepeatMode, RepeatSpec, SharedDerivedBool, SharedDerivedColor, SharedF32, SharedUsize, SpringSpec, TweenSpec};
use crate::shared_derived;
use crate::ui::item::{Children, ItemKind, ItemProps};
use crate::ui::loading_indicator_styles::{LoadingIndicatorStyleExt, SharedLoadingIndicatorStyle};
use crate::ui::{rectangle, Color, Item, Radius, RectanglePropsTrait, SetColor, Size};
use lazy_static::lazy_static;
use letclone::clone;
use material_shapes::{MaterialShapes, Morph, MorphToPath, RoundedPolygon};
use proc_macro::ItemProps;
use skia_safe::{Paint, Path};
use std::ops::Deref;
use std::time::Duration;
use crate::event::ItemEvent;

#[derive(ItemProps)]
pub struct LoadingIndicatorProps {
    pub item_props: ItemProps,
    #[constructor]
    pub is_contained: SharedDerivedBool,
    pub indicator_color: SharedDerivedColor,
    pub container_color: SharedDerivedColor,
}

impl LoadingIndicatorProps {
    pub fn new(mut item_props: ItemProps, is_contained: impl Into<SharedDerivedBool>) -> Self {
        let style = SharedLoadingIndicatorStyle::new(
            item_props.window_context.theme(),
            &item_props.item_state,
        );
        let width = shared_derived!(style.container_width => Size::Fixed(container_width.get()));
        let height = shared_derived!(style.container_height => Size::Fixed(container_height.get()));
        item_props.width = width;
        item_props.height = height;
        let is_contained = is_contained.into();
        let container_color = shared_derived!(
            style.contained_container_color, is_contained =>
            if is_contained.get() {
                contained_container_color.get()
            } else {
                Color::TRANSPARENT
            }
        );
        container_color.subscribe(
            next_id(),
            {
                clone!(container_color);
                move || println!("Container Color Updated: {:?}", container_color.get())
            }
        );
        let indicator_color = shared_derived!(
            style.active_indicator_color, style.contained_indicator_color, is_contained =>
            if is_contained.get() {
                contained_indicator_color.get()
            } else {
                active_indicator_color.get()
            }
        );
        Self {
            item_props,
            is_contained,
            indicator_color,
            container_color,
        }
    }
}

pub fn loading_indicator(props: impl Into<LoadingIndicatorProps>) -> Item {
    let props = props.into();
    let w = &props.window_context;
    let shared_theme = w.theme();
    let item_state = &props.item_state;
    let style = SharedLoadingIndicatorStyle::new(shared_theme, item_state);

    let background =
        rectangle(
            w.rectangle_props(&props.container_color)
             .radius(style.container_shape)
        );
    // drop(theme);
    // let is_contained = props.is_contained.clone();
    // props.container_color.depends_on(&is_contained);
    // props.indicator_color.depends_on(&is_contained);

    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props.background(background),
        Children::new(),
    )
}

lazy_static!(
    static ref POLYGONS: [RoundedPolygon; 7] = [
        MaterialShapes::soft_burst().normalized(),
        MaterialShapes::cookie_9_sided().normalized(),
        MaterialShapes::pentagon().normalized(),
        MaterialShapes::pill().normalized(),
        MaterialShapes::sunny().normalized(),
        MaterialShapes::cookie_4_sided().normalized(),
        MaterialShapes::oval().normalized()
    ];
);
fn item_event(props: &LoadingIndicatorProps) -> ItemEvent {
    let morph_sequence = morph_sequence(POLYGONS.deref());
    let shapes_scale_factor = calculate_scale_factor(POLYGONS.deref()) * (38.0 / 48.0);

    let morph_progress = SharedF32::new(0.0);
    let morph_rotation_target_angle = SharedF32::new(QUARTER_ROTATION);
    let global_rotation = SharedF32::new(0.0);
    let current_morph_index = SharedUsize::new(0);

    clone!(
        props.item_updater,
        props.event_loop_proxy()
    );
    morph_progress.subscribe(
        next_id(),
        {
            clone!(
                item_updater,
                event_loop_proxy
            );
            move || {
                item_updater.lock().request_update();
                event_loop_proxy.request_update_layout();
            }
        },
    );

    props.spawn_task({
        clone!(
             morph_progress,
             current_morph_index,
             morph_rotation_target_angle,
             event_loop_proxy
        );
        let morph_sequence_len = morph_sequence.len();
        async move {
            loop {
                let task = tokio::spawn({
                    clone!(
                        morph_progress,
                        current_morph_index,
                        morph_rotation_target_angle,
                        event_loop_proxy
                    );
                    async move {
                        morph_progress.animate_to_async(
                            1.0,
                            SpringSpec::new(0.6, 200.0)
                                .visibility_threshold(0.1),
                            &event_loop_proxy,
                        ).await;
                        let current_index = current_morph_index.get();
                        let next_index = (current_index + 1) % morph_sequence_len;
                        current_morph_index.set(next_index);
                        let current_angle = morph_rotation_target_angle.get();
                        let new_angle = (current_angle + QUARTER_ROTATION) % FULL_ROTATION;
                        morph_rotation_target_angle.set(new_angle);
                        morph_progress.set(0.0);
                    }
                });
                tokio::time::sleep(Duration::from_millis(650)).await;
                task.await.unwrap();
            }
        }
    });

    global_rotation.animate_to(
        FULL_ROTATION,
        RepeatSpec::new(
            TweenSpec::new()
                .duration(Duration::from_millis(4666)),
            RepeatMode::Restart,
            RepeatCount::Infinite,
        ),
        &event_loop_proxy,
    );

    let style = SharedLoadingIndicatorStyle::new(props.window_context.theme(), &props.item_state);
    let indicator_color = shared_derived!(style.contained_indicator_color, style.active_indicator_color, props.is_contained || {
        if is_contained.get() {
            contained_indicator_color.get()
        } else {
            active_indicator_color.get()
        }
    });
    ItemEvent::new()
        .set_record_animation_value({
            clone!(props.indicator_color);
            move |item| {
                let frame = &mut item.target_frame;
                let indicator_color = indicator_color.get();
                frame.set_color_param("indicator_color", indicator_color);
            }
        })
        .set_draw({
            let morph_progress = morph_progress.clone();
            let current_morph_index = current_morph_index.clone();
            let morph_rotation_target_angle = morph_rotation_target_angle.clone();
            let global_rotation = global_rotation.clone();
            move |item, canvas| {
                let current_frame = item.current_frame();
                let indicator_color = current_frame.get_color_param("indicator_color").unwrap_or(Color::TRANSPARENT);

                let current_index = current_morph_index.get();
                let morph = &morph_sequence[current_index];
                let progress = morph_progress.get();
                let path = morph.to_path(
                    progress,
                    0,
                    None,
                    None,
                    None,
                    None,
                );
                let path = progress_path(
                    path,
                    (current_frame.width(), current_frame.height()),
                    shapes_scale_factor,
                );
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_any_color(indicator_color);
                paint.set_style(skia_safe::paint::Style::Fill);


                let morph_rotation_target_angle = morph_rotation_target_angle.get();
                let global_rotation = global_rotation.get();
                let total_rotation = progress * 90.0 + morph_rotation_target_angle + global_rotation;

                let x = current_frame.x();
                let y = current_frame.y();
                canvas.save();
                canvas.translate((x, y));
                let center = skia_safe::Point::new(current_frame.width() / 2.0, current_frame.height() / 2.0);
                canvas.rotate(total_rotation, Some(center));
                canvas.draw_path(&path, &paint);
                canvas.restore();
            }
        })
}

fn progress_path(
    path: Path,
    size: (f32, f32),
    scale_factor: f32,
) -> Path {
    let scale_x = size.0 * scale_factor;
    let scale_y = size.1 * scale_factor;
    let bounds = path.bounds();
    let center_x = bounds.center_x();
    let center_y = bounds.center_y();

    let size_center_x = size.0 / 2.0;
    let size_center_y = size.1 / 2.0;

    let translate_x = size_center_x - scale_x / 2.0;
    let translate_y = size_center_y - scale_y / 2.0;
    path.make_scale((scale_x, scale_y)).make_offset((translate_x, translate_y))
}

const FULL_ROTATION: f32 = 360.0;
const QUARTER_ROTATION: f32 = FULL_ROTATION / 4.0;

fn calculate_scale_factor(polygons: &[RoundedPolygon]) -> f32 {
    let mut scale_factor = 1.0_f32;
    for polygon in polygons {
        let bounds = polygon.calculate_bounds(None);
        let max_bounds = polygon.calculate_max_bounds();
        let scale_x = bounds.width() / max_bounds.width();
        let scale_y = bounds.height() / max_bounds.height();
        scale_factor = scale_factor.min(scale_x.max(scale_y));
    }
    scale_factor
}

trait SizeExt {
    fn width(&self) -> f32;
    fn height(&self) -> f32;
}

impl SizeExt for &[f32; 4] {
    fn width(&self) -> f32 {
        self[2] - self[0]
    }
    fn height(&self) -> f32 {
        self[3] - self[1]
    }
}

impl SizeExt for [f32; 4] {
    fn width(&self) -> f32 {
        self[2] - self[0]
    }
    fn height(&self) -> f32 {
        self[3] - self[1]
    }
}

fn morph_sequence(polygons: &[RoundedPolygon]) -> Vec<Morph<'_>> {
    let mut morphs = Vec::new();
    for i in 0..polygons.len() {
        if i + 1 < polygons.len() {
            morphs.push(Morph::new(&polygons[i], &polygons[i + 1]));
        } else {
            morphs.push(Morph::new(&polygons[i], &polygons[0]));
        }
    }
    morphs
}

pub mod loading_indicator_styles {
    use crate::shared::{SharedDerivedColor, SharedDerivedF32, SharedSource};
    use crate::theme::shape::{corner, Corner};
    use crate::theme::{color, StateStyles};
    use crate::ui::item::ItemState;
    use crate::ui::{Color, Radius};
    use crate::{shared_derived, Theme};
    use proc_macro::style;

    #[style]
    pub struct LoadingIndicatorStyle {
        active_indicator_color: Color,
        contained_container_color: Color,
        contained_indicator_color: Color,

        container_width: f32,
        container_height: f32,
        active_indicator_size: f32,

        container_shape: Corner,
    }

    impl Default for LoadingIndicatorStyle {
        fn default() -> Self {
            Self {
                active_indicator_color: color::PRIMARY.into(),
                contained_container_color: color::PRIMARY_CONTAINER.into(),
                contained_indicator_color: color::ON_PRIMARY_CONTAINER.into(),
                container_width: 48.0.into(),
                container_height: 48.0.into(),
                active_indicator_size: 38.0.into(),
                container_shape: corner::FULL.into(),
            }
        }
    }

    pub fn loading_indicator_style() -> StateStyles<LoadingIndicatorStyle> {
        StateStyles::enabled(LoadingIndicatorStyle::default())
    }
    pub const LOADING_INDICATOR_STYLE: &str = "loading_indicator_style";
    pub fn apply_loading_indicator_style(theme: &mut Theme) {
        let styles = loading_indicator_style();
        theme.set_loading_indicator_style(LOADING_INDICATOR_STYLE, styles);
    }

    pub struct SharedLoadingIndicatorStyle {
        pub active_indicator_color: SharedDerivedColor,
        pub contained_container_color: SharedDerivedColor,
        pub contained_indicator_color: SharedDerivedColor,

        pub container_width: SharedDerivedF32,
        pub container_height: SharedDerivedF32,
        pub active_indicator_size: SharedDerivedF32,

        pub container_shape: Radius,
    }

    impl SharedLoadingIndicatorStyle {
        pub fn new(theme: &SharedSource<Theme>, item_state: &SharedSource<ItemState>) -> Self {
            let active_indicator_color = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_active_indicator_color(&theme).cloned().unwrap()
            });
            let contained_container_color = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_contained_container_color(&theme).cloned().unwrap()
            });
            let contained_indicator_color = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_contained_indicator_color(&theme).cloned().unwrap()
            });
            let container_width = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                *style.get_container_width(&theme).unwrap()
            });
            let container_height = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                *style.get_container_height(&theme).unwrap()
            });
            let active_indicator_size = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                *style.get_active_indicator_size(&theme).unwrap()
            });

            let top_start = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_container_shape(&theme).cloned().unwrap().top_start
            });
            let top_end = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_container_shape(&theme).cloned().unwrap().top_end
            });
            let bottom_start = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_container_shape(&theme).cloned().unwrap().bottom_start
            });
            let bottom_end = shared_derived!(theme, item_state ||{
                let theme = theme.lock();
                let style = theme.get_loading_indicator_style(LOADING_INDICATOR_STYLE, item_state.get()).unwrap();
                style.get_container_shape(&theme).cloned().unwrap().bottom_end
            });
            Self {
                active_indicator_color,
                contained_container_color,
                contained_indicator_color,
                container_width,
                container_height,
                active_indicator_size,
                container_shape: Radius::new()
                    .top_start(top_start)
                    .top_end(top_end)
                    .bottom_start(bottom_start)
                    .bottom_end(bottom_end),
            }
        }
    }
}

impl From<&WindowContext> for LoadingIndicatorProps {
    fn from(w: &WindowContext) -> Self {
        LoadingIndicatorProps::new(
            ItemProps::new(w),
            true,
        )
    }
}

impl From<WindowContext> for LoadingIndicatorProps {
    fn from(w: WindowContext) -> Self {
        LoadingIndicatorProps::new(
            ItemProps::new(&w),
            true,
        )
    }
}