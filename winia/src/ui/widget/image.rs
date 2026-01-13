use std::ops::DerefMut;
use std::sync::Arc;
use clonelet::clone;
use parking_lot::Mutex;
use proc_macro::ItemProps;
use crate::{bind_properties, define_props};
use crate::app::EventLoopProxy;
use crate::drawable::{Drawable, ImageDrawable};
use crate::shared::{SharedDerived, SharedDerivedBool, SharedDerivedDrawable, SharedDrawable};
use crate::ui::{Alignment, Color, HorizontalAlignment, Item, Orientation, VerticalAlignment};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, MeasureMode, ItemUpdater, PhysicalX};

static DRAWABLE_X: &str = "drawable_x";
static DRAWABLE_Y: &str = "drawable_y";
static DRAWABLE_WIDTH: &str = "drawable_width";
static DRAWABLE_HEIGHT: &str = "drawable_height";
static DRAWABLE_COLOR: &str = "drawable_color";

/// The scale mode of the image
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    /// Retain the original size of the image.
    Original,
    /// Stretch the image non-uniformly to fill the item, ignoring the aspect ratio.
    Stretch,
    /// Scale the image uniformly to completely cover the item, cropping the image if necessary.
    /// (maintaining the aspect ratio)
    Cover,
    /// Scale the image uniformly to fit entirely within the item, potentially leaving empty space.
    /// (maintaining the aspect ratio)
    Contain,
}

/*define_props!(
    ImagePropsTrait;
    image_props;
    ImageProps {
        drawable: SharedDerivedDrawable,
        align_content: SharedDerived<Alignment>,
        dpi_sensitive: SharedDerivedBool,
        oversize_scale_mode: SharedDerived<ScaleMode>,
        undersize_scale_mode: SharedDerived<ScaleMode>,
        color: SharedDerived<Option<Color>>,
    }
);*/

#[derive(ItemProps)]
pub struct ImageProps {
    pub item_props: ItemProps,
    #[constructor]
    pub drawable: SharedDerivedDrawable,
    pub align_content: SharedDerived<Alignment>,
    pub dpi_sensitive: SharedDerivedBool,
    pub oversize_scale_mode: SharedDerived<ScaleMode>,
    pub undersize_scale_mode: SharedDerived<ScaleMode>,
    pub color: SharedDerived<Option<Color>>,
}

impl ImageProps {
    pub fn new(item_props: ItemProps, drawable: impl Into<SharedDerivedDrawable>) -> Self {
        Self {
            item_props,
            drawable: drawable.into(),
            align_content: Alignment::center().into(),
            dpi_sensitive: true.into(),
            oversize_scale_mode: ScaleMode::Contain.into(),
            undersize_scale_mode: ScaleMode::Original.into(),
            color: None.into(),
        }
    }
}

pub fn image(image_props: ImageProps) -> Item {
    let event_loop_proxy = image_props.item_props.window_context.event_loop_proxy().clone();
    let item_updater = image_props.item_props.item_updater.clone();
    let image_drawable = image_props.drawable.clone();
    let item = Item::new(
        ItemKind::Widget,
        item_event(&image_props),
        image_props,
        vec![]
    );
    let id = item.id();
    fn add_redraw_requester(
        id: u32,
        item_updater: &Arc<Mutex<ItemUpdater>>,
        event_loop_proxy: &EventLoopProxy,
        drawable: &mut Box<dyn Drawable>,
    ) {
        drawable.add_redraw_requester(
            id,
            Box::new({
                let item_updater = item_updater.clone();
                let event_loop_proxy = event_loop_proxy.clone();
                move || {
                    item_updater.lock().request_update();
                    event_loop_proxy.request_update_layout();
                }
            })
        );
    }
    add_redraw_requester(
        id,
        &item_updater,
        &event_loop_proxy,
        image_drawable.lock().deref_mut(),
    );
    image_drawable.add_interceptor(id, move|old, mut new|{
        old.remove_redraw_requester(id);
        add_redraw_requester(
            id,
            &item_updater,
            &event_loop_proxy,
            &mut new,
        );
        Some(new)
    });
    item
}

