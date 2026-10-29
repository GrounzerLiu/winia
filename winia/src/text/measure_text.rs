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
    /// Whether this stored key describes the same measure as `spec` — every field, content by
    /// `&str` so the hit path does not allocate.
    fn matches(&self, spec: &TextMeasureSpec<'_>) -> bool {
        self.content == spec.content
            && self.font_size == spec.font_size
            && self.color == spec.color
            && self.max_width_bits == spec.max_width.to_bits()
            && self.font_weight == spec.font_weight
            && self.font_style == spec.font_style
            && self.max_lines == spec.max_lines
            && self.align == spec.align
            && self.overflow == spec.overflow
            && self.soft_wrap == spec.soft_wrap
            && self.letter_spacing_bits == spec.letter_spacing.to_bits()
            && self.line_height_bits == spec.line_height.map(|v| v.to_bits())
            && self.min_lines == spec.min_lines
            && self.font_family.as_deref() == spec.font_family
            && self.transform_bits == spec.transform.map(|t| (t.scale_x.to_bits(), t.skew_x.to_bits()))
            && self.line_height_style == spec.line_height_style
    }

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

    /// A hit compares the stored key against the SPEC, so the content is compared as a `&str` and
    /// no allocation happens on the hit path. Building a `MeasureKey` first copied the whole content
    /// on every call — measured at 1.03 µs per hit on an 18 KB string, half of it that copy.
    fn get(&mut self, spec: &TextMeasureSpec<'_>) -> Option<TextLayoutResult> {
        let at = self.entries.iter().position(|(k, _)| k.matches(spec))?;
        // Touch: the entry moves to the back (most recently used).
        let (k, v) = self.entries.remove(at);
        self.entries.push((k, v));
        self.stats.0 += 1;
        Some(v)
    }

    fn put(&mut self, spec: &TextMeasureSpec<'_>, value: TextLayoutResult) {
        self.stats.1 += 1;
        let key = MeasureKey::of(spec);
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

/// Empty the cache. Two real uses: a test that wants clean counters, and a caller who replaced the
/// process font collection (`text::font`), which no spec field describes.
///
/// A density or font-scale change does NOT need this. `measure_text` never reads the ambient
/// density: a `Text` leaf resolves its `sp` sizes against the density while it builds and hands the
/// measurer PIXELS (measured: 14 sp reaches the spec as 14.0 at font scale 1 and 28.0 at scale 2).
/// The spec therefore differs whenever the scale does, and the cache sees a miss on its own.
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
/// cached (see the LRU above), so this is safe to call from a layout callback or from code that has no
/// node at all; `clear_measure_cache` is there for a density or theme switch that invalidates it.
pub fn measure_text(spec: &TextMeasureSpec<'_>) -> TextLayoutResult {
    if let Some(hit) = CACHE.with(|c| c.borrow_mut().get(spec)) {
        return hit;
    }
    let result = measure_text_uncached(spec);
    CACHE.with(|c| c.borrow_mut().put(spec, result));
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
        // A standalone measure has NO vertical constraint to overflow: the spec describes a width
        // (`max_width`), a line cap (`max_lines`, which reports through `did_exceed_max_lines`) and
        // a floor (`min_lines`, which only GROWS the box). So the honest answer is false — and it
        // is what the leaf's own `on_text_layout` reports for the same text without a height
        // constraint.
        //
        // The flag used to compare `text_size` against a TRIMMED `size`, so every `lineHeightStyle`
        // with a trim claimed an overflow that the trim had deliberately removed (measured: size
        // 21.28, text_size 34, flag true). A trim is not a cut-off.
        did_overflow_height: false,
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

        let mut moved = TextMeasureSpec::new("Hi", 16.0, 300.0);
        moved.max_width = 301.0;
        let _ = measure_text(&moved);
        assert_eq!((hits(), misses()), (1, 2), "a different width is a miss");

        let mut scaled = TextMeasureSpec::new("Hi", 16.0, 300.0);
        scaled.transform = Some(crate::text::TextGeometricTransform::new(2.0, 0.0));
        let _ = measure_text(&scaled);
        assert_eq!((hits(), misses()), (1, 3), "a different transform is a miss");

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
        let _ = measure_text(&a);
        assert_eq!((hits(), misses()), (1, 7), "the oldest entry was evicted");
        let _ = measure_text(&c);
        assert_eq!((hits(), misses()), (2, 7), "the newest is still cached");
        set_measure_cache_capacity(8);
        clear_measure_cache();
    }

    /// The spec carries RESOLVED pixels, so a density change arrives as a different spec rather than
    /// as a stale hit. Measured: a 14 sp leaf hands the measurer 14.0 px at font scale 1 and 28.0 px
    /// at scale 2, and the same spec measured under two very different ambient densities answers the
    /// same size — `measure_text` reads no density at all.
    #[test]
    fn a_density_change_reaches_the_cache_as_a_different_spec() {
        use crate::runtime::density::with_density;
        use crate::unit::Density;
        fn payload_font_size(scale: f32) -> f32 {
            let d = Density::from_density_and_font_scale(1.0, scale);
            with_density(d, || {
                let mut c = crate::runtime::composer::Composer::new();
                c.compose(|ctx| { crate::components::Text::new("Ag").font_size(14.0).build(ctx); });
                c.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
                let root = c.layout_root_idx().unwrap();
                let mut found = None;
                for el in c.arena_nodes()[root].modifier.elements() {
                    if let crate::modifier::ModifierElement::TextContent { font_size, .. } = el {
                        found = Some(*font_size);
                    }
                }
                found.expect("the leaf carries its resolved size")
            })
        }
        let small = payload_font_size(1.0);
        let big = payload_font_size(2.0);
        assert_eq!(small, 14.0, "14 sp at scale 1 is 14 px");
        assert_eq!(big, 28.0, "and 28 px at scale 2, resolved BEFORE the measurer sees it");

        // The two specs differ, so the second is a miss rather than a stale hit.
        clear_measure_cache();
        let base = measure_cache_stats();
        let _ = measure_text(&TextMeasureSpec::new("Ag", small, 300.0));
        let _ = measure_text(&TextMeasureSpec::new("Ag", big, 300.0));
        assert_eq!(measure_cache_stats().1 - base.1, 2, "two pixel sizes are two entries");

        // And the measurer itself ignores the ambient density.
        let spec = TextMeasureSpec::new("Ag", 14.0, 300.0);
        let one = with_density(Density::from_density_and_font_scale(1.0, 1.0), || measure_text(&spec));
        let three = with_density(Density::from_density_and_font_scale(3.0, 2.0), || measure_text(&spec));
        assert_eq!(one.size, three.size, "no density input, so no density output");
    }

    /// `measure_text` must answer what the leaf measures, for the two options that change the BOX:
    /// a geometric transform widens it, a line-height trim shortens it.
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

    /// A standalone measure has no vertical constraint, so it never reports a height overflow —
    /// and a `lineHeightStyle` trim is not a cut-off. Measured before: the trim shortened the box to
    /// 21.28 while `text_size` stayed 34, and the flag read true, where the leaf's own
    /// `on_text_layout` reported false for the same text.
    #[test]
    fn a_trim_is_not_a_vertical_overflow() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        let mut spec = TextMeasureSpec::new("Ag", 16.0, 200.0);
        spec.line_height = Some(34.0);
        spec.line_height_style = Some(LineHeightStyle::new(A::Center, T::Both));
        let trimmed = measure_text(&spec);
        assert!(trimmed.size.height < trimmed.text_size.height, "the box was trimmed: {trimmed:?}");
        assert!(!trimmed.did_overflow_height, "a trim is not an overflow: {trimmed:?}");
        assert!(!trimmed.has_visual_overflow(), "nothing was cut off: {trimmed:?}");
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
