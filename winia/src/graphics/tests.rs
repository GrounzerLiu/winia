    use super::*;


    /// The element comparison treats two dynamic axes as the same parameter, but a literal turning
    /// into a state (or back) as a CHANGE: Skip would otherwise keep drawing the stale literal.
    #[test]
    fn axis_value_equality_separates_parameters_from_values() {
        use crate::runtime::state::State;
        let a = SymbolAxes { wght: AxisValue::from(&State::new(400.0f32)), ..Default::default() };
        let b = SymbolAxes { wght: AxisValue::from(&State::new(700.0f32)), ..Default::default() };
        assert_eq!(a, b, "two dynamic axes are the same parameter");

        let literal = SymbolAxes { wght: AxisValue::from(400.0f32), ..Default::default() };
        assert_ne!(literal, a, "a literal and a state are different requests");
        assert_ne!(a, literal, "and the comparison is symmetric");

        let same_literal = SymbolAxes { wght: AxisValue::from(400.0f32), ..Default::default() };
        assert_eq!(literal, same_literal, "two equal literals are equal");
    }

    #[test]
    fn wrap_svg_path_is_valid_document() {
        let wrapped = wrap_svg_path("M19 13h-6v6z", PathFillType::NonZero);
        assert!(wrapped.starts_with("<svg"));
        assert!(wrapped.contains("M19 13h-6v6z"));
        assert!(wrapped.contains("fill-rule=\"nonzero\""));
        assert_eq!(parse_view_box(&wrapped), Some((24.0, 24.0)));
    }
    #[test]
    fn axis_clamps_to_official_ranges() {
        let axes = SymbolAxes {
            fill: AxisValue::Static(5.0),
            grad: AxisValue::Static(-999.0),
            opsz: AxisValue::Static(2.0),
            wght: AxisValue::Static(9999.0),
        };
        assert_eq!(axes.evaluated(), (1.0, -50.0, 20.0, 700.0));
    }
    #[test]
    fn view_box_parsing() {
        assert_eq!(
            parse_view_box("<svg viewBox='0 0 24 24'><path d='M0 0'/></svg>"),
            Some((24.0, 24.0))
        );
        // width/height 属性优先于 viewBox（布局尺寸语义）
        assert_eq!(
            parse_view_box("<svg width='32' height='32' viewBox='0 0 960 960'></svg>"),
            Some((32.0, 32.0))
        );
        assert_eq!(parse_view_box("<svg width='24px' height='24px'></svg>"), Some((24.0, 24.0)));
        assert_eq!(parse_view_box("<svg></svg>"), None);
    }

    #[test]
    fn test_content_scale_fit_keeps_aspect() {
        // bounds 200x100、图 100x60（1.67:1）→ Fit：s=min(2, 1.67)=1.67 → 166.7x100 居中
        let r = content_scale_rect(ContentScale::Fit, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 60.0, ImageAlignment::Center, false);
        assert!((r.width() - 166.7).abs() < 0.1, "宽 {}", r.width());
        assert!((r.height() - 100.0).abs() < 0.01, "高 {}", r.height());
        assert!((r.left - 16.65).abs() < 0.1, "x {}", r.left);
        assert!((r.top - 0.0).abs() < 0.01, "y {}", r.top);
    }
    #[test]
    fn test_content_scale_crop_covers_bounds() {
        // bounds 200x100、图 100x100 → Crop：scale=max(2,1)=2 → 200x200，裁剪上下
        let r = content_scale_rect(ContentScale::Crop, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 100.0, ImageAlignment::Center, false);
        assert_eq!((r.width(), r.height()), (200.0, 200.0));
        assert_eq!((r.left, r.top), (0.0, -50.0), "超出部分在上下");
    }
    #[test]
    fn test_content_scale_inside_never_upscales() {
        // bounds 200x100、图 100x50 → Inside：100x50（不放大）
        let r = content_scale_rect(ContentScale::Inside, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (100.0, 50.0));
        // 大图 400x200 → Inside：缩小到 200x100
        let r2 = content_scale_rect(ContentScale::Inside, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 400.0, 200.0, ImageAlignment::Center, false);
        assert_eq!((r2.width(), r2.height()), (200.0, 100.0));
    }
    #[test]
    fn test_content_scale_fill_width_height() {
        // 非对称 bounds 200x80（区分两分支）：
        // FillWidth：宽 200，高按比例（100x50 → 200x100，超出高）
        let r = content_scale_rect(ContentScale::FillWidth, Rect::from_xywh(0.0, 0.0, 200.0, 80.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (200.0, 100.0));
        // FillHeight：高 80，宽按比例（100x50 → 160x80）
        let r2 = content_scale_rect(ContentScale::FillHeight, Rect::from_xywh(0.0, 0.0, 200.0, 80.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r2.width(), r2.height()), (160.0, 80.0));
    }
    #[test]
    fn test_content_scale_none_keeps_the_intrinsic_size() {
        let r = content_scale_rect(ContentScale::None, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (100.0, 50.0), "None must not scale");
        let c = content_scale_rect(ContentScale::None, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::Center, false);
        assert_eq!((c.left, c.top), (50.0, 25.0), "alignment still places it");
        // The old winia behaviour is still reachable, under Compose's real name.
        let f = content_scale_rect(ContentScale::FillBounds, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((f.width(), f.height()), (200.0, 100.0), "FillBounds stretches");
    }
    #[test]
    fn test_alignment_positions_within_bounds() {
        let bounds = Rect::from_xywh(0.0, 0.0, 200.0, 100.0);
        // Fit 图 100x60 → 167x100：TopStart → (0,0)；BottomEnd → (33,0)
        let tl = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::TopStart, false);
        assert_eq!((tl.left, tl.top), (0.0, 0.0));
        let br = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::BottomEnd, false);
        assert!((br.left - 33.3).abs() < 0.1, "x {}", br.left);
        assert_eq!(br.top, 0.0, "高已填满，y=0");
        // RTL：TopStart 镜像到右侧
        let rtl = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::TopStart, true);
        assert!((rtl.left - 33.3).abs() < 0.1, "RTL x {}", rtl.left);
        assert_eq!(rtl.top, 0.0);
    }
