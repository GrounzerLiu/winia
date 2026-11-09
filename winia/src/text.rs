//! 文本处理基础设施 — 索引映射、Paragraph 封装、TextLayout、内联元素

pub mod font;
pub mod style;
pub use font::{FontError, register_font, register_font_bytes};
pub mod annotated;
pub mod decor;
pub mod transformation;
pub use decor::{DecoMode, DecoStyle, FontEdge, FontHint, RichSpanStyle};
pub mod field;
mod index_bimap;
mod inline_drawable;
mod layout_result;
mod measure_text;
mod paragraph;
mod paragraph_builder;
pub mod selection;
mod text_layout;

pub use annotated::{AnnotatedString, SpanStyle};
pub use field::{TextFieldColors, TextFieldSlotRole, TextFieldVariant};
pub use index_bimap::IndexBiMap;
pub use inline_drawable::{ImageDrawable, InlineDrawable, SvgDrawable};
pub use layout_result::TextLayoutResult;
pub use measure_text::{TextMeasureSpec, measure_text};
pub use paragraph::{DisplaySpan, Paragraph, build_plain_paragraph};
pub use paragraph_builder::ParagraphBuilder;
pub use selection::{
    LOCAL_TEXT_SELECTION_COLORS, Selection, SelectionAdjustment, SelectionRegistrar,
    TextSelectionColors,
};
pub use style::{
    FontSlant, FontWeight, LineHeightAlignment, LineHeightStyle, LineHeightTrim, TextAlign,
    TextBrush, TextDecoration, TextDrawStyle, TextGeometricTransform, TextOverflow, TextShadow,
    TextStyle,
};
pub use text_layout::TextLayout;
pub use transformation::{
    IdentityMapping, IdentityTransformation, OffsetMapping, PasswordTransformation,
    TransformedText, VisualTransformation,
};
