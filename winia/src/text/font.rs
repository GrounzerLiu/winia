//! Font infrastructure — a thread-local `FontCollection` cache.
//!
//! Loading a font manager is one of the most expensive things text rendering does. This module
//! keeps one `FontCollection` per thread so the render layer and the measure layer share it.

use skia_safe::textlayout::FontCollection;
use std::cell::RefCell;

thread_local! {
    static FONT_COLLECTION: RefCell<Option<FontCollection>> = const { RefCell::new(None) };
}

/// A CLONE of the collection: Skia wants its own handle at every use site.
pub fn get_font_collection() -> FontCollection {
    FONT_COLLECTION.with(|fc| {
        let mut opt = fc.borrow_mut();
        if opt.is_none() {
            let mut collection = FontCollection::new();
            collection.set_default_font_manager(skia_safe::FontMgr::default(), None);
            *opt = Some(collection);
        }
        opt.as_ref().unwrap().clone()
    })
}
