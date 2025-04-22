use crate::shared::Shared;
use crate::ui::component::{Drawable, ImageDrawable};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub type SharedDrawable = Shared<Box<dyn Drawable>>;

impl SharedDrawable {

    pub fn from_file(file_path: impl Into<PathBuf>) -> Option<Self> {
        ImageDrawable::from_file(file_path).map(|drawable| {
            let drawable: Box<dyn Drawable> = Box::new(drawable);
            Shared::from(drawable)
        })
    }

    pub fn from_file_async(file_path: impl Into<PathBuf> + Send + 'static) -> Self {
        Shared::from_async(
            async move {
                let image = ImageDrawable::from_file_async(file_path).await;
                let drawable: Box<dyn Drawable> = if let Some(image) = image {
                    Box::new(image)
                } else {
                    Box::new(ImageDrawable::empty())
                };
                drawable
            },
            {
                let drawable: Box<dyn Drawable> = Box::new(ImageDrawable::empty());
                drawable
            }
        )
    }

    pub fn from_url(url: impl Into<PathBuf>) -> Option<Self> {
        ImageDrawable::from_url(url).map(|drawable| {
            let drawable: Box<dyn Drawable> = Box::new(drawable);
            Shared::from(drawable)
        })
    }

    pub fn from_url_async(url: impl Into<PathBuf> + Send + 'static) -> Self {
        Shared::from_async(
            async move {
                let image = ImageDrawable::from_url_async(url).await;
                let drawable: Box<dyn Drawable> = if let Some(image) = image {
                    Box::new(image)
                } else {
                    Box::new(ImageDrawable::empty())
                };
                drawable
            },
            {
                let drawable: Box<dyn Drawable> = Box::new(ImageDrawable::empty());
                drawable
            }
        )
    }
    
    pub fn empty() -> Self {
        let drawable: Box<dyn Drawable> = Box::new(ImageDrawable::empty());
        Shared::from(drawable)
    }
}

impl From<&str> for SharedDrawable {
    fn from(url: &str) -> Self {
        if url.starts_with("http") {
            SharedDrawable::from_url_async(url.to_string())
        } else {
            SharedDrawable::from_file_async(url.to_string())
        }
    }
}
