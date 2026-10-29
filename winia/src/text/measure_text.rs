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
    /// Compose's `textGeometricTransform`: widens the drawn run, and the box with it.
    pub transform: Option<crate::text::TextGeometricTransform>,
    /// Compose's `lineHeightStyle`: trims the extra leading off the box ends.
    pub line_height_style: Option<crate::text::LineHeightStyle>,
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
            transform: None,
            line_height_style: None,
        }
    }
}


// ── The measurer's cache ──

/// A key that covers every field `TextMeasureSpec` lays out with.
///
/// `PartialEq` rather than a hash: the spec holds an `f32` per knob and a `&str`, and a hash of
/// floats would have to be lossy to be cheap (two sizes a ULP apart would collide and answer each
/// other's size). Comparison on a hit is a handful of float compares, which is far cheaper than the
/// paragraph it saves.
#[derive(Clone, PartialEq)]
struct MeasureKey {
    content: String,
    font_size: f32,
    color: Color,
    max_width_bits: u32,
    font_weight: FontWeight,
    font_style: FontSlant,
    max_lines: usize,
    align: TextAlign,
    overflow: TextOverflow,
    soft_wrap: bool,
    letter_spacing_bits: u32,
    line_height_bits: Option<u32>,
    min_lines: usize,
    font_family: Option<String>,
    transform_bits: Option<(u32, u32)>,
    line_height_style: Option<crate::text::LineHeightStyle>,
}

impl MeasureKey {
    fn of(spec: &TextMeasureSpec<'_>) -> Self {
        MeasureKey {
            content: spec.content.to_string(),
            font_size: spec.font_size,
            color: spec.color,
            max_width_bits: spec.max_width.to_bits(),
            font_weight: spec.font_weight,
            font_style: spec.font_style,
            max_lines: spec.max_lines,
            align: spec.align,
            overflow: spec.overflow,
            soft_wrap: spec.soft_wrap,
            letter_spacing_bits: spec.letter_spacing.to_bits(),
            line_height_bits: spec.line_height.map(|v| v.to_bits()),
            min_lines: spec.min_lines,
            font_family: spec.font_family.map(|f| f.to_string()),
            transform_bits: spec.transform.map(|t| (t.scale_x.to_bits(), t.skew_x.to_bits())),
            line_height_style: spec.line_height_style,
        }
    }
}

/// How many distinct measures the cache keeps — Compose's `TextMeasurer` default is 8.
const DEFAULT_CAPACITY: usize = 8;

/// The measurer's cache, oldest-use first.
struct MeasureCache {
    entries: Vec<(MeasureKey, TextLayoutResult)>,
    capacity: usize,

    /// How many times a measure was answered from the cache, and how many times it was not.
    pub stats: (usize, usize),
}

impl MeasureCache {
    const fn new() -> Self { MeasureCache { entries: Vec::new(), capacity: DEFAULT_CAPACITY, stats: (0, 0) } }

    fn get(&mut self, key: &MeasureKey) -> Option<TextLayoutResult> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        // Touch: the entry moves to the back (most recently used).
        let (k, v) = self.entries.remove(at);
        self.entries.push((k, v));
        self.stats.0 += 1;
        Some(v)
    }

    fn put(&mut self, key: MeasureKey, value: TextLayoutResult) {
        self.stats.1 += 1;
        if let Some(at) = self.entries.iter().position(|(k, _)| *k == key) {
            self.entries.remove(at);
        } else if self.entries.len() == self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((key, value));
    }

    fn clear(&mut self) { self.entries.clear(); }
}

thread_local! {
    /// One cache per thread: a paragraph is built with the thread-local font collection, and a
    /// measure is a layout-layer question — a `Mutex` here would serialise every measuring thread.
    static CACHE: std::cell::RefCell<MeasureCache> = const { std::cell::RefCell::new(MeasureCache::new()) };
}

/// How many distinct measures the cache keeps (Compose's `TextMeasurer` default).
pub fn measure_cache_capacity() -> usize {
    CACHE.with(|c| c.borrow().capacity)
}

/// Keep at most `capacity` distinct measures. Dropping the bound below the current count evicts
/// the oldest entries immediately, which is what Compose's `TextMeasurer` does on construction.
pub fn set_measure_cache_capacity(capacity: usize) {
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.capacity = capacity.max(1);
        while cache.entries.len() > cache.capacity {
            cache.entries.remove(0);
        }
    });
}

/// For tests and for a density/theme switch that invalidates every cached size: empty the cache.
pub fn clear_measure_cache() {
    CACHE.with(|c| c.borrow_mut().clear());
}

/// The cache's `(hits, misses)` since it was last cleared.
pub fn measure_cache_stats() -> (usize, usize) {
    CACHE.with(|c| c.borrow().stats)
}

/// Lay one text out and report what it measured — the same numbers `Text::on_text_layout` sees.
///
/// The result is a snapshot: `size` is the box (grown to `min_lines`), `text_size` is the
/// paragraph's own size, and the baselines and overflow flags come from the same pass. Nothing is
/// cached, so this is safe to call from a layout callback or from code that has no node at all.
pub fn measure_text(spec: &TextMeasureSpec<'_>) -> TextLayoutResult {
    let key = MeasureKey::of(spec);
    if let Some(hit) = CACHE.with(|c| c.borrow_mut().get(&key)) {
        return hit;
    }
    let result = measure_text_uncached(spec);
    CACHE.with(|c| c.borrow_mut().put(key, result));
    result
}

