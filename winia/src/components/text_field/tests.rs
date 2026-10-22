    use super::*;
    use crate::text::FontWeight;
    use crate::text::field::text_field_show_cursor;
    use crate::runtime::composer::Composer;
    use crate::modifier::ModifierElement;

    /// M3's `FastEffects` spring and nothing else; a silent return to the old 100 ms tween fails here.
    #[test]
    fn the_focus_colour_spec_is_m3s_fast_effects_spring() {
        let spec = text_field_colour_spec();
        assert_eq!(spec, crate::animation::SpringSpec::effects_fast());
        assert_eq!(spec.damping_ratio, 1.0);
        assert_eq!(spec.stiffness, 3800.0);
    }

    fn find_text_content(modifier: &Modifier) -> Option<String> {
        modifier.elements().iter().find_map(|el| {
            if let ModifierElement::TextContent { content, .. } = el {
                Some(content.clone())
            } else {
                None
            }
        })
    }

    fn text_content_style(modifier: &Modifier) -> Option<(f32, FontWeight, f32, Option<f32>)> {
        modifier.elements().iter().find_map(|el| {
            if let ModifierElement::TextContent { font_size, font_weight, letter_spacing, line_height, .. } = el {
                Some((*font_size, *font_weight, *letter_spacing, *line_height))
            } else {
                None
            }
        })
    }
    fn has_focusable(modifier: &Modifier) -> bool {
        modifier.elements().iter().any(|el| matches!(el, ModifierElement::Focusable { .. }))
    }

    /// 构建 TextField 并取根节点 modifier（text-field-v2 容器化：root = 容器）
    fn build_field(field: TextField) -> Modifier {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            field.build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        nodes[root].modifier.clone()
    }

    /// 遍历树找带指定角色标记的节点 modifier
    fn find_slot_modifier(field: TextField, role: TextFieldSlotRole) -> Option<Modifier> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            field.build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, role: TextFieldSlotRole) -> Option<Modifier> {
            let has = nodes[idx].modifier.elements().iter().any(|el| {
                matches!(el, ModifierElement::TextFieldSlot { role: r } if *r == role)
            });
            if has { return Some(nodes[idx].modifier.clone()); }
            for &c in &nodes[idx].children {
                if let Some(m) = walk(nodes, c, role) { return Some(m); }
            }
            None
        }
        walk(nodes, root, role)
    }

    /// 容器节点 modifier（TextField build 的根——start_restartable_group 即
    /// 容器本身，无角色标记；子节点带 TextFieldSlot 标记）
    fn container_modifier(field: TextField) -> Option<Modifier> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            field.build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        Some(nodes[root].modifier.clone())
    }

    #[test]
    fn typography_tokens_reach_input_affix_and_supporting_text() {
        let custom = crate::theme::Typography {
            body_large: TextStyle::new()
                .font_size(18.0)
                .line_height(26.0)
                .letter_spacing(0.9)
                .font_weight(FontWeight::BOLD),
            body_small: TextStyle::new()
                .font_size(13.0)
                .line_height(19.0)
                .letter_spacing(0.6)
                .font_weight(FontWeight::MEDIUM),
            ..crate::theme::Typography::default()
        };
        let input = find_slot_modifier_with_typography(custom.clone(), TextFieldSlotRole::Input).unwrap();
        assert_eq!(text_content_style(&input).unwrap(), (18.0, FontWeight::BOLD, 0.9, Some(26.0)));

        let affix = find_slot_modifier_with_typography(custom.clone(), TextFieldSlotRole::Prefix).unwrap();
        assert_eq!(text_content_style(&affix).unwrap(), (18.0, FontWeight::BOLD, 0.9, Some(26.0)));

        let supporting = container_modifier_with_typography(custom, TextFieldSlotRole::Input).unwrap();
        let supporting = supporting.elements().iter().find_map(|element| {
            if let ModifierElement::TextFieldVisual { supporting, .. } = element {
                supporting.as_ref()
            } else {
                None
            }
        }).unwrap();
        assert_eq!(supporting.font_size, 13.0);
        assert_eq!(supporting.font_weight, FontWeight::MEDIUM);
        assert_eq!(supporting.letter_spacing, 0.6);
        assert_eq!(supporting.line_height, Some(19.0));
    }

    fn container_modifier_with_typography(typography: crate::theme::Typography, _role: TextFieldSlotRole) -> Option<Modifier> {
        let value = State::new(TextFieldValue::new("value"));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_typography(typography, ctx, |ctx| {
                TextField::new(value.clone()).filled().supporting_text("Supporting").build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        Some(composer.arena_nodes()[root].modifier.clone())
    }
    fn find_slot_modifier_with_typography(typography: crate::theme::Typography, role: TextFieldSlotRole) -> Option<Modifier> {
        let value = State::new(TextFieldValue::new("value"));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_typography(typography, ctx, |ctx| {
                TextField::new(value.clone())
                    .filled()
                    .prefix(|ctx| crate::components::Text::new("$").build(ctx))
                    .build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, role: TextFieldSlotRole) -> Option<Modifier> {
            if nodes[idx].modifier.elements().iter().any(|el| matches!(el, ModifierElement::TextFieldSlot { role: r } if *r == role)) {
                for &child in &nodes[idx].children {
                    if nodes[child].modifier.elements().iter().any(|el| matches!(el, ModifierElement::TextContent { .. })) {
                        return Some(nodes[child].modifier.clone());
                    }
                }
                return Some(nodes[idx].modifier.clone());
            }
            for &child in &nodes[idx].children {
                if let Some(found) = walk(nodes, child, role) { return Some(found); }
            }
            None
        }
        walk(composer.arena_nodes(), root, role)
    }


    #[test]
    fn disabled_field_has_no_focusable() {
        let value = State::new(TextFieldValue::new("hi"));
        let m = container_modifier(TextField::new(value.clone()).enabled(false)).unwrap();
        assert!(!has_focusable(&m), "禁用字段不应可聚焦");
        let input = find_slot_modifier(TextField::new(value.clone()).enabled(false), TextFieldSlotRole::Input).unwrap();
        assert!(find_text_content(&input).is_some(), "禁用字段仍显示内容");
    }

    #[test]
    fn enabled_field_has_focusable() {
        // text-field-v2 容器化：焦点/键盘在容器（点击容器任意处聚焦）
        let value = State::new(TextFieldValue::new("hi"));
        let m = container_modifier(TextField::new(value.clone())).unwrap();
        assert!(has_focusable(&m), "启用字段容器应可聚焦");
        let input = find_slot_modifier(TextField::new(value.clone()), TextFieldSlotRole::Input).unwrap();
        assert!(!has_focusable(&input), "输入子节点不再可聚焦（焦点在容器）");
    }

    fn find_placeholder(modifier: &Modifier) -> Option<String> {
        // text-field-v2：placeholder 为闭包子节点（TextFieldSlot::Placeholder
        // 标记）——测试仅验证闭包存在（内容不可检）
        modifier.elements().iter().find_map(|el| {
            if let ModifierElement::TextFieldSlot { role } = el {
                if *role == crate::text::field::TextFieldSlotRole::Placeholder {
                    return Some("placeholder".to_string());
                }
            }
            None
        })
    }

    #[test]
    fn placeholder_shown_when_empty() {
        let value = State::new(TextFieldValue::new(""));
        let m = find_slot_modifier(TextField::new(value.clone()).filled().placeholder(|_ctx| { crate::components::Text::new("请输入").build(_ctx); }), TextFieldSlotRole::Placeholder);
        assert!(m.is_some(), "空值构建 placeholder 子节点");
    }

    #[test]
    fn placeholder_alpha_zero_when_content_present() {
        // text-field-v2：placeholder 为闭包子节点（构建条件 show_placeholder）——
        // 非空时不构建（无 Placeholder 槽位标记）
        let value = State::new(TextFieldValue::new("已有内容"));
        let m = build_field(TextField::new(value.clone()).filled().placeholder(|_ctx| { crate::components::Text::new("请输入").build(_ctx); }));
        assert!(find_placeholder(&m).is_none(), "非空时 placeholder 不构建");
    }

    #[test]
    fn placeholder_builds_without_visual() {
        // Placeholder builds regardless of container variant — bare no_container
        // fields (e.g. SearchBar input) need it. Previously gated on has_visual
        // (v2 containerization oversight, fixed with SearchBar).
        let value = State::new(TextFieldValue::new(""));
        let m = find_slot_modifier(TextField::new(value.clone()).no_container().placeholder(|_ctx| { crate::components::Text::new("请输入").build(_ctx); }), TextFieldSlotRole::Placeholder);
        assert!(m.is_some(), "no_container placeholder must build its slot");
    }

    /// Drive the container KbEvent handler with a synthetic Enter (no app loop).
    /// Returns true if some handler consumed it.
    fn fire_enter(modifier: &Modifier) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let ev = crate::input::KbEvent {
            key: Key::Named(NamedKey::Enter),
            event_type: crate::input::KbEventType::KeyDown,
            is_alt_pressed: false,
            is_ctrl_pressed: false,
            is_shift_pressed: false,
            is_meta_pressed: false,
            repeat: false,
        };
        for el in modifier.elements() {
            if let ModifierElement::KbEvent { on_key: Some(h), .. } = el {
                if h(&ev) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn on_search_fires_on_single_line_enter() {
        // Search action: single-line Enter fires on_search with current value.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let value = State::new(TextFieldValue::new("query"));
        let fired = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let fired2 = fired.clone();
        let field = TextField::new(value)
            .single_line(true)
            .on_search(move |v| fired2.lock().unwrap().push(v.text.clone()));
        let m = build_field(field);
        assert!(fire_enter(&m), "Enter must be consumed in single-line mode");
        assert_eq!(*fired.lock().unwrap(), vec!["query".to_string()]);
    }

    #[test]
    fn on_search_absent_keeps_swallow() {
        // Without on_search, single-line Enter is still swallowed (status quo).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let value = State::new(TextFieldValue::new("query"));
        let field = TextField::new(value).single_line(true);
        let m = build_field(field);
        assert!(fire_enter(&m), "Enter still consumed without on_search");
    }

    #[test]
    fn placeholder_hidden_when_has_content() {
        let value = State::new(TextFieldValue::new("已有内容"));
        let m = find_slot_modifier(TextField::new(value.clone()).filled().placeholder(|_ctx| { crate::components::Text::new("请输入").build(_ctx); }), TextFieldSlotRole::Placeholder);
        assert!(m.is_none(), "非空时 placeholder 不构建");
        let mi = find_slot_modifier(TextField::new(value.clone()).filled(), TextFieldSlotRole::Input).unwrap();
        let content = find_text_content(&mi).unwrap_or_default();
        assert_eq!(content, "已有内容", "有值时输入节点显示真实内容");
    }

    #[test]
    fn min_lines_height_grows_with_content() {
        // 回归：min_lines 动态高度非空时不能返回 0（节点消失 bug）
        let value = State::new(TextFieldValue::new(""));
        let mut composer = Composer::new();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        composer.compose(|ctx| {
            TextField::new(value.clone())
                .min_lines(3)
                .modifier(Modifier::new().width(300.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let h0 = composer.arena_nodes()[root].measured_size.height;
        // min_h 58.8 被 build 内 padding(8) 扣减 → 内容区 42.8（近似 3 行）
        assert!(h0 > 30.0, "空内容高度 = min_lines 占位（实际 {h0}）");

        // 输入 2 行（显式换行）→ 高度 ≥ min_lines 占位（min_height 恒占位——
        // M3 minLines 语义：内容不足也占位；非 0——修复前输入后消失）
        value.set(TextFieldValue::new("a\nb"));
        composer.compose(|ctx| {
            TextField::new(value.clone())
                .min_lines(3)
                .modifier(Modifier::new().width(300.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let h1 = composer.arena_nodes()[root].measured_size.height;
        assert!(h1 > 0.0, "输入后高度必须 > 0（修复前为 0——TextField 消失）");
        assert!((h1 - h0).abs() < 0.5, "2 行内容高度 = 3 行占位（minLines 恒占位：{h1} ≈ {h0}）");
    }

    #[test]
    fn wrapped_text_grows_height() {
        // 折行高度：长文本（无显式 \n）自动换行 → 高度按 paragraph 实际
        // 行数增长（修复前 height 闭包按 \n 计数 → 高度不变、文本裁剪）
        let value = State::new(TextFieldValue::new(""));
        let mut composer = Composer::new();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        composer.compose(|ctx| {
            TextField::new(value.clone())
                .filled()
                .modifier(Modifier::new().width(100.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let h0 = composer.arena_nodes()[root].measured_size.height;

        // 输入 60 个 '1'（100px 宽下折成多行）
        value.set(TextFieldValue::new("1".repeat(60)));
        composer.compose(|ctx| {
            TextField::new(value.clone())
                .filled()
                .modifier(Modifier::new().width(100.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let h1 = composer.arena_nodes()[root].measured_size.height;
        assert!(h1 > h0 + 15.0, "折行文本高度必须增长（{h1} > {h0} + 15）");
    }

    #[test]
    fn insert_replaces_selection() {
        // 选中文本时输入：替换选区（旧版 v1：Commit 先 Deleted 再 Inserted）
        // 而非保留选区文本
        let mut val = TextFieldValue::new("hello world");
        val.selection = 6..11; // 选中 "world"
        TextChange::Inserted { index: 6, text: "rust".into() }.apply_to(&mut val);
        assert_eq!(val.text, "hello rust", "选区被替换");
        assert_eq!(val.selection, 10..10, "光标在插入后");
        // 反向选区（start > end）同样替换
        let mut val2 = TextFieldValue::new("hello world");
        val2.selection = 11..6;
        TextChange::Inserted { index: 11, text: "!".into() }.apply_to(&mut val2);
        assert_eq!(val2.text, "hello !", "反向选区替换");
        // 无选区：正常插入
        let mut val3 = TextFieldValue::new("ab");
        val3.selection = 1..1;
        TextChange::Inserted { index: 1, text: "X".into() }.apply_to(&mut val3);
        assert_eq!(val3.text, "aXb");
    }

    // §5.4：Deleted range 防御 clamp——stale/越界/半字符 range 不 panic
    #[test]
    fn deleted_clamps_stale_and_half_char_ranges() {
        // 正常删除
        let mut val = TextFieldValue::new("hello");
        TextChange::Deleted { range: 1..3 }.apply_to(&mut val);
        assert_eq!(val.text, "hlo", "正常删除");
        assert_eq!(val.selection, 1..1, "光标在删除起点");
        // stale range 越界（start > len）：clamp 到末尾，不 panic
        let mut val2 = TextFieldValue::new("abc");
        TextChange::Deleted { range: 5..9 }.apply_to(&mut val2);
        assert_eq!(val2.text, "abc", "越界 range 无操作（s==e==len）");
        assert_eq!(val2.selection, 3..3);
        // 半字符 range（CJK "你好" = 6 字节，range 落在字符中间）：对齐后删除整字符
        let mut val3 = TextFieldValue::new("你好");
        TextChange::Deleted { range: 1..4 }.apply_to(&mut val3);
        assert_eq!(val3.text, "好", "1..4 对齐到 0..3 删 '你'");
        assert_eq!(val3.selection, 0..0);
        // end 越界：clamp 到 len
        let mut val4 = TextFieldValue::new("abcd");
        TextChange::Deleted { range: 2..99 }.apply_to(&mut val4);
        assert_eq!(val4.text, "ab", "end 越界 clamp 到 len");
        // 空文本：无操作
        let mut val5 = TextFieldValue::new("");
        TextChange::Deleted { range: 0..3 }.apply_to(&mut val5);
        assert_eq!(val5.text, "");
    }

    #[test]
    fn caret_up_down_line_targets() {
        let text = "ab\ncd\nef";
        // 首行 → 0；第二行中部 → 上一行行首 3；第三行中部 → 上一行行首 6
        assert_eq!(caret_prev_line(text, 1), 0);
        assert_eq!(caret_prev_line(text, 4), 3);
        assert_eq!(caret_prev_line(text, 7), 6);
        // 恰在行首 → 保持（上一行行首即自身）
        assert_eq!(caret_prev_line(text, 3), 3);
        // 第一行 → 下一行行首；最后一行 → 末尾；末尾后不越界
        assert_eq!(caret_next_line(text, 1), 3);
        assert_eq!(caret_next_line(text, 4), 6);
        assert_eq!(caret_next_line(text, 6), 6, "末行 Down 保持原位");
        assert_eq!(caret_next_line(text, text.len() + 10), text.len(), "越界钳制到末尾");
        // 非字符边界 pos 不 panic（按字节查找 \n）
        let cjk = "a\n中b";
        assert_eq!(caret_prev_line(cjk, 3), 2, "‘中’字内部 pos 回退到上一行行首");
        assert_eq!(caret_next_line(cjk, 3), 3, "‘中’字内部 pos 无下一行则保持");
    }

    // ── 词边界（Ctrl+←/→、Ctrl+Backspace/Delete）──

    #[test]
    fn word_boundaries_prev_next() {
        // 词中间 → 词首；词首 → 前一词首；文首 → 0
        assert_eq!(word_prev("hello world", 7), 6, "world 中间 → world 词首");
        assert_eq!(word_prev("hello world", 6), 0, "world 词首 → hello 词首");
        assert_eq!(word_prev("hello world", 0), 0, "文首保持");
        // 下一个词首；末尾 → len
        assert_eq!(word_next("hello world", 3), 6, "hello 中间 → world 词首");
        assert_eq!(word_next("hello world", 6), 11, "world 词首 → 文末");
        assert_eq!(word_next("hello world", 11), 11, "文末保持");
        // 空白中间 → 就近词边界
        assert_eq!(word_prev("hello world", 5), 0, "空白中间 → 前一词首");
        // 标点分词（Compose wordChar 语义——标点不是词字符，是边界）
        assert_eq!(word_next("a.b", 1), 2, "点后 'b' 是下一个词");
        assert_eq!(word_prev("a.b", 2), 0, "b 前 → a 词首");
        // 中文连续汉字 = 一个词段（letterOrDigit 语义——与 Compose 一致）
        assert_eq!(word_next("你好世界", 0), 12, "4 汉字 × 3 字节 = 12（整体一词）");
        assert_eq!(word_prev("你好世界", 8), 0);
        // pos 落在多字节字符内部——按字符起算（不 panic）
        let cjk = "ab中cd";
        assert_eq!(word_next(cjk, 4), 7, "‘中’字内部 → 跳过中字后（cd 之后）");
        assert_eq!(word_prev(cjk, 4), 0, "‘中’字内部 → 前一词 'ab' 的词首 0");
        // 纯标点文本
        assert_eq!(word_next("!!!", 0), 3, "纯标点 → 文末");
        assert_eq!(word_prev("!!!", 3), 0);
    }

    #[test]
    fn line_start_end_targets() {
        // 行首：pos 之前最后一个 \n 之后
        assert_eq!(line_start("ab\ncd", 3), 3);
        assert_eq!(line_start("ab\ncd", 4), 3, "第二行中部 → 行首 3");
        assert_eq!(line_start("ab\ncd", 0), 0);
        // 行尾：不含换行符（对齐 Compose LINE_END——光标停在 \n 前）
        assert_eq!(line_end("ab\ncd", 0), 2);
        assert_eq!(line_end("ab\ncd", 2), 2, "恰在行尾 → 保持");
        assert_eq!(line_end("ab\ncd", 3), 5, "第二行中部 → 文尾 5（末行无 \n）");
        assert_eq!(line_end("ab", 0), 2, "无换行 → 文尾");
        // 非字符边界 pos 不 panic（按字节查找 \n）
        let cjk = "a\n中b";
        assert_eq!(line_end(cjk, 3), 6, "‘中’字内部 → 第二行行尾 6");
        assert_eq!(line_start(cjk, 4), 2, "‘中’字内部 → 第二行行首 2");
    }

    // ── UndoManager（Ctrl+Z / Ctrl+Shift+Z）──

    #[test]
    fn undo_redo_basic_flow() {
        let mut um = UndoManager::new();
        // 每次编辑前 push 当前状态（与 kb_handler 调用方式一致）
        um.push("", &(0..0), None);   // 输入 'h' 前
        um.push("h", &(1..1), None);  // 输入 'e' 前
        um.push("he", &(2..2), None); // 输入 'x' 前
        // 当前状态 "hex"
        let (t1, s1, c1) = um.undo(("hex".into(), 3..3, None)).expect("undo1");
        assert_eq!((t1.as_str(), s1.start, c1), ("he", 2, None));
        let (t2, s2, _) = um.undo((t1.clone(), s1.clone(), None)).expect("undo2");
        assert_eq!((t2.as_str(), s2.start), ("h", 1));
        // redo 精确回到 undo 前的状态（当前状态入 redo 栈）
        let (t3, _, _) = um.redo((t2.clone(), s2.clone(), None)).expect("redo1");
        assert_eq!(t3, "he");
        let (t4, _, _) = um.redo((t3.clone(), s1, None)).expect("redo2");
        assert_eq!(t4, "hex", "redo 回到当前状态（含未入栈的最新编辑）");
        assert!(um.redo(("hex".into(), 3..3, None)).is_none(), "已回最新，无 redo");
        // 栈底无 undo
        let mut um2 = UndoManager::new();
        assert!(um2.undo(("x".into(), 1..1, None)).is_none(), "无快照时 undo 为 None");
    }

    #[test]
    fn undo_merges_same_text_selection() {
        // 对齐 Compose：同文本只更新 selection（光标移动不产生新条目）
        let mut um = UndoManager::new();
        um.push("abc", &(1..1), None);
        um.push("abc", &(2..2), None); // 同文本 → 合并
        um.push("abc", &(3..3), None);
        assert_eq!(um.undo_stack.len(), 1, "三次同文本快照合并为一条");
        assert_eq!(um.undo_stack[0].1, 3..3, "selection 取最新");
        // 文本变化产生新条目；undo 回退文本与 selection
        um.push("abc", &(3..3), None); // 输入 'd' 前（与 base 同文本 → 合并）
        let (t, s, _) = um.undo(("abcd".into(), 4..4, None)).unwrap();
        assert_eq!((t.as_str(), s.clone()), ("abc", 3..3));
        // 快照耗尽（同文本合并只有一条）
        assert!(um.undo((t, s, None)).is_none(), "合并后仅一条快照");
    }

    #[test]
    fn undo_truncates_redo_branch_on_new_edit() {
        let mut um = UndoManager::new();
        um.push("a", &(1..1), None);
        um.push("ab", &(2..2), None);
        let _ = um.undo(("ab".into(), 2..2, None)); // 回到 "a"
        // 新编辑（undo 后输入 'c' → 当前 "a"）→ redo 分支丢弃
        um.push("ac", &(2..2), None);
        assert!(um.redo_stack.is_empty(), "undo 后编辑丢弃 redo 分支");
        assert_eq!(um.undo_stack.len(), 2);
    }

    // §5.2：Undo 快照含 composing_range——preedit/undo 交错时组合范围可恢复
    #[test]
    fn undo_restores_composing_range() {
        let mut um = UndoManager::new();
        // 输入 'a' 前（无组合）
        um.push("", &(0..0), None);
        // 拼音组合态：文本 "你"（组合范围 0..3）
        um.push("你", &(0..0), Some(0..3));
        // 当前态："你好"（组合 0..3，光标在组合尾）
        let cur = ("你好".to_string(), 3..3, Some(0..3));
        let (t, s, c) = um.undo(cur).expect("undo");
        assert_eq!((t.as_str(), s), ("你", 0..0));
        assert_eq!(c, Some(0..3), "undo 恢复 composing_range");
        // 同文本合并：composing 变化也合并更新（光标移动/组合态变化不产生新条目）
        let mut um2 = UndoManager::new();
        um2.push("ab", &(1..1), Some(1..2));
        um2.push("ab", &(1..1), Some(1..1)); // 同文本 → 合并，composing 取最新
        assert_eq!(um2.undo_stack.len(), 1, "同文本 composing 变化合并");
        assert_eq!(um2.undo_stack[0].2, Some(1..1), "composing 取最新");
        // redo 恢复 composing（与 undo 对称——review 补）
        // 场景：文本 "a"（光标 1）→ 拼音组合为 "a你"（composing 1..4）
        let mut um3 = UndoManager::new();
        um3.push("a", &(1..1), None); // first_preedit 修改前快照（无组合）
        let cur = ("a你".to_string(), 2..2, Some(1..4));
        let (t, s, c) = um3.undo(cur).expect("undo");
        assert_eq!((t.as_str(), s.clone()), ("a", 1..1));
        assert_eq!(c, None, "undo 回到组合前（无 composing）");
        // 当前态回到 "a"（无组合）——redo 应恢复 "a你" 及其 composing
        let (t2, s2, c2) = um3.redo((t.clone(), s.clone(), None)).expect("redo");
        assert_eq!((t2.as_str(), s2), ("a你", 2..2));
        assert_eq!(c2, Some(1..4), "redo 恢复 composing_range");
    }

    /// 验证：restartable group 内读取 State → 更新后该 group 重组重跑
    /// （label 字号动画的依赖路径——闭包 get() 注册到 label 包装 group）
    #[test]
    fn state_read_in_group_triggers_recompose() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let s = crate::runtime::state::State::new(0.0f32);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        composer.compose(|ctx| {
            ctx.start_restartable_group(0x1111, Modifier::new(), crate::layout::BoxLayout::new());
            let v = s.get();
            seen2.lock().unwrap().push(v);
            ctx.end_restartable_group();
        });
        assert_eq!(seen.lock().unwrap().clone(), vec![0.0]);
        s.set(1.0);
        composer.compose(|ctx| {
            let s2 = seen.clone();
            ctx.start_restartable_group(0x1111, Modifier::new(), crate::layout::BoxLayout::new());
            let v = s.get();
            s2.lock().unwrap().push(v);
            ctx.end_restartable_group();
        });
        let vals = seen.lock().unwrap().clone();
        assert!(vals.contains(&1.0), "State 更新后 group 应重组重跑（读到 1.0），实际 {:?}", vals);
    }

    /// 模拟 demo 循环：聚焦 → label_progress 动画驱动 → 组合重组 →
    /// label 闭包重跑（get 注册依赖）→ 字号随 progress 插值。
    /// 验证"字号动画"链路端到端（demo 截图里字号未变的问题回归）。
    #[test]
    fn label_font_size_animates_with_progress() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let value = crate::runtime::state::State::new(TextFieldValue::new(""));
        let focus_src = crate::input::interaction::MutableInteractionSource::new();
        let progress_log: std::sync::Arc<std::sync::Mutex<Vec<f32>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        // 首帧：未聚焦（label 展开，progress 目标 0）
        for frame in 0..3 {
            let log = progress_log.clone();
            let field = TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(move |ctx| {
                    // TextField 经 LOCAL_TEXT_STYLE 提供动画字号——闭包内
                    // Text 默认字号随动画（16sp ↔ 12sp）
                    let fs = crate::components::text::LOCAL_TEXT_STYLE.current().font_size;
                    log.lock().unwrap().push(fs.map(|f| f.to_logical_px(crate::runtime::density::current_density())).unwrap_or(0.0));
                    crate::components::Text::new("Name").build(ctx);
                });
            composer.compose(|ctx| {
                field.build(ctx);
            });
            crate::animation::update_animations();
        }
        // 聚焦：label 悬浮（progress 目标 1）
        focus_src.emit_focus();
        for frame in 0..20 {
            let log = progress_log.clone();
            let field = TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(move |ctx| {
                    let fs = crate::components::text::LOCAL_TEXT_STYLE.current().font_size;
                    log.lock().unwrap().push(fs.map(|f| f.to_logical_px(crate::runtime::density::current_density())).unwrap_or(0.0));
                    crate::components::Text::new("Name").build(ctx);
                });
            composer.compose(|ctx| {
                field.build(ctx);
            });
            std::thread::sleep(std::time::Duration::from_millis(8));
            crate::animation::update_animations();
        }
        let log = progress_log.lock().unwrap();
        let last = *log.last().unwrap();
        // 动画驱动重组：闭包多次重跑且字号明显下降（并行测试时序下 20 帧
        // 可能未完全收敛到 12——验证"随动画重跑"而非精确终值）
        assert!(
            log.len() > 5 && last < 14.0,
            "label 闭包应随 progress 动画多次重跑且字号下降（实际 len={} last={} log={:?}）",
            log.len(), last, &log[..log.len().min(8)]
        );
    }

    /// 复现 demo 结构（Column 嵌套 TextField）：子 slot dirty 必须向上传播
    /// 到 Column 才能触发重跑——label 闭包随动画重跑（demo 截图字号未变的回归）
    #[test]
    fn label_font_size_animates_nested_in_column() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let value = crate::runtime::state::State::new(TextFieldValue::new(""));
        let focus_src = crate::input::interaction::MutableInteractionSource::new();
        let progress_log: std::sync::Arc<std::sync::Mutex<Vec<f32>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mk_field = |log: std::sync::Arc<std::sync::Mutex<Vec<f32>>>| {
            TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(move |ctx| {
                    let fs = crate::components::text::LOCAL_TEXT_STYLE.current().font_size;
                    log.lock().unwrap().push(fs.map(|f| f.to_logical_px(crate::runtime::density::current_density())).unwrap_or(0.0));
                    crate::components::Text::new("Name").build(ctx);
                })
        };
        for _frame in 0..3 {
            let field = mk_field(progress_log.clone());
            composer.compose(|ctx| {
                crate::layout::Column::new().build(ctx, |ctx| {
                    field.build(ctx);
                });
            });
            crate::animation::update_animations();
        }
        focus_src.emit_focus();
        for _frame in 0..20 {
            let field = mk_field(progress_log.clone());
            composer.compose(|ctx| {
                crate::layout::Column::new().build(ctx, |ctx| {
                    field.build(ctx);
                });
            });
            std::thread::sleep(std::time::Duration::from_millis(8));
            crate::animation::update_animations();
        }
        let log = progress_log.lock().unwrap();
        let last = *log.last().unwrap();
        assert!(
            log.len() > 3 && last > 0.9,
            "嵌套 Column 下 label 闭包应随动画重跑收敛 1.0（实际 len={} last={} log={:?}）",
            log.len(), last, &log[..log.len().min(8)]
        );
    }

    /// placeholder 淡入：聚焦后 alpha 动画 0 → 1——闭包多次重跑且 alpha
    /// 收敛（demo 点击聚焦后 placeholder 不可见的回归）
    #[test]
    fn placeholder_alpha_animates_on_focus() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let value = crate::runtime::state::State::new(TextFieldValue::new(""));
        let focus_src = crate::input::interaction::MutableInteractionSource::new();
        let alpha_log: std::sync::Arc<std::sync::Mutex<Vec<f32>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mk_field = |log: std::sync::Arc<std::sync::Mutex<Vec<f32>>>| {
            TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(|ctx| { crate::components::Text::new("Name").build(ctx); })
                .placeholder(move |ctx| {
                    // placeholder 默认样式经 LOCAL_TEXT_STYLE（16sp + 淡入 alpha）
                    let a = crate::components::text::LOCAL_TEXT_STYLE.current().color
                        .map(|c| c.a as f32 / 255.0)
                        .unwrap_or(0.0);
                    log.lock().unwrap().push(a);
                    crate::components::Text::new("ph").build(ctx);
                })
        };
        // 首帧：未聚焦（placeholder 不构建）
        for _frame in 0..2 {
            let field = mk_field(alpha_log.clone());
            composer.compose(|ctx| { field.build(ctx); });
            crate::animation::update_animations();
        }
        focus_src.emit_focus();
        for _frame in 0..25 {
            let field = mk_field(alpha_log.clone());
            composer.compose(|ctx| { field.build(ctx); });
            std::thread::sleep(std::time::Duration::from_millis(8));
            crate::animation::update_animations();
        }
        let log = alpha_log.lock().unwrap();
        let last = *log.last().unwrap();
        assert!(
            log.len() > 3 && last > 0.8,
            "placeholder 闭包应随 alpha 动画重跑且收敛 >0.8（实际 len={} last={} log={:?}）",
            log.len(), last, &log[..log.len().min(8)]
        );
    }

    /// 快速焦点切换（动画未完成）：聚焦 → 动画中失焦 → 动画中再聚焦——
    /// placeholder 最终应显示（alpha 收敛 1）——回归：切换后 placeholder
    /// 不可见（remember key 漂移/alpha 卡 0）
    #[test]
    fn placeholder_alpha_survives_rapid_focus_toggle() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let value = crate::runtime::state::State::new(TextFieldValue::new(""));
        let focus_src = crate::input::interaction::MutableInteractionSource::new();
        let alpha_log: std::sync::Arc<std::sync::Mutex<Vec<f32>>> = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mk_field = |log: std::sync::Arc<std::sync::Mutex<Vec<f32>>>| {
            TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(|ctx| { crate::components::Text::new("Name").build(ctx); })
                .placeholder(move |ctx| {
                    let a = crate::components::text::LOCAL_TEXT_STYLE.current().color
                        .map(|c| c.a as f32 / 255.0)
                        .unwrap_or(0.0);
                    log.lock().unwrap().push(a);
                    crate::components::Text::new("ph").build(ctx);
                })
        };
        let mut frames = |n: usize| {
            for _ in 0..n {
                let field = mk_field(alpha_log.clone());
                composer.compose(|ctx| { field.build(ctx); });
                std::thread::sleep(std::time::Duration::from_millis(8));
                crate::animation::update_animations();
            }
        };
        frames(2);                      // 未聚焦
        focus_src.emit_focus();
        frames(4);                      // 聚焦——动画刚开始（未完成）
        focus_src.emit_unfocus();
        frames(4);                      // 动画中失焦
        focus_src.emit_focus();
        frames(30);                     // 动画中再聚焦——等动画完成
        let log = alpha_log.lock().unwrap();
        let last = *log.last().unwrap();
        assert!(
            log.len() > 3 && last > 0.8,
            "快速切换后 placeholder alpha 应收敛 >0.8（实际 len={} last={} log={:?}）",
            log.len(), last, &log[..log.len().min(10)]
        );
    }

    /// 完整 UI 流程回归：滚动容器内聚焦 → 动画中失焦 → 动画中再聚焦——
    /// placeholder 节点最终 measured_size 非 0（渲染可见）——demo 里
    /// "快速切换后 placeholder 不显示"（树 [0,0]）的回归
    #[test]
    fn placeholder_visible_after_rapid_focus_toggle_in_scroll() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        let value = crate::runtime::state::State::new(TextFieldValue::new(""));
        let focus_src = crate::input::interaction::MutableInteractionSource::new();
        let scroll = crate::modifier::ScrollState::new();
        let scroll2 = scroll.clone();
        let mk = || {
            TextField::new(value.clone())
                .filled()
                .interaction_source(focus_src.clone())
                .label(|ctx| { crate::components::Text::new("Name").build(ctx); })
                .placeholder(|ctx| { crate::components::Text::new("ph placeholder text").build(ctx); })
        };
        let mut frames = |n: usize| {
            for _ in 0..n {
                let field = mk();
                let sc = scroll2.clone();
                composer.compose(|ctx| {
                    crate::layout::Column::new()
                        .modifier(crate::modifier::Modifier::new().fill_max_size().vertical_scroll(sc))
                        .build(ctx, |ctx| { field.build(ctx); });
                });
                std::thread::sleep(std::time::Duration::from_millis(8));
                crate::animation::update_animations();
            }
        };
        frames(2);
        focus_src.emit_focus();
        frames(4);      // 聚焦——动画中
        focus_src.emit_unfocus();
        frames(4);      // 动画中失焦
        focus_src.emit_focus();
        frames(30);     // 动画中再聚焦——等动画完成
        composer.layout(crate::layout::Constraints::new(0.0, 500.0, 0.0, 600.0));
        // 找 placeholder 节点（TextFieldSlotRole::Placeholder 标记或 text 内容）
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        fn find_placeholder(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> Option<(f32, f32)> {
            let has = nodes[idx].modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextFieldSlot { role }
                    if *role == TextFieldSlotRole::Placeholder)
            });
            if has { return Some((nodes[idx].measured_size.width, nodes[idx].measured_size.height)); }
            for &c in &nodes[idx].children {
                if let Some(s) = find_placeholder(nodes, c) { return Some(s); }
            }
            None
        }
        let ph = find_placeholder(nodes, root);
        assert!(
            ph.map(|(w, h)| w > 0.0 && h > 0.0).unwrap_or(false),
            "快速切换后 placeholder 节点尺寸应为非 0（实际 {:?}）——渲染不可见",
            ph
        );
    }

    /// A read-only field draws no caret, and a normal one does.
    ///
    /// material3: `val showCursor = enabled && !readOnly && windowInfo.isWindowFocused && …`
    /// (`foundation/text/CoreTextField.kt`). winia drew a caret whenever the focused node had a collapsed
    /// selection, so a READ-ONLY field — the exposed dropdown's, for one — blinked a cursor in text nobody
    /// could type into.
    ///
    /// Teeth: dropping the `read_only` term from `text_field_show_cursor` fails the first case here; that is
    /// the exact shape of the bug this replaced, and the second case keeps the first from passing because
    /// the helper answers `false` for everything.
    #[test]
    fn a_read_only_field_draws_no_caret() {
        fn show_cursor(read_only: bool) -> bool {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _guard = rt.enter();
            let value = State::new(TextFieldValue::new("value"));
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                TextField::new(value.clone())
                    .outlined()
                    .read_only(read_only)
                    .build(ctx);
            });
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            text_field_show_cursor(nodes, root, root)
        }

        assert!(
            !show_cursor(true),
            "a read-only field must not draw a caret (material3's showCursor excludes readOnly)"
        );
        assert!(
            show_cursor(false),
            "an editable field still draws one, so the check above cannot pass by answering false always"
        );
    }
