use std::collections::HashMap;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use skia_safe::{BlendMode, Canvas, Data, FilterMode, FontMgr, Image, MipmapMode, Paint, Rect, SamplingOptions};
use skia_safe::canvas::SaveLayerRec;
use skia_safe::svg::Dom;
use skia_safe::wrapper::PointerWrapper;
use crate::ui::{Color, SetColor};

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
    fn add_redraw_requester(&mut self, id: u32, redraw_requester: Box<dyn Fn() + Send>);
    fn remove_redraw_requester(&mut self, id: u32);
    fn request_redraw(&self);
    fn clone_drawable(&self) -> Box<dyn Drawable>;
    fn is_empty(&self) -> bool {
        self.get_intrinsic_width() == 0.0 && self.get_intrinsic_height() == 0.0
    }
}

enum ImageType {
    Svg { dom: Dom },
    Raster { image: Image },
}

// `Dom` is not `Send` because [`Dom` can be neither Send nor Sync because it supports mutation (set_container_size)](https://github.com/rust-skia/rust-skia/commit/6a8ffecb6727af269d3beffb49f98e5ccf14e7d6).
// But `Svg` is `Send` because it does not mutate `Dom`.
unsafe impl Send for ImageType {}

lazy_static! {
    static ref DRAWABLES: Mutex<HashMap<PathBuf, ImageDrawable>> = Mutex::new(HashMap::new());
}

#[derive(Clone)]
pub struct ImageDrawable {
    redraw_requester: Arc<Mutex<HashMap<u32, Box<dyn Fn() + Send>>>>,
    image_type: Arc<Mutex<Option<ImageType>>>,
    width: f32,
    height: f32,
    color: Option<Color>,
}

impl Drawable for ImageDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        let image_type = self.image_type.lock();
        if let Some(image_type) = image_type.deref() {
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
                        paint.set_anti_alias(true);
                        paint.set_any_color(color);
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
                        paint.set_anti_alias(true);
                        paint.set_any_color(color);
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
        let image_type = self.image_type.lock();
        match image_type.deref() {
            Some(ImageType::Svg { dom, .. }) => dom.inner().fContainerSize.fWidth,
            Some(ImageType::Raster { image, .. }) => image.width() as f32,
            None => 0.0,
        }
    }

    fn get_intrinsic_height(&self) -> f32 {
        let image_type = self.image_type.lock();
        match image_type.deref() {
            Some(ImageType::Svg { dom, .. }) => dom.inner().fContainerSize.fHeight,
            Some(ImageType::Raster { image, .. }) => image.height() as f32,
            None => 0.0,
        }
    }

    fn set_width(&mut self, width: f32) {
        self.width = width;
        self.request_redraw();
    }

    fn set_height(&mut self, height: f32) {
        self.height = height;
        self.request_redraw();
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn set_color(&mut self, color: Option<Color>) {
        self.color = color;
        self.request_redraw();
    }

    fn get_color(&self) -> Option<Color> {
        self.color
    }

    fn add_redraw_requester(&mut self, id: u32, redraw_requester: Box<dyn Fn() + Send>) {
        let mut map = self.redraw_requester.lock();
        map.insert(id, redraw_requester);
    }

    fn remove_redraw_requester(&mut self, id: u32) {
        let mut map = self.redraw_requester.lock();
        map.remove(&id);
    }

    fn request_redraw(&self) {
        let map = self.redraw_requester.lock();
        for (_, redraw_requester) in map.deref() {
            redraw_requester();
        }
    }

    fn clone_drawable(&self) -> Box<dyn Drawable> {
        Box::new(ImageDrawable {
            redraw_requester: self.redraw_requester.clone(),
            image_type: self.image_type.clone(),
            width: self.width,
            height: self.height,
            color: self.color,
        })
    }
}

impl From<&ImageDrawable> for Box<dyn Drawable> {
    fn from(value: &ImageDrawable) -> Self {
        value.clone_drawable()
    }
}

impl From<ImageDrawable> for Box<dyn Drawable> {
    fn from(value: ImageDrawable) -> Self {
        value.clone_drawable()
    }
}

impl ImageDrawable {
    pub fn empty() -> Self {
        Self {
            redraw_requester: Arc::new(Mutex::new(HashMap::new())),
            image_type: Arc::new(Mutex::new(None)),
            width: 0.0,
            height: 0.0,
            color: None,
        }
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        if DRAWABLES.lock().contains_key(&path) {
            return DRAWABLES.lock().get(&path).cloned();
        }
        let is_svg = if let Some(ext) = path.extension() {
            ext == "svg"
        } else {
            false
        };

        let bytes = fs::read(&path).ok()?;
        Self::from_bytes(&bytes, is_svg)
    }

    pub async fn from_file_async(path: impl Into<PathBuf>) -> Option<Self> {
        let path = path.into();
        if DRAWABLES.lock().contains_key(&path) {
            return DRAWABLES.lock().get(&path).cloned();
        }
        let is_svg = if let Some(ext) = path.extension() {
            ext == "svg"
        } else {
            false
        };
        let bytes = tokio::fs::read(&path).await.ok()?;
        Self::from_bytes(&bytes, is_svg)
    }

    pub fn from_url(url: impl Into<PathBuf>) -> Option<Self> {
        let url = url.into();
        if DRAWABLES.lock().contains_key(&url) {
            return DRAWABLES.lock().get(&url).cloned();
        }
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

    pub async fn from_url_async(url: impl Into<PathBuf>) -> Option<Self> {
        let url = url.into();
        if DRAWABLES.lock().contains_key(&url) {
            return DRAWABLES.lock().get(&url).cloned();
        }
        let response = reqwest::get(url.to_str()?).await;
        let response = match response {
            Ok(response) => response,
            Err(e) => {
                println!("Error: {:?}", e);
                return None;
            }
        };
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
            let image = Image::from_encoded(Data::new_copy(bytes))?;
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
            redraw_requester: Arc::new(Mutex::new(HashMap::new())),
            image_type: Arc::new(Mutex::new(Some(image_type))),
            width,
            height,
            color: None,
        })
    }
}