/// The layout itself, without the cache — what a miss runs.
fn measure_text_uncached(spec: &TextMeasureSpec<'_>) -> TextLayoutResult {
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
        None,
        spec.max_width,
    );

    let scale_x = spec.transform.map(|t| t.scale_x).unwrap_or(1.0).max(0.0001);
    let text_size = Size::new(
        (para.max_intrinsic_width() * scale_x).ceil().min(spec.max_width),
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

    // Same `lineHeightStyle` rule as the leaf: the extra leading trimmed at the first line's top /
    // the last line's bottom comes off the box.
    let geometry = crate::layout::node::line_geometry_from(
        spec.line_height,
        spec.line_height_style,
        para.natural_line_height(),
    );
    if geometry.trim_top > 0.0 || geometry.trim_bottom > 0.0 {
        size.height = (size.height - geometry.trim_top - geometry.trim_bottom).max(0.0);
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
    let longest_line = para.longest_line() * scale_x;
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
        assert_eq!(result.text_size.width, 40.0, "the box clamps the reported width: {result:?}");
        assert!(result.did_overflow_width, "the line itself is wider than the box: {result:?}");
        assert!(result.has_visual_overflow());
    }

    /// The cache answers the SAME numbers and reports the hit; a changed knob is a miss (the key
    /// covers every field), and the bound evicts the oldest.
    ///
    /// The counters are read as DELTAS: the cache is thread-local and the suite runs single
    /// threaded, so other tests have already measured through it.
    #[test]
    fn the_measurer_cache_answers_repeats_and_misses_on_change() {
        clear_measure_cache();
        let base = measure_cache_stats();
        let hits = || measure_cache_stats().0 - base.0;
        let misses = || measure_cache_stats().1 - base.1;
        let spec = TextMeasureSpec::new("Hi", 16.0, 300.0);
        let first = measure_text(&spec);
        assert_eq!((hits(), misses()), (0, 1), "the first call is a miss");
        let again = measure_text(&spec);
        assert_eq!((hits(), misses()), (1, 1), "the repeat is a hit");
        assert_eq!(first, again, "a hit answers the identical result");

        // Every field of the spec is part of the key: change one, get a miss.
        let mut moved = TextMeasureSpec::new("Hi", 16.0, 300.0);
        moved.max_width = 301.0;
        let _ = measure_text(&moved);
        assert_eq!((hits(), misses()), (1, 2), "a different width is a miss");

        let mut scaled = TextMeasureSpec::new("Hi", 16.0, 300.0);
        scaled.transform = Some(crate::text::TextGeometricTransform::new(2.0, 0.0));
        let _ = measure_text(&scaled);
        assert_eq!((hits(), misses()), (1, 3), "a different transform is a miss");

        // The bound: capacity 2 keeps the two most recent.
        clear_measure_cache();
        set_measure_cache_capacity(2);
        let a = TextMeasureSpec::new("a", 16.0, 300.0);
        let b = TextMeasureSpec::new("b", 16.0, 300.0);
        let c = TextMeasureSpec::new("c", 16.0, 300.0);
        let _ = measure_text(&a);
        let _ = measure_text(&b);
        let _ = measure_text(&c);
        assert_eq!(measure_cache_capacity(), 2);
        assert_eq!((hits(), misses()), (1, 6), "three new measures");
        // "a" was evicted, so asking again is a miss; "c" is still there.
        let _ = measure_text(&a);
        assert_eq!((hits(), misses()), (1, 7), "the oldest entry was evicted");
        let _ = measure_text(&c);
        assert_eq!((hits(), misses()), (2, 7), "the newest is still cached");
        set_measure_cache_capacity(8);
        clear_measure_cache();
    }

    /// `measure_text` must answer what the leaf measures, for the two options that change the BOX:
    /// a geometric transform widens it, a line-height trim shortens it. Both were ignored, so the
    /// public measurer disagreed with `on_text_layout` for the same text.
    #[test]
    fn the_measurer_honours_the_transform_and_the_line_height_trim() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T, TextGeometricTransform};
        let plain = measure_text(&TextMeasureSpec::new("Ag", 16.0, 200.0));
        let mut scaled_spec = TextMeasureSpec::new("Ag", 16.0, 200.0);
        scaled_spec.transform = Some(TextGeometricTransform::new(2.0, 0.0));
        let scaled = measure_text(&scaled_spec);
        assert!(
            (scaled.text_size.width - plain.text_size.width * 2.0).abs() < 1.0,
            "twice the width: {} vs {}",
            plain.text_size.width,
            scaled.text_size.width
        );
        assert!((scaled.text_size.height - plain.text_size.height).abs() < 0.5, "the height is untouched");

        let mut tall_spec = TextMeasureSpec::new("Ag", 16.0, 200.0);
        tall_spec.line_height = Some(34.0);
        let tall = measure_text(&tall_spec);
        let mut trimmed_spec = tall_spec.clone();
        trimmed_spec.line_height_style = Some(LineHeightStyle::new(A::Center, T::Both));
        let trimmed = measure_text(&trimmed_spec);
        assert!(
            trimmed.size.height < tall.size.height - 2.0,
            "the trim shortens the box: {} vs {}",
            tall.size.height,
            trimmed.size.height
        );
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
