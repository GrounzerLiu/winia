/*use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;
use skia_safe::{BlendMode, Canvas, Color, Data, FilterMode, FontMgr, Image as SkiaImage, MipmapMode, Paint, Rect, SamplingOptions};
use skia_safe::canvas::SaveLayerRec;
use skia_safe::svg::Dom;
use skia_safe::wrapper::PointerWrapper;
use crate::shared::{BoolProperty, Gettable, Property};
use crate::ui::app::{AppContext, UserEvent};
use crate::ui::Item;
use crate::ui::item::{DisplayParameter, ItemEvent, MeasureMode, Orientation};

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
    image: SkiaImage,
    width: f32,
    height: f32,
}

impl ImageDrawable {
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = Data::new_copy(fs::read(path).unwrap().as_slice());
        let image = SkiaImage::from_encoded(data).unwrap();
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
        let image = SkiaImage::from_encoded(data).unwrap();
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
    image: Arc<Mutex<Option<ImageDrawable>>>,
}

impl NetworkImage {
    pub fn from_url(url: &PathBuf, app: &AppContext) -> Self {
        let image = Arc::new(Mutex::new(None));
        let image_clone = image.clone();
        let url = url.clone();
        let app_proxy = app.event_loop_proxy.clone();
        std::thread::spawn(move || {
            let response = reqwest::blocking::get(url.to_str().unwrap());
            if let Ok(response) = response {
                let bytes = response.bytes();
                if let Ok(bytes) = bytes {
                    let bytes = bytes.as_ref();
                    let image = ImageDrawable::from_bytes(bytes);
                    let mut image_guard = image_clone.lock().unwrap();
                    *image_guard = Some(image);
                    let app_proxy = app_proxy.lock().unwrap();
                    if let Some(app_proxy) = app_proxy.as_ref() {
                        app_proxy.send_event(UserEvent::ReLayout);
                    }
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
        if let Some(image) = self.image.lock().unwrap().as_ref() {
            image.draw(canvas, x, y);
        }
    }

    fn get_intrinsic_width(&self) -> f32 {
        if let Some(image) = self.image.lock().unwrap().as_ref() {
            image.get_intrinsic_width()
        } else {
            0.0
        }
    }

    fn get_intrinsic_height(&self) -> f32 {
        if let Some(image) = self.image.lock().unwrap().as_ref() {
            image.get_intrinsic_height()
        } else {
            0.0
        }
    }

    fn set_width(&mut self, width: f32) {
        if let Some(image) = self.image.lock().unwrap().as_mut() {
            image.set_width(width);
        }
    }

    fn set_height(&mut self, height: f32) {
        if let Some(image) = self.image.lock().unwrap().as_mut() {
            image.set_height(height);
        }
    }

    fn width(&self) -> f32 {
        if let Some(image) = self.image.lock().unwrap().as_ref() {
            image.width()
        } else {
            0.0
        }
    }

    fn height(&self) -> f32 {
        if let Some(image) = self.image.lock().unwrap().as_ref() {
            image.height()
        } else {
            0.0
        }
    }
}

type DrawableImpl = Arc<Mutex<Box<dyn Drawable>>>;

lazy_static!(
    static ref DRAWABLES: Mutex<HashMap<PathBuf, DrawableImpl>> = Mutex::new(HashMap::new());
);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    NoScale,
    FitLongerSide,
    FitShorterSide,
    FitBothSides,
}

struct ImageProperties {
    drawable: Property<Option<DrawableImpl>>,
    /// whether the image should be scaled with the dpi when there is no specific size set
    dpi_sensitive: BoolProperty,
    /// the scale mode when the image is larger than the item
    oversize_scale_mode: Property<ScaleMode>,
    /// the scale mode when the image is smaller than the item
    undersize_scale_mode: Property<ScaleMode>,
}

impl ImageProperties {
    fn drawable(&self) -> DrawableImpl {
        let drawable = self.drawable.value().as_ref().unwrap().clone();
        drawable
    }
}

// pub struct Image {
//     item: Item,
//     properties: Arc<Mutex<ImageProperties>>,
// }
// 
// impl Image {
//     pub fn new(app: AppContext) -> Self {
//         let properties = Arc::new(Mutex::new(ImageProperties {
//             drawable: None.into(),
//             dpi_sensitive: true.into(),
//             oversize_scale_mode: ScaleMode::FitLongerSide.into(),
//             undersize_scale_mode: ScaleMode::FitLongerSide.into(),
//         }));
//         
//         fn drawable_intrinsic_size(drawable: &dyn Drawable, orientation: Orientation) -> f32 {
//             match orientation {
//                 Orientation::Horizontal => drawable.get_intrinsic_width(),
//                 Orientation::Vertical => drawable.get_intrinsic_height(),
//             }
//         }
//         
//         fn insert_size(display_parameter: &mut DisplayParameter, orientation: Orientation, size: f32){
//             match orientation {
//                 Orientation::Vertical => {
//                     display_parameter.float_params.insert("drawable_width".to_string(), size);
//                 }
//                 Orientation::Horizontal => {
//                     display_parameter.float_params.insert("drawable_height".to_string(), size);
//                 }
//             }
//         }
// 
//         let item_event = ItemEvent::new()
//             .measure({
//                 let properties = properties.clone();
//                 move |item, orientation, mode| {
//                     
//                     let min_size = item.get_min_size(orientation);
//                     let max_size = item.get_max_size(orientation);
//                     let measure_parameter = item.get_measure_parameter();
//                     match mode {
//                         MeasureMode::Specified(size) => {
//                             measure_parameter.set_size(orientation, size.clamp(min_size, max_size));
//                         }
//                         MeasureMode::Unspecified(_) => {
//                             measure_parameter.set_size(orientation, min_size);
//                         }
//                     }
//                 }
//             })
//             .draw({
//                 let properties = properties.clone();
//                 move |item, canvas| {
//                     let display_parameter = item.get_display_parameter().clone();
//                     let drawable_x = display_parameter.float_params["drawable_x"];
//                     let drawable_y = display_parameter.float_params["drawable_y"];
//                     let drawable_width = display_parameter.float_params["drawable_width"];
//                     let drawable_height = display_parameter.float_params["drawable_height"];
//                     let parent_x = display_parameter.parent_x;
//                     let parent_y = display_parameter.parent_y;
//                     
//                     let properties_guard = properties.lock().unwrap();
//                     let drawable = &properties_guard.drawable;
//                     if let Some(drawable) = drawable.value().as_ref(){
//                         let drawable_guard = drawable.lock().unwrap();
//                         drawable_guard.as_ref().set_width(drawable_width);
//                         drawable_guard.as_ref().set_height(drawable_height);
//                         drawable_guard.draw(canvas, parent_x + drawable_x, parent_y + drawable_y);
//                     }
//                 }
//             });
// 
//         Self {}
//     }
// }*/