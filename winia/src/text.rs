//! 文本处理基础设施 — 索引映射、Paragraph 封装、TextLayout、内联元素

pub mod style;
pub mod font;
pub mod annotated;
pub mod decor;
pub mod transformation;
pub use decor::{DecoMode, DecoStyle, FontEdge, FontHint, RichSpanStyle};
pub mod selection;
pub mod field;
mod layout_result;
mod index_bimap;
mod paragraph;
mod paragraph_builder;
mod text_layout;
mod inline_drawable;
mod measure_text;

pub use annotated::{AnnotatedString, SpanStyle};
pub use style::{
    FontSlant, FontWeight, LineHeightAlignment, LineHeightStyle, LineHeightTrim, TextAlign, TextBrush,
    TextDecoration, TextDrawStyle, TextGeometricTransform, TextOverflow, TextShadow, TextStyle,
};
pub use layout_result::TextLayoutResult;
pub use measure_text::{measure_text, TextMeasureSpec};
pub use index_bimap::IndexBiMap;
pub use paragraph::{build_plain_paragraph, DisplaySpan, Paragraph};
pub use paragraph_builder::ParagraphBuilder;
pub use text_layout::TextLayout;
pub use inline_drawable::{InlineDrawable, ImageDrawable, SvgDrawable};
pub use selection::{Selection, SelectionRegistrar};
pub use field::{TextFieldColors, TextFieldSlotRole, TextFieldVariant};
pub use transformation::{
    IdentityMapping, IdentityTransformation, OffsetMapping, PasswordTransformation,
    TransformedText, VisualTransformation,
};
