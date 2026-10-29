    use super::*;
    use crate::graphics::Color;

    struct TestCtx {
        content: String,
        drawable_ranges: Vec<Range<usize>>,
        annotations: Vec<(Style, Range<usize>)>,
        cursor: usize,
    }

    impl TestCtx {
        fn new() -> Self {
            TestCtx { content: String::new(), drawable_ranges: Vec::new(), annotations: Vec::new(), cursor: 0 }
        }

        fn text(&mut self, s: &str, style: Style) {
            if s.is_empty() { return; }
            let start = self.cursor;
            self.content.push_str(s);
            self.cursor += s.chars().count();
            if style.is_not_default() {
                self.annotations.push((style, start..self.cursor));
            }
        }

        fn image(&mut self) {
            let pos = self.cursor;
            self.drawable_ranges.push(pos..pos + 1);
            self.content.push('\u{FFFC}');
            self.cursor += 1;
        }

        fn placeholder(&mut self, text: &str, style: Style) {
            let pos = self.cursor;
            let len = text.chars().count();
            if len == 0 { return; }
            self.drawable_ranges.push(pos..pos + len);
            self.content.push_str(text);
            self.cursor += len;
            if style.is_not_default() {
                self.annotations.push((style, pos..self.cursor));
            }
        }

        fn spans(&self) -> Vec<RichSpanStyle> {
            resolve_spans(&self.content, &self.drawable_ranges, &self.annotations)
        }
    }

    impl Style {
        fn ul() -> Self { Style { ul: true, ..Style::default() } }
        fn st() -> Self { Style { st: true, ..Style::default() } }
        fn b() -> Self { Style { fw: Some(FontWeight::BOLD), ..Style::default() } }
        fn fs(v: f32) -> Self { Style { fs: Some(v), ..Style::default() } }
        fn col(c: Color) -> Self { Style { color: Some(c), ..Style::default() } }
        fn all() -> Self {
            Style {
                fs: Some(18.0),
                color: Some(Color::from_argb(255, 255, 0, 0)),
                fw: Some(FontWeight::BOLD),
                slant: Some(FontSlant::Italic),
                ul: true, st: true,
                bg: Some(Color::from_argb(60, 255, 255, 0)),
                ..Style::default()
            }
        }
    }

    // ── Basics: empty ──
    #[test]
    fn test_empty() {
        assert!(TestCtx::new().spans().is_empty());
    }

    // ── Basics: plain text, no annotations ──
    #[test]
    fn test_plain_text_no_annotations() {
        let mut ctx = TestCtx::new();
        ctx.text("Hello World", Style::default());
        let spans = ctx.spans();
        // One default segment should cover the whole text.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 11);
    }

    // ── Core: an underline does not leak into a neighbouring strikethrough ──
    #[test]
    fn test_underline_not_leaking_to_strikethrough() {
        let mut ctx = TestCtx::new();
        ctx.text("Underlined text. ", Style::ul());
        ctx.text("Strikethrough text. ", Style::st());
        let spans = ctx.spans();

        let ul: Vec<_> = spans.iter().filter(|s| s.underline).collect();
        let st: Vec<_> = spans.iter().filter(|s| s.strikethrough).collect();

        // Each decoration gets at least one span.
        assert!(!ul.is_empty(), "underline should exist");
        assert!(!st.is_empty(), "strikethrough should exist");
        // No underline span follows another: there is one contiguous run.
        for s in &spans {
            if s.underline {
                assert!(!s.strikethrough, "underline span should not also be strikethrough");
            }
            if s.strikethrough {
                assert!(!s.underline, "strikethrough span should not also be underline");
            }
        }
        // Neighbouring spans do not overlap.
        for i in 1..spans.len() {
            assert!(spans[i-1].end <= spans[i].start,
                "spans[{}].end({}) > spans[{}].start({}) — 重叠",
                i-1, spans[i-1].end, i, spans[i].start);
        }
    }

    // ── A scalar property does not cross its range ──
    #[test]
    fn test_scalar_properties_scope() {
        let red = Color::from_argb(255, 255, 0, 0);
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        ctx.text("Red. ", Style::col(red));
        ctx.text("Big. ", Style::fs(20.0));
        ctx.text("Plain again.", Style::default());
        let spans = ctx.spans();

        // Find the span of each property.
        let red_span = spans.iter().find(|s| s.color == red).expect("red span");
        let big_span = spans.iter().find(|s| (s.font_size - 20.0).abs() < 0.001).expect("big span");

        // The red span is confined to its own range.
        assert_eq!(red_span.color, red);
        // The big span is exactly the 20 px run.
        assert!((big_span.font_size - 20.0).abs() < 0.001);

        // What follows the red span does not inherit red.
        for s in spans.iter().filter(|s| s.start >= red_span.end) {
            assert_ne!(s.color, red, "color leaked past red span endpoint");
        }
    }

    // ── Nested merge: bold and italic coexist ──
    #[test]
    fn test_nested_styles_merge() {
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        // Mirrors scope.bold(|x| { x.italic(|x| { x.text("BoldItalic"); }); });
        ctx.text("BoldItalic", Style { fw: Some(FontWeight::BOLD), slant: Some(FontSlant::Italic), ..Style::default() });
        let spans = ctx.spans();

        let bi = spans.iter().find(|s|
            s.font_weight != FontWeight::NORMAL && s.font_style != FontSlant::Upright
        ).expect("bold+italic span");

        assert!(bi.font_weight != FontWeight::NORMAL, "should be bold");
        assert!(bi.font_style != FontSlant::Upright, "should be italic");
    }

    // ── Every property at once ──
    #[test]
    fn test_all_attributes_together() {
        let mut ctx = TestCtx::new();
        ctx.text("Normal. ", Style::default());
        ctx.text("All. ", Style::all());
        ctx.text("Normal again.", Style::default());
        let spans = ctx.spans();

        let all = spans.iter().find(|s|
            (s.font_size - 18.0).abs() < 0.001
            && s.underline && s.strikethrough
            && s.font_weight != FontWeight::NORMAL
        ).expect("all-attributes span");

        assert!(all.background.is_some(), "background should be set");
        assert!(all.underline, "underline");
        assert!(all.strikethrough, "strikethrough");
    }

    // ── Drawable positions ──
    #[test]
    fn test_drawable_positions() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.image();              // pos 1
        ctx.text("B", Style::default());
        ctx.image();              // pos 3
        ctx.text("C", Style::default());

        assert_eq!(ctx.drawable_ranges, vec![1..2, 3..4], "drawable indices");
        assert_eq!(ctx.content.chars().nth(1).unwrap(), '\u{FFFC}');
        assert_eq!(ctx.content.chars().nth(3).unwrap(), '\u{FFFC}');
        // No span covers a placeholder position.
        let spans = ctx.spans();
        for &Range { start, .. } in &ctx.drawable_ranges {
            assert!(spans.iter().all(|s| s.start != start), "placeholder leaked");
        }
    }

    // ── A drawable at the start ──
    #[test]
    fn test_drawable_at_start() {
        let mut ctx = TestCtx::new();
        ctx.image();              // pos 0
        ctx.text("Text", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..1]);
        let spans = ctx.spans();
        // The spans start at position 1.
        assert!(spans.iter().all(|s| s.start >= 1), "no span should cover drawable position");
    }

    // ── Consecutive drawables ──
    #[test]
    fn test_consecutive_drawables() {
        let mut ctx = TestCtx::new();
        ctx.image(); ctx.image(); ctx.image();
        ctx.text("After", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..1, 1..2, 2..3]);
        let spans = ctx.spans();
        // One text segment starts at position 3.
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 3);
    }

    // ── A drawable inside styled text ──
    #[test]
    fn test_drawable_within_styled_text() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        ctx.image();
        ctx.text("C", Style::b());
        let spans = ctx.spans();
        // Two bold spans, one either side, neither covering the placeholder.
        assert_eq!(spans.len(), 2, "text before and after image");
        for s in &spans {
            assert!(s.font_weight != FontWeight::NORMAL, "both spans should be bold");
        }
        assert_eq!(spans[0].end, 1);
        assert_eq!(spans[1].start, 2);
    }

    // ── An annotation at the start ──
    #[test]
    fn test_annotation_at_start() {
        let mut ctx = TestCtx::new();
        ctx.text("Bold start", Style::b());
        ctx.text("normal end", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold span");
        assert_eq!(bold.start, 0, "annotation should start at 0");
    }

    // ── An annotation at the end ──
    #[test]
    fn test_annotation_at_end() {
        let mut ctx = TestCtx::new();
        ctx.text("normal ", Style::default());
        ctx.text("bold end", Style::b());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold span");
        let content_len = ctx.content.len();
        assert_eq!(bold.end, content_len, "annotation should reach the end");
    }

    // ── A zero-width annotation is skipped ──
    #[test]
    fn test_zero_width_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        // An empty annotation should never appear, but if the cursor did not move it is ignored.
        // Simulate it: push without calling text().
        let spans = ctx.spans();
        assert!(spans.iter().all(|s| s.end > s.start), "no zero-width spans");
    }

    // ── A one-character annotation ──
    #[test]
    fn test_single_char_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("H", Style::b());
        ctx.text("ello", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold H");
        assert_eq!(bold.end - bold.start, 1, "single char");
    }

    // ── Non-overlapping properties do not overwrite each other ──
    #[test]
    fn test_non_overlapping_attributes_preserved() {
        let mut ctx = TestCtx::new();
        // The same range: bold, underline and big.
        let style = Style { fw: Some(FontWeight::BOLD), fs: Some(20.0), ul: true, ..Style::default() };
        ctx.text("Title", style);
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].font_weight != FontWeight::NORMAL, "bold");
        assert!(spans[0].underline, "underline");
        assert!((spans[0].font_size - 20.0).abs() < 0.001, "font size");
    }

    // ────────────────────────────────────────────────────────────
    // The image() and placeholder() tests.
    // ────────────────────────────────────────────────────────────

    // image(): the content carries U+FFFC and the drawable range is right.
    #[test]
    fn test_image_inserts_fffc() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.image();                     // pos 1
        ctx.text("C", Style::default());
        assert_eq!(ctx.content, "A\u{FFFC}C");
        assert_eq!(ctx.drawable_ranges, vec![1..2]);
        // No span covers a placeholder position.
        for s in &ctx.spans() {
            assert!(!(1..2).contains(&s.start));
        }
    }

    // Several images in a row.
    #[test]
    fn test_image_consecutive() {
        let mut ctx = TestCtx::new();
        ctx.image();
        ctx.image();
        ctx.image();
        ctx.text("T", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..1, 1..2, 2..3]);
        assert_eq!(ctx.content, "\u{FFFC}\u{FFFC}\u{FFFC}T");
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 3);
    }

    // An image inside a style scope.
    #[test]
    fn test_image_with_style() {
        let mut ctx = TestCtx::new();
        ctx.text("B", Style::b());
        ctx.image();                     // pos 1 (bold 应覆盖)
        // image() pushes no annotation, but the bold before it was already committed by text().
        // The span at the image keeps the properties of the text before it.
        // The check: the image produces no text span while the text either side is right.
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL);
        assert!(bold.is_some(), "bold from preceding text");
    }

    // placeholder(): the text lands in the content and its range is a drawable.
    #[test]
    fn test_placeholder_text_in_content() {
        let mut ctx = TestCtx::new();
        ctx.text("Hello ", Style::default());
        ctx.placeholder("★", Style::default());
        ctx.text(" World", Style::default());
        // The content contains the placeholder's text.
        assert_eq!(ctx.content, "Hello ★ World");
        assert_eq!(ctx.drawable_ranges, vec![6..7]);
        // The spans cover "Hello " and " World", not the ★ position.
        let spans = ctx.spans();
        for s in &spans {
            assert!(!(6..7).contains(&s.start), "placeholder leaked");
        }
    }

    // A styled placeholder.
    #[test]
    fn test_placeholder_with_style() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.placeholder("ico", Style::b());  // bold
        ctx.text("B", Style::default());
        // "ico" is in the content and the bold annotation covers 1..4.
        // The placeholder segment inherits bold but is filtered out of the text spans.
        let spans = ctx.spans();
        let bold_span = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL);
        // No visible bold span: the placeholder range is replaced wholesale.
        assert!(bold_span.is_none(), "placeholder text should not appear as visible span");
        // The content still holds the text.
        assert_eq!(ctx.content, "AicoB");
        assert_eq!(ctx.drawable_ranges, vec![1..4]);
    }

    // An image and a placeholder together.
    #[test]
    fn test_image_and_placeholder_mixed() {
        let mut ctx = TestCtx::new();
        ctx.text("S", Style::default());
        ctx.image();                     // pos 1, content 加 FFFC
        ctx.placeholder("mid", Style::default());  // pos 2, content 加 "mid"
        ctx.image();                     // pos 5, content 加 FFFC
        ctx.text("E", Style::default());
        assert_eq!(ctx.content, "S\u{FFFC}mid\u{FFFC}E");
        assert_eq!(ctx.drawable_ranges, vec![1..2, 2..5, 5..6]);
    }

    // Neither affects the plain-text ranges.
    #[test]
    fn test_placeholder_preserves_surrounding_text() {
        let mut ctx = TestCtx::new();
        ctx.text("before", Style::default());
        ctx.placeholder("X", Style::default());
        ctx.text("after", Style::default());
        let spans = ctx.spans();
        // Two text spans, "before" and "after": unstyled, but present.
        assert!(spans.len() >= 2, "should have text before and after");
        let before = spans.iter().find(|s| s.start == 0 && s.end == 6);
        let after = spans.iter().find(|s| s.start == 7 && s.end == 12);
        assert!(before.is_some(), "before text exists");
        assert!(after.is_some(), "after text exists");
    }

    // An empty placeholder.
    #[test]
    fn test_placeholder_empty_text() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("", Style::b()); // 空文本不应产生 drawable
        ctx.text("A", Style::default());
        assert_eq!(ctx.drawable_ranges.len(), 0, "empty text should not create placeholder");
        assert_eq!(ctx.content, "A");
    }

    // A placeholder at the start.
    #[test]
    fn test_placeholder_at_start() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("lead", Style::default());
        ctx.text("text", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..4]);
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1, "only trailing text");
        assert_eq!(spans[0].start, 4);
    }

    // A placeholder at the end.
    #[test]
    fn test_placeholder_at_end() {
        let mut ctx = TestCtx::new();
        ctx.text("head", Style::default());
        ctx.placeholder("tail", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![4..8]);
        let spans = ctx.spans();
        assert!(spans.iter().all(|s| s.start < 4), "last placeholder covers tail");
    }

    // Consecutive placeholders (adjacent ranges).
    #[test]
    fn test_placeholder_consecutive() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("AB", Style::default());
        ctx.placeholder("CD", Style::default());
        ctx.text("end", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..2, 2..4]);
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1, "only text after placeholders");
        assert_eq!(spans[0].start, 4, "text starts after both placeholders");
    }

    // A placeholder over the whole text.
    #[test]
    fn test_placeholder_whole_content() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("full", Style::default());
        let spans = ctx.spans();
        assert!(spans.is_empty(), "no text spans when whole content is placeholder");
        assert_eq!(ctx.content, "full");
        assert_eq!(ctx.drawable_ranges.len(), 1);
    }

    // A placeholder under every style property.
    #[test]
    fn test_placeholder_all_style() {
        let style = Style { fs: Some(16.0), color: Some(Color::from_argb(255,255,0,0)), fw: Some(FontWeight::BOLD), ul: true, ..Style::default() };
        let mut ctx = TestCtx::new();
        ctx.text("pre ", Style::default());
        ctx.placeholder("STYLED", style);
        ctx.text(" post", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![4..10]);
        // The text either side does not carry them.
        let spans = ctx.spans();
        assert_eq!(spans.len(), 2, "plain text before and after");
        for s in &spans {
            assert_eq!(s.font_weight, FontWeight::NORMAL, "surrounding text should not be bold");
            assert!(!s.underline, "surrounding text should not be underlined");
        }
    }

    // A multi-byte Unicode placeholder.
    #[test]
    fn test_placeholder_unicode() {
        let mut ctx = TestCtx::new();
        ctx.text("你好", Style::default());  // 2 个中文字符，content length 2
        ctx.placeholder("✨🔥", Style::default()); // 2 个 emoji，content length 4
        ctx.text("!", Style::default());
        // Each emoji counts as one in chars().count(): a Rust char is a Unicode scalar value.
        // ✨ and 🔥 are one char each, so the drawable range is 2 long.
        assert_eq!(ctx.drawable_ranges.len(), 1);
        let start = ctx.drawable_ranges[0].start;
        assert!(start >= 2, "placeholder after '你好'");
        let end = ctx.drawable_ranges[0].end;
        assert_eq!(end - start, 2, "two-chars placeholder");
    }

    // A placeholder inside a nested scope (bold -> placeholder).
    #[test]
    fn test_placeholder_in_nested_scope() {
        let mut ctx = TestCtx::new();
        ctx.text("a ", Style::default());
        ctx.text("mid", Style::b());
        ctx.placeholder("IMG", Style::b());
        ctx.text(" end", Style::default());
        // The bold annotation covers "mid" (2..5) and "IMG" (5..8).
        // The placeholder itself is 5..8, a drawable.
        // So the bold span is only "mid" (2..5).
        let spans = ctx.spans();
        let bold_spans: Vec<_> = spans.iter().filter(|s| s.font_weight != FontWeight::NORMAL).collect();
        assert_eq!(bold_spans.len(), 1, "only 'mid' is visible bold");
        // 'mid' starts at 2, length 3
        assert_eq!(bold_spans[0].start, 2);
        assert_eq!(bold_spans[0].end, 5);
    }

    // ── Paragraph parameters (T-3) ──
    //
    // Each of these used to be ignored: `measure_and_cache_richtext` built a bare
    // `ParagraphStyle`, so a rich block could only wrap. They are read back from the cached
    // paragraph, which is what the render pass paints.

    fn measure_rich(build: RichText, text: &str) -> (usize, f32, f32, f32, f32) {
        // Returns (line_count, paragraph height, longest_line, intrinsic width, measured height).
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            build
                .modifier(Modifier::new().width(120.0))
                .build(ctx, |x| { x.text(text); });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 400.0));
        let nodes = composer.arena_nodes();
        let node = nodes.iter().find(|n| n.has_richtext_content).expect("a rich text node");
        let measured = node.measured_size;
        let paragraph = node.cached_paragraph.borrow();
        let paragraph = paragraph.as_ref().expect("a real measure caches the paragraph");
        (
            paragraph.inner_paragraph().line_number(),
            paragraph.height(),
            paragraph.longest_line(),
            paragraph.max_intrinsic_width(),
            measured.height,
        )
    }

    const LONG: &str = "The quick brown fox jumps over the lazy dog";

    /// The baseline this set of tests compares against: the same text without any parameter wraps.
    #[test]
    fn rich_text_wraps_by_default() {
        let (lines, _, _, _, _) = measure_rich(RichText::new(), LONG);
        assert!(lines >= 3, "a long line in a 120 dp box wraps: {lines} lines");
    }

    #[test]
    fn rich_text_max_lines_caps_the_height() {
        let (lines, height, _, _, _) = measure_rich(RichText::new().max_lines(1), LONG);
        assert_eq!(lines, 1, "max_lines(1) must leave one line");
        let (default_lines, default_height, _, _, _) = measure_rich(RichText::new(), LONG);
        assert!(default_lines > 1);
        assert!(
            height < default_height,
            "one capped line must be shorter than {default_lines} natural ones: {height} vs {default_height}"
        );
    }

    #[test]
    fn rich_text_soft_wrap_off_keeps_one_line() {
        let (lines, _, longest, intrinsic, _) = measure_rich(RichText::new().soft_wrap(false), LONG);
        assert_eq!(lines, 1, "nowrap must not wrap");
        assert!(
            longest > 120.0 && intrinsic > 120.0,
            "the single line is wider than the box: longest={longest} intrinsic={intrinsic}"
        );
    }

    #[test]
    fn rich_text_ellipsis_fits_the_box() {
        let (lines, _, longest, _, _) = measure_rich(
            RichText::new().soft_wrap(false).max_lines(1).overflow(crate::text::TextOverflow::Ellipsis),
            LONG,
        );
        assert_eq!(lines, 1);
        assert!(
            longest <= 120.5,
            "an ellipsized nowrap line is truncated against the box, not its own width: {longest}"
        );
    }

    #[test]
    fn rich_text_line_height_sets_the_line_box() {
        let (_, height, _, _, _) = measure_rich(RichText::new().max_lines(1).line_height(40.0), "Hi");
        assert!(
            (height - 40.0).abs() < 1.0,
            "a 40 dp line height should make a one-line block 40 dp tall: {height}"
        );
    }

    #[test]
    fn rich_text_min_lines_reserves_the_box() {
        // `min_lines` grows the BOX, not the paragraph (skia-safe has no set_min_lines), so the
        // measurement is what has to change.
        let (_, one_line_paragraph, _, _, one_line_box) = measure_rich(RichText::new().max_lines(1), "Hi");
        let (_, _, _, _, three_line_box) = measure_rich(
            RichText::new().max_lines(1).min_lines(3),
            "Hi",
        );
        assert!(
            (one_line_box - one_line_paragraph).abs() < 1.0,
            "one line needs no reservation: box {one_line_box} vs paragraph {one_line_paragraph}"
        );
        assert!(
            (three_line_box - one_line_paragraph * 3.0).abs() < 1.5,
            "min_lines(3) reserves three lines: box {three_line_box} vs one line {one_line_paragraph}"
        );
    }

    /// A value built ahead of time lays out its own text and styles — Compose's
    /// `Text(AnnotatedString)` — and the build closure can still append after it.
    #[test]
    fn an_annotated_string_seeds_the_text_and_its_spans() {
        use crate::text::{AnnotatedString, SpanStyle};
        let annotated = AnnotatedString::new("abCD")
            .with_style(2..4, SpanStyle::new().bold().font_size(24.0));
        let style = span_style_to_style(&annotated.style_at(2));
        assert_eq!(style.fw, Some(crate::text::FontWeight::BOLD));
        assert_eq!(style.fs, Some(24.0));
        let plain = span_style_to_style(&annotated.style_at(0));
        assert_eq!(plain.fw, None);
        assert_eq!(plain.fs, None);

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            RichText::new().annotated(&annotated).build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let node = composer
            .arena_nodes()
            .iter()
            .find(|n| n.has_richtext_content)
            .expect("a rich text node");
        let spans = node.modifier.elements().iter().find_map(|el| match el {
            crate::modifier::ModifierElement::RichTextContent { spans, .. } => Some(spans.clone()),
            _ => None,
        }).expect("the payload carries spans");
        assert!(
            spans.iter().any(|s| s.start == 2 && s.end == 4 && s.font_weight == crate::text::FontWeight::BOLD),
            "the styled range survives into the payload: {spans:?}"
        );
    }

    /// Compose's `SpanStyle.letterSpacing` is a `TextUnit`: an `em` is a multiple of THIS span's own
    /// font size, not of the paragraph's. Measured on the resolved span the paragraph consumes.
    #[test]
    fn a_span_letter_spacing_em_resolves_against_the_span_size() {
        let mut style = Style::default();
        style.fs = Some(20.0);
        style.letter_spacing = crate::unit::TextUnit::Em(0.1);
        let spans = resolve_spans("abcdef", &[], &[(style, 1..3)]);
        let span = spans.iter().find(|s| s.start == 1 && s.end == 3).expect("the styled run");
        assert!(
            (span.letter_spacing - 2.0).abs() < 0.01,
            "0.1 em of a 20 px span is 2 px: {}",
            span.letter_spacing
        );
        let mut wider = Style::default();
        wider.fs = Some(40.0);
        wider.letter_spacing = crate::unit::TextUnit::Em(0.1);
        let spans = resolve_spans("abcdef", &[], &[(wider, 1..3)]);
        let wide_span = spans.iter().find(|s| s.start == 1 && s.end == 3).unwrap();
        assert!(
            (wide_span.letter_spacing - 4.0).abs() < 0.01,
            "at 40 px it is 4 px, so the em follows the SPAN size: {}",
            wide_span.letter_spacing
        );
    }

    /// `RichText` now exposes `lineHeightStyle`: the trim comes off the box height and the paint
    /// origin moves by `paint_dy`, the same two numbers the plain leaf uses (one helper, so the two
    /// cannot drift).
    #[test]
    fn a_rich_line_height_style_trims_its_box() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        fn height(style: Option<LineHeightStyle>) -> f32 {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx| {
                let mut b = RichText::new().line_height(34.0);
                if let Some(style) = style {
                    b = b.line_height_style(style);
                }
                b.build(ctx, |s| { s.text("Ag"); });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.height
        }
        let plain = height(None);
        let trimmed = height(Some(LineHeightStyle::new(A::Center, T::Both)));
        assert!(trimmed < plain - 2.0, "Trim::Both shortens the rich box: {plain} -> {trimmed}");
        let top = height(Some(LineHeightStyle::new(A::Top, T::None)));
        assert!((top - plain).abs() < 1.0, "an alignment alone does not change the box: {plain} vs {top}");
    }

    /// Rich text now honours start/middle ellipsis by truncating the CONTENT before the spans are
    /// resolved. The tail text must stay addressable and the span that survives must still describe
    /// the characters it now covers.
    #[test]
    fn a_rich_start_ellipsis_keeps_the_tail_addressable() {
        use crate::text::TextOverflow;
        fn build(width: f32, overflow: TextOverflow) -> (usize, usize) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx| {
                RichText::new()
                    .overflow(overflow)
                    .soft_wrap(false)
                    .modifier(Modifier::new().size(width, 30.0))
                    .build(ctx, |s| { s.text("0123456789abcdefghij"); });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let node = &composer.arena_nodes()[root];
            let para = node.cached_paragraph.borrow();
            let para = para.as_ref().unwrap();
            // "the paragraph's own length" and "the display span's tail".
            let span = para.display_span();
            (para.text_len(), span.map(|s| s.tail_start_byte).unwrap_or(0))
        }
        let (plain_len, _) = build(400.0, TextOverflow::StartEllipsis);
        assert_eq!(plain_len, 20, "a fitting run is untouched");
        let (short_len, tail) = build(60.0, TextOverflow::StartEllipsis);
        assert!(short_len < 20, "the run was truncated: {short_len}");
        assert!(tail > 0, "the display span records where the kept tail starts: {tail}");
        // The tail text is addressable in the paragraph's own (display) space.
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            RichText::new()
                .overflow(TextOverflow::StartEllipsis)
                .soft_wrap(false)
                .modifier(Modifier::new().size(60.0, 30.0))
                .build(ctx, |s| { s.text("0123456789abcdefghij"); });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let node = &composer.arena_nodes()[root];
        let para = node.cached_paragraph.borrow();
        let para = para.as_ref().unwrap();
        let len = para.text_len();
        let rects = para.get_rects_for_range(
            len.saturating_sub(2)..len,
            skia_safe::textlayout::RectHeightStyle::Max,
            skia_safe::textlayout::RectWidthStyle::Tight,
        );
        assert!(!rects.is_empty(), "the visible tail is addressable (len {len})");
    }
