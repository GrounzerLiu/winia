//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement};
use crate::layout::{IntrinsicSize};
use crate::text::{RichSpanStyle};
use crate::transition::{abs_rect_upward, find_idx_by_slot, TransitionRole};
use crate::text::FontSlant;
use skia_safe::FontStyle as SkFontStyle;
use skia_safe::textlayout::TextStyle as SkTextStyle;
use super::constraints::Constraints;
use crate::unit::{Offset, Size};
use crate::transition::{FlightMeasure, FlightMeasureFrame};
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局节点 ID 生成器
static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);



/// 检查 modifier 中是否包含 TextContent
pub(crate) fn modifier_has_text(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::TextContent { .. }))
}

/// What a modifier says about its text, in the form the frame cache stores and compares.
///
/// Every field here affects measurement or rendering: the content, the alignment, **the colour** (an
/// alpha 0→1 fade must re-measure, or the cached paragraph keeps its transparent paint and the text
/// never appears), the size, weight, slant, line count, spacing, line height and overflow. A change in
/// any of them has to invalidate a folded measurement even when the slot stayed clean.
pub(crate) type TextSnapshot = (
    String,
    crate::text::TextAlign,
    crate::graphics::Color,
    f32,
    crate::text::FontWeight,
    crate::text::FontSlant,
    usize,
    bool,
    f32,
    Option<f32>,
    crate::text::TextOverflow,
);

/// The text a modifier carries, or `None` if it has no text element. Rich text is deliberately
/// reported as a fixed value that differs from any plain snapshot — a rich-text change is
/// conservatively treated as "changed".
pub(crate) fn text_snapshot(modifier: &Modifier) -> Option<TextSnapshot> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::TextContent { content, align, color, font_size, font_weight, font_style, max_lines, soft_wrap, letter_spacing, line_height, overflow, .. } => {
            Some((content.clone(), *align, *color, *font_size, *font_weight, *font_style, *max_lines, *soft_wrap, *letter_spacing, *line_height, *overflow))
        }
        ModifierElement::RichTextContent { .. } => Some(("<richtext>".to_string(), crate::text::TextAlign::Left, crate::graphics::Color::TRANSPARENT, 0.0, crate::text::FontWeight::NORMAL, crate::text::FontSlant::Upright, 0, true, 0.0, None, crate::text::TextOverflow::Clip)),
        _ => None,
    })
}

/// Whether `modifier` still carries the text `snap` was taken from — the comparison that decides if a
/// folding node's `cached_paragraph` is still valid.
///
/// It compares FIELD BY FIELD against the stored snapshot instead of building a second snapshot and
/// comparing tuples: `text_snapshot` clones the content `String`, so the tuple form allocated on
/// every reused text node every frame to answer a question that is `true` almost every time. Same
/// answer, same conservative direction (anything that differs — including a modifier that lost its
/// text element — reports `false`, which forces the re-measure).
pub(crate) fn text_content_matches(modifier: &Modifier, snap: &TextSnapshot) -> bool {
    modifier.elements().iter().any(|el| match el {
        ModifierElement::TextContent { content, align, color, font_size, font_weight, font_style, max_lines, soft_wrap, letter_spacing, line_height, overflow, .. } => {
            content == &snap.0
                && *align == snap.1
                && *color == snap.2
                && *font_size == snap.3
                && *font_weight == snap.4
                && *font_style == snap.5
                && *max_lines == snap.6
                && *soft_wrap == snap.7
                && *letter_spacing == snap.8
                && *line_height == snap.9
                && *overflow == snap.10
        }
        // Rich text reports the fixed sentinel `text_snapshot` uses: comparing the two is what kept a
        // rich-text node from re-measuring every frame, and that is the behaviour kept here.
        ModifierElement::RichTextContent { .. } => snap.0 == "<richtext>",
        _ => false,
    })
}

/// 检查 modifier 中是否包含 RichTextContent
pub(crate) fn modifier_has_richtext(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::RichTextContent { .. }))
}

/// 检查 modifier 中是否包含 ImageContent（Image 组件）
pub(crate) fn modifier_has_image(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::ImageContent { .. }))
}


mod arena;
mod focus;
mod geometry;
mod hit;
mod intrinsic;
mod measure;

pub(crate) use arena::*;
pub(crate) use focus::*;
pub(crate) use hit::*;
pub(crate) use intrinsic::*;
pub(crate) use measure::*;

pub use arena::*;
pub use focus::*;
pub use geometry::*;
pub use hit::*;
pub use intrinsic::*;
pub use measure::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod intrinsic_tests;
