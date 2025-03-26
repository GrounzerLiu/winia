use crate::impl_property_layout;
use crate::shared::{Children, Gettable, Shared, SharedBool, SharedDrawable};
use crate::ui::app::AppContext;
use crate::ui::item::{Alignment, LogicalX, MeasureMode, Orientation};
use crate::ui::Item;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use proc_macro::item;
use skia_safe::canvas::SaveLayerRec;
use skia_safe::svg::Dom;
use skia_safe::wrapper::PointerWrapper;
use skia_safe::{
    BlendMode, Canvas, Color, Data, FilterMode, FontMgr, Image as SkiaImage, MipmapMode, Paint,
    Rect, SamplingOptions,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub trait Drawable: Send {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32);
    fn get_intrinsic_width(&self) -> f32;
    fn get_intrinsic_height(&self) -> f32;
    fn set_width(&mut self, width: f32);
    fn set_height(&mut self, height: f32);
    fn width(&self) -> f32;
    fn height(&self) -> f32;
    fn set_color(&mut self, color: Option<Color>);
    fn get_color(&self) -> Option<Color>;
}

enum ImageType {
    Svg { dom: Dom },
    Raster { image: SkiaImage },
}

// `Dom` is not `Send` because [`Dom` can be neither Send nor Sync because it supports mutation (set_container_size)](https://github.com/rust-skia/rust-skia/commit/6a8ffecb6727af269d3beffb49f98e5ccf14e7d6).
// But `Svg` is `Send` because it does not mutate `Dom`.
unsafe impl Send for ImageType {}

pub struct ImageDrawable {
    image_type: Option<ImageType>,
    width: f32,
    height: f32,
    color: Option<Color>,
}

impl Drawable for ImageDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        if let Some(image_type) = &self.image_type {
            match image_type {
                ImageType::Svg { dom } => {
                    if let Some(color) = self.color {
                        let save_layer_rec = SaveLayerRec::default();
                        canvas.save_layer(&save_layer_rec);
                        canvas.translate((x, y));
                        canvas.scale((
                            self.width / dom.inner().fContainerSize.fWidth,
                            self.height / dom.inner().fContainerSize.fHeight,
                        ));
                        dom.render(canvas);
                        let mut paint = Paint::default();
                        paint.set_color(color);
                        paint.set_blend_mode(BlendMode::SrcIn);
                        canvas.draw_paint(&paint);
                        canvas.restore();
                    } else {
                        canvas.save();
                        canvas.translate((x, y));
                        canvas.scale((
                            self.width / dom.inner().fContainerSize.fWidth,
                            self.height / dom.inner().fContainerSize.fHeight,
                        ));
                        dom.render(canvas);
                        canvas.restore();
                    }
                }
                ImageType::Raster { image } => {
                    if let Some(color) = self.color {
                        let save_layer_rec = SaveLayerRec::default();
                        canvas.save_layer(&save_layer_rec);
                        let sampling_options =
                            SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
                        canvas.draw_image_rect_with_sampling_options(
                            image,
                            None,
                            Rect::from_xywh(x, y, self.width, self.height),
                            sampling_options,
                            &Paint::default(),
                        );
                        let mut paint = Paint::default();
                        paint.set_color(color);
                        paint.set_blend_mode(BlendMode::SrcIn);
                        canvas.draw_paint(&paint);
                        canvas.restore();
                    } else {
                        let sampling_options =
                            SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
                        canvas.draw_image_rect_with_sampling_options(
                            image,
                            None,
                            Rect::from_xywh(x, y, self.width, self.height),
                            sampling_options,
                            &Paint::default(),
                        );
                    }
                }
            }
        }
    }

    fn get_intrinsic_width(&self) -> f32 {
        match &self.image_type {
            Some(ImageType::Svg { dom, .. }) => dom.inner().fContainerSize.fWidth,
            Some(ImageType::Raster { image, .. }) => image.width() as f32,
            None => 0.0,
        }
    }

    fn get_intrinsic_height(&self) -> f32 {
        match &self.image_type {
            Some(ImageType::Svg { dom, .. }) => dom.inner().fContainerSize.fHeight,
            Some(ImageType::Raster { image, .. }) => image.height() as f32,
            None => 0.0,
        }
    }

    fn set_width(&mut self, width: f32) {
        self.width = width;
    }

    fn set_height(&mut self, height: f32) {
        self.height = height;
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn set_color(&mut self, color: Option<Color>) {
        self.color = color;
    }

    fn get_color(&self) -> Option<Color> {
        self.color
    }
}

