use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::Mutex;
use lazy_static::lazy_static;
use skia_safe::{BlendMode, Canvas, Color, Data, FontMgr, M44, Matrix, Paint, Rect, SamplingOptions};
use skia_safe::canvas::SaveLayerRec;
use crate::ui::{Gravity, Item, ItemEvent, LayoutDirection, MeasureMode};
use skia_safe::Image as SkImage;
use skia_safe::svg::Dom;
use skia_safe::wrapper::PointerWrapper;
use crate::app::{SharedApp, UserEvent};
use crate::{FilterMode, MipmapMode};
use crate::property::{BoolProperty, Gettable, SharedProperty};

pub trait Drawable: Sync + Send {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32);
    fn get_intrinsic_width(&self) -> f32;
    fn get_intrinsic_height(&self) -> f32;
    fn set_width(&mut self, width: f32);
    fn set_height(&mut self, height: f32);
    fn width(&self) -> f32;
    fn height(&self) -> f32;
}

pub struct Svg {
    dom: Dom,
    width: f32,
    height: f32,
    color: Option<Color>,
}

impl Svg {
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = Data::new_copy(fs::read(&path).unwrap().as_slice());
        let font_mgr = FontMgr::new();
        let dom = Dom::from_bytes(&data, &font_mgr).unwrap();
        let width = dom.inner().fContainerSize.fWidth;
        let height = dom.inner().fContainerSize.fHeight;
        Self {
            dom,
            width,
            height,
            color: None,
        }
    }

    pub fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        if let Some(color) = self.color {
            let save_layer_rec = SaveLayerRec::default();
            canvas.save_layer(&save_layer_rec);
            canvas.translate((x, y));
            canvas.scale((self.width / self.get_intrinsic_width(), self.height / self.get_intrinsic_height()));
            self.dom.render(canvas);
            let mut paint = Paint::default();
            paint.set_color(color);
            paint.set_blend_mode(BlendMode::SrcIn);
            canvas.draw_paint(&paint);
            canvas.restore();
        } else {
            canvas.save();
            canvas.translate((x, y));
            canvas.scale((self.width / self.get_intrinsic_width(), self.height / self.get_intrinsic_height()));
            self.dom.render(canvas);
            canvas.restore();
        }
    }

    pub fn set_color(&mut self, color: Option<Color>) {
        self.color = color;
    }

    pub fn get_color(&self) -> Option<Color> {
        self.color
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }

    pub fn set_height(&mut self, height: f32) {
        self.height = height;
    }

    pub fn get_intrinsic_width(&self) -> f32 {
        self.dom.inner().fContainerSize.fWidth
    }

    pub fn get_intrinsic_height(&self) -> f32 {
        self.dom.inner().fContainerSize.fHeight
    }
}

impl Drawable for Svg {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        self.draw(canvas, x, y);
    }

    fn get_intrinsic_width(&self) -> f32 {
        self.get_intrinsic_width()
    }

    fn get_intrinsic_height(&self) -> f32 {
        self.get_intrinsic_height()
    }

    fn set_width(&mut self, width: f32) {
        self.set_width(width);
    }

    fn set_height(&mut self, height: f32) {
        self.set_height(height);
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }
}

pub struct ImageDrawable {
    image: SkImage,
    width: f32,
    height: f32,
}

impl ImageDrawable {
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = Data::new_copy(fs::read(&path).unwrap().as_slice());
        let image = SkImage::from_encoded(data).unwrap();
        let width = image.width() as f32;
        let height = image.height() as f32;
        Self {
            image,
            width,
            height,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let data = Data::new_copy(bytes);
        let image = SkImage::from_encoded(data).unwrap();
        let width = image.width() as f32;
        let height = image.height() as f32;
        Self {
            image,
            width,
            height,
        }
    }
}

impl Drawable for ImageDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        let sampling_options = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
        canvas.draw_image_rect_with_sampling_options(
            &self.image,
            None,
            Rect::from_xywh(x, y, self.width, self.height),
            sampling_options,
            &Paint::default(),
        );
    }

    fn get_intrinsic_width(&self) -> f32 {
        self.image.width() as f32
    }

    fn get_intrinsic_height(&self) -> f32 {
        self.image.height() as f32
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
}

pub struct NetworkImage {
    image: Arc<RwLock<Option<ImageDrawable>>>,
}

