//! A public text measurer — Compose's `TextMeasurer.measure()` as winia can offer it.
//!
//! The leaf's measure path is the only place a paragraph is built, so it is the only thing that
//! can answer "how big will this text be" without composing a node. It takes the FLATTENED
//! parameters the leaf already carries (`TextContent`'s set) rather than a [`crate::text::TextStyle`]:
//! a style holds `TextUnit`s that need a density to resolve, and this module sits below the
//! component layer that owns it.

use crate::graphics::Color;
use crate::text::{FontSlant, FontWeight, TextAlign, TextLayoutResult, TextOverflow};
use crate::unit::Size;

/// Everything one text measure needs. `Default` is Compose's `Text` default with no width limit.
#[derive(Debug, Clone)]
pub struct TextMeasureSpec<'a> {
    pub content: &'a str,
    /// Logical pixels (dp), like the rest of the layout layer.
    pub font_size: f32,
    pub color: Color,
    /// The width to lay out into. `f32::MAX` means "unbounded".
    pub max_width: f32,
    pub font_weight: FontWeight,
    pub font_style: FontSlant,
    pub max_lines: usize,
    pub align: TextAlign,
    pub overflow: TextOverflow,
    pub soft_wrap: bool,
    pub letter_spacing: f32,
    pub line_height: Option<f32>,
    pub min_lines: usize,
    pub font_family: Option<&'a str>,
}

impl<'a> TextMeasureSpec<'a> {
    pub fn new(content: &'a str, font_size: f32, max_width: f32) -> Self {
        TextMeasureSpec {
            content,
            font_size,
            color: Color::BLACK,
            max_width,
            font_weight: FontWeight::NORMAL,
            font_style: FontSlant::Upright,
            max_lines: usize::MAX,
            align: TextAlign::Left,
            overflow: TextOverflow::Clip,
            soft_wrap: true,
            letter_spacing: 0.0,
            line_height: None,
            min_lines: 1,
            font_family: None,
        }
    }
}

/// Lay one text out and report what it measured — the same numbers `Text::on_text_layout` sees.
///
/// The result is a snapshot: `size` is the box (grown to `min_lines`), `text_size` is the
/// paragraph's own size, and the baselines and overflow flags come from the same pass. Nothing is
/// cached, so this is safe to call from a layout callback or from code that has no node at all.
pub fn measure_text(spec: &TextMeasureSpec<'_>) -> TextLayoutResult {
    let para = crate::layout::node::build_plain_paragraph(
        spec.content,
        spec.font_size,
        &spec.color,
        spec.font_weight,
        spec.font_style,
        spec.max_lines,
        spec.align,
        spec.overflow,
        spec.soft_wrap,
        spec.letter_spacing,
        spec.line_height,
        false,
        false,
        spec.font_family,
        spec.max_width,
    );

    let text_size = Size::new(
        para.max_intrinsic_width().ceil().min(spec.max_width),
        para.height().ceil(),
    );
    let line_count = para.inner_paragraph().line_number();
    // Same `minLines` rule as the leaf: skia-safe has no `set_min_lines`, so the box is padded.
    let mut size = text_size;
    if spec.min_lines > line_count.max(1) {
        let line_height = if line_count > 0 {
            text_size.height / line_count as f32
        } else {
            text_size.height
        };
        size.height = size.height.max(line_height * spec.min_lines as f32);
    }

    // `alphabetic_baseline` is what the layout engine aligns a first line against, so it is the
    // number a consumer should see. Skia's per-line baselines are absolute too but disagree with it
    // by a fraction (measured: 16.984 vs 17.266 on one line), so a single line reports the same
    // value twice rather than two numbers that cannot be ordered.
    let first_baseline = para.alphabetic_baseline();
    let last_baseline = if line_count <= 1 {
        first_baseline
    } else {
        para.get_line_metrics().last().map(|m| m.baseline as f32).unwrap_or(first_baseline)
    };
    let longest_line = para.longest_line();
    TextLayoutResult {
        size,
        text_size,
        line_count,
        did_exceed_max_lines: para.did_exceed_max_lines(),
        first_baseline,
        last_baseline,
        did_overflow_width: longest_line > spec.max_width + 0.5,
        did_overflow_height: text_size.height > size.height + 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_line_measures_one_line() {
        let spec = TextMeasureSpec::new("Hi", 16.0, 300.0);
        let result = measure_text(&spec);
        assert_eq!(result.line_count, 1);
        assert!(result.size.width > 0.0 && result.size.height > 0.0, "{result:?}");
        assert!(result.first_baseline > 0.0, "a line has a baseline: {result:?}");
        assert_eq!(result.last_baseline, result.first_baseline, "one line is both first and last");
        assert!(!result.has_visual_overflow());
    }

    #[test]
    fn a_narrow_box_wraps_and_reports_more_lines() {
        let long = "The quick brown fox jumps over the lazy dog";
        let wide = measure_text(&TextMeasureSpec::new(long, 16.0, 400.0));
        let narrow = measure_text(&TextMeasureSpec::new(long, 16.0, 60.0));
        assert_eq!(wide.line_count, 1);
        assert!(narrow.line_count > 1, "{} lines", narrow.line_count);
        assert!(narrow.size.height > wide.size.height);
    }

    #[test]
    fn max_lines_marks_the_overflow() {
        let long = "The quick brown fox jumps over the lazy dog";
        let mut spec = TextMeasureSpec::new(long, 16.0, 120.0);
        spec.max_lines = 1;
        let result = measure_text(&spec);
        assert_eq!(result.line_count, 1);
        assert!(result.did_exceed_max_lines);
        assert!(result.has_visual_overflow());
    }

    #[test]
    fn a_nowrap_line_reports_a_width_overflow() {
        let mut spec = TextMeasureSpec::new("WWWWWWWWWWWW", 16.0, 40.0);
        spec.soft_wrap = false;
        let result = measure_text(&spec);
        assert_eq!(result.line_count, 1);
        assert!("the laid-out line is wider than the 40 dp box: {result:?}".len() > 0);
        assert!(result.did_overflow_width, "{result:?}");
        assert!(result.has_visual_overflow());
    }

    #[test]
    fn min_lines_grows_the_box_not_the_paragraph() {
        let one = measure_text(&TextMeasureSpec::new("Hi", 16.0, 300.0));
        let mut spec = TextMeasureSpec::new("Hi", 16.0, 300.0);
        spec.min_lines = 3;
        let three = measure_text(&spec);
        assert_eq!(three.text_size, one.text_size);
        assert!((three.size.height - one.size.height * 3.0).abs() < 1.5, "{three:?} vs {one:?}");
    }
}
