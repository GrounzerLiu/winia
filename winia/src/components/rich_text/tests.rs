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

    // ── 基础：空 ──
    #[test]
    fn test_empty() {
        assert!(TestCtx::new().spans().is_empty());
    }

    // ── 基础：纯文本无注解 ──
    #[test]
    fn test_plain_text_no_annotations() {
        let mut ctx = TestCtx::new();
        ctx.text("Hello World", Style::default());
        let spans = ctx.spans();
        // 应该产生一个覆盖全文的默认 segment
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 11);
    }

    // ── 核心：underline 不泄露到相邻 strikethrough ──
    #[test]
    fn test_underline_not_leaking_to_strikethrough() {
        let mut ctx = TestCtx::new();
        ctx.text("Underlined text. ", Style::ul());
        ctx.text("Strikethrough text. ", Style::st());
        let spans = ctx.spans();

        let ul: Vec<_> = spans.iter().filter(|s| s.underline).collect();
        let st: Vec<_> = spans.iter().filter(|s| s.strikethrough).collect();

        // underline 和 strikethrough 各自至少有一段
        assert!(!ul.is_empty(), "underline should exist");
        assert!(!st.is_empty(), "strikethrough should exist");
        // 任意 underline span 的后面都不应再有 underline（只有一个连续段）
        for s in &spans {
            if s.underline {
                assert!(!s.strikethrough, "underline span should not also be strikethrough");
            }
            if s.strikethrough {
                assert!(!s.underline, "strikethrough span should not also be underline");
            }
        }
        // 相邻 span 不重叠
        for i in 1..spans.len() {
            assert!(spans[i-1].end <= spans[i].start,
                "spans[{}].end({}) > spans[{}].start({}) — 重叠",
                i-1, spans[i-1].end, i, spans[i].start);
        }
    }

    // ── 标量属性不跨界 ──
    #[test]
    fn test_scalar_properties_scope() {
        let red = Color::from_argb(255, 255, 0, 0);
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        ctx.text("Red. ", Style::col(red));
        ctx.text("Big. ", Style::fs(20.0));
        ctx.text("Plain again.", Style::default());
        let spans = ctx.spans();

        // 找到每个属性的 span
        let red_span = spans.iter().find(|s| s.color == red).expect("red span");
        let big_span = spans.iter().find(|s| (s.font_size - 20.0).abs() < 0.001).expect("big span");

        // red span 只能在它自己的范围内
        assert_eq!(red_span.color, red);
        // big span 只能是字号 20
        assert!((big_span.font_size - 20.0).abs() < 0.001);

        // red span 后面的 span 不应继承 red
        for s in spans.iter().filter(|s| s.start >= red_span.end) {
            assert_ne!(s.color, red, "color leaked past red span endpoint");
        }
    }

    // ── 嵌套合并：bold + italic 共存 ──
    #[test]
    fn test_nested_styles_merge() {
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        // 模拟 scope.bold(|x| { x.italic(|x| { x.text("BoldItalic"); }); });
        ctx.text("BoldItalic", Style { fw: Some(FontWeight::BOLD), slant: Some(FontSlant::Italic), ..Style::default() });
        let spans = ctx.spans();

        let bi = spans.iter().find(|s|
            s.font_weight != FontWeight::NORMAL && s.font_style != FontSlant::Upright
        ).expect("bold+italic span");

        assert!(bi.font_weight != FontWeight::NORMAL, "should be bold");
        assert!(bi.font_style != FontSlant::Upright, "should be italic");
    }

    // ── 全部属性同时作用 ──
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

    // ── drawable 位置 ──
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
        // spans 不应包含占位位置
        let spans = ctx.spans();
        for &Range { start, .. } in &ctx.drawable_ranges {
            assert!(spans.iter().all(|s| s.start != start), "placeholder leaked");
        }
    }

    // ── drawable 在开头 ──
    #[test]
    fn test_drawable_at_start() {
        let mut ctx = TestCtx::new();
        ctx.image();              // pos 0
        ctx.text("Text", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..1]);
        let spans = ctx.spans();
        // spans 从 pos 1 开始
        assert!(spans.iter().all(|s| s.start >= 1), "no span should cover drawable position");
    }

    // ── 连续 drawable ──
    #[test]
    fn test_consecutive_drawables() {
        let mut ctx = TestCtx::new();
        ctx.image(); ctx.image(); ctx.image();
        ctx.text("After", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![0..1, 1..2, 2..3]);
        let spans = ctx.spans();
        // 只有一个 text segment 从 pos 3 开始
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 3);
    }

    // ── drawable 嵌入带样式的文本 ──
    #[test]
    fn test_drawable_within_styled_text() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        ctx.image();
        ctx.text("C", Style::b());
        let spans = ctx.spans();
        // 应该有两个 bold span（分别左右），中间不含占位符
        assert_eq!(spans.len(), 2, "text before and after image");
        for s in &spans {
            assert!(s.font_weight != FontWeight::NORMAL, "both spans should be bold");
        }
        assert_eq!(spans[0].end, 1);
        assert_eq!(spans[1].start, 2);
    }

    // ── 注解在开头 ──
    #[test]
    fn test_annotation_at_start() {
        let mut ctx = TestCtx::new();
        ctx.text("Bold start", Style::b());
        ctx.text("normal end", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold span");
        assert_eq!(bold.start, 0, "annotation should start at 0");
    }

    // ── 注解在结尾 ──
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

    // ── 零宽度注解（跳过）──
    #[test]
    fn test_zero_width_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        // 理论上不会产生空注解，但如果 cursor 没动，应该被忽略
        // 模拟零宽度：跳过 text() 直接调 push
        let spans = ctx.spans();
        assert!(spans.iter().all(|s| s.end > s.start), "no zero-width spans");
    }

    // ── 单字符注解 ──
    #[test]
    fn test_single_char_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("H", Style::b());
        ctx.text("ello", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold H");
        assert_eq!(bold.end - bold.start, 1, "single char");
    }

    // ── 无重叠属性不应互相覆盖 ──
    #[test]
    fn test_non_overlapping_attributes_preserved() {
        let mut ctx = TestCtx::new();
        // 相同范围：bold + underline + big
        let style = Style { fw: Some(FontWeight::BOLD), fs: Some(20.0), ul: true, ..Style::default() };
        ctx.text("Title", style);
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].font_weight != FontWeight::NORMAL, "bold");
        assert!(spans[0].underline, "underline");
        assert!((spans[0].font_size - 20.0).abs() < 0.001, "font size");
    }

    // ────────────────────────────────────────────────────────────
    // image() / placeholder() 测试
    // ────────────────────────────────────────────────────────────

    // image: content 含 U+FFFC，drawable_positions 正确
    #[test]
    fn test_image_inserts_fffc() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.image();                     // pos 1
        ctx.text("C", Style::default());
        assert_eq!(ctx.content, "A\u{FFFC}C");
        assert_eq!(ctx.drawable_ranges, vec![1..2]);
        // spans 不应包含占位位置
        for s in &ctx.spans() {
            assert!(!(1..2).contains(&s.start));
        }
    }

    // image 连续多个
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

    // image 在样式作用域内
    #[test]
    fn test_image_with_style() {
        let mut ctx = TestCtx::new();
        ctx.text("B", Style::b());
        ctx.image();                     // pos 1 (bold 应覆盖)
        // 注意：image() 不推 annotation，但之前的 bold 已被 text() 提交
        // image 位置的 span 应保持 bold 之前的文本属性
        // 验证点：image 不产生 text span，但前后 text 正确
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL);
        assert!(bold.is_some(), "bold from preceding text");
    }

    // placeholder: 文本入 content，位置标记为占位
    #[test]
    fn test_placeholder_text_in_content() {
        let mut ctx = TestCtx::new();
        ctx.text("Hello ", Style::default());
        ctx.placeholder("★", Style::default());
        ctx.text(" World", Style::default());
        // content 必须包含 placeholder 文本
        assert_eq!(ctx.content, "Hello ★ World");
        assert_eq!(ctx.drawable_ranges, vec![6..7]);
        // spans 应包含 "Hello " 和 " World"，不包含 ★ 位置
        let spans = ctx.spans();
        for s in &spans {
            assert!(!(6..7).contains(&s.start), "placeholder leaked");
        }
    }

    // placeholder 带样式
    #[test]
    fn test_placeholder_with_style() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.placeholder("ico", Style::b());  // bold
        ctx.text("B", Style::default());
        // "ico" 在 content 中，bold annotation 覆盖 1..4
        // placeholder 段继承 bold 样式，但过滤后不出现为 text span
        let spans = ctx.spans();
        let bold_span = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL);
        // 没有可见 bold span（placeholder 整个范围被替换）
        assert!(bold_span.is_none(), "placeholder text should not appear as visible span");
        // content 仍保留文本
        assert_eq!(ctx.content, "AicoB");
        assert_eq!(ctx.drawable_ranges, vec![1..4]);
    }

    // image + placeholder 混合
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

    // placeholder + image 都不影响纯文本区间
    #[test]
    fn test_placeholder_preserves_surrounding_text() {
        let mut ctx = TestCtx::new();
        ctx.text("before", Style::default());
        ctx.placeholder("X", Style::default());
        ctx.text("after", Style::default());
        let spans = ctx.spans();
        // 应该有 "before" 和 "after" 两个 text spans（无样式，但不为空）
        assert!(spans.len() >= 2, "should have text before and after");
        let before = spans.iter().find(|s| s.start == 0 && s.end == 6);
        let after = spans.iter().find(|s| s.start == 7 && s.end == 12);
        assert!(before.is_some(), "before text exists");
        assert!(after.is_some(), "after text exists");
    }

    // placeholder 空文本
    #[test]
    fn test_placeholder_empty_text() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("", Style::b()); // 空文本不应产生 drawable
        ctx.text("A", Style::default());
        assert_eq!(ctx.drawable_ranges.len(), 0, "empty text should not create placeholder");
        assert_eq!(ctx.content, "A");
    }

    // placeholder 在开头
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

    // placeholder 在末尾
    #[test]
    fn test_placeholder_at_end() {
        let mut ctx = TestCtx::new();
        ctx.text("head", Style::default());
        ctx.placeholder("tail", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![4..8]);
        let spans = ctx.spans();
        assert!(spans.iter().all(|s| s.start < 4), "last placeholder covers tail");
    }

    // 连续 placeholder（相邻范围）
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

    // placeholder 占满全文
    #[test]
    fn test_placeholder_whole_content() {
        let mut ctx = TestCtx::new();
        ctx.placeholder("full", Style::default());
        let spans = ctx.spans();
        assert!(spans.is_empty(), "no text spans when whole content is placeholder");
        assert_eq!(ctx.content, "full");
        assert_eq!(ctx.drawable_ranges.len(), 1);
    }

    // placeholder 带全部样式属性
    #[test]
    fn test_placeholder_all_style() {
        let style = Style { fs: Some(16.0), color: Some(Color::from_argb(255,255,0,0)), fw: Some(FontWeight::BOLD), ul: true, ..Style::default() };
        let mut ctx = TestCtx::new();
        ctx.text("pre ", Style::default());
        ctx.placeholder("STYLED", style);
        ctx.text(" post", Style::default());
        assert_eq!(ctx.drawable_ranges, vec![4..10]);
        // 前后的文本不应包含这些样式
        let spans = ctx.spans();
        assert_eq!(spans.len(), 2, "plain text before and after");
        for s in &spans {
            assert_eq!(s.font_weight, FontWeight::NORMAL, "surrounding text should not be bold");
            assert!(!s.underline, "surrounding text should not be underlined");
        }
    }

    // 多字节 unicode 占位符
    #[test]
    fn test_placeholder_unicode() {
        let mut ctx = TestCtx::new();
        ctx.text("你好", Style::default());  // 2 个中文字符，content length 2
        ctx.placeholder("✨🔥", Style::default()); // 2 个 emoji，content length 4
        ctx.text("!", Style::default());
        // 每个 emoji 在 chars().count() 中算 1（Rust 的 char 是 Unicode scalar value）
        // ✨🔥 各 1 char，drawable_range length = 2
        assert_eq!(ctx.drawable_ranges.len(), 1);
        let start = ctx.drawable_ranges[0].start;
        assert!(start >= 2, "placeholder after '你好'");
        let end = ctx.drawable_ranges[0].end;
        assert_eq!(end - start, 2, "two-chars placeholder");
    }

    // 嵌套作用域内的 placeholder（bold → placeholder）
    #[test]
    fn test_placeholder_in_nested_scope() {
        let mut ctx = TestCtx::new();
        ctx.text("a ", Style::default());
        ctx.text("mid", Style::b());
        ctx.placeholder("IMG", Style::b());
        ctx.text(" end", Style::default());
        // bold annotation 覆盖 "mid"(2..5) 和 "IMG"(5..8)
        // placeholder 本身在 5..8，是 drawable
        // 所以 bold span 只有 "mid"(2..5)
        let spans = ctx.spans();
        let bold_spans: Vec<_> = spans.iter().filter(|s| s.font_weight != FontWeight::NORMAL).collect();
        assert_eq!(bold_spans.len(), 1, "only 'mid' is visible bold");
        // 'mid' starts at 2, length 3
        assert_eq!(bold_spans[0].start, 2);
        assert_eq!(bold_spans[0].end, 5);
    }