impl NetworkImage {
    pub fn from_url(url: &PathBuf, app: &SharedApp) -> Self {
        let image = Arc::new(RwLock::new(None));
        let image_clone = image.clone();
        let url = url.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            let response = reqwest::blocking::get(url.to_str().unwrap());
            if let Ok(response) = response {
                let bytes = response.bytes();
                if let Ok(bytes) = bytes {
                    let bytes = bytes.as_ref();
                    let image = ImageDrawable::from_bytes(bytes);
                    let mut image_guard = image_clone.write().unwrap();
                    *image_guard = Some(image);
                    // app.request_layout();
                    // app.send_event(UserEvent::Empty);
                    //TODO: Maybe request_layout() not working outside the main thread.
                }
            }
        });
        Self {
            image,
        }
    }
}

impl Drawable for NetworkImage {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        if let Some(image) = self.image.read().unwrap().as_ref() {
            image.draw(canvas, x, y);
        }
    }

    fn get_intrinsic_width(&self) -> f32 {
        if let Some(image) = self.image.read().unwrap().as_ref() {
            image.get_intrinsic_width()
        } else {
            0.0
        }
    }

    fn get_intrinsic_height(&self) -> f32 {
        if let Some(image) = self.image.read().unwrap().as_ref() {
            image.get_intrinsic_height()
        } else {
            0.0
        }
    }

    fn set_width(&mut self, width: f32) {
        if let Some(image) = self.image.write().unwrap().as_mut() {
            image.set_width(width);
        }
    }

    fn set_height(&mut self, height: f32) {
        if let Some(image) = self.image.write().unwrap().as_mut() {
            image.set_height(height);
        }
    }

    fn width(&self) -> f32 {
        if let Some(image) = self.image.read().unwrap().as_ref() {
            image.width()
        } else {
            0.0
        }
    }

    fn height(&self) -> f32 {
        if let Some(image) = self.image.read().unwrap().as_ref() {
            image.height()
        } else {
            0.0
        }
    }
}


type DrawableImpl = Box<dyn Drawable>;

lazy_static!(
    static ref DRAWABLES: Mutex<HashMap<PathBuf, Arc<RwLock<DrawableImpl>>>> = Mutex::new(HashMap::new());
);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    NoScale,
    FitLongerSide,
    FitShorterSide,
    FitBothSides,
}

struct ImageProperties {
    image: SharedProperty<Option<Arc<RwLock<DrawableImpl>>>>,
    /// whether the image should be scaled with the dpi when there is no specific size set
    dpi_sensitive: BoolProperty,
    /// the scale mode when the image is larger than the item
    oversize_scale_mode: SharedProperty<ScaleMode>,
    /// the scale mode when the image is smaller than the item
    undersize_scale_mode: SharedProperty<ScaleMode>,
}

pub struct Image {
    item: Item,
    properties: Arc<Mutex<ImageProperties>>,
}