fn item_event(props: &ImageProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            clone!(props.drawable,
                   props.dpi_sensitive,
                   props.oversize_scale_mode,
                   props.undersize_scale_mode);
            move |item, width_mode, height_mode| {
                let oversize_scale_mode = oversize_scale_mode.get();
                let undersize_scale_mode = undersize_scale_mode.get();
                let dpi_sensitive = dpi_sensitive.get();
                let drawable = drawable.lock();
                let scale_factor = item.window_context().scale_factor();
                let drawable_width = drawable.get_intrinsic_width()
                    / if dpi_sensitive { scale_factor } else { 1.0 };
                let drawable_height = drawable.get_intrinsic_height()
                    / if dpi_sensitive { scale_factor } else { 1.0 };

                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let padding_vertical = item.get_padding(Orientation::Vertical);

                let (width, height) = match (width_mode, height_mode) {
                    (MeasureMode::Specified(width), MeasureMode::Specified(height)) => {
                        (width, height)
                    }
                    (MeasureMode::Specified(width), MeasureMode::Unspecified(_)) => {
                        let scale_mode = if drawable_width > width {
                            oversize_scale_mode
                        } else {
                            undersize_scale_mode
                        };

                        let height = match scale_mode {
                            ScaleMode::Original => drawable_height + padding_vertical,
                            ScaleMode::Stretch => {
                                drawable_height * (width - padding_horizontal) / drawable_width
                                    + padding_vertical
                            }
                            ScaleMode::Cover => {
                                drawable_height * (width - padding_horizontal) / drawable_width
                                    + padding_vertical
                            }
                            ScaleMode::Contain => {
                                if drawable_width < width - padding_horizontal {
                                    drawable_height + padding_vertical
                                } else {
                                    drawable_height * (width - padding_horizontal)
                                        / drawable_width
                                        + padding_vertical
                                }
                            }
                        };

                        (width, height)
                    }
                    (MeasureMode::Unspecified(_), MeasureMode::Specified(height)) => {
                        let scale_mode = if drawable_height > height {
                            oversize_scale_mode
                        } else {
                            undersize_scale_mode
                        };

                        let width = match scale_mode {
                            ScaleMode::Original => drawable_width + padding_horizontal,
                            ScaleMode::Stretch => {
                                drawable_width * (height - padding_vertical) / drawable_height
                                    + padding_horizontal
                            }
                            ScaleMode::Cover => {
                                drawable_width * (height - padding_vertical) / drawable_height
                                    + padding_horizontal
                            }
                            ScaleMode::Contain => {
                                if drawable_height < height - padding_vertical {
                                    drawable_width + padding_horizontal
                                } else {
                                    drawable_width * (height - padding_vertical)
                                        / drawable_height
                                        + padding_horizontal
                                }
                            }
                        };
                        (width, height)
                    }
                    (MeasureMode::Unspecified(_), MeasureMode::Unspecified(_)) => (
                        drawable_width + padding_horizontal,
                        drawable_height + padding_vertical,
                    ),
                };

                let measure_frame = &mut item.measure_frame;
                measure_frame.width = width;
                measure_frame.height = height;
            }
        })
        .set_layout({
            clone!(
                props.drawable,
                props.align_content,
                props.dpi_sensitive,
                props.oversize_scale_mode,
                props.undersize_scale_mode,
                props.color,
                props.layout_direction
            );
            move |item, width, height| {
                let drawable = drawable.lock();
                let layout_direction = layout_direction.get();

                let align = align_content.get();
                let padding_start = item.props().padding.start.get();
                let padding_end = item.props().padding.end.get();
                let padding_top = item.props().padding.top.get();
                let padding_bottom = item.props().padding.bottom.get();
                let padding_horizontal = padding_start + padding_end;
                let padding_vertical = padding_top + padding_bottom;

                let drawable_width = drawable.get_intrinsic_width();
                let drawable_height = drawable.get_intrinsic_height();

                let scale_factor = item.window_context().scale_factor();
                let drawable_width = drawable_width
                    / if dpi_sensitive.get() {
                    scale_factor
                } else {
                    1.0
                };
                let drawable_height = drawable_height
                    / if dpi_sensitive.get() {
                    scale_factor
                } else {
                    1.0
                };

                let scale_mode = if drawable_width > width - padding_horizontal
                    || drawable_height > height - padding_vertical
                {
                    oversize_scale_mode.get()
                } else {
                    undersize_scale_mode.get()
                };

                let mut x = 0.0;

                let (x, y, width, height) = match scale_mode {
                    ScaleMode::Original => {
/*                        match align {
                            Alignment::TopStart => {
                                x = x + padding_start;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopEnd => {
                                x = x + width - drawable_width - padding_end;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterStart => {
                                x = x + padding_start;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomStart => {
                                x = x + padding_start;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                        }*/
                        match align.horizontal() {
                            HorizontalAlignment::Start => {
                                x = x + padding_start;
                            }
                            HorizontalAlignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                            }
                            HorizontalAlignment::End => {
                                x = x + width - drawable_width - padding_end;
                            }
                        }
                        let y = match align.vertical() {
                            VerticalAlignment::Top => {
                                padding_top
                            }
                            VerticalAlignment::Center => {
                                (height - drawable_height) / 2.0
                            }
                            VerticalAlignment::Bottom => {
                                height - drawable_height - padding_bottom
                            }
                        };
                        (
                            x.physical_x(layout_direction, width, drawable_width),
                            y,
                            drawable_width,
                            drawable_height,
                        )
                    },
                    ScaleMode::Stretch => {
                        x = x + padding_start;
                        let y = padding_top;
                        let width = width - padding_horizontal;
                        let height = height - padding_vertical;
                        (x.physical_x(layout_direction, width, width), y, width, height)
                    }
                    ScaleMode::Cover => {
                        let scale = {
                            let scale_x = (width - padding_horizontal) / drawable_width;
                            let scale_y = (height - padding_vertical) / drawable_height;
                            scale_x.max(scale_y)
                        };
                        let drawable_width = drawable_width * scale;
                        let drawable_height = drawable_height * scale;
                        /*match align {
                            Alignment::TopStart => {
                                x = x + padding_start;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopEnd => {
                                x = x + width - drawable_width - padding_end;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterStart => {
                                x = x + padding_start;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomStart => {
                                x = x + padding_start;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                        }*/
                        match align.horizontal() {
                            HorizontalAlignment::Start => {
                                x = x + padding_start;
                            }
                            HorizontalAlignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                            }
                            HorizontalAlignment::End => {
                                x = x + width - drawable_width - padding_end;
                            }
                        }
                        let y = match align.vertical() {
                            VerticalAlignment::Top => {
                                padding_top
                            }
                            VerticalAlignment::Center => {
                                (height - drawable_height) / 2.0
                            }
                            VerticalAlignment::Bottom => {
                                height - drawable_height - padding_bottom
                            }
                        };
                        (
                            x.physical_x(layout_direction, width, drawable_width),
                            y,
                            drawable_width,
                            drawable_height,
                        )
                    }
                    ScaleMode::Contain => {
                        let scale = {
                            let scale_x = (width - padding_horizontal) / drawable_width;
                            let scale_y = (height - padding_vertical) / drawable_height;
                            scale_x.min(scale_y)
                        };

                        let drawable_width = drawable_width * scale;
                        let drawable_height = drawable_height * scale;
                        /*match align {
                            Alignment::TopStart => {
                                x = x + padding_start;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::TopEnd => {
                                x = x + width - drawable_width - padding_end;
                                (
                                    x.physical_value(drawable_width),
                                    padding_top,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterStart => {
                                x = x + padding_start;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::CenterEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = (height - drawable_height) / 2.0;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomStart => {
                                x = x + padding_start;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomCenter => {
                                x = x + (width - drawable_width) / 2.0;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                            Alignment::BottomEnd => {
                                x = x + width - drawable_width - padding_end;
                                let y = height - drawable_height - padding_bottom;
                                (
                                    x.physical_value(drawable_width),
                                    y,
                                    drawable_width,
                                    drawable_height,
                                )
                            }
                        }*/
                        match align.horizontal() {
                            HorizontalAlignment::Start => {
                                x = x + padding_start;
                            }
                            HorizontalAlignment::Center => {
                                x = x + (width - drawable_width) / 2.0;
                            }
                            HorizontalAlignment::End => {
                                x = x + width - drawable_width - padding_end;
                            }
                        }
                        let y = match align.vertical() {
                            VerticalAlignment::Top => {
                                padding_top
                            }
                            VerticalAlignment::Center => {
                                (height - drawable_height) / 2.0
                            }
                            VerticalAlignment::Bottom => {
                                height - drawable_height - padding_bottom
                            }
                        };
                        (
                            x.physical_x(layout_direction, width, drawable_width),
                            y,
                            drawable_width,
                            drawable_height,
                        )
                    }
                };

                let color = color.get();

                let target_frame = &mut item.target_frame;
                target_frame.set_float_param(DRAWABLE_X, x);
                target_frame.set_float_param(DRAWABLE_Y, y);
                target_frame.set_float_param(DRAWABLE_WIDTH, width);
                target_frame.set_float_param(DRAWABLE_HEIGHT, height);
                if let Some(color) = color {
                    target_frame.set_color_param(DRAWABLE_COLOR, color);
                }
            }
        })
        .set_draw({
            clone!(props.drawable);
            move |item, canvas| {
                let mut drawable = drawable.lock();

                let frame = item.current_frame();
                let drawable_x = frame.get_float_param(DRAWABLE_X).unwrap();
                let drawable_y = frame.get_float_param(DRAWABLE_Y).unwrap();
                let drawable_width = frame.get_float_param(DRAWABLE_WIDTH).unwrap();
                let drawable_height =
                    frame.get_float_param(DRAWABLE_HEIGHT).unwrap();
                let color = frame.get_color_param(DRAWABLE_COLOR);
                if let Some(color) = color {
                    drawable.set_color(Some(color));
                } else {
                    drawable.set_color(None);
                }

                drawable.set_width(drawable_width);
                drawable.set_height(drawable_height);
                let x = frame.x() + drawable_x;
                let y = frame.y() + drawable_y;
                drawable.draw(canvas, x, y);
            }
        })
}