impl ImageDrawable {
    pub fn from_file(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        let is_svg = if let Some(ext) = path.extension() {
            ext == "svg"
        } else {
            false
        };

        let bytes = fs::read(&path).ok()?;
        Self::from_bytes(&bytes, is_svg)
    }

    pub fn from_url(url: &PathBuf) -> Option<Self> {
        let response = reqwest::blocking::get(url.to_str()?).ok()?;
        let binding = response.bytes().ok()?;
        let bytes = binding.as_ref();
        let is_svg = if let Some(ext) = url.extension() {
            ext == "svg"
        } else {
            false
        };
        Self::from_bytes(bytes, is_svg)
    }

    pub async fn from_url_async(url: &PathBuf) -> Option<Self> {
        let response = reqwest::get(url.to_str()?).await.ok()?;
        let bytes = response.bytes().await.ok()?;
        let is_svg = if let Some(ext) = url.extension() {
            ext == "svg"
        } else {
            false
        };
        Self::from_bytes(&bytes, is_svg)
    }

    fn image_type_from_bytes(bytes: &[u8], is_svg: bool) -> Option<ImageType> {
        if is_svg {
            let font_mgr = FontMgr::new();
            let dom = Dom::from_bytes(&Data::new_copy(bytes), font_mgr).ok()?;
            Some(ImageType::Svg { dom })
        } else {
            let image = SkiaImage::from_encoded(Data::new_copy(bytes))?;
            Some(ImageType::Raster { image })
        }
    }

    pub fn from_bytes(bytes: &[u8], is_svg: bool) -> Option<Self> {
        let image_type = Self::image_type_from_bytes(bytes, is_svg)?;
        let width = match &image_type {
            ImageType::Svg { dom, .. } => dom.inner().fContainerSize.fWidth,
            ImageType::Raster { image, .. } => image.width() as f32,
        };
        let height = match &image_type {
            ImageType::Svg { dom, .. } => dom.inner().fContainerSize.fHeight,
            ImageType::Raster { image, .. } => image.height() as f32,
        };
        Some(Self {
            image_type: Some(image_type),
            width,
            height,
            color: None,
        })
    }
}

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
struct ImageProperty {
    drawable: SharedDrawable,
    /// whether the image should be scaled with the dpi when there is no specific size set
    dpi_sensitive: SharedBool,
    /// the scale mode when the image is larger than the item
    oversize_scale_mode: Shared<ScaleMode>,
    /// the scale mode when the image is smaller than the item
    undersize_scale_mode: Shared<ScaleMode>,
    color: Shared<Option<Color>>,
}

#[item(drawable: impl Into<SharedDrawable>)]
pub struct Image {
    item: Item,
    property: Shared<ImageProperty>,
}

impl_property_layout!(Image, dpi_sensitive, SharedBool);
impl_property_layout!(Image, oversize_scale_mode, Shared<ScaleMode>);
impl_property_layout!(Image, undersize_scale_mode, Shared<ScaleMode>);
impl_property_layout!(Image, color, Shared<Option<Color>>);

