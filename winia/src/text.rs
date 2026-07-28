//! 文本处理基础设施 — 索引映射、Paragraph 封装、TextLayout

mod index_bimap;
mod paragraph;
mod paragraph_builder;
mod text_layout;

pub use index_bimap::IndexBiMap;
pub use paragraph::Paragraph;
pub use paragraph_builder::ParagraphBuilder;
pub use text_layout::TextLayout;

