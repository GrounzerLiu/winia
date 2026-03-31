use std::path::PathBuf;
use crate::drawable::{Drawable, ImageDrawable};
use crate::icon::{IconDrawable, MaterialSymbol};
use crate::shared::{Shared, SharedDerived, SharedSource};
use crate::ui::Color;

pub type SharedDrawable = SharedSource<Box<dyn Drawable>>;
pub type SharedDerivedDrawable = SharedDerived<Box<dyn Drawable>>;

impl SharedDrawable {
    pub fn from_file(path: impl Into<PathBuf>) -> Option<Self> {
        ImageDrawable::from_file(path).map(|drawable| {
            let drawable_box: Box<dyn Drawable> = Box::new(drawable);
            SharedSource::new(drawable_box)
        })
    }

    pub fn from_file_async(path: impl Into<PathBuf> + Send + 'static, default_image: Option<Box<dyn Drawable>>) -> Self {
        let default_image: Box<dyn Drawable> = match default_image {
            Some(img) => img,
            None => Box::new(ImageDrawable::empty()),
        };
        Shared::from_async(
            async move {
                let image = ImageDrawable::from_file_async(path).await;
                match image {
                    Some(img) => Some(Box::new(img) as Box<dyn Drawable>),
                    None => None,
                }
            },
            default_image
        )
    }

    pub fn from_url(url: impl Into<PathBuf>) -> Option<Self> {
        ImageDrawable::from_url(url).map(|drawable| {
            let drawable: Box<dyn Drawable> = Box::new(drawable);
            Shared::from(drawable)
        })
    }

    pub fn from_url_async(url: impl Into<PathBuf> + Send + 'static, default_image: Option<Box<dyn Drawable>>) -> Self {
        let url = url.into();
        let default_image: Box<dyn Drawable> = match default_image {
            Some(img) => img,
            None => Box::new(ImageDrawable::empty()),
        };
        Shared::from_async(
            async move {
                let image = ImageDrawable::from_url_async(url).await;
                match image {
                    Some(img) => Some(Box::new(img) as Box<dyn Drawable>),
                    None => None,
                }
            },
            default_image
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
            SharedDrawable::from_url_async(url.to_string(), None)
        } else {
            SharedDrawable::from_file_async(url.to_string(), None)
        }
    }
}

impl From<&str> for SharedDerivedDrawable {
    fn from(url: &str) -> Self {
        SharedDrawable::from(url).into()
    }
}

impl From<IconDrawable> for SharedDrawable {
    fn from(drawable: IconDrawable) -> Self {
        SharedSource::new(drawable.clone_drawable())
    }
}

impl From<IconDrawable> for SharedDerivedDrawable {
    fn from(drawable: IconDrawable) -> Self {
        SharedDerived::new_derived(drawable.clone_drawable())
    }
}

impl From<MaterialSymbol> for SharedDrawable {
    fn from(symbol: MaterialSymbol) -> Self {
        SharedDrawable::from(IconDrawable::new(symbol))
    }
}

impl From<MaterialSymbol> for SharedDerivedDrawable {
    fn from(symbol: MaterialSymbol) -> Self {
        SharedDerivedDrawable::from(IconDrawable::new(symbol))
    }
}