impl Image {
    pub fn new(app_context: AppContext, drawable: impl Into<SharedDrawable>) -> Self {
        let drawable = drawable.into();
        let item = Item::new(app_context.clone(), Children::new()).clip(true);
        let id = item.data().get_id();
        let event_loop_proxy = app_context.event_loop_proxy();
        let property: Shared<ImageProperty> = ImageProperty {
            drawable: drawable.layout_when_changed(&event_loop_proxy, id),
            dpi_sensitive: SharedBool::from_static(true).layout_when_changed(&event_loop_proxy, id),
            oversize_scale_mode: Shared::from_static(ScaleMode::Contain)
                .layout_when_changed(&event_loop_proxy, id),
            undersize_scale_mode: Shared::from_static(ScaleMode::Contain)
                .layout_when_changed(&event_loop_proxy, id),
            color: Shared::from_static(None).redraw_when_changed(&event_loop_proxy, id),
        }
        .into();

        item.data()
            .set_measure({
                let property = property.clone();
                move |item, width_mode, height_mode| {
                    let property = property.value();
                    let oversize_scale_mode = property.oversize_scale_mode.get();
                    let undersize_scale_mode = property.undersize_scale_mode.get();
                    let dpi_sensitive = property.dpi_sensitive.get();
                    let drawable_ = property.drawable.value();
                    let drawable = drawable_.lock();
                    let scale_factor = item.get_app_context().scale_factor();
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
                        (MeasureMode::Specified(width), MeasureMode::Unspecified(height)) => {
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
                        (MeasureMode::Unspecified(width), MeasureMode::Specified(height)) => {
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
                        (MeasureMode::Unspecified(width), MeasureMode::Unspecified(height)) => (
                            drawable_width + padding_horizontal,
                            drawable_height + padding_vertical,
                        ),
                    };

                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                }
            })
            .set_layout({
                let property = property.clone();
                move |item, width, height| {
                    let property = property.value();
                    let drawable_ = property.drawable.value();
                    let drawable = drawable_.lock();

                    let align = item.get_align_content().get();
                    let padding_start = item.get_padding_start().get().to_dp(item.get_app_context());
                    let padding_top = item.get_padding_top().get().to_dp(item.get_app_context());
                    let padding_end = item.get_padding_end().get().to_dp(item.get_app_context());
                    let padding_bottom = item.get_padding_bottom().get().to_dp(item.get_app_context());
                    let padding_horizontal = padding_start + padding_end;
                    let padding_vertical = padding_top + padding_bottom;

                    let drawable_width = drawable.get_intrinsic_width();
                    let drawable_height = drawable.get_intrinsic_height();

                    let scale_factor = item.get_app_context().scale_factor();
                    let drawable_width = drawable_width
                        / if property.dpi_sensitive.get() {
                            scale_factor
                        } else {
                            1.0
                        };
                    let drawable_height = drawable_height
                        / if property.dpi_sensitive.get() {
                            scale_factor
                        } else {
                            1.0
                        };

                    let scale_mode = if drawable_width > width - padding_horizontal
                        || drawable_height > height - padding_vertical
                    {
                        property.oversize_scale_mode.get()
                    } else {
                        property.undersize_scale_mode.get()
                    };

                    let mut x = LogicalX::new(item.get_layout_direction().get(), 0.0, width);

                    let (x, y, width, height) = match scale_mode {
                        ScaleMode::Original => match align {
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
                        },
                        ScaleMode::Stretch => {
                            x = x + padding_start;
                            let y = padding_top;
                            let width = width - padding_horizontal;
                            let height = height - padding_vertical;
                            (x.physical_value(width), y, width, height)
                        }
                        ScaleMode::Cover => {
                            let scale = {
                                let scale_x = (width - padding_horizontal) / drawable_width;
                                let scale_y = (height - padding_vertical) / drawable_height;
                                scale_x.max(scale_y)
                            };
                            let drawable_width = drawable_width * scale;
                            let drawable_height = drawable_height * scale;
                            match align {
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
                            }
                        }
                        ScaleMode::Contain => {
                            let scale = {
                                let scale_x = (width - padding_horizontal) / drawable_width;
                                let scale_y = (height - padding_vertical) / drawable_height;
                                scale_x.min(scale_y)
                            };

                            let drawable_width = drawable_width * scale;
                            let drawable_height = drawable_height * scale;
                            match align {
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
                            }
                        }
                    };

                    let color = property.color.get();

                    let target_parameter = item.get_target_parameter();
                    target_parameter.set_float_param(DRAWABLE_X, x);
                    target_parameter.set_float_param(DRAWABLE_Y, y);
                    target_parameter.set_float_param(DRAWABLE_WIDTH, width);
                    target_parameter.set_float_param(DRAWABLE_HEIGHT, height);
                    if let Some(color) = color {
                        target_parameter.set_color_param(DRAWABLE_COLOR, color);
                    }
                }
            })
            .set_draw({
                let property = property.clone();
                move |item, canvas| {
                    let property = property.value();
                    let drawable_ = property.drawable.value();
                    let mut drawable = drawable_.lock();

                    let display_parameter = item.get_display_parameter();
                    let drawable_x = display_parameter.get_float_param(DRAWABLE_X).unwrap();
                    let drawable_y = display_parameter.get_float_param(DRAWABLE_Y).unwrap();
                    let drawable_width = display_parameter.get_float_param(DRAWABLE_WIDTH).unwrap();
                    let drawable_height =
                        display_parameter.get_float_param(DRAWABLE_HEIGHT).unwrap();
                    let color = display_parameter.get_color_param(DRAWABLE_COLOR);
                    if let Some(color) = color {
                        drawable.set_color(Some(color));
                    } else {
                        drawable.set_color(None);
                    }

                    drawable.set_width(drawable_width);
                    drawable.set_height(drawable_height);
                    let x = display_parameter.x() + drawable_x;
                    let y = display_parameter.y() + drawable_y;
                    drawable.draw(canvas, x, y);
                }
            });

        Self { item, property }
    }
}