impl Image {
    pub fn new(app: SharedApp) -> Self {
        let properties = Arc::new(Mutex::new(ImageProperties {
            image: None.into(),
            dpi_sensitive: true.into(),
            oversize_scale_mode: ScaleMode::FitLongerSide.into(),
            undersize_scale_mode: ScaleMode::FitLongerSide.into(),
        }));
        let mut item = Item::new(
            app,
            ItemEvent::default()
                .set_on_draw(
                    {
                        let properties = properties.clone();
                        move |item, canvas| {
                            let layout_params = item.get_layout_params();
                            let mut x = layout_params.x;
                            let mut y = layout_params.y;
                            let width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                            let height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;

                            if item.get_enable_clipping().get() {
                                canvas.save();
                                match item.get_layout_direction().get() {
                                    LayoutDirection::LeftToRight => {
                                        x += layout_params.padding_start;
                                    }
                                    LayoutDirection::RightToLeft => {
                                        x += layout_params.padding_end;
                                    }
                                }
                                y += layout_params.padding_top;
                                canvas.clip_rect(Rect::from_xywh(x, y, width, height), None, None);
                            }

                            if let Some(background) = item.get_background().lock().as_mut() {
                                background.draw(canvas);
                            }

                            let properties = properties.lock().unwrap();
                            let image = &properties.image;
                            if let Some(drawable) = image.lock().as_ref() {
                                let mut x = 0.0;
                                let mut y = 0.0;

                                match item.get_vertical_gravity().get() {
                                    Gravity::Start => {
                                        match item.get_layout_direction().get() {
                                            LayoutDirection::LeftToRight => {
                                                x = layout_params.x + layout_params.padding_start;
                                            }
                                            LayoutDirection::RightToLeft => {
                                                x = layout_params.x + layout_params.width - layout_params.padding_end - drawable.read().unwrap().width();
                                            }
                                        }
                                    }
                                    Gravity::Center => {
                                        x = layout_params.x + (layout_params.width - drawable.read().unwrap().width()) / 2.0;
                                    }
                                    Gravity::End => {
                                        match item.get_layout_direction().get() {
                                            LayoutDirection::LeftToRight => {
                                                x = layout_params.x + layout_params.width - layout_params.padding_end - drawable.read().unwrap().width();
                                            }
                                            LayoutDirection::RightToLeft => {
                                                x = layout_params.x + layout_params.padding_start;
                                            }
                                        }
                                    }
                                }

                                match item.get_horizontal_gravity().get() {
                                    Gravity::Start => {
                                        y = layout_params.y + layout_params.padding_top;
                                    }
                                    Gravity::Center => {
                                        y = layout_params.y + (layout_params.height - drawable.read().unwrap().height()) / 2.0;
                                    }
                                    Gravity::End => {
                                        y = layout_params.y + layout_params.height - layout_params.padding_bottom - drawable.read().unwrap().height();
                                    }
                                }

                                if properties.dpi_sensitive.get() {
                                    drawable.read().unwrap().draw(canvas, x, y);
                                } else {
                                    canvas.save();
                                    canvas.translate((x, y));
                                    let scale_factor = item.get_app().scale_factor();
                                    canvas.scale((1.0 / scale_factor, 1.0 / scale_factor));
                                    drawable.read().unwrap().draw(canvas, 0.0, 0.0);
                                    canvas.restore();
                                }
                            };

                            if let Some(foreground) = item.get_foreground().lock().as_mut() {
                                foreground.draw(canvas);
                            }

                            if item.get_enable_clipping().get() {
                                canvas.restore();
                            }
                        }
                    }
                )
                .set_on_measure(
                    {
                        let properties = properties.clone();
                        move |item, width_measure_mode, height_measure_mode| {
                            let mut layout_params = item.get_layout_params().clone();
                            layout_params.init_from_item(item);
                            let min_width = layout_params.min_width;
                            let min_height = layout_params.min_height;
                            let max_width = layout_params.max_width;
                            let max_height = layout_params.max_height;

                            let properties_guard = properties.lock().unwrap();

                            let image = &properties_guard.image;


                            if let Some(image) = image.lock().as_ref() {
                                let mut image_width = 0.0;
                                let mut image_height = 0.0;

                                if properties_guard.dpi_sensitive.get() {
                                    image_width = image.read().unwrap().width();
                                    image_height = image.read().unwrap().height();
                                } else {
                                    image_width = image.read().unwrap().width() / item.get_app().scale_factor();
                                    image_height = image.read().unwrap().height() / item.get_app().scale_factor();
                                }

                                match width_measure_mode {
                                    MeasureMode::Specified(width) => {
                                        match height_measure_mode {
                                            MeasureMode::Specified(height) => {
                                                layout_params.width = width.clamp(min_width, max_width);
                                                layout_params.height = height.clamp(min_height, max_height);
                                                let is_undersize = image_width < layout_params.width && image_height < layout_params.height;

                                                let scale_mode = if is_undersize {
                                                    properties_guard.undersize_scale_mode.get()
                                                } else {
                                                    properties_guard.oversize_scale_mode.get()
                                                };

                                                match scale_mode {
                                                    ScaleMode::NoScale => {}
                                                    ScaleMode::FitLongerSide => {
                                                        if layout_params.width > layout_params.height {
                                                            let expected_image_width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                                                            let scale = expected_image_width / image_width;
                                                            image.write().unwrap().set_width(expected_image_width);
                                                            image.write().unwrap().set_height(image_height * scale);
                                                        } else {
                                                            let expected_image_height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;
                                                            let scale = expected_image_height / image_height;
                                                            image.write().unwrap().set_height(expected_image_height);
                                                            image.write().unwrap().set_width(image_width * scale);
                                                        }
                                                    }
                                                    ScaleMode::FitShorterSide => {
                                                        if layout_params.width < layout_params.height {
                                                            let expected_image_width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                                                            let scale = expected_image_width / image_width;
                                                            image.write().unwrap().set_width(expected_image_width);
                                                            image.write().unwrap().set_height(image_height * scale);
                                                        } else {
                                                            let expected_image_height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;
                                                            let scale = expected_image_height / image_height;
                                                            image.write().unwrap().set_height(expected_image_height);
                                                            image.write().unwrap().set_width(image_width * scale);
                                                        }
                                                    }
                                                    ScaleMode::FitBothSides => {
                                                        let expected_image_width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                                                        let expected_image_height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;
                                                        image.write().unwrap().set_width(expected_image_width);
                                                        image.write().unwrap().set_height(expected_image_height);
                                                    }
                                                }
                                            }
                                            MeasureMode::Unspecified(height) => {
                                                layout_params.width = width.clamp(min_width, max_width);

                                                let is_undersize = image_width < layout_params.width && image_height < layout_params.height;
                                                let scale_mode = if is_undersize {
                                                    properties_guard.undersize_scale_mode.get()
                                                } else {
                                                    properties_guard.oversize_scale_mode.get()
                                                };

                                                match scale_mode {
                                                    ScaleMode::NoScale => {
                                                        layout_params.height = image_height.clamp(min_height, height);
                                                    }
                                                    _ => {
                                                        let expected_image_width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                                                        let scale = expected_image_width / image_width;
                                                        layout_params.height = image_height * scale;
                                                        image.write().unwrap().set_width(expected_image_width);
                                                        image.write().unwrap().set_height(layout_params.height);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    MeasureMode::Unspecified(width) => {
                                        match height_measure_mode {
                                            MeasureMode::Specified(height) => {
                                                layout_params.height = height.clamp(min_height, max_height);

                                                let is_undersize = image_width < layout_params.width && image_height < layout_params.height;
                                                let scale_mode = if is_undersize {
                                                    properties_guard.undersize_scale_mode.get()
                                                } else {
                                                    properties_guard.oversize_scale_mode.get()
                                                };

                                                match scale_mode {
                                                    ScaleMode::NoScale => {
                                                        layout_params.width = image_width.clamp(min_width, width);
                                                    }
                                                    _ => {
                                                        let expected_image_height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;
                                                        let scale = expected_image_height / image_height;
                                                        layout_params.width = image_width * scale;
                                                        image.write().unwrap().set_width(layout_params.width);
                                                        image.write().unwrap().set_height(expected_image_height);
                                                    }
                                                }
                                            }
                                            MeasureMode::Unspecified(height) => {
                                                let is_undersize = image_width < layout_params.width && image_height < layout_params.height;
                                                let scale_mode = if is_undersize {
                                                    properties_guard.undersize_scale_mode.get()
                                                } else {
                                                    properties_guard.oversize_scale_mode.get()
                                                };

                                                match scale_mode {
                                                    ScaleMode::NoScale => {
                                                        layout_params.width = image_width.clamp(min_width, width);
                                                        layout_params.height = image_height.clamp(min_height, height);
                                                    }
                                                    _ => {
                                                        let expected_image_width = (image_width + layout_params.padding_start + layout_params.padding_end).clamp(min_width, max_width);
                                                        let expected_image_height = (image_height + layout_params.padding_top + layout_params.padding_bottom).clamp(min_height, max_height);
                                                        image.write().unwrap().set_width(expected_image_width);
                                                        image.write().unwrap().set_height(expected_image_height);
                                                        layout_params.width = expected_image_width;
                                                        layout_params.height = expected_image_height;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(background) = item.get_background().lock().as_mut() {
                                background.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                            }

                            if let Some(foreground) = item.get_foreground().lock().as_mut() {
                                foreground.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                            }

                            item.set_layout_params(&layout_params);
                        }
                    }
                )
        );
        Self {
            item,
            properties,
        }
    }

    pub fn source(mut self, source: impl Into<PathBuf>) -> Self {
        let source = source.into();
        let mut drawables = DRAWABLES.lock().unwrap();
        if let Some(drawable) = drawables.get(&source) {
            let mut properties = self.properties.lock().unwrap();
            properties.image.set_value(Some(drawable.clone()));
        } else {
            if source.starts_with("http://") || source.starts_with("https://") {
                let drawable = NetworkImage::from_url(&source, &self.item.get_app());
                let drawable = Arc::new(RwLock::new(Box::new(drawable) as DrawableImpl));
                drawables.insert(source.clone(), drawable.clone());
                let mut properties = self.properties.lock().unwrap();
                properties.image.set_value(Some(drawable.clone()));
            } else if let Some(extension) = source.extension() {
                if extension == "svg" {
                    let drawable = Svg::from_file(source.clone());
                    let drawable = Arc::new(RwLock::new(Box::new(drawable) as DrawableImpl));
                    drawables.insert(source.clone(), drawable.clone());
                    let mut properties = self.properties.lock().unwrap();
                    properties.image.set_value(Some(drawable));
                } else {
                    let drawable = ImageDrawable::from_file(source.clone());
                    let drawable = Arc::new(RwLock::new(Box::new(drawable) as DrawableImpl));
                    drawables.insert(source.clone(), drawable.clone());
                    let mut properties = self.properties.lock().unwrap();
                    properties.image.set_value(Some(drawable));
                }
            }
        }
        self
    }

    pub fn item(self) -> Item {
        self.item
    }
}