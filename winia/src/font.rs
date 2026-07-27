//! 字体基础设施 — thread-local FontCollection 缓存
//!
//! 字体管理器加载是文本渲染中最昂贵的操作之一。
//! 本模块提供 thread-local 级别的 FontCollection 单例，
//! 供渲染层和测量层共用。

use skia_safe::textlayout::FontCollection;
use std::cell::RefCell;

thread_local! {
    static FONT_COLLECTION: RefCell<Option<FontCollection>> = const { RefCell::new(None) };
}

/// 获取一个 <b>clone</b> 后的 FontCollection（Skia 要求每次使用时 clone）
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
