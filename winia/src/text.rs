//! 文本处理基础设施 — 索引映射、Paragraph 封装、TextLayout、内联元素

pub mod style;
pub mod font;
pub mod transformation;
pub mod selection;
pub mod field;
mod index_bimap;
mod paragraph;
mod paragraph_builder;
mod text_layout;
mod inline_drawable;

pub use style::{FontSlant, FontWeight, TextAlign, TextOverflow, TextStyle};
pub use index_bimap::IndexBiMap;
pub use paragraph::{build_plain_paragraph, Paragraph};
pub use paragraph_builder::ParagraphBuilder;
pub use text_layout::TextLayout;
pub use inline_drawable::{InlineDrawable, ImageDrawable, SvgDrawable};
pub use selection::{Selection, SelectionRegistrar};
pub use field::{TextFieldColors, TextFieldSlotRole, TextFieldVariant};
pub use transformation::{
    IdentityMapping, IdentityTransformation, OffsetMapping, PasswordTransformation,
    TransformedText, VisualTransformation,
};
