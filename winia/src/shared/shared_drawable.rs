use crate::shared::Shared;
use crate::ui::component::{Drawable, ImageDrawable};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

lazy_static! {
    static ref DRAWABLES: Mutex<HashMap<PathBuf, Arc<Mutex<dyn Drawable>>>> =
        Mutex::new(HashMap::new());
}

pub type SharedDrawable = Shared<Arc<Mutex<dyn Drawable>>>;

impl SharedDrawable {
    fn from_image_drawable(path: &PathBuf, drawable: Option<ImageDrawable>) -> Option<Self> {
        if let Some(drawable) = drawable {
            let drawable = Arc::new(Mutex::new(drawable));
            let mut drawables = DRAWABLES.lock();
            drawables.insert(path.clone(), drawable.clone());
            Some(Self::from_static(drawable.clone()))
        } else {
            None
        }
    }

    pub fn from_file(file_path: impl Into<PathBuf>) -> Option<Self> {
        let file_path = file_path.into();
        let drawable = {
            let drawables = DRAWABLES.lock();
            let drawable = drawables.get(&file_path);
            drawable.map(|drawable| drawable.clone())
        };
        if let Some(drawable) = drawable {
            return Some(Self::from_static(drawable.clone()));
        }

        let drawable = ImageDrawable::from_file(file_path.clone());
        Self::from_image_drawable(&file_path, drawable)
    }

    pub fn from_url(url: impl Into<PathBuf>) -> Option<Self> {
        let url = url.into();
        let drawable = {
            let drawables = DRAWABLES.lock();
            let drawable = drawables.get(&url);
            drawable.map(|drawable| drawable.clone())
        };
        if let Some(drawable) = drawable {
            return Some(Self::from_static(drawable.clone()));
        }

        let drawable = ImageDrawable::from_url(&url);
        Self::from_image_drawable(&url, drawable)
    }

    pub async fn from_url_async(url: impl Into<PathBuf>) -> Option<Self> {
        let url = url.into();
        let drawable = {
            let drawables = DRAWABLES.lock();
            let drawable = drawables.get(&url);
            drawable.map(|drawable| drawable.clone())
        };
        if let Some(drawable) = drawable {
            return Some(Self::from_static(drawable.clone()));
        }

        let drawable = ImageDrawable::from_url_async(&url).await;
        Self::from_image_drawable(&url, drawable)
    }
}

impl From<&str> for SharedDrawable {
    fn from(url: &str) -> Self {
        if url.starts_with("http") {
            SharedDrawable::from_url(url)
        } else {
            SharedDrawable::from_file(url)
        }
        .unwrap()
    }
}
