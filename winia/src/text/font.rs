//! Font infrastructure — a thread-local `FontCollection` cache.
//!
//! Loading a font manager is one of the most expensive things text rendering does. This module
//! keeps one `FontCollection` per thread so the render layer and the measure layer share it.
//! Asset fonts are registered lazily: the collection is rebuilt when the font epoch changes.

use skia_safe::textlayout::{FontCollection, TypefaceFontProvider};
use skia_safe::{FontMgr, FontStyle};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FontError {
    #[error("font family name must not be empty")]
    EmptyFamily,
    #[error("font asset path must not be empty")]
    EmptyAsset,
    #[error("font data is not a valid typeface")]
    InvalidFont,
}

enum FontSource {
    Asset(String),
    Bytes(Vec<u8>),
}

struct AssetFont {
    family: String,
    source: FontSource,
}

static ASSET_FONTS: LazyLock<Mutex<Vec<AssetFont>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static FONT_EPOCH: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static FONT_COLLECTION: RefCell<Option<(u64, u64, FontCollection)>> =
        const { RefCell::new(None) };
}

/// Register a font loaded from the application asset bundle.
///
/// The bytes are resolved lazily when the font collection is built, so this may be called before
/// the application event loop initializes the bundle.
pub fn register_font(family: &str, asset: &str) -> Result<(), FontError> {
    if family.trim().is_empty() {
        return Err(FontError::EmptyFamily);
    }
    if asset.trim().is_empty() {
        return Err(FontError::EmptyAsset);
    }
    let mut fonts = ASSET_FONTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    fonts.push(AssetFont {
        family: family.to_owned(),
        source: FontSource::Asset(asset.to_owned()),
    });
    FONT_EPOCH.fetch_add(1, Ordering::Release);
    Ok(())
}

/// Register a font from bytes, validating that Skia can parse it.
pub fn register_font_bytes(family: &str, bytes: Vec<u8>) -> Result<(), FontError> {
    if family.trim().is_empty() {
        return Err(FontError::EmptyFamily);
    }
    if FontMgr::default().new_from_data(&bytes, None).is_none() {
        return Err(FontError::InvalidFont);
    }
    let mut fonts = ASSET_FONTS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    fonts.push(AssetFont {
        family: family.to_owned(),
        source: FontSource::Bytes(bytes),
    });
    FONT_EPOCH.fetch_add(1, Ordering::Release);
    Ok(())
}

/// A CLONE of the collection: Skia wants its own handle at every use site.
pub fn get_font_collection() -> FontCollection {
    let epoch = FONT_EPOCH.load(Ordering::Acquire);
    let asset_epoch = crate::assets::generation();
    FONT_COLLECTION.with(|cache| {
        if let Some((cached_epoch, cached_assets, collection)) = cache.borrow().as_ref()
            && *cached_epoch == epoch
            && *cached_assets == asset_epoch
        {
            return collection.clone();
        }

        let mut collection = FontCollection::new();
        let system = FontMgr::default();
        collection.set_default_font_manager(system.clone(), None);

        let fonts = ASSET_FONTS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !fonts.is_empty() {
            let mut provider = TypefaceFontProvider::new();
            for font in fonts.iter() {
                let bytes = match &font.source {
                    FontSource::Bytes(bytes) => Some(bytes.clone()),
                    FontSource::Asset(path) => match crate::assets::load(path) {
                        Some(bytes) => Some(bytes),
                        None => {
                            log::warn!("[fonts] asset not found: {path}");
                            None
                        }
                    },
                };
                let Some(bytes) = bytes else {
                    continue;
                };
                let Some(typeface) = system.new_from_data(&bytes, None) else {
                    log::warn!("[fonts] invalid typeface for family {}", font.family);
                    continue;
                };
                provider.register_typeface(typeface, font.family.as_str());
            }
            collection.set_asset_font_manager(Some(FontMgr::from(provider)));
        }
        drop(fonts);

        *cache.borrow_mut() = Some((epoch, asset_epoch, collection.clone()));
        collection
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_font_bytes() {
        assert!(matches!(
            register_font_bytes("WiniaBroken", vec![0, 1, 2, 3]),
            Err(FontError::InvalidFont)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn registers_a_system_font_fixture() {
        let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") else {
            return;
        };
        register_font_bytes("WiniaTestMono", bytes).unwrap();
        let mut collection = get_font_collection();
        let faces = collection.find_typefaces(&["WiniaTestMono"], FontStyle::normal());
        assert!(!faces.is_empty(), "registered typeface is resolvable");
        let system_faces = collection.find_typefaces(&["Consolas"], FontStyle::normal());
        assert!(
            !system_faces.is_empty(),
            "system fonts remain available as fallback"
        );
    }
}
