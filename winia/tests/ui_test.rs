//! UI 测试用例（tests/ui_fixtures/ 下的 fixture exe 通过 stdin/stdout 管道驱动）。
//!
//! 设计原则（按测试用例设计原则）：
//! - **独立**：每个用例对应一个单一场景 fixture（互不依赖、无共享状态）
//! - **聚焦**：每个用例验证一个行为（Given/When/Then）
//! - **可复现**：用例自带重试（click_until——debug 注入链路偶发丢事件）与等待
//!   （expect_text_timeout——异步重组）
//! - **自清理**：UiTest Drop 优雅关闭（q → 限时 → kill），launch 清理残留进程

mod ui;
use ui::UiTest;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// fixture_click：点击计数
// ═══════════════════════════════════════════════════════════════

/// Given 初始 Count: 0 与 10 个静态行
/// When  点击 +1 三次
/// Then  Count 依次 1/2/3，静态行保持（点击后结构不塌缩）
#[test]
fn click_updates_state_and_keeps_structure() {
    let mut app = UiTest::launch("click");
    app.expect_text("Count: 0"); // 等首帧就绪
    let (x, y, w, h) = app.find("+1").expect("找不到 +1 按钮");
    for expected in 1..=3 {
        app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), {
            let needle = format!("Count: {expected}");
            move |t| UiTest::tree_texts(t).iter().any(|s| s.contains(&needle))
        });
        app.expect_text(&format!("Count: {expected}"));
    }
    // 结构完整（点击 3 次后静态行仍在）
    app.expect_text("Line 0");
    app.expect_text("Line 9");
}

// ═══════════════════════════════════════════════════════════════
// fixture_toggle：条件结构切换
// ═══════════════════════════════════════════════════════════════

/// Given Show Alt 按钮（初始分支 B：Add 10）
/// When  点击切换两次
/// Then  A（Alternative）↔ B（Add 10）互斥——A 出现时 B 消失，反之亦然
#[test]
fn toggle_switches_conditional_branch_exclusively() {
    let mut app = UiTest::launch("toggle");
    app.expect_text("Add 10"); // 初始分支 B
    let (x, y, w, h) = app.find("Show Alt").expect("找不到切换按钮");
    // 切到分支 A：Alternative 出现且 Add 10 消失
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        let texts = UiTest::tree_texts(t);
        texts.iter().any(|s| s.contains("Alternative"))
            && !texts.iter().any(|s| s.contains("Add 10"))
    });
    // 切回分支 B
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        let texts = UiTest::tree_texts(t);
        texts.iter().any(|s| s.contains("Add 10"))
            && !texts.iter().any(|s| s.contains("Alternative"))
    });
}

// ═══════════════════════════════════════════════════════════════
// fixture_subwindow：声明式多窗口
// ═══════════════════════════════════════════════════════════════

/// Given 主窗口（Main window 文本）
/// When  点击 Open sub window 再点击 Close sub window
/// Then  打开后两窗口树同时存在（window_count 1→2）、主窗口内容保持；关闭后回到 1
#[test]
fn subwindow_open_close_preserves_main_and_trees() {
    let mut app = UiTest::launch("subwindow");
    app.expect_text("Main window"); // 等首帧就绪
    assert_eq!(app.window_count(), 1, "初始应只有主窗口");
    let (x, y, w, h) = app.find("Open sub window").expect("找不到子窗口按钮");
    // 开——两窗口树同时存在（互不覆盖）
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        UiTest::tree_texts(t).iter().any(|s| s.contains("Sub count"))
    });
    app.refresh();
    assert_eq!(app.window_count(), 2, "子窗口打开后应有 2 个窗口");
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|s| s.contains("Main window")),
        "主窗口树应同时存在（多窗口不覆盖）。当前: {}",
        texts.join(" | ")
    );
    // 关——回到单窗口
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        !UiTest::tree_texts(t).iter().any(|s| s.contains("Sub count"))
    });
    app.refresh();
    assert_eq!(app.window_count(), 1, "关闭后应回到 1 个窗口");
    app.expect_text("Main window");
}

// ═══════════════════════════════════════════════════════════════
// fixture_scroll：滚动容器
// ═══════════════════════════════════════════════════════════════

/// Given 30 行内容在 150px 滚动容器内 + `offset:` 实时文本
/// When  向下滚动 -300 再向上 +600
/// Then  offset 文本先 300 后回 0（真实滚动行为——vertical_scroll 只改偏移不移除节点；
///       负 dy = 向下滚：current - dy）
#[test]
fn scroll_container_keeps_content() {
    let mut app = UiTest::launch("scroll");
    app.expect_text("offset: 0"); // 等首帧就绪（初始偏移 0）
    app.expect_text("Line 0");
    // 向下滚动（负 dy）：offset 增大
    app.scroll(-300.0);
    app.expect_text_timeout("offset: 300", Duration::from_secs(5));
    // 向上回滚：offset 归零（clamp 到 0——滚回顶部）
    app.scroll(600.0);
    app.expect_text_timeout("offset: 0", Duration::from_secs(5));
    // 内容完整（滚动是偏移不是移除）
    app.expect_text("Line 0");
    app.expect_text("Line 29");
}

/// Given 30 行内容在 150px 滚动容器内 + `offset:` 实时文本
/// When  按下滚动区向上拖 100px（内容跟随手指）后松手
/// Then  offset 先随拖拽增大；松手后惯性 fling 继续推进（速度 ~100px/160ms
///        → 衰减极限 ≈ 拖拽 100 + 625/4.2 ≈ 249px < 滚动极限 570）
#[test]
fn drag_scroll_follows_pointer_and_flings() {
    let mut app = UiTest::launch("scroll");
    app.expect_text("offset: 0");
    // The scrolling area sits around y 64..214 (Column padding 16 + title ~28 + offset text ~20).
    //
    // The gesture is RETRIED, because the debug pointer path can be stalled by load and the fling
    // distance then depends on WHEN the app processed the moves: the velocity tracker measures the
    // finger from processing timestamps, so a gesture drained in one frame flings several times
    // farther and lands on the clamp (measured: offset 570 for the same drag that gives ~140 when
    // paced). A retry is safe — it only runs when the attempt did not scroll or did not fling — and
    // the retry CANNOT fix the distance itself, so an attempt that overshoots is a failure, not
    // something to try again.
    let mut fling_ok = false;
    let mut last = String::new();
    let mut overshoot = None;
    for attempt in 1..=3 {
        app.drag(100.0, 190.0, 100.0, 90.0);
        let o1 = read_offset(&mut app);
        if o1 <= 30.0 {
            last = format!("the drag did not move the content (offset {o1})");
        } else {
            std::thread::sleep(Duration::from_millis(300));
            let o2 = read_offset(&mut app);
            std::thread::sleep(Duration::from_millis(300));
            let o3 = read_offset(&mut app);
            if o3 > V_SCROLL_LIMIT + 0.5 {
                overshoot = Some(o3);
                break;
            }
            if o3 > o1 && o3 >= o2 {
                fling_ok = true;
                break;
            }
            last = format!("no fling after the drag ({o1} -> {o2} -> {o3})");
        }
        eprintln!("[ui-test] drag/fling attempt {attempt} did not hold: {last} — retrying");
    }
    if let Some(o) = overshoot {
        panic!("the fling overshot the scroll limit: {o} > {V_SCROLL_LIMIT}");
    }
    assert!(fling_ok, "a fast drag must fling the content: {last}");
}

/// Click a tag, send keys, and retry the whole sequence until `expected` appears.
///
/// The debug pointer path can drop a click under load (the same stall `click_until` retries), and a
/// dropped click means the keys go to whatever had focus before — the text never changes and a plain
/// `expect_text_timeout` then fails for a reason that has nothing to do with the component.
///
/// Retrying re-sends the keys, so it is only allowed while the label still reads its BASELINE value
/// (nothing was typed yet). If the label changed but does not contain `expected` — the keys landed and
/// the text simply is not what the test expects, e.g. a second copy from an earlier attempt — that is
/// reported as a failure instead of typing a third copy into it, which would make `expected`
/// unreachable and blame the click.
fn click_tag_and_type_until(app: &mut UiTest, tag: &str, keys: &[&str], expected: &str) {
    let label = expected.split(':').next().unwrap_or(expected).to_string();
    let label_text = |app: &mut UiTest| -> Option<String> {
        app.refresh();
        app.all_texts().into_iter().find(|t| t.contains(&label))
    };
    let baseline = label_text(app).unwrap_or_default();
    for attempt in 1..=3 {
        app.click_tag(tag);
        for key in keys {
            app.key(key);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.refresh();
            if app.all_texts().iter().any(|t| t.contains(expected)) {
                return;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(120));
        }
        let now = label_text(app).unwrap_or_default();
        assert!(
            now == baseline,
            "the field changed but not as expected: `{now}` (wanted `{expected}`) — the keys landed              and re-typing would only add to them"
        );
        eprintln!(
            "[ui-test] attempt {attempt}: `{expected}` never appeared after clicking `{tag}` and typing {keys:?} — retrying"
        );
    }
    panic!("`{expected}` never appeared after 3 clicks on `{tag}` and typing {keys:?}");
}

/// The `fixture_scroll` scroll limits (content minus viewport): a fling may reach them exactly, but
/// must never pass them.
const V_SCROLL_LIMIT: f32 = 570.0;
const H_SCROLL_LIMIT: f32 = 2100.0;

/// 读取 `offset: X` 文本（树文本带 `text(...)` 描述前缀——子串定位）
fn read_offset(app: &mut UiTest) -> f32 {
    app.refresh();
    app.all_texts()
        .iter()
        .find_map(|s| {
            s.find("offset: ").and_then(|i| {
                s[i + "offset: ".len()..]
                    .trim_end_matches(')')
                    .trim()
                    .parse::<f32>()
                    .ok()
            })
        })
        .unwrap_or(-1.0)
}

/// 读取 `hoffset: X` 文本（横向滚动偏移）
fn read_hoffset(app: &mut UiTest) -> f32 {
    app.refresh();
    app.all_texts()
        .iter()
        .find_map(|s| {
            s.find("hoffset: ").and_then(|i| {
                s[i + "hoffset: ".len()..]
                    .trim_end_matches(')')
                    .trim()
                    .parse::<f32>()
                    .ok()
            })
        })
        .unwrap_or(-1.0)
}

/// Given 40 列内容在 300px 横向滚动容器内 + `hoffset:` 实时文本
/// When  横向滚动（dx 负 = 内容左移——与垂直"负 dy = 下滚"对称）再反向滚回
/// Then  hoffset 先 300 后回 0（horizontal_scroll 与垂直对称；内容 = 偏移不是移除）
#[test]
fn horizontal_scroll_container_keeps_content() {
    let mut app = UiTest::launch("scroll");
    app.expect_text("hoffset: 0"); // 等首帧就绪
    app.expect_text("C0");
    // 向右滚（dx -300）：offset 增大
    app.scroll_delta(-300.0, 0.0);
    app.expect_text_timeout("hoffset: 300", Duration::from_secs(5));
    // 向左滚回：offset 归零
    app.scroll_delta(600.0, 0.0);
    app.expect_text_timeout("hoffset: 0", Duration::from_secs(5));
    app.expect_text("C0");
    app.expect_text("C39");
}

/// Given 横向滚动容器（40 列 × 60px = 2400px，视口 300px，极限 2100）
/// When  在滚动区向左拖 100px（内容跟随手指）后松手
/// Then  hoffset 先随拖拽增大；松手后惯性 fling 继续推进（不超过极限 2100）
#[test]
fn horizontal_drag_scroll_follows_pointer_and_flings() {
    let mut app = UiTest::launch("scroll");
    app.expect_text("hoffset: 0");
    // The horizontal area sits around y 94..154 (title ~32 + offset text ~19 + hoffset text ~19).
    // Retried for the same reason as the vertical case above, with the same rule: the retry cannot
    // fix a fling that overshot the limit, so that is a failure rather than another attempt.
    let mut fling_ok = false;
    let mut last = String::new();
    let mut overshoot = None;
    for attempt in 1..=3 {
        app.drag(250.0, 120.0, 150.0, 120.0);
        let o1 = read_hoffset(&mut app);
        if o1 <= 30.0 {
            last = format!("the drag did not move the content (hoffset {o1})");
        } else {
            std::thread::sleep(Duration::from_millis(300));
            let o2 = read_hoffset(&mut app);
            std::thread::sleep(Duration::from_millis(300));
            let o3 = read_hoffset(&mut app);
            if o3 > H_SCROLL_LIMIT + 0.5 {
                overshoot = Some(o3);
                break;
            }
            if o3 > o1 && o3 >= o2 {
                fling_ok = true;
                break;
            }
            last = format!("no fling after the drag ({o1} -> {o2} -> {o3})");
        }
        eprintln!("[ui-test] horizontal drag/fling attempt {attempt} did not hold: {last} — retrying");
    }
    if let Some(o) = overshoot {
        panic!("the fling overshot the scroll limit: {o} > {H_SCROLL_LIMIT}");
    }
    assert!(fling_ok, "a fast drag must fling the content: {last}");
}

// ═══════════════════════════════════════════════════════════════
// fixture_nest：多级 if/else 结构切换（3 态循环）
// ═══════════════════════════════════════════════════════════════

/// Given switch 按钮（level 1 → 2 → other 三态循环，每态节点数不同）
/// When  点击 6 次（每态重复进入 2 次）
/// Then  第 i 与第 i+3 次节点数一致（结构稳定、key 不漂移）、按钮位置不漂移
#[test]
fn nest_structure_switch_cycles_stably() {
    let mut app = UiTest::launch("nest");
    app.expect_text("level 1"); // 等首帧就绪（初始 level 1）
    let (x, y, w, h) = app.find("switch").expect("找不到 switch 按钮");
    let pos0 = (x, y);
    // prev 记录"点击前"的节点数——首次也验证点击生效（渲染断下点击可能丢失）
    app.refresh();
    let mut prev_count = Some(app.all_texts().len());
    let mut counts = Vec::new();
    for i in 0..6 {
        let prev = prev_count;
        // 点击 + 等结构切换（节点数变化；丢点击自动重试）
        app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), move |t| {
            let n = UiTest::tree_texts(t).len();
            n != prev.unwrap()
        });
        app.refresh();
        let n = app.all_texts().len();
        counts.push(n);
        prev_count = Some(n);
        let btn = app.find("switch").expect("switch 按钮消失（塌缩）");
        assert!(
            (btn.0 - pos0.0).abs() < 1.0 && (btn.1 - pos0.1).abs() < 1.0,
            "按钮位置漂移：{pos0:?} → ({}, {})",
            btn.0, btn.1
        );
        // 三态循环：第 i 与第 i+3 次进入同一状态——节点数一致。
        // 已知限制：debug 渲染断导致点击重试时可能跳两态（双触发）——偶发假失败可重跑。
        if i >= 3 {
            assert_eq!(
                counts[i], counts[i - 3],
                "结构状态重复进入节点数不一致：{} vs {}（完整序列 {counts:?}）",
                counts[i], counts[i - 3]
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ═══════════════════════════════════════════════════════════════
// fixture_text_field：真实窗口 TextField 交互
// ═══════════════════════════════════════════════════════════════

/// 覆盖容器点击聚焦、逐字符输入、删除和状态重组。
#[test]
fn text_field_focus_input_and_backspace_update_state() {
    let mut app = UiTest::launch("text_field");
    app.expect_text("username:");
    click_tag_and_type_until(&mut app, "username-field", &["a", "b", "c"], "username: abc");
    assert!(app.tag_is_focused("username-field"), "username 应保持焦点");
    app.key("Backspace");
    app.expect_text_timeout("username: ab", Duration::from_secs(5));
}

/// 密码字段只公开长度/有效性，并覆盖 Enter 产生多行的真实键盘路由。
#[test]
fn text_field_password_and_multiline_states_update() {
    let mut app = UiTest::launch("text_field");
    app.expect_text("password-status: invalid");
    click_tag_and_type_until(&mut app, "password-field", &["p", "a", "s", "s"], "password-length: 4");
    app.expect_text_timeout("password-status: valid", Duration::from_secs(5));
    app.expect_text("text(••••)");
    assert!(
        !app.all_texts().iter().any(|text| text.contains("text(pass)")),
        "密码输入节点不应暴露明文"
    );

    let (_, _, _, notes_h) = app.find_tag("notes-field").expect("notes tag");
    assert!(notes_h >= 56.0, "min_lines 字段应高于单行，实际 {notes_h}");
    click_tag_and_type_until(&mut app, "notes-field", &["n", "Enter", "2"], "notes-lines: 2");
    app.refresh();
    let (_, _, _, notes_h_after) = app.find_tag("notes-field").expect("notes tag after input");
    assert!(notes_h_after >= notes_h, "新增行后 TextField 不应塌缩：{notes_h} -> {notes_h_after}");
}

/// error/read-only/disabled 状态在真实输入路由中保持各自约束。
#[test]
fn text_field_error_readonly_and_disabled_states_are_enforced() {
    let mut app = UiTest::launch("text_field");
    app.expect_text("error-status: required");
    click_tag_and_type_until(&mut app, "error-field", &["x"], "error-status: none");

    // Poll for the focus instead of asserting it right after the click: the debug path can drop a
    // click under load, and this assertion is about the field accepting focus, not about the click
    // landing within one frame.
    let mut focused = false;
    for _ in 0..3 {
        app.click_tag("readonly-field");
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if app.tag_is_focused("readonly-field") {
                focused = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(120));
        }
        if focused {
            break;
        }
        eprintln!("[ui-test] readonly-field did not take focus — clicking again");
    }
    assert!(focused, "a read-only field must still take focus");
    app.key("x");
    app.key("Backspace");
    app.expect_text_timeout("readonly-value: Read only", Duration::from_secs(5));

    app.click_tag("disabled-field");
    app.key("x");
    app.expect_text_timeout("disabled-value: Locked", Duration::from_secs(5));
    assert!(!app.tag_is_focused("disabled-field"), "禁用字段不应获得焦点");
}

// ═══════════════════════════════════════════════════════════════
// fixture_top_app_bar：变体高度与共享 offset 折叠
// ═══════════════════════════════════════════════════════════════

#[test]
fn top_app_bar_variants_collapse_and_restore_with_scroll() {
    let mut app = UiTest::launch("top_app_bar");
    app.expect_text("standard-scrolled: false");
    app.expect_text("large-scrolled: false");
    app.expect_text("large-collapsed: false");
    let (_, appbar_y, _, expanded_h) = app.find_tag("large-appbar").expect("large app bar");
    let (_, _, _, standard_h) = app.find_tag("standard-appbar").expect("standard app bar");
    assert_eq!(standard_h, 64.0, "Standard 滚动前应固定 64dp");
    assert!(expanded_h >= 140.0, "large 初始应展开：{expanded_h}");

    app.scroll(-24.0);
    app.expect_text_timeout("standard-scrolled: true", Duration::from_secs(5));
    app.expect_text_timeout("large-scrolled: true", Duration::from_secs(5));
    let (_, _, _, standard_scrolled_h) = app.find_tag("standard-appbar").expect("standard app bar scrolled");
    assert!((standard_scrolled_h - standard_h).abs() <= 1.0, "Standard 滚动后高度仍应为 64dp");
    app.expect_text_timeout("large-collapsed: false", Duration::from_secs(5));
    let (_, _, _, mid_one_h) = app.find_tag("large-appbar").expect("large app bar mid one");
    assert!(mid_one_h > 68.0 && mid_one_h < expanded_h, "第一中间高度应连续：{expanded_h} -> {mid_one_h}");

    app.scroll(-48.0);
    let (_, _, _, mid_two_h) = app.find_tag("large-appbar").expect("large app bar mid two");
    assert!(mid_two_h > 68.0 && mid_two_h <= mid_one_h, "第二中间高度不得反向展开：{mid_one_h} -> {mid_two_h}");

    app.scroll(-260.0);
    app.expect_text_timeout("large-collapsed: true", Duration::from_secs(5));
    let (_, _, _, collapsed_h) = app.find_tag("large-appbar").expect("collapsed large app bar");
    assert!(collapsed_h <= 68.0, "large 折叠高度应接近 standard：{collapsed_h}");

    app.scroll(600.0);
    app.expect_text_timeout("standard-scrolled: false", Duration::from_secs(5));
    app.expect_text_timeout("large-scrolled: false", Duration::from_secs(5));
    app.expect_text_timeout("large-collapsed: false", Duration::from_secs(5));
    let (_, _, _, restored_h) = app.find_tag("large-appbar").expect("restored large app bar");
    assert!(restored_h >= expanded_h, "回滚后应恢复展开高度：{expanded_h} -> {restored_h}");
}

#[test]
fn scaffold_fab_clicks_and_rtl_mirrors_without_changing_content_inset() {
    let mut app = UiTest::launch("scaffold");
    app.expect_text("fab-count: 0");
    app.expect_text("direction: ltr");
    let (_, top_y, _, top_h) = app.find_tag("scaffold-top").expect("scaffold top");
    let (_, content_y, _, content_h) = app.find_tag("scaffold-content-probe").expect("scaffold content");
    let (fab_x, fab_y, fab_w, fab_h) = app.find_tag("scaffold-fab").expect("scaffold fab");
    assert!((content_y - (top_y + top_h)).abs() <= 1.0, "content 应从 top bar 下方开始：top=({}, {}) content={}", top_y, top_h, content_y);
    assert!(content_h > 0.0);
    assert!((fab_w - 56.0).abs() <= 1.0 && (fab_h - 56.0).abs() <= 1.0, "regular FAB 尺寸");

    app.click_tag("scaffold-fab");
    app.expect_text_timeout("fab-count: 1", Duration::from_secs(5));
    app.click_tag("direction-toggle");
    app.expect_text_timeout("direction: rtl", Duration::from_secs(5));
    let (rtl_fab_x, rtl_fab_y, _, _) = app.find_tag("scaffold-fab").expect("rtl scaffold fab");
    let (_, rtl_content_y, _, rtl_content_h) = app.find_tag("scaffold-content-probe").expect("rtl scaffold content");
    assert!(rtl_fab_x < fab_x, "RTL FAB 应镜像到 start side：{fab_x} -> {rtl_fab_x}");
    assert!((rtl_fab_y - fab_y).abs() <= 1.0, "RTL 不应改变 FAB 垂直位置");
    assert!((rtl_content_y - content_y).abs() <= 1.0 && (rtl_content_h - content_h).abs() <= 1.0, "RTL 不应改变内容垂直 inset");
}

#[test]
fn nested_scroll_top_app_bar_consumes_before_child_content() {
    let mut app = UiTest::launch("nested_scroll");
    app.expect_text("height-offset: 0");
    app.expect_text("child-offset: 0");
    app.scroll(-24.0);
    app.expect_text_timeout("height-offset: -24", Duration::from_secs(5));
    app.expect_text("child-offset: 0");
    app.scroll(-120.0);
    app.expect_text_timeout("height-offset: -88", Duration::from_secs(5));
    app.expect_text_timeout("child-offset:", Duration::from_secs(5));
    app.scroll(600.0);
    app.expect_text_timeout("height-offset: 0", Duration::from_secs(5));
}

// ═══════════════════════════════════════════════════════════════
// fixture_resize：窗口 resize 后自适应内容更新（Phase 4.3 E2E）
// ═══════════════════════════════════════════════════════════════

/// Given 初始窗口 400x300（window-size: 400x300）
/// When  通过 debug 命令 resize 到 600x400
/// Then  window-size 文本更新为 600x400（SurfaceResized → window_size_state → 重组）
#[test]
fn resize_updates_adaptive_window_size_content() {
    let mut app = UiTest::launch("resize");
    app.expect_text_timeout("window-size: 400x300", Duration::from_secs(5));
    app.resize(600.0, 400.0);
    app.expect_text_timeout("window-size: 600x400", Duration::from_secs(5));
    // 再 resize 一次（缩小）——确认不是一次性更新
    app.resize(320.0, 240.0);
    app.expect_text_timeout("window-size: 320x240", Duration::from_secs(5));
}

// ═══════════════════════════════════════════════════════════════
// fixture_lazy_resize：resize 那一帧自己就把新视口盖住（同帧收敛，app 路径）
// ═══════════════════════════════════════════════════════════════

/// What:  a `LazyColumn` filling the window, resized 300 -> 900 tall.
/// When:  `fpc` (forget the recorded frames) -> `w 400 900` -> `fp` (the recorded frames).
/// Then:  the FIRST frame rendered after the resize took TWO compose+layout rounds, and it is the only
///        frame that took more than one.
///
/// Why the round count is the assertion: one frame of two rounds is a frame that caught up with its own
/// measurement, while two frames of one round each is the one-frame lag this replaced. Both leave the
/// same tree, so no tree assertion can tell them apart — and by the time a `t` query is answered those
/// frames have passed. This is the real frame handler's path (`PerWindow::recompose_layout_render`), the
/// one part the unit tests in `lazy_column.rs` cannot reach.
#[test]
fn a_resize_frame_covers_its_new_viewport_within_one_frame() {
    let mut app = UiTest::launch("lazy_resize");
    app.expect_text_timeout("Item 0", Duration::from_secs(5));

    app.clear_frame_passes();
    app.send("w 400 900");
    std::thread::sleep(Duration::from_millis(400));
    let resize = app.frame_passes().expect("fp answered");
    eprintln!("resize 帧的 compose+layout 轮数: {resize:?}");
    assert!(
        resize.frames >= 1,
        "the resize must have rendered at least one frame: {resize:?}"
    );
    assert_eq!(
        resize.multi, 1,
        "exactly one frame — the resize one — should need a second round: {resize:?}"
    );
    // Not strictly "the FIRST frame": under the full suite's parallel load the resize itself can land one
    // frame later than the request (measured: passes `[1, 2, 1]` — the convergence happened, in the second
    // frame). The property is that the frame which OBSERVES the resize converges within itself rather than a
    // later one catching up, so either of the first two is accepted; a three-round frame, or a convergence
    // that lands later than that, still fails.
    assert!(
        matches!(resize.passes.first().copied(), Some(2))
            || matches!(resize.passes.get(1).copied(), Some(2)),
        "the resize's own frame must be the one that converged, within the first two: {resize:?}"
    );

    // End to end in the real window: the new bottom is covered. 900px / 48px rows puts row 18 at the
    // bottom edge, so it has to exist in the tree.
    app.tree();
    assert!(
        app.find_tag("lr-row-18").is_some(),
        "the grown viewport's bottom row must be composed (rows seen: {})",
        app.all_texts().len()
    );

    // And the convergence must NOT cost a scrolling frame a second round — the same claim the unit test
    // makes, through the real frame handler this time. Negative dy scrolls FORWARD: a positive wheel at
    // the top of the list clamps to the same offset, changes nothing, and renders no frame at all (which
    // is how this step first measured `frames: 0`). The screenshot request guarantees the frame.
    app.clear_frame_passes();
    app.scroll(-200.0);
    app.send("r");
    std::thread::sleep(Duration::from_millis(300));
    let scroll = app.frame_passes().expect("fp answered");
    eprintln!("滚动帧的 compose+layout 轮数: {scroll:?}");
    assert!(
        scroll.frames >= 1,
        "the scroll must have rendered a frame: {scroll:?}"
    );
    assert_eq!(
        scroll.multi, 0,
        "a scrolling frame must stay at one compose+layout round: {scroll:?}"
    );
}

// ═══════════════════════════════════════════════════════════════
// fixture_overlay：Popup overlay 树条目随 visible 出现/消失（Phase 4.3 E2E）
// ═══════════════════════════════════════════════════════════════

/// Given Popup 初始关闭（popup-open: no，overlay 条目 0）
/// When  点击 Toggle Popup 按钮打开
/// Then  popup-open: yes 且 overlay 树条目出现（overlay_count 1）
/// When  点击 overlay 外部区域（Popup 默认 dismiss_on_outside——dismiss + 状态同步）
/// Then  popup-open: no 且 overlay 条目消失（overlay_count 0）
#[test]
fn popup_overlay_appears_and_disappears_with_visible() {
    let mut app = UiTest::launch("overlay");
    app.expect_text_timeout("popup-open: no", Duration::from_secs(5));
    // 初始无 overlay 条目
    app.refresh();
    assert_eq!(app.overlay_count(), 0, "初始无 overlay 条目");
    // 打开（按钮在窗口左上 (16,51)——点中心）
    let (x, y, w, h) = app.find_tag("toggle-overlay").expect("找不到 toggle 按钮");
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text_timeout("popup-open: yes", Duration::from_secs(5));
    // 轮询 overlay 条目出现
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        if app.overlay_count() == 1 { break; }
        assert!(std::time::Instant::now() < deadline, "overlay 条目应出现");
        std::thread::sleep(Duration::from_millis(100));
    }
    // 点击 overlay 外部（窗口右下 (200,250)——远离按钮与 popup）
    // ⚠ Popup dismiss_on_outside=true：外部点击 → dismiss + on_dismiss 同步 show=false
    app.click(200.0, 250.0);
    app.expect_text_timeout("popup-open: no", Duration::from_secs(5));
    // 轮询 overlay 条目消失
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        if app.overlay_count() == 0 { break; }
        assert!(std::time::Instant::now() < deadline, "overlay 条目应消失");
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ═══════════════════════════════════════════════════════════════
// fixture_panic：build 内 panic 被捕获后窗口存活（Phase 4.3 E2E）
// ═══════════════════════════════════════════════════════════════

/// Given 窗口正常渲染（count: 0）
/// When  点击 Panic（build 内触发一次性 panic——catch_unwind 捕获，本帧跳过）
/// Then  窗口存活（树可查、count 按钮仍可点）——状态复位后下一帧恢复
#[test]
fn panic_in_build_is_caught_and_window_survives() {
    let mut app = UiTest::launch("panic");
    app.expect_text_timeout("count: 0", Duration::from_secs(5));
    // 点 Panic——panic 被 catch_unwind 捕获（本帧跳过），窗口不应崩
    let (px, py, pw, ph) = app.find_tag("panic-btn").expect("找不到 panic 按钮");
    app.click(px + pw / 2.0, py + ph / 2.0);
    // 等待 panic 处理（catch_unwind → 下帧恢复）——树仍可查即窗口存活
    std::thread::sleep(Duration::from_millis(500));
    app.refresh();
    assert!(app.tree().is_some(), "panic 后窗口应存活（树可查）");
    // 后续交互正常：点 Count 两次
    let (cx, cy, cw, ch) = app.find_tag("count-btn").expect("找不到 count 按钮");
    app.click(cx + cw / 2.0, cy + ch / 2.0);
    app.expect_text_timeout("count: 1", Duration::from_secs(5));
    app.click(cx + cw / 2.0, cy + ch / 2.0);
    app.expect_text_timeout("count: 2", Duration::from_secs(5));
}

/// A tap on a button in a popup must not steal the keyboard from the field beside it.
/// `clickable` does not request focus (Compose's `Clickable.kt` delegates a `FocusableNode` and
/// never calls `requestFocus`), but `overlay_down` used to focus the deepest focusable node on
/// the hit path — the DEBUG-CLICK rule — so a button in a popup took focus.
#[test]
fn clicking_an_overlay_button_does_not_steal_focus() {
    let mut app = UiTest::launch("overlay_focus");
    app.expect_text("dialog-open: no");

    // Open the dialog (a popup).
    app.click_tag("open-dialog");
    app.expect_text_timeout("dialog-open: yes", Duration::from_secs(5));

    // It opens with nothing focused, so whatever ends up focused below came from the tap.
    assert!(
        !app.overlay_tag_is_focused("dialog-field"),
        "the tap is what focuses the field, not the dialog opening"
    );

    // Tap the popup's field: it takes focus and receives the keyboard (the popup input path).
    app.click_overlay_tag("dialog-field");
    for key in ["h", "i"] {
        app.key(key);
    }
    app.expect_text_timeout("dialog-field: hi", Duration::from_secs(5));
    assert!(
        app.overlay_tag_is_focused("dialog-field"),
        "a tap on a popup field focuses it (the popup input path)"
    );

    // Tap the action button in the same popup: the tap must LAND (otherwise the focus
    // assertions below would pass on a click that missed), the button must not take focus, and
    // the field must keep it.
    app.click_overlay_tag("dialog-action");
    app.expect_text_timeout("action-clicks: 1", Duration::from_secs(5));
    assert!(
        !app.overlay_tag_is_focused("dialog-action"),
        "a clickable does not request focus (as in Compose): the button must not light up"
    );
    assert!(
        app.overlay_tag_is_focused("dialog-field"),
        "nor may it take the field's focus"
    );

    // And the keyboard must still reach the field.
    app.key("!");
    app.expect_text_timeout("dialog-field: hi!", Duration::from_secs(5));
}

/// A popup's pointer-down must dispatch the press gesture, so a component's own `on_press`
/// runs inside a popup exactly as it does in the main tree.
///
/// The overlay path used to dispatch `on_click` only (on up), which is why `overlay_down`
/// carried a focus heuristic at all: a popup `TextField` focuses through
/// `on_press → FocusRequester::request_focus`, and that callback never fired. This asserts the
/// gesture itself, on a zone that has no click of any kind — `on_press` is its only channel.
#[test]
fn an_overlay_press_zone_receives_the_press_gesture() {
    let mut app = UiTest::launch("overlay_focus");
    app.expect_text("dialog-open: no");

    app.click_tag("open-dialog");
    app.expect_text_timeout("dialog-open: yes", Duration::from_secs(5));
    app.expect_text("presses: 0");

    app.click_overlay_tag("dialog-press-zone");
    app.expect_text_timeout("presses: 1", Duration::from_secs(5));
}

/// Tab belongs to a modal overlay, and closing one gives the keyboard back to the page.
///
/// Keyboard only, no pointer: Tab reaches the page's button, Enter opens the dialog, Tab moves focus
/// INSIDE the dialog (before this, Tab walked the page behind the scrim, so a dialog could not be
/// reached without a mouse), Escape closes it — and the page still holds the focus it had, so the
/// next Tab continues from there instead of starting over.
#[test]
fn tab_owns_the_keyboard_inside_a_modal_overlay_and_the_page_gets_it_back() {
    let mut app = UiTest::launch("overlay_focus");
    app.expect_text("dialog-open: no");

    // The fixture's only focusable on the page is the button that opens the dialog.
    app.key("Tab");
    assert!(app.tag_is_focused("open-dialog"), "Tab must reach the page's button");
    app.key("Enter");
    app.expect_text_timeout("dialog-open: yes", Duration::from_secs(5));
    // The page still remembers where it was, but it must not keep DRAWING a ring behind the scrim:
    // the memory lives in a slot key while a modal owns the keyboard, so the tree shows no focus at
    // all until something inside the dialog takes it.
    assert_eq!(
        app.focused_tags(),
        Vec::<String>::new(),
        "a modal takes the page's ring off the tree (its focus is remembered, not drawn)"
    );

    // Tab now works inside the dialog: its first focusable is the field in the text slot.
    app.key("Tab");
    assert!(
        app.overlay_tag_is_focused("dialog-field"),
        "Tab must move focus inside the dialog, not behind its scrim"
    );
    assert_eq!(
        app.focused_tags(),
        vec!["dialog-field".to_string()],
        "exactly one node shows focus while a modal is up"
    );
    for key in ["h", "i"] {
        app.key(key);
    }
    app.expect_text_timeout("dialog-field: hi", Duration::from_secs(5));

    // Escape closes it, and the page is still standing where it was.
    app.key("Escape");
    app.expect_text_timeout("dialog-open: no", Duration::from_secs(5));
    assert!(
        app.tag_is_focused("open-dialog"),
        "closing an overlay must hand the focus back, so a keyboard user does not re-Tab"
    );
    assert_eq!(
        app.focused_tags(),
        vec!["open-dialog".to_string()],
        "…and the page's ring is the only one again"
    );
}

/// A drag inside a popup reaches the same value as the same drag in the main tree.
///
/// The overlay drag path used to pass WINDOW coordinates into callbacks whose contract is
/// node-local (`fire_gesture_action` subtracts a position from the arena it was handed, and a
/// popup's arena is layer-local), so a popup `on_drag` saw its `pos` shifted by the popup's screen
/// origin. `Slider` reads `pos.0`, which is why a drag in a popup landed on the wrong value — 0.40
/// for a drag to the 10% point of its own track, against 0.08 for the identical drag in the main
/// tree, before the fix.
#[test]
fn a_drag_inside_a_popup_reaches_the_same_value_as_in_the_main_tree() {
    let mut app = UiTest::launch("popup_drag");
    app.expect_text("main: 0.00");
    app.expect_text("popup: 0.00");

    let (mx, my, mw, mh) = app.find_tag("main-slider").expect("no main-slider");
    let (px, py, pw, ph) = app
        .find_tag_in_overlay("popup-slider")
        .expect("no popup-slider in the popup entries");
    // The fixture offsets the popup horizontally on purpose: at screen origin (0, 0) a layer-vs-scene
    // coordinate mistake is invisible, which is how the bug survived. (The fixture also passes
    // `dismiss_on_outside(false)`, so the page drag below is not eaten by a dismissal.)
    assert!(
        px > 100.0,
        "the fixture's popup must not sit at the window origin (popup slider x = {px})"
    );

    // One gesture per slider: from the middle of its track to 10% into it. The same gesture relative
    // to the track must end on the same value in both arenas.
    let drag = |app: &mut UiTest, x: f32, y: f32, w: f32, h: f32| {
        let (from_x, to_x, mid_y) = (x + w / 2.0, x + w * 0.10, y + h / 2.0);
        app.send(&format!("d {} {}", from_x as i32, mid_y as i32));
        for i in 1..=8 {
            let t = i as f32 / 8.0;
            app.send(&format!("m {} {}", (from_x + (to_x - from_x) * t) as i32, mid_y as i32));
        }
        app.send(&format!("u {} {}", to_x as i32, mid_y as i32));
        std::thread::sleep(Duration::from_millis(300));
    };
    drag(&mut app, mx, my, mw, mh);
    drag(&mut app, px, py, pw, ph);

    // `all_texts` yields the tree's `mod` strings (e.g. `text(main: 0.00)`), so pull the number out
    // of the label and stop at the trailing bracket.
    let read = |texts: &[String], label: &str| -> Option<f32> {
        let t = texts.iter().find(|t| t.contains(label))?;
        let rest = &t[t.find(label)? + label.len()..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        num.parse().ok()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let (main, popup) = loop {
        app.refresh();
        let texts = app.all_texts();
        let main = read(&texts, "main: ");
        let popup = read(&texts, "popup: ");
        if let Some((m, p)) = main.zip(popup) {
            if m > 0.0 && p > 0.0 {
                break (m, p);
            }
        }
        assert!(
            Instant::now() < deadline,
            "a drag did not move either slider (main = {main:?}, popup = {popup:?})"
        );
        std::thread::sleep(Duration::from_millis(120));
    };
    assert!(
        (0.04..0.20).contains(&main) && (0.04..0.20).contains(&popup),
        "both drags ended 10% into their own track, so both values must be near 0.10 — the popup one is the layer-offset 0.40 without the layer-local conversion (main = {main}, popup = {popup}, popup slider x = {px})"
    );
    assert!(
        (main - popup).abs() < 0.06,
        "the same drag must give the same value inside a popup and in the main tree (main = {main}, popup = {popup})"
    );
}

/// `dismiss_on_outside` decides whether an outside press closes an overlay.
///
/// The outside-press tail used to close any overlay that was `modal || dismiss_on_outside`, so the
/// flag was a silent no-op for every modal overlay — a dialog asked to stay open closed anyway, and
/// a modal overlay still has to CONSUME that press (its scrim blocks what is behind it), which the
/// page button's click count pins down.
#[test]
fn a_modal_dialog_with_dismiss_on_outside_false_stays_open() {
    let mut app = UiTest::launch("dialog_dismiss");
    app.expect_text("a: closed / b: closed");

    // The page button must be able to increment its counter at all, or the two "the press was
    // consumed" assertions below would hold vacuously (a `page-clicks: 0` that can never move).
    app.click_tag("page-button");
    app.expect_text_timeout("page-clicks: 1", Duration::from_secs(5));

    // Phase A: the flag is off, so an outside press neither closes the dialog nor reaches the page.
    app.click_tag("open-a");
    app.expect_text_timeout("a: open", Duration::from_secs(5));
    assert_eq!(app.overlay_count(), 1, "the dialog is open");

    app.click_tag("page-button");
    app.expect_text("page-clicks: 1");
    app.expect_text("a: open");
    assert_eq!(
        app.overlay_count(),
        1,
        "a modal dialog with dismiss_on_outside(false) must not close on an outside press"
    );

    // Phase B: with the default flag the same press closes it.
    app.key("Escape");
    app.expect_text_timeout("a: closed", Duration::from_secs(5));
    app.click_tag("open-b");
    app.expect_text_timeout("b: open", Duration::from_secs(5));
    app.click_tag("page-button");
    app.expect_text_timeout("b: closed", Duration::from_secs(5));
    // ... and that press was consumed by the dismissal, not delivered to the page.
    app.expect_text("page-clicks: 1");
    assert_eq!(app.overlay_count(), 0, "the default dialog closes");
}

/// The tap family (tap / long-press) fires inside popup content, like the main tree.
///
/// `overlay_down` created no gesture tracker, so a popup node's `on_tap` / `on_double_tap` /
/// `on_long_press` never ran: only `on_press` (pointer-down) and `on_click` (on release) existed
/// there. The two zones in this fixture are identical, one per arena, and must count the same
/// gestures. Driven with explicit down/up (not the synthetic `c x y` click): `on_tap` lives on the
/// gesture path, which a synthetic click never enters — that is also why every other fixture taps
/// clickable buttons instead.
#[test]
fn a_popup_tap_zone_fires_the_tap_family_like_the_main_tree() {
    let mut app = UiTest::launch("popup_tap");
    app.expect_text("main-taps: 0");
    app.expect_text("popup-taps: 0");

    let zone_rect = |app: &mut UiTest, zone: &str| -> (f32, f32, f32, f32) {
        if zone == "main-tap-zone" {
            app.find_tag(zone).expect("no main-tap-zone")
        } else {
            app.find_tag_in_overlay(zone).expect("no popup-tap-zone")
        }
    };
    // Down and up back-to-back: both events then land in the same drain of the debug queue, so the
    // tracker measures a hold of ~0 and the gesture is a tap (the long-press threshold is 500 ms, see
    // `LONG_PRESS_TIMEOUT_MS`). A fixed sleep between them was the fragile part — under load the
    // measured hold grew past the threshold and the tap became a hold. A stall that starts between
    // the two writes and ends before the up is drained is still possible in principle; that is what
    // the polling timeouts below are for, not the burst alone.
    let tap = |app: &mut UiTest, x: f32, y: f32| {
        app.send(&format!("d {} {}", x as i32, y as i32));
        app.send(&format!("u {} {}", x as i32, y as i32));
        std::thread::sleep(Duration::from_millis(200));
    };

    // A tap on the page zone: the main-tree path is the reference.
    let (x, y, w, h) = zone_rect(&mut app, "main-tap-zone");
    tap(&mut app, x + w / 2.0, y + h / 2.0);
    app.expect_text_timeout("main-taps: 1", Duration::from_secs(5));

    // The same tap inside the popup.
    let (x, y, w, h) = zone_rect(&mut app, "popup-tap-zone");
    tap(&mut app, x + w / 2.0, y + h / 2.0);
    app.expect_text_timeout("popup-taps: 1", Duration::from_secs(5));

    // And a hold long enough for the long-press threshold, in both arenas. The sleep IS the gesture
    // (700 ms > the 500 ms threshold) and a stall only lengthens it, so this direction is safe; what
    // is not safe is a stall between the two writes, which would measure the hold short — hence the
    // retry, which cannot double-count (a short-measured attempt fires a Tap, and these zones have no
    // `on_tap`).
    for (zone, counter) in [("main-tap-zone", "main-holds"), ("popup-tap-zone", "popup-holds")] {
        let (x, y, w, h) = zone_rect(&mut app, zone);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            app.send(&format!("d {} {}", cx as i32, cy as i32));
            std::thread::sleep(Duration::from_millis(700));
            app.send(&format!("u {} {}", cx as i32, cy as i32));
            let poll = Instant::now() + Duration::from_secs(2);
            let mut counted = false;
            loop {
                app.refresh();
                if app.all_texts().iter().any(|t| t.contains(&format!("{counter}: 1"))) {
                    counted = true;
                    break;
                }
                if Instant::now() > poll { break; }
                std::thread::sleep(Duration::from_millis(100));
            }
            if counted { break; }
            assert!(
                Instant::now() < deadline,
                "a 700 ms hold never registered as a long press ({counter})"
            );
            eprintln!("[ui-test] hold did not register — retrying");
        }
    }
}

/// A popup's `on_double_tap` works, including the deferred single tap that precedes it.
///
/// A node with `on_double_tap` never gets its `on_tap` immediately: the tap waits out the double-tap
/// window (`PendingTap`), and a fast second tap in that window turns the pair into a double tap. In a
/// popup both halves have to be re-fired into the popup's own arena — the deferred tap carries it
/// (`PendingTap::overlay_id`).
#[test]
fn a_popup_double_tap_zone_fires_and_defers_its_single_tap() {
    let mut app = UiTest::launch("popup_tap");
    app.expect_text("popup-singles: 0");
    app.expect_text("popup-doubles: 0");
    app.expect_text("popup-taps: 0");

    let (x, y, w, h) = app
        .find_tag_in_overlay("popup-double-zone")
        .expect("no popup-double-zone");
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);

    // One tap: the single tap is deferred, then fired once the window closes (no double followed).
    // Burst down/up (see the note in the tap-family test): a fixed gap is what turns a tap into a
    // long press under load, and this tap must also stay inside the 300 ms double-tap window.
    app.send(&format!("d {} {}", cx as i32, cy as i32));
    app.send(&format!("u {} {}", cx as i32, cy as i32));
    app.expect_text_timeout("popup-singles: 1", Duration::from_secs(5));
    app.expect_text("popup-doubles: 0");

    // Two taps inside the window: one double tap, and no extra single. Bursts again — in one drain
    // they are microseconds apart, which is what the window wants. (A stall between the two bursts
    // would split them into two singles; the assertions below then fail loudly rather than pass.)
    for _ in 0..2 {
        app.send(&format!("d {} {}", cx as i32, cy as i32));
        app.send(&format!("u {} {}", cx as i32, cy as i32));
    }
    app.expect_text_timeout("popup-doubles: 1", Duration::from_secs(5));
    app.expect_text("popup-singles: 1");
}

/// A popup's content follows a CALLER-side value.
///
/// An overlay is a separate composer whose groups re-enter for state they read or parameters they
/// declare — nothing inside it can see that the caller handed it a new content closure. So content
/// built from a value the caller computed and captured (the ordinary shape: format it, then pass it
/// in) used to keep the value of the FIRST closure forever: measured on a probe fixture, the page read
/// `page-n: 3` while the popup still read `popup-n: 0`. `sync_overlays` now marks a reused overlay for
/// recomposition, which is what re-runs its content.
#[test]
fn popup_content_follows_a_caller_side_value() {
    let mut app = UiTest::launch("popup_content");
    app.expect_text("page-n: 0");
    // The popup's own text lives in the popup entries (`all_texts` covers the main tree).
    app.expect_overlay_text("popup-n: 0");

    app.click_tag("bump");
    app.expect_text_timeout("page-n: 1", Duration::from_secs(5));
    app.expect_overlay_text_timeout("popup-n: 1", Duration::from_secs(5));

    // A second bump, so the check is not just "the first update happened to land".
    app.click_tag("bump");
    app.expect_text_timeout("page-n: 2", Duration::from_secs(5));
    app.expect_overlay_text_timeout("popup-n: 2", Duration::from_secs(5));
}

/// The expanded SearchBar's results follow the query.
///
/// The caller filters in ITS scope (`SearchBarState::query_text()` in the parent, then the list is
/// captured by the content lambda) — the shape the Compose sample uses. The panel's body declared the
/// query for its own group, but the caller's lambda sits behind a nested group that declares nothing,
/// so it never re-entered and the list kept the first, unfiltered rows while the input field updated.
/// The items are addressed by tag inside the popup entries, so this asserts what the panel actually
/// shows rather than what the page computed.
#[test]
fn search_results_follow_the_query() {
    let mut app = UiTest::launch("search_results");
    app.expect_text("query: ");

    app.click_tag("open-search");
    // The unfiltered list is what the panel shows first.
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.find_tag_in_overlay("item-Banana").is_none() {
        assert!(Instant::now() < deadline, "the panel never showed its results");
        std::thread::sleep(Duration::from_millis(100));
        app.refresh();
    }
    assert!(app.find_tag_in_overlay("item-Apple").is_some(), "unfiltered: Apple is there");

    // Type "bl": the panel must narrow to the two berries, and the field must have taken the keys.
    for key in ["b", "l"] {
        app.key(key);
    }
    app.expect_text_timeout("query: bl", Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        let has_berries = app.find_tag_in_overlay("item-Blackberry").is_some()
            && app.find_tag_in_overlay("item-Blueberry").is_some();
        let stale_gone = app.find_tag_in_overlay("item-Apple").is_none()
            && app.find_tag_in_overlay("item-Banana").is_none()
            && app.find_tag_in_overlay("item-Cherry").is_none();
        if has_berries && stale_gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the results must follow the query: Blackberry/Blueberry present = {has_berries}, \
             Apple/Banana/Cherry gone = {stale_gone} (the list is showing what the FIRST closure \
             captured when this fails)"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A tap survives its own popup moving under the finger.
///
/// The gesture measures displacement against the overlay's arena origin. Using the LIVE origin adds the
/// popup's own motion to that displacement, so a panel that moves while it is pressed (the expanded
/// `SearchBar` slides for `SEARCH_BAR_EXPAND_MS`) can push a stationary finger past the tap slop and
/// cancel the tap. Here the popup's tap zone moves the popup 300 px on PRESS, so the overlay travels
/// between the press and the release while the pointer stays exactly where it was — the tap must fire.
#[test]
fn a_tap_survives_its_own_popup_moving() {
    let mut app = UiTest::launch("popup_slide_tap");
    app.expect_text("taps: 0");

    // The main tree's first text does not prove the POPUP entry is in the same frame's dump, so wait
    // for the zone itself (and keep the tree fresh: `find_tag_in_overlay` reads the cached one).
    let zone = |app: &mut UiTest| -> Option<(f32, f32, f32, f32)> {
        app.refresh();
        app.find_tag_in_overlay("popup-tap-zone")
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while zone(&mut app).is_none() {
        assert!(Instant::now() < deadline, "the popup never showed its tap zone");
        std::thread::sleep(Duration::from_millis(100));
    }
    let before = zone(&mut app).expect("zone").0;

    // Press and release at the same point, back to back: the zone jumps the popup on the press, so the
    // overlay is already elsewhere when the release arrives.
    let (x, y, w, h) = zone(&mut app).expect("zone");
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    app.send(&format!("d {} {}", cx as i32, cy as i32));
    // A frame must pass between the press and the release: the jump is a state change, so the popup is
    // still at its old place in the frame the press lands in. 150 ms is 3x under the long-press
    // threshold (500 ms), so the gesture stays a tap even on a loaded machine.
    std::thread::sleep(Duration::from_millis(150));
    // A move at the SAME screen point — what a stationary finger still produces once the window moves
    // under it. This is the event that decides the tap: the tracker compares a move against the down
    // position, so an overlay origin that is read live puts the popup's own 300 px into that
    // comparison, crosses the 8 px slop and turns the tap into a cancelled drag. The release position
    // never enters the decision, which is why the press/release pair alone proves nothing.
    app.send(&format!("m {} {}", cx as i32, cy as i32));
    app.send(&format!("u {} {}", cx as i32, cy as i32));

    app.expect_text_timeout("taps: 1", Duration::from_secs(5));
    // And the popup really did move under the finger, or this test would prove nothing.
    let moved = zone(&mut app).expect("zone").0 - before;
    assert!(
        moved > 25.0,
        "the popup must have moved under the finger for this test to mean anything (moved {moved} px)"
    );
}

/// A popup node that is BOTH tappable and draggable gets both — and its drag fires once.
///
/// `overlay_down` created no tracker for a drag target (the overlay drag machinery owned it), so a
/// custom draggable card inside a popup could not tap: only its `on_press` and `on_drag_*` worked. The
/// tracker now owns such a node and the same-node overlay drag session is stood down, which is what
/// this pins — a tap fires, a drag fires `on_drag_start` / `on_drag_end` exactly once each (double
/// dispatch is the failure mode of letting both run), and the drag does not also produce a tap.
#[test]
fn a_popup_drag_target_also_taps_once() {
    let mut app = UiTest::launch("popup_tap");
    app.expect_text("popup-drag-taps: 0");
    app.expect_text("popup-drag-starts: 0");

    let (x, y, w, h) = app
        .find_tag_in_overlay("popup-drag-zone")
        .expect("no popup-drag-zone");
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);

    // A tap: the tap family must fire on a drag-capable node too.
    app.send(&format!("d {} {}", cx as i32, cy as i32));
    app.send(&format!("u {} {}", cx as i32, cy as i32));
    app.expect_text_timeout("popup-drag-taps: 1", Duration::from_secs(5));
    app.expect_text("popup-drag-starts: 0");

    // A drag: one start and one end, no double dispatch, and no tap from the moved gesture.
    app.send(&format!("d {} {}", cx as i32, cy as i32));
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        app.send(&format!("m {} {}", (cx + 60.0 * t) as i32, cy as i32));
        std::thread::sleep(Duration::from_millis(20));
    }
    app.send(&format!("u {} {}", cx + 60.0, cy as i32));
    app.expect_text_timeout("popup-drag-starts: 1", Duration::from_secs(5));
    app.expect_text_timeout("popup-drag-ends: 1", Duration::from_secs(5));
    app.expect_text("popup-drag-taps: 1");
}

/// A `RangeSlider` drag moves the thumb the PRESS resolved, and leaves the other one alone.
///
/// The press-resolution rule (the nearer thumb owns the gesture, so it never swaps mid-drag) is the
/// part of a range slider a real gesture has to get right; the unit tests can only exercise it
/// against a synthetic track width. The end is measured off the same inset axis the track draws its
/// thumbs on (`8 + (w - 16) × f`), so a wrong pixel-to-value mapping fails the value assertion
/// rather than passing on a coincidence.
#[test]
fn range_slider_drags_the_thumb_the_press_resolved() {
    let mut app = UiTest::launch("range_slider");
    app.expect_text("range-start: 0.20");
    app.expect_text("range-end: 0.80");

    let (x, y, w, h) = app.find_tag("range-slider").expect("no range-slider");
    let cy = y + h / 2.0;
    let thumb_x = |f: f32| x + 8.0 + (w - 16.0) * f;
    // The component's own pixel → value mapping, for an LTR track with no steps.
    let value_at = |px: f32| ((px - x - 8.0) / (w - 16.0)).clamp(0.0, 1.0);

    // Drag the START thumb (at 0.2) right to about 0.42: the press lands far nearer it than the end
    // thumb, so it — and only it — follows the finger.
    let px_start = thumb_x(0.42).round();
    let v_start = value_at(px_start);
    assert!(
        (0.40..0.45).contains(&v_start),
        "the drag target should land near 0.42, got {v_start} — the fixture geometry moved"
    );
    app.drag(thumb_x(0.2), cy, px_start, cy);
    let start_text = format!("range-start: {v_start:.2}");
    app.expect_text_timeout(&start_text, Duration::from_secs(5));
    app.expect_text("range-end: 0.80");

    // Same for the END thumb (at 0.8), dragged left to about 0.55.
    let px_end = thumb_x(0.55).round();
    let v_end = value_at(px_end);
    assert!((0.53..0.58).contains(&v_end), "the drag target should land near 0.55, got {v_end}");
    app.drag(thumb_x(0.8), cy, px_end, cy);
    app.expect_text_timeout(&format!("range-end: {v_end:.2}"), Duration::from_secs(5));
    app.expect_text(&start_text);
}

/// The keyboard follows FOCUS, one thumb at a time — Compose's model for a two-thumb slider.
///
/// Each thumb is its own focusable node, so `k Tab` focuses the start thumb and the arrow keys move
/// THAT one; another `k Tab` moves focus to the end thumb, and the arrows then move it. A press does
/// NOT focus a thumb: a click must not take the keyboard from wherever it was, the same rule the
/// overlay tests state and what the plain `Slider` does — so this test never presses anything and the
/// keyboard is reached through Tab alone.
#[test]
fn range_slider_keyboard_moves_the_focused_thumb() {
    let mut app = UiTest::launch("range_slider");
    app.expect_text("range-start: 0.20");
    app.expect_text("range-end: 0.80");

    // Tab into the component: the start thumb takes focus (it is the first focusable in the tree).
    app.send("k Tab");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.tag_is_focused("range-slider") {
        assert!(Instant::now() < deadline, "k Tab should focus a thumb of the slider");
        std::thread::sleep(Duration::from_millis(50));
    }

    // One arrow step moves the focused (start) thumb by 1% of the range, and nothing else.
    app.key_until("ArrowRight", "range-start: 0.21", Duration::from_secs(5));
    app.expect_text("range-end: 0.80");

    // Tab hands focus to the END thumb; the arrows now move that one and leave the start alone.
    app.send("k Tab");
    app.key_until("ArrowLeft", &format!("range-end: {:.2}", 0.80 - 0.01), Duration::from_secs(5));
    app.expect_text("range-start: 0.21");
}

/// A segmented row's selection follows the click, one item at a time, and a multi-choice row toggles
/// its items independently — the two behaviours the row scopes differ by, in a real window.
#[test]
fn segmented_buttons_pick_and_toggle() {
    let mut app = UiTest::launch("segmented_button");
    app.expect_text("picked: 0");
    // Third item, then back to the first: exactly one is selected either way.
    app.click_tag("seg-2");
    app.expect_text_timeout("picked: 2", Duration::from_secs(5));
    app.click_tag("seg-0");
    app.expect_text_timeout("picked: 0", Duration::from_secs(5));

    // Multi-choice: each item toggles on its own.
    app.expect_text("bold: false italic: false");
    app.click_tag("mseg-0");
    app.expect_text_timeout("bold: true italic: false", Duration::from_secs(5));
    app.click_tag("mseg-1");
    app.expect_text_timeout("bold: true italic: true", Duration::from_secs(5));
    app.click_tag("mseg-0");
    app.expect_text_timeout("bold: false italic: true", Duration::from_secs(5));
}

/// A window follows a theme change the application makes itself: the switch pins light and dark, and what
/// was DRAWN changes both ways.
///
/// This is the layer the theme regression lived at — the frame behind the tree flipped while every
/// component kept the colors it started with, because the window's per-frame content wrapper re-provided
/// the palette sampled when the window was created. Nothing in the layout tree shows it (a theme color is
/// resolved when the node is built, and `bg(...)` prints as `<dynamic>`), so the assertion reads the
/// frame: the centre of the window is the fixture's theme-painted surface.
#[test]
fn theme_follows_the_windows_own_switch() {
    let mut app = UiTest::launch("theme_follow");

    // The startup theme is whatever the machine says, so the test establishes both ends itself. `None`
    // from the helper means no pixel could be read at all — that must fail, not read as "black".
    app.click_tag("theme-dark");
    let dark = app.wait_centre_luma(Duration::from_secs(5), |l| l < 96.0);
    assert!(dark.is_some_and(|l| l < 96.0), "the dark theme must paint a dark surface, luma={dark:?}");

    app.click_tag("theme-light");
    let light = app.wait_centre_luma(Duration::from_secs(5), |l| l > 160.0);
    assert!(light.is_some_and(|l| l > 160.0), "the light theme must paint a light surface, luma={light:?}");

    // And back: a second flip has to land as well (the first one must not have been the only one applied).
    app.click_tag("theme-dark");
    let dark_again = app.wait_centre_luma(Duration::from_secs(5), |l| l < 96.0);
    assert!(dark_again.is_some_and(|l| l < 96.0), "a second switch to dark must land too, luma={dark_again:?}");
}

/// The type scale and direction a window is DECLARED under reach the window's content.
///
/// The content is composed by the window's own composer, not by the declaring tree, so both values have to
/// be published into the window's theme cell every frame. Dropping that publish changes nothing that
/// errors: the content silently composes under the defaults (LTR, 14 px). Hence this case — an RTL row
/// (which mirrors) and a `ListItem` whose height follows `Typography::body_large` (declared as 32 px, vs the
/// Material default of 16).
#[test]
fn the_window_content_composes_under_the_declared_typography_and_direction() {
    let mut app = UiTest::launch("theme_typography");

    let first = app.find_tag("ttyp-first").expect("the first tagged box");
    let second = app.find_tag("ttyp-second").expect("the second tagged box");
    assert!(
        first.0 > second.0,
        "the declared RTL direction has to mirror the row: first={first:?}, second={second:?}"
    );

    let item = app.find_tag("ttyp-headline").expect("the styled headline text");
    assert!(
        item.3 > DEFAULT_HEADLINE_HEIGHT,
        "the declared 32 px type scale has to reach the headline: headline={item:?}"
    );
}

/// The same headline text measured under the DEFAULT type scale (`body_large` = 16 px / 24 line height):
/// its node is 24 px tall, and the declared scale (32 px / 40) has to clear that. Measured, not derived —
/// the node's height is whatever the text style resolved to.
const DEFAULT_HEADLINE_HEIGHT: f32 = 30.0;

/// The drag routing inside a `ModalBottomSheet`: the list owns its own scroll, the panel owns the rest.
///
/// Compose M3's rule, checked against `ConsumeSwipeWithinBottomSheetBoundsNestedScrollConnection` and
/// pinned here because the arbitration is three-way (an inner drag component beats a scroll, a scroll beats
/// the panel's `on_drag`) and breaks quietly: an upward delta goes to the sheet first (expands-first) and
/// only the leftover to the list; a downward delta is the list's while it can scroll, and the sheet's once
/// it cannot — so dragging down with the list at its top collapses the sheet, which IS the dismissal
/// gesture rather than a bug (that is what this fixture's shape used to look like from the outside).
///
/// The androidx source, for whoever needs it again (`material3/SheetDefaults.kt`):
/// `onPreScroll { if (delta < 0 && source == NestedScrollSource.UserInput) dispatchRawDelta(delta) }`,
/// `onPostScroll { if (source == NestedScrollSource.UserInput && delta != 0f) dispatchRawDelta(delta) }`.
#[test]
fn bottom_sheet_drag_routing_keeps_the_list_in_charge_of_its_own_scroll() {
    // `find_tag_in_overlay` reads the cached tree, and the sheet animates its settle, so every read is
    // preceded by a refresh and every expectation is polled.
    fn row_y(app: &mut UiTest, tag: &str) -> Option<f32> {
        app.refresh();
        app.find_tag_in_overlay(tag).map(|(_, y, _, _)| y)
    }
    // The lowest-indexed row on screen. Scroll amount after a drag depends on the fling, so assertions about
    // "did the list move" are written against this index rather than against a specific row tag or y.
    fn first_visible_row(app: &mut UiTest) -> Option<usize> {
        app.refresh();
        (0..40).find(|i| app.find_tag_in_overlay(&format!("bs-row-{i}")).is_some())
    }
    fn wait_for(app: &mut UiTest, what: &str, mut cond: impl FnMut(&mut UiTest) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            app.refresh();
            if cond(app) {
                return;
            }
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let mut app = UiTest::launch("bottom_sheet");
    app.click_tag("bs-open");
    app.expect_overlay_text("sheet header");
    let row0 = app.find_tag_in_overlay("bs-row-0").expect("row 0 in the sheet");
    let (x, y) = (row0.0 + 20.0, row0.1 + 20.0);

    // (1) An upward drag while the sheet is half expanded: the sheet expands, the LIST does not scroll —
    // row 0 is still the top row, just higher up on screen.
    app.drag(x, y, x, y - 100.0);
    wait_for(&mut app, "the sheet expands under an upward drag", |a| {
        row_y(a, "bs-row-0").is_some_and(|y0| y0 < row0.1)
    });
    // Settle, not merely "started moving": an upward delta belongs to the sheet until it is fully expanded,
    // so a list drag begun mid-settle has part of its 140 px eaten by the sheet and may not push row 0 out at
    // all. That window is what makes this test fail on a slow backend, where the settle takes more frames.
    wait_for(&mut app, "the expanded sheet comes to rest", |a| {
        let first = row_y(a, "bs-row-0");
        std::thread::sleep(Duration::from_millis(50));
        let second = row_y(a, "bs-row-0");
        matches!((first, second), (Some(f), Some(s)) if f < row0.1 && (f - s).abs() <= 1.0)
    });

    // (2) Now that it is expanded, the same gesture scrolls the LIST: row 0 leaves the viewport.
    let start_y = row_y(&mut app, "bs-row-0").expect("row 0 after expanding") + 20.0;
    app.drag(x, start_y, x, start_y - 140.0);
    wait_for(&mut app, "an expanded sheet must let the list scroll (row 0 should leave)", |a| {
        row_y(a, "bs-row-0").is_none()
    });

    // (3) A downward drag now belongs to the LIST: it scrolls back and the sheet stays open.
    // Two things here cannot be hardcoded. Which rows are on screen depends on how far step (2)'s drag and
    // fling carried the list, so ask for the first row that IS visible rather than for two hand-picked tags
    // (row 1 leaves the viewport after one row of scrolling, row 3 after three). And one 150 px drag is only
    // guaranteed to bring the list a few rows back, while step (4) needs it AT THE TOP — so the first drag
    // makes the point (a lower-indexed row appears, the sheet is untouched) and drags repeat, bounded, until
    // row 0 is back.
    let before_row = first_visible_row(&mut app).expect("a row on screen after step (2)");
    for attempt in 0..12 {
        let visible = (0..40)
            .find_map(|i| app.find_tag_in_overlay(&format!("bs-row-{i}")))
            .unwrap_or_else(|| panic!("no row on screen while scrolling back (attempt {attempt})"));
        app.drag(visible.0 + 20.0, visible.1 + 15.0, visible.0 + 20.0, visible.1 + 150.0);
        wait_for(&mut app, "a downward drag must scroll the list back, not close the sheet", |a| {
            a.overlay_count() == 1
        });
        let now = first_visible_row(&mut app).expect("a row on screen after scrolling back");
        if attempt == 0 {
            assert!(now < before_row, "the downward drag scrolled the list back under the sheet");
        }
        if now == 0 {
            break;
        }
    }
    assert_eq!(
        first_visible_row(&mut app),
        Some(0),
        "the list is back at its top row before the sheet gesture"
    );

    // (4) A long downward drag with the list at its top goes to the SHEET — M3's dismissal gesture — and
    // from Expanded that is PartiallyExpanded first, so the sheet is still there (its footer moves down
    // with the panel). The wait requires the footer to come to REST (two reads a frame apart): a settle
    // tween is still running when the drag helper returns, and a footer that is merely passing through the
    // sampled position would satisfy a bare `>` without the release having landed anywhere.
    let before = row_y(&mut app, "bs-footer").expect("the sheet footer");
    let top = row_y(&mut app, "bs-row-0").expect("row 0 at the top again") + 15.0;
    app.drag(x, top, x, top + 400.0);
    wait_for(&mut app, "a downward drag at the top of the list must move the sheet", |a| {
        let first = row_y(a, "bs-footer");
        std::thread::sleep(Duration::from_millis(50));
        let second = row_y(a, "bs-footer");
        match (first, second) {
            (Some(f), Some(s)) => f > before + 50.0 && (f - s).abs() <= 1.0,
            _ => false,
        }
    });
    assert_eq!(
        app.overlay_count(),
        1,
        "Expanded → PartiallyExpanded is the first half of the dismissal"
    );

    // (5) …and the second long drag finishes it.
    let partial = row_y(&mut app, "bs-footer").expect("the footer in the partial sheet");
    let top = row_y(&mut app, "bs-row-0").expect("row 0 still in the partial sheet") + 15.0;
    app.drag(x, top, x, top + 400.0);
    wait_for(&mut app, "the sheet dismisses after the second downward drag", |a| {
        let _ = partial;
        a.overlay_count() == 0
    });
}

/// The sheet's own surface: M3's EXPANDED sheet is square-cornered, and the radius follows the drag on
/// the way there — so the shape has to be painted per frame.
///
/// The sheet expands by animating its `offset`, which recomposes nothing: with a build-time shape the
/// corners went square only after an unrelated compose (a list scroll) re-ran the closure, and stayed
/// square after collapsing for the same reason. Reported from the demo exactly that way.
#[test]
fn the_bottom_sheet_panel_is_square_when_expanded_and_rounded_again_when_not() {
    fn panel_top(app: &mut UiTest) -> f32 {
        app.refresh();
        let (_, y, _, _) = app.find_tag_in_overlay("bs-content").expect("the sheet content");
        // The drag-handle row sits above it: padding_vertical(12) + a 4 px handle + 12.
        y - 28.0
    }
    /// Wait until the panel's top-left corner is (or is not) painted like the surface just below it: a
    /// rounded corner shows what is BEHIND the panel there, a square one shows the panel itself. Waiting
    /// on the property rather than on a tree coordinate is the point — the sheet slides by animating an
    /// offset, so the node positions in the debug tree do not follow the motion, and an early read sees
    /// the sheet mid-flight.
    fn settle_shape(app: &mut UiTest, square: bool, what: &str) -> ((u8, u8, u8, u8), (u8, u8, u8, u8)) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let top = panel_top(app);
            // (2, top+2) is inside a 28 dp corner arc; (2, top+40) is clear of it.
            let corner = app.pixel_at_logical(2.0, top + 2.0).expect("the corner pixel");
            let inside = app.pixel_at_logical(2.0, top + 40.0).expect("a pixel on the panel");
            if (corner == inside) == square {
                return (corner, inside);
            }
            assert!(
                Instant::now() < deadline,
                "{what}: corner={corner:?} panel={inside:?} (top={top})"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let mut app = UiTest::launch("bottom_sheet");
    app.click_tag("bs-open");
    app.expect_overlay_text("sheet header");

    // Half expanded: the corner is cut away, so the pixel there is the page (dimmed by the scrim).
    settle_shape(&mut app, false, "a partially expanded sheet keeps its 28 dp top corners");

    // Expand. M3 squares the corners only when the sheet reaches the full window height.
    let row = app.find_tag_in_overlay("bs-row-2").expect("a list row");
    app.drag(row.0 + 20.0, row.1 + 20.0, row.0 + 20.0, row.1 - 320.0);
    settle_shape(&mut app, true, "an expanded sheet is square-cornered (M3 ExpandedShape)");

    // Collapse again: the corners have to come back. The drag has to start on the HANDLE, not on a list
    // row: the expand drag scrolled the list, so a downward drag there would go to the list (which is
    // exactly the routing the other sheet test pins).
    let top = panel_top(&mut app);
    app.drag(240.0, top + 14.0, 240.0, top + 14.0 + 320.0);
    settle_shape(&mut app, false, "collapsing brings the rounded corners back");
}

/// Clicking the scrim dismisses the sheet — the overlay really goes away, with nothing left drawing.
///
/// A user hit the failure this guards: the panel slid out of view but the overlay stayed open, so only its
/// modal scrim was left on screen (a dim layer no further interaction removed). The panel's "I am done
/// sliding" observer used to be evaluated on a single compose where the tween is still registered; it now
/// measures geometry while the slide runs, and the overlay layer drops a closing overlay past a deadline
/// regardless of its fade.
#[test]
fn clicking_the_scrim_dismisses_the_sheet_completely() {
    let mut app = UiTest::launch("bottom_sheet");
    app.click_tag("bs-open");
    app.expect_overlay_text("sheet header");

    // The scrim: inside the overlay layer, above the panel.
    app.click(40.0, 40.0);
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.overlay_count() > 0 {
        assert!(Instant::now() < deadline, "the sheet's overlay must be gone after a scrim click");
        std::thread::sleep(Duration::from_millis(50));
        app.refresh();
    }
    // …and the page is undimmed again: a point above the sheet's panel reads the same before and after
    // the sheet was ever opened (with the sheet open it is the page under the scrim).
    let page = app.pixel_at_logical(240.0, 100.0).expect("a page pixel");
    app.click_tag("bs-open");
    app.expect_overlay_text("sheet header");
    std::thread::sleep(Duration::from_millis(300));
    let dimmed = app.pixel_at_logical(240.0, 100.0).expect("a page pixel under the scrim");
    assert!(dimmed.0 < page.0, "the scrim dims the page: {page:?} → {dimmed:?}");
}


/// A popup that is ALREADY OPEN follows the theme as well.
///
/// Its content composes in a composer of its own, under a `CompositionLocal` snapshot captured when the
/// popup was declared — a snapshot holding the palette as a value. It follows because the declaring tree
/// re-runs on a theme change (that is what `refresh_theme` marks dirty) and hands the popup a FRESH
/// snapshot; this test is what keeps that claim honest, since nothing about the snapshot looks live.
#[test]
fn an_open_popup_follows_the_theme() {
    let mut app = UiTest::launch("theme_follow");
    app.click_tag("theme-dark");
    let dark = app.wait_centre_luma(Duration::from_secs(5), |l| l < 96.0);
    assert!(dark.is_some_and(|l| l < 96.0), "the window must be dark first, luma={dark:?}");

    app.click_tag("theme-popup");
    app.expect_overlay_text("popup");
    let (x, y, w, h) = app.find_tag_in_overlay("theme-popup-panel").expect("the popup panel");
    let (px, py) = (x + w / 2.0, y + h / 2.0);
    let opened = app.wait_pixel_luma(px, py, Duration::from_secs(5), |l| l < 96.0);
    assert!(opened.is_some_and(|l| l < 96.0), "the popup opens on the current theme, luma={opened:?}");

    // Switch the theme while it stays open.
    app.click_tag("theme-light");
    let after = app.wait_pixel_luma(px, py, Duration::from_secs(5), |l| l > 160.0);
    assert!(after.is_some_and(|l| l > 160.0), "an OPEN popup must follow the theme, luma={after:?}");
    // …and the window behind it did too.
    let window = app.wait_centre_luma(Duration::from_secs(5), |l| l > 160.0);
    assert!(window.is_some_and(|l| l > 160.0), "the window follows as well, luma={window:?}");
}

// ═══════════════════════════════════════════════════════════════
// fixture_swipe_dismiss: SwipeToDismissBox rows inside a scrolling list
// ═══════════════════════════════════════════════════════════════

/// Reads the list's scroll offset out of the fixture's status line.
fn read_voffset(app: &mut UiTest) -> f32 {
    app.refresh();
    app.all_texts()
        .iter()
        .find_map(|s| {
            s.find("voffset: ").and_then(|i| {
                s[i + "voffset: ".len()..]
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
            })
        })
        .unwrap_or(-1.0)
}

/// Whether the fixture's tree currently contains `needle` (a fresh read, not a cached tree).
fn has_text(app: &mut UiTest, needle: &str) -> bool {
    app.refresh();
    app.all_texts().iter().any(|t| t.contains(needle))
}

/// Drags `from → to` until the fixture reports `expected`, retrying the gesture itself: the debug
/// event injection drops an `m`/`u` occasionally (the same flakiness the scroll cases retry for), and a
/// dropped RELEASE leaves a row parked mid-swipe instead of settled — nothing else can dismiss it.
///
/// Each retry first taps the row's own start point: a tap is a press/release pair, which ends any
/// gesture a dropped `u` left open. The window is generous because a settle tween has to finish before
/// `on_dismiss` reports; the tray cannot hide a real regression, because a row parked at a dismiss
/// anchor has its drag callbacks gated off — a re-drag then does nothing at all, so a row that never
/// settles can never pass.
fn drag_until_reported(app: &mut UiTest, from: (f32, f32), to: (f32, f32), expected: &str) {
    for attempt in 1..=4 {
        app.drag(from.0, from.1, to.0, to.1);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if has_text(app, expected) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        eprintln!("[ui-test] attempt {attempt}: `{expected}` was not reported — resetting and retrying");
        app.click(from.0, from.1);
    }
    app.expect_text_timeout(expected, Duration::from_secs(5));
}

/// Given a list of swipe rows, When a finger drags VERTICALLY over one of them, Then the list scrolls
/// and no row is dismissed.
///
/// This is the arbitration that makes the component usable in a list: the press is claimed by both the
/// row's `on_drag` and the list's scroll, and the finger's axis decides which one keeps it
/// (`app::gesture_move`). Without that, the row — the deeper node — would take the gesture, ignore the
/// vertical motion, and leave the list stuck.
#[test]
fn a_vertical_drag_over_a_swipe_row_scrolls_the_list() {
    let mut app = UiTest::launch("swipe_dismiss");
    app.expect_text("count: 10");
    assert_eq!(read_voffset(&mut app), 0.0, "the list starts at the top");

    // Row 3 sits around y 175..223; the finger travels 140 px upwards inside it. Retried for the same
    // reason as the scroll cases (the injection link drops the occasional event).
    let mut offset = 0.0;
    for attempt in 1..=3 {
        app.drag(190.0, 200.0, 190.0, 60.0);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            offset = read_voffset(&mut app);
            if offset > 40.0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if offset > 40.0 {
            break;
        }
        eprintln!("[ui-test] vertical drag attempt {attempt} did not scroll (voffset={offset})");
    }
    assert!(offset > 40.0, "a vertical drag must scroll the list, voffset={offset}");
    app.expect_text("count: 10"); // …and it must not have dismissed anything
    app.expect_text("last: none");
}

/// Given a swipe row, When the finger drags it horizontally past the dismiss threshold, Then the row
/// leaves and `on_dismiss` reports which way it went.
#[test]
fn a_long_horizontal_drag_dismisses_the_row() {
    let mut app = UiTest::launch("swipe_dismiss");
    app.expect_text("count: 10");

    // Row 0 sits around y 31..79; the finger travels 280 px to the left.
    drag_until_reported(&mut app, (340.0, 55.0), (60.0, 55.0), "count: 9");

    app.expect_text("last: 0 left");
    app.expect_text("row 1"); // the rest of the list is untouched
}

/// Given a swipe row, When the finger drags it a short way and lets go slowly, Then the row springs
/// back: the distance is under the 56 px positional threshold and the release is too slow to be a
/// fling.
#[test]
fn a_short_slow_horizontal_drag_settles_the_row_back() {
    let mut app = UiTest::launch("swipe_dismiss");
    app.expect_text("count: 10");

    // 12 px over eight steps: the last step is ~1.5 px per 20 ms (~75 px/s, a clear margin under the
    // 125 px/s fling threshold), so the release is judged by distance — and 12 px is far below the
    // 56 px threshold. A larger distance shrinks that margin: the velocity is `delta / dt` per
    // PROCESSED delta, so a step the fixture drains together with the previous one would read as a
    // fling and this row would dismiss.
    app.drag(300.0, 55.0, 288.0, 55.0);
    std::thread::sleep(Duration::from_millis(500)); // let the settle animation finish

    app.expect_text("count: 10");
    app.expect_text("last: none");
    app.expect_text("row 0");
}

/// Given a row whose leftward direction is switched off, When it is dragged that way, Then it stays
/// put — while the other direction still dismisses it.
#[test]
fn a_disabled_dismiss_direction_leaves_the_row_in_place() {
    let mut app = UiTest::launch("swipe_dismiss");
    app.expect_text("count: 10");

    // Row 1 sits around y 79..127.
    app.drag(340.0, 103.0, 60.0, 103.0); // leftwards: refused
    std::thread::sleep(Duration::from_millis(500));
    app.expect_text("count: 10");
    app.expect_text("last: none");
    // (This half alone would also pass if the drag had been dropped altogether; the rightward drag
    // below is what proves the row is alive and took the refusal on purpose.)

    drag_until_reported(&mut app, (60.0, 103.0), (340.0, 103.0), "count: 9"); // rightwards: allowed
    app.expect_text("last: 1 right");
}

/// Given a list whose rows are keyed by item id, When the top row is dismissed, Then the row that
/// moves up into its place is a LIVE row: draggable in its own right, not carrying the departed row's
/// parked offset.
///
/// A rebuild that is not keyed — a plain `for` loop over the data — hands the arriving item the
/// remembered state of the position it moved into. The box would come up settled at the dismiss
/// anchor: content parked off the row and gestures gated off, which is what this case catches.
/// `LazyColumn::items_from` keys the item's composition group, so the state follows the ITEM.
#[test]
fn the_row_that_moves_into_a_dismissed_slot_is_live() {
    let mut app = UiTest::launch("swipe_dismiss");
    app.expect_text("count: 10");

    // Row 0 goes first, towards the trailing edge…
    drag_until_reported(&mut app, (340.0, 55.0), (60.0, 55.0), "count: 9");
    app.expect_text("last: 0 left");

    // The arriving row must not be carrying the departed row's parked offset: its content sits at the
    // row's own left edge. Text alone cannot see the difference — the parked symptom is GEOMETRY (the
    // content laid out one width away, leaving nothing but the background visible), so assert bounds.
    app.refresh();
    let (x, _, _, _) = app.find_tag("sd-row-1").expect("the row that moved up into the freed slot");
    assert!(
        (x - 12.0).abs() < 1.0,
        "the row that moved up must sit at the list's left edge (x=12), got x={x}"
    );

    // …and it must still take a dismissal of its own, the other way (row 1 refuses the leftward
    // direction, so the rightward one is what proves it alive).
    drag_until_reported(&mut app, (60.0, 55.0), (340.0, 55.0), "count: 8");
    app.expect_text("last: 1 right");
}


// ── Semantics (accessibility) ──

/// The semantics snapshot reaches a client, names what it should, follows a click, and carries the
/// overlays — the four things a screen reader depends on. The model's own rules are unit-tested in
/// `winia/src/semantics.rs`; this is the channel.
#[test]
fn semantics_are_published_per_frame_and_follow_the_state() {
    let mut app = UiTest::launch("semantics");

    /// The main tree, or an empty slice.
    fn main_tree(snapshot: &serde_json::Value) -> &Vec<serde_json::Value> {
        snapshot["main"].as_array().expect("main is an array")
    }

    /// Depth-first search of the whole snapshot (main tree only) for a name.
    fn find<'a>(items: &'a [serde_json::Value], name: &str) -> Option<&'a serde_json::Value> {
        for item in items {
            if item["name"] == name {
                return Some(item);
            }
            if let Some(hit) = find(item["children"].as_array().map(Vec::as_slice).unwrap_or(&[]), name) {
                return Some(hit);
            }
        }
        None
    }

    /// The first element declared as a live region, depth-first.
    fn find_live_region<'a>(items: &'a [serde_json::Value]) -> Option<&'a serde_json::Value> {
        for item in items {
            if item["liveRegion"].is_string() {
                return Some(item);
            }
            if let Some(hit) = find_live_region(
                item["children"].as_array().map(Vec::as_slice).unwrap_or(&[]),
            ) {
                return Some(hit);
            }
        }
        None
    }

    let snapshot = app.semantics_until(Duration::from_secs(3), |s| {
        find(main_tree(s), "Merged button").is_some()
    })
    .expect("a semantics snapshot");

    // A click target reads as ONE element named by its label, with the role the component declared.
    let button = find(main_tree(&snapshot), "Merged button").expect("the button by its label");
    assert_eq!(button["role"], "button");
    assert_eq!(button["clickable"], true);
    assert!(
        button["children"].as_array().is_some_and(|c| c.is_empty()),
        "the label was absorbed, not left hanging: {button}"
    );

    // A disabled control still reports its role, its name AND that it is disabled.
    let disabled = find(main_tree(&snapshot), "Disabled button").expect("the disabled button");
    assert_eq!(disabled["role"], "button");
    assert_eq!(disabled["state"]["enabled"], false);

    // Roles that differ in Compose differ here: checked vs selected.
    let switch = find_role(main_tree(&snapshot), "switch").expect("the switch");
    assert_eq!(switch["state"]["checked"], "on");
    let radio = find_role(main_tree(&snapshot), "radiobutton").expect("the radio");
    assert_eq!(radio["state"]["selected"], false);

    // An icon is announced only when it was given a description.
    let icon = find(main_tree(&snapshot), "Described icon").expect("the described icon");
    assert_eq!(icon["role"], "image");

    // Plain text is an element with a name and no role.
    let text = find(main_tree(&snapshot), "Plain label").expect("the plain text");
    assert_eq!(text["role"], serde_json::Value::Null);

    // A progress bar reports its value — what a screen reader announces as a percentage. The
    // INDETERMINATE one reports the role and no value: "in progress" is not "0 percent".
    let bar = find_role(main_tree(&snapshot), "progressbar").expect("the determinate progress bar");
    assert_eq!(bar["state"]["progress"]["value"], 0.25);
    assert_eq!(bar["state"]["progress"]["min"], 0.0);
    assert_eq!(bar["state"]["progress"]["max"], 1.0);
    let mut bars: Vec<&serde_json::Value> = Vec::new();
    for node in main_tree(&snapshot) {
        collect_role(node, "progressbar", &mut bars);
    }
    assert_eq!(bars.len(), 2, "both progress bars are in the tree");
    let with_value = bars
        .iter()
        .filter(|bar| bar["state"]["progress"].is_object())
        .count();
    assert_eq!(with_value, 1, "exactly one of them reports a value");

    // A snackbar, shown by a real click, is a LIVE REGION whose message is announced unprompted — and
    // its action stays a separate, invokable element. Both halves matter: declaring the mode on the
    // whole bar instead absorbed the action button, leaving a screen reader able to hear the message
    // but with nothing to invoke.
    app.click_tag("sem-show-snackbar");
    let with_snackbar = app
        .semantics_until(Duration::from_secs(3), |s| {
            find_live_region(main_tree(s)).is_some()
        })
        .expect("a snapshot with the snackbar");
    let items = main_tree(&with_snackbar);
    let region = find_live_region(items).expect("the snackbar's message is a live region");
    assert_eq!(region["liveRegion"], "polite", "a snackbar is information, not an alarm");
    assert_eq!(region["name"], "Saved", "and the announced text is the message");
    let undo = find(items, "Undo").expect("the action button must stay its own element");
    assert_eq!(undo["role"], "button");
    assert_eq!(undo["clickable"], true, "and stay invokable, not absorbed by the region");

    // The state follows a real click: on → off through the checkbox's own handler.
    let checkbox = find_role(main_tree(&snapshot), "checkbox").expect("the checkbox");
    assert_eq!(checkbox["state"]["checked"], "on");
    let (cx, cy) = node_center(checkbox);
    app.click(cx, cy);
    let after = app
        .semantics_until(Duration::from_secs(3), |s| {
            find_role(main_tree(s), "checkbox")
                .and_then(|c| c["state"]["checked"].as_str().map(str::to_string))
                .as_deref()
                == Some("off")
        })
        .expect("a snapshot after the click");
    let checkbox = find_role(main_tree(&after), "checkbox").expect("the checkbox after the click");
    assert_eq!(
        checkbox["state"]["checked"], "off",
        "the snapshot must follow the click, not lag a frame behind it"
    );

    // An open dialog is part of the snapshot, and its own contents are in it.
    app.click_tag("sem-open-dialog");
    let with_dialog = app
        .semantics_until(Duration::from_secs(3), |s| {
            s["overlays"].as_array().is_some_and(|o| !o.is_empty())
        })
        .expect("a snapshot with the dialog");
    let overlays = with_dialog["overlays"].as_array().expect("overlays is an array");
    let dialog_tree = overlays[0]["tree"].as_array().expect("an overlay tree");
    assert!(
        find(dialog_tree, "Dialog title").is_some(),
        "a modal's contents must be reachable: {dialog_tree:?}"
    );
    assert!(find(dialog_tree, "Confirm").is_some());

    /// Every element with this role, depth-first.
    fn collect_role<'a>(item: &'a serde_json::Value, role: &str, out: &mut Vec<&'a serde_json::Value>) {
        if item["role"] == role {
            out.push(item);
        }
        for child in item["children"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            collect_role(child, role, out);
        }
    }

    /// The first element in the tree with this role.
    fn find_role<'a>(items: &'a [serde_json::Value], role: &str) -> Option<&'a serde_json::Value> {
        for item in items {
            if item["role"] == role {
                return Some(item);
            }
            if let Some(hit) = find_role(item["children"].as_array().map(Vec::as_slice).unwrap_or(&[]), role) {
                return Some(hit);
            }
        }
        None
    }

    /// The centre of an element, from the bounds the snapshot reports (logical coordinates).
    fn node_center(node: &serde_json::Value) -> (f32, f32) {
        let b = node["bounds"].as_array().expect("bounds");
        let (x, y, w, h) = (
            b[0].as_f64().unwrap_or(0.0) as f32,
            b[1].as_f64().unwrap_or(0.0) as f32,
            b[2].as_f64().unwrap_or(0.0) as f32,
            b[3].as_f64().unwrap_or(0.0) as f32,
        );
        (x + w / 2.0, y + h / 2.0)
    }
}

// ── Observable collections ──

/// A `StateList` drives a `LazyColumn` through real mutations in a real window: push adds a row, pop
/// takes the last one away, removing the FIRST re-keys the rest.
///
/// The unit tests in `winia/src/core/state_list.rs` cover the collection's own contract. What this adds
/// is the integration a caller depends on: the snapshot goes into `items_from` without copying
/// elements, and a mutation reaches the RENDERED rows. The row count is the assertion that matters — a
/// list that failed to notify would keep drawing the rows it was built with.
#[test]
fn a_state_list_drives_a_lazy_column_through_real_mutations() {
    /// The count the fixture prints, as the tree reports it.
    fn count(app: &mut UiTest) -> Option<usize> {
        app.refresh();
        // The tree reports the node as , so the digits have to be taken out of the
        // middle rather than parsed as the whole tail.
        app.all_texts()
            .into_iter()
            .find_map(|text| {
                let after = text.split("count:").nth(1)?;
                let digits: String = after.trim().chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
    }
    /// Poll: a click lands on the next frame, so the count follows a moment later.
    fn wait_for_count(app: &mut UiTest, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if count(app) == Some(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the rendered count never became {expected}; the tree still says {:?}",
                count(app)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn has_row(app: &mut UiTest, id: i32) -> bool {
        app.refresh();
        app.find_tag(&format!("sl-row-{id}")).is_some()
    }

    let mut app = UiTest::launch("state_list");
    wait_for_count(&mut app, 3);
    assert!(
        has_row(&mut app, 1) && has_row(&mut app, 3),
        "the fixture's first three rows must be composed"
    );

    // push: a new row appears, and it is the one the fixture computed (one past the largest id).
    app.click_tag("sl-push");
    wait_for_count(&mut app, 4);
    assert!(has_row(&mut app, 4), "the pushed row must be composed");

    app.click_tag("sl-push");
    wait_for_count(&mut app, 5);
    assert!(has_row(&mut app, 5));

    // pop: back to four, and the id that goes away is the last one.
    app.click_tag("sl-pop");
    wait_for_count(&mut app, 4);
    assert!(!has_row(&mut app, 5), "the popped row must be gone");
    assert!(has_row(&mut app, 1), "and the others stay");

    // remove(0): the first row leaves and the rest keep their identity — their keys are ids, not
    // positions, so a list that re-keyed by index would land the wrong content here.
    app.click_tag("sl-drop-first");
    wait_for_count(&mut app, 3);
    assert!(!has_row(&mut app, 1), "the first row must be gone");
    assert!(
        has_row(&mut app, 2) && has_row(&mut app, 3) && has_row(&mut app, 4),
        "the remaining rows keep their ids"
    );

    // Emptying it: popping past the start is not a panic, it is an empty list.
    for _ in 0..4 {
        app.click_tag("sl-pop");
    }
    wait_for_count(&mut app, 0);
    assert!(!has_row(&mut app, 2), "and no rows are left");
}

// ── Long press fires at its deadline ──

/// `on_long_press` arrives while the finger is still DOWN, at the 500 ms deadline — not on release.
///
/// This is the user-visible half of the change: a caller that wants to open a context menu (or start
/// a drag, or buzz) on a hold has to be told at the hold, and until now it was told at the release.
/// The assertion that makes it a test of THAT: the pointer is never released before the count is
/// checked, and the release then adds nothing.
///
/// Driven with explicit `d`/`u` so the press is genuinely held: the fixture's zones are the same ones
/// `a_popup_tap_zone_fires_the_tap_family_like_the_main_tree` uses.
#[test]
fn a_long_press_fires_while_the_pointer_is_still_down() {
    /// The counter the fixture prints, as the tree reports it.
    fn counter(app: &mut UiTest, label: &str) -> Option<u32> {
        app.refresh();
        app.all_texts().into_iter().find_map(|text| {
            let after = text.split(&format!("{label}:")).nth(1)?;
            let digits: String = after.trim().chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
    }
    fn wait_for(app: &mut UiTest, label: &str, expected: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if counter(app, label) == Some(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "`{label}` never became {expected}; the tree says {:?}",
                counter(app, label)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// [`wait_for`] without the panic, for a caller that may need to re-inject a gesture.
    fn wait_until(app: &mut UiTest, label: &str, expected: u32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if counter(app, label) == Some(expected) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let mut app = UiTest::launch("popup_tap");
    app.expect_text("main-holds: 0");
    let (x, y, w, h) = app.find_tag("main-tap-zone").expect("the page tap zone");
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);

    // Press and HOLD: no `u` yet, and the count must arrive on its own. The press is re-injected if nothing
    // arrives at all, because under the full suite's parallel load an injected pointer-down can be lost
    // before the app ever sees it (measured: `main-holds` stayed 0 for a whole five-second wait while the
    // same test passes alone twice). Each attempt is released first, so no attempt inherits the last one's
    // gesture state.
    let mut held = false;
    for attempt in 0..3 {
        if attempt > 0 {
            app.send(&format!("u {} {}", cx as i32, cy as i32));
            std::thread::sleep(Duration::from_millis(60));
        }
        app.send(&format!("d {} {}", cx as i32, cy as i32));
        if wait_until(&mut app, "main-holds", 1, Duration::from_millis(1500)) {
            held = true;
            break;
        }
    }
    assert!(
        held,
        "a held press must fire the long press on its own; the tree says {:?}",
        counter(&mut app, "main-holds")
    );
    assert_eq!(
        counter(&mut app, "main-taps"),
        Some(0),
        "a hold is not a tap, and the press is still down"
    );

    // Releasing ends the gesture without a second long press and without a tap.
    app.send(&format!("u {} {}", cx as i32, cy as i32));
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(counter(&mut app, "main-holds"), Some(1), "the release must not fire it again");
    assert_eq!(counter(&mut app, "main-taps"), Some(0), "and must not turn the hold into a tap");

    // A quick press-and-release is still a tap, in the same window and the same fixture: the deadline
    // machinery did not swallow short presses.
    app.send(&format!("d {} {}", cx as i32, cy as i32));
    app.send(&format!("u {} {}", cx as i32, cy as i32));
    wait_for(&mut app, "main-taps", 1);
    assert_eq!(counter(&mut app, "main-holds"), Some(1), "and no hold was invented");

    // The popup's arena goes through the same path (the deadline sweep dispatches through the
    // gesture's own arena, so a popup hold has to fire too).
    let (px, py, pw_, ph) = app.find_tag_in_overlay("popup-tap-zone").expect("the popup tap zone");
    app.send(&format!("d {} {}", (px + pw_ / 2.0) as i32, (py + ph / 2.0) as i32));
    wait_for(&mut app, "popup-holds", 1);
    app.send(&format!("u {} {}", (px + pw_ / 2.0) as i32, (py + ph / 2.0) as i32));
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(counter(&mut app, "popup-holds"), Some(1), "and only once there either");
}


/// `BoxWithConstraints` composes its content DURING measurement, with the constraints that
/// measurement computed — Compose's `SubcomposeLayout` relation.
///
/// The regression this pins is the difference between the subcomposition and the frame-lagged
/// approximation that preceded it: the content must print the parent's cap on the FIRST frame the
/// window publishes (the old implementation printed its unbounded initial value and only learned the
/// real one a composition later), the box must take its size from that content, and a change of the
/// parent's cap must re-arrange the content rather than wait for the next frame. The content fills
/// the width it is given, so the box's measured width IS the cap the scope reported.
#[test]
fn box_with_constraints_composes_its_content_at_measure_time() {
    let mut app = UiTest::launch("bwc");

    // Frame one: the scope already carries the real cap (200), not the unbounded placeholder, and the
    // box is sized to the content it composed (the text fills the box, so both are 200 wide).
    app.expect_text("BWC max 200");
    let (w, h) = app.find_tag_size("bwc-box").expect("the box is in the tree");
    assert!(w > 0.0, "the box has a real width, got {w}");
    assert!(h > 0.0, "and a real height, got {h}");
    assert!(
        !app.all_texts().iter().any(|t| t.contains("BWC-not-measured")),
        "the first frame must not report the unmeasured placeholder: {:?}",
        app.all_texts()
    );

    // Narrowing the parent re-arranges the content in the same measure pass. `click_until`, not
    // `click_tag`: the debug click path loses the occasional click (`docs/ui-testing.md`), and this
    // one is asserted by its effect rather than retried by hand.
    let (bx, by, bw, bh) = app.find_tag("bwc-narrow").expect("the narrow button");
    app.click_until(bx + bw / 2.0, by + bh / 2.0, Duration::from_secs(2), |tree| {
        ui::UiTest::tree_texts(tree).iter().any(|t| t.contains("BWC max 120"))
    });
    app.expect_text_timeout("BWC max 120", Duration::from_secs(5));
    let (w2, _) = app.find_tag_size("bwc-box").expect("the box is still in the tree");
    assert!(w2 > 0.0, "the box is still sized after the cap changed, got {w2}");

    // And back — the width has to follow in both directions. `click_until` here for the same reason
    // as the narrow click: the debug click path loses the occasional click (`docs/ui-testing.md`),
    // and this direction is asserted by its effect rather than retried by hand.
    let (wx, wy, ww, wh) = app.find_tag("bwc-wide").expect("the wide button");
    app.click_until(wx + ww / 2.0, wy + wh / 2.0, Duration::from_secs(2), |tree| {
        ui::UiTest::tree_texts(tree).iter().any(|t| t.contains("BWC max 200"))
    });
    app.expect_text_timeout("BWC max 200", Duration::from_secs(5));
    let (w3, _) = app.find_tag_size("bwc-box").expect("the box is still in the tree");
    assert!(w3 > 0.0, "and still sized after the cap returned, got {w3}");

    // One adopted child, not an accumulation of them: a subcomposed subtree is replaced, never
    // stacked (two live copies would claim one synthetic key and trip the arena's dup-key guard).
    let tree = app.tree().expect("a tree");
    assert_eq!(
        ui::UiTest::tree_texts(&tree)
            .iter()
            .filter(|t| t.contains("BWC max"))
            .count(),
        1,
        "exactly one subcomposed content node after three frames: {:?}",
        ui::UiTest::tree_texts(&tree)
    );
}

/// A caller-supplied `TabRow` indicator composes DURING measurement, with the positions the row just
/// computed, and what it draws lands on the selected tab.
///
/// This runs through a real `#[composable]` frame on purpose — every defect this slot had was
/// invisible to a unit test: the macro refused to inject statement keys into a TWO-parameter content
/// closure (the fixture PANICKED on startup until that was fixed), the adopted subtree's node was
/// painted at the window origin, and the row counted the adopted child as one of its own (so a
/// three-tab row measured its tabs for a four-tab one, 488/4 = 122 instead of 488/3 ≈ 162.7). The
/// geometric assertion below is what catches the last one.
#[test]
fn a_caller_supplied_tab_indicator_is_composed_at_measure_time() {
    let mut app = UiTest::launch("tab_indicator");
    app.expect_text("sel 0");

    let (x0, y0, w0, h0) = app
        .find_tag("custom-indicator")
        .expect("the caller's indicator is in the tree");
    assert!(w0 > 0.0 && h0 > 0.0, "the indicator has a real box: {w0}x{h0}");
    assert!(y0 > 0.0, "and a real position: y={y0}");
    assert!(
        x0 > 0.0 && x0 + w0 < 420.0,
        "inside the window: x={x0} w={w0}"
    );

    // Switching to the third tab has to move it by two tab widths. The row is 420 wide with 16 of
    // padding on both sides, so 388/3 ≈ 129.3 per tab: the delta is ≈ 258.7 minus the difference in
    // content widths, and ±10 still fails a four-tab row (2 × 122 = 244).
    let (bx, by, bw, bh) = app.find_tag("pick-third").expect("the third-tab button");
    app.click_until(bx + bw / 2.0, by + bh / 2.0, Duration::from_secs(2), |tree| {
        ui::UiTest::tree_texts(tree).iter().any(|t| t.contains("sel 2"))
    });
    app.expect_text_timeout("sel 2", Duration::from_secs(5));

    let (x2, _, w2, _) = app
        .find_tag("custom-indicator")
        .expect("the indicator is still in the tree");
    assert!(w2 > 0.0, "still sized after the selection changed: {w2}");
    assert!(
        (x2 - x0 - 258.7).abs() < 10.0,
        "the indicator followed the selection by two tab widths: x {x0} -> {x2} (expected ≈ {})",
        x0 + 258.7
    );
}

// ═══════════════════════════════════════════════════════════════
// fixture_dropdown_menu：弹出菜单的既有行为（本轮先钉住，再对齐 Compose）
// ═══════════════════════════════════════════════════════════════

/// What:  a `DropdownMenu` anchored to a trigger button, three items (one disabled).
/// When:  open it with the trigger, then click an ENABLED item.
/// Then:  the item's callback ran (`dm-picked`), the menu closed (`dm-open: no`), and the popup entry
///        is gone from the tree.
///
/// The menu had no test at all before this round, and the Compose-alignment work rewrites its geometry
/// and API — this is the behaviour that must not move while that happens.
#[test]
fn dropdown_menu_opens_and_an_item_pick_closes_it() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    assert_eq!(app.overlay_count(), 0, "a closed menu has no popup entry");

    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_text_timeout("dm-open: yes", Duration::from_secs(5));

    // The menu itself lives in an overlay entry, which the main-tree assertions deliberately ignore.
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    app.expect_overlay_text_timeout("删除", Duration::from_secs(5));
    assert_eq!(app.overlay_count(), 1, "one popup entry while the menu is open");

    app.click_overlay_tag("dm-item-new");
    app.expect_text_timeout("dm-picked: new", Duration::from_secs(5));
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));

    // …and its entry is removed, not left behind (the failure mode the overlay `active` recording exists
    // for: a stale popup that keeps accepting clicks).
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        app.refresh();
        if app.overlay_count() == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the popup entry must be removed once the menu closes"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What:  the same menu.
/// When:  the DISABLED item is clicked.
/// Then:  nothing fires and the menu stays open — Compose's `DropdownMenuItem(enabled = false)` is not
///        clickable at all, so the click cannot reach the dismiss path either.
#[test]
fn dropdown_menu_a_disabled_item_neither_fires_nor_dismisses() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("删除", Duration::from_secs(5));

    app.click_overlay_tag("dm-item-delete");
    std::thread::sleep(Duration::from_millis(300));
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("dm-picked: none")),
        "a disabled item must not fire its callback: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("dm-open: yes")),
        "a disabled item must not dismiss the menu: {texts:?}"
    );
}

/// What:  the same menu.
/// When:  a click lands outside it (the popup area is 160 wide, anchored left).
/// Then:  the menu dismisses through `on_dismiss_request`, and nothing was picked.
#[test]
fn dropdown_menu_dismisses_on_an_outside_click() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_text_timeout("dm-open: yes", Duration::from_secs(5));
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));

    // Far right of the menu's own area (the menu is 160 wide, anchored to the left column).
    app.click(360.0, 220.0);
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("dm-picked: none")),
        "an outside click must not pick anything: {texts:?}"
    );
}

/// What:  an open `DropdownMenu` with three items.
/// When:  the tree is read.
/// Then:  the geometry matches material3's menu metrics, read off the androidx sources
///        (`material3/Menu.kt`: item `sizeIn(minWidth 112dp, maxWidth 280dp, minHeight 48dp)`,
///        `DropdownMenuVerticalPadding = 8dp` around the column).
///
/// The item numbers are the reason this test exists: the component used to hard-code `size(160, 36)`,
/// so every one of these assertions fails on the old geometry.
#[test]
fn dropdown_menu_geometry_matches_the_material3_metrics() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    app.refresh();

    for tag in ["dm-item-new", "dm-item-rename", "dm-item-delete"] {
        let (_, _, w, h) = app.find_tag_in_overlay(tag).unwrap_or_else(|| {
            panic!("{tag} should be in the popup entry (sizes: {:?})", app.overlay_texts())
        });
        assert!(
            (112.0..=280.0).contains(&w),
            "{tag} width must be inside material3's sizeIn(112dp, 280dp), got {w}"
        );
        assert!(
            h >= 48.0,
            "{tag} height must be at least MenuListItemContainerHeight (48dp), got {h}"
        );
    }

    // One surface around the column, with 8dp above and below: 3 items × ≥48 + 16.
    let (_, _, cw, ch) = app.find_tag_in_overlay("dm-container").expect("the menu container");
    assert!(
        (112.0..=280.0).contains(&cw),
        "the container is as wide as its widest item (no extra padding in M3), got {cw}"
    );
    assert!(
        ch >= 3.0 * 48.0 + 16.0,
        "the container must carry the 8dp vertical padding around three 48dp items (≥160), got {ch}"
    );
    assert!(
        ch < 3.0 * 48.0 + 16.0 + 40.0,
        "and it must not add more than that (rows are 48dp, not the old 36), got {ch}"
    );

    // The label is centred in its 48dp item — material3's `Row(verticalAlignment = CenterVertically)`.
    // Without it the label sits at the item's top and the menu reads as asymmetric padding even though
    // the container's own 8dp is symmetric (the tree showed `pos:[12,0]` in a 48-tall row before).
    let (ix, iy, _, ih) = app.find_tag_in_overlay("dm-item-new").expect("the first item");
    let (tx, ty, _, th) = app
        .overlay_text_rect("新建文件")
        .expect("the first item's label");
    assert!(
        (tx - ix - 12.0).abs() < 0.5,
        "the label keeps the item's 12dp horizontal padding: label x {tx} against item x {ix}"
    );
    let gap_top = ty - iy;
    let gap_bottom = (iy + ih) - (ty + th);
    eprintln!("菜单项: item=({ix},{iy},{ih}) label=({tx},{ty},{th}) 上={gap_top} 下={gap_bottom}");
    assert!(
        (gap_top - gap_bottom).abs() <= 1.0,
        "the label must be vertically centred in the item: {gap_top} above it, {gap_bottom} below"
    );
}

/// What:  an open `DropdownMenu`.
/// When:  a pixel inside the menu's 8dp vertical padding is read, and the same read is taken far from
///        the menu.
/// Then:  the two differ — the container's surface is PAINTED (m3 `surfaceContainer`), not left
///        transparent over the page. Nothing in the layout tree can show this: the surface colour is
///        resolved when the node is built and `bg(...)` prints as `<dynamic>`.
#[test]
fn dropdown_menu_paints_its_surface() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));

    let (cx, cy, _, _) = app.find_tag_in_overlay("dm-container").expect("the menu container");
    // The container's own 8dp vertical padding: no item and no text there, so the pixel is the surface.
    let inside = app
        .pixel_at_logical(cx + 4.0, cy + 4.0)
        .expect("a pixel inside the menu container");
    let page = app
        .pixel_at_logical(360.0, 480.0)
        .expect("a pixel on the page, far from the menu");

    let delta = [inside.0, inside.1, inside.2]
        .iter()
        .zip([page.0, page.1, page.2].iter())
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap_or(0);
    assert!(
        delta >= 6,
        "the menu's surface must paint over the page (got inside={inside:?} page={page:?}, max delta {delta})"
    );
}

/// What:  a menu with more items than the window can show (20 × 48dp against a 520px window).
/// When:  it is opened from a trigger near the top.
/// Then:  material3's two properties hold — the menu stays INSIDE the window, and its remaining items
///        are reachable (the content scrolls) instead of hanging off the bottom edge.
///
/// The bounds are the point: before this round nothing capped or scrolled the menu, so its container
/// was as tall as its content (960+16) and started below the anchor — everything past the window edge
/// was unreachable.
#[test]
fn dropdown_menu_a_long_menu_fits_the_window_and_scrolls_to_its_end() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-many-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-many-toggle").expect("the long-menu trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("长项 0", Duration::from_secs(5));
    app.refresh();

    let (cx, cy, cw, ch) = app
        .find_tag_in_overlay("dm-many-container")
        .expect("the long menu container");
    let (_, wh) = app.frame_size().map(|(w, h)| (w, h)).unwrap_or((0, 0));
    let window_h = wh as f32 / 1.5; // the capture is at the window's scale (1.5 on this machine)
    eprintln!("长菜单: container=({cx},{cy},{cw},{ch}) window_h≈{window_h}");
    // material3's `MenuVerticalMargin` is 48dp and the provider requires a candidate to sit within
    // `[margin, window - margin]` — so a menu that fills the space it was given still floats clear of
    // both window edges instead of bleeding into them.
    let margin = 48.0;
    assert!(
        cy >= margin - 1.0,
        "the menu must keep MenuVerticalMargin (48dp) from the top edge, got y={cy}"
    );
    assert!(
        cy + ch <= window_h - margin + 1.0,
        "and from the bottom edge: bottom {} > window {window_h} - {margin}",
        cy + ch
    );
    assert!(
        ch <= window_h - 2.0 * margin + 1.0,
        "so its height is capped at window - 2*margin, got {ch}"
    );

    // The menu scrolls. The wheel goes over the menu (a wheel is routed by what is under it — an overlay
    // has its own arena — and an injected `s` follows the same precedence), and what is asserted is the
    // scroll OFFSET: scrolling moves the render translation, so the items' reported rects never change.
    assert!(
        app.find_tag_in_overlay("dm-many-19").is_some(),
        "the last item must be composed (it is inside the scroll container)"
    );
    assert_eq!(
        app.overlay_scroll_offset("dm-many-container"),
        Some(0.0),
        "a freshly opened menu is at the top"
    );
    let (item19_y, item19_h) = app
        .find_tag_in_overlay("dm-many-19")
        .map(|(_, y, _, h)| (y, h))
        .expect("the last item");

    app.send(&format!("m {} {}", (cx + cw / 2.0) as i32, (cy + ch / 2.0) as i32));
    std::thread::sleep(Duration::from_millis(150));
    // Known flake, left as it is: the injected wheel events do not all reach the menu — measured
    // offsets of 120 and 240 against the 552 range, i.e. one or two of these twelve landed. Sending
    // forty instead made it WORSE (four runs in five), so the loss is not "not enough events" and
    // the cause is in the debug input path, not here. Reproduced on the tree before the module
    // restructure (two of three runs passed alone), so it is not that either. The assertion below
    // is exact and stays that way: it is about where the scroll STOPS.
    for _ in 0..12 {
        app.scroll_delta(0.0, -120.0);
    }
    std::thread::sleep(Duration::from_millis(200));
    app.refresh();
    let offset = app
        .overlay_scroll_offset("dm-many-container")
        .expect("the menu is still a scroll container");
    // 20 items × 48dp + the 8dp padding above and below, against the 520px it was given.
    let content_h = 20.0 * 48.0 + 16.0;
    let range = content_h - ch;
    eprintln!("长菜单 offset={offset} (range≈{range}, container h={ch})");
    assert!(
        (offset - range).abs() <= 1.0,
        "the wheel must stop exactly at the end: offset {offset} against a range of {range}"
    );
    // …and the content then ends flush with the container's bottom less its own 8dp padding. The clamp
    // used to work from the padding-DEDUCTED viewport instead, which let the content scroll 16px too far:
    // the reported "why is there so much blank at the bottom" (24px under the last item, 8 of them real).
    let rendered_y = item19_y - offset;
    let rendered_bottom = rendered_y + item19_h;
    eprintln!(
        "长项 19: layout y={item19_y} h={item19_h} → rendered y={rendered_y} bottom={rendered_bottom}"
    );
    assert!(
        (rendered_bottom - (cy + ch - 8.0)).abs() <= 1.0,
        "the last item must end at the container's bottom less its 8dp padding: item bottom \
         {rendered_bottom} against {}",
        cy + ch - 8.0
    );
}

/// What:  a menu whose content FITS the space it was given (three items, 160 against 160), and then a
///        menu that does not fit (30 items).
/// When:  the same gesture runs inside each — pointer down, a fast upward drag, release.
/// Then:  the fitted menu does not move at all, and the long one does.
///
/// The fitted case is the reported bug: the drag itself was already clamped by the node's
/// `content - viewport`, so it moved nothing, but the MOMENTUM after the release was clamped against
/// `fling_limit` — and a limit of 0 was read as "unknown, do not clamp above" (`if limit > 0.0 { limit }
/// else { f32::MAX }`), so the content slid out of its container. Measured before the fix: offset 0 -> 74.
/// The long menu is the control: if the gesture stopped being a fling at all, this test would pass
/// vacuously.
#[test]
fn dropdown_menu_does_not_fling_when_the_content_fits() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    app.refresh();
    let (_, cy, _, ch) = app.find_tag_in_overlay("dm-container").expect("the short menu");

    // Fast steps: the harness's `drag` sleeps 20ms per step, which is slow enough that the release may
    // not fling at all — and then this test would pass without exercising anything.
    app.fling_inside(70.0, cy + ch - 20.0, 130.0);
    std::thread::sleep(Duration::from_millis(600));
    app.refresh();
    let short_offset = app.overlay_scroll_offset("dm-container");
    assert_eq!(
        short_offset,
        Some(0.0),
        "a menu whose content fits must not move, fling included (offset {short_offset:?})"
    );

    // Control: same gesture, a menu that really is scrollable.
    app.click(bx + bw / 2.0, by + bh / 2.0); // close the short one
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (lx, ly, lw, lh) = app.find_tag("dm-many-toggle").expect("the long-menu trigger");
    app.click(lx + lw / 2.0, ly + lh / 2.0);
    app.expect_overlay_text_timeout("长项 0", Duration::from_secs(5));
    app.refresh();
    let (_, mcy, _, mch) = app.find_tag_in_overlay("dm-many-container").expect("the long menu");
    app.fling_inside(70.0, mcy + mch - 20.0, 130.0);
    std::thread::sleep(Duration::from_millis(600));
    app.refresh();
    let long_offset = app
        .overlay_scroll_offset("dm-many-container")
        .expect("the long menu is a scroll container");
    assert!(
        long_offset > 0.0,
        "the control gesture must still fling a menu that CAN scroll (offset {long_offset})"
    );
}

/// What:  a `DropdownMenu` opened from its trigger, with the pointer left alone.
/// When:  the frame right after the click is captured once and TWO points are read from it — one near the
///        menu's top-left (covered as soon as the menu is on screen at all) and one in its bottom-right
///        band (covered only once it has grown to its full size).
/// Then:  the inner point already shows the menu's surface and the outer one still shows the page: the
///        menu is animating in, growing from its anchor.
///
/// This is material3's menu transition (`Menu.kt`'s `DropdownMenuContent`): scale `ClosedScaleTarget =
/// 0.8f` → `ExpandedScaleTarget = 1f`, alpha `0f` → `1f`, with `transformOrigin =
/// calculateTransformOrigin(anchorBounds, menuBounds)`. Without it the menu appears at full size in one
/// frame and the outer point shows the surface in the first capture — which fails this test.
#[test]
fn dropdown_menu_animates_in_from_its_anchor() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");

    // The probes need the menu's SETTLED geometry, and waiting for it takes longer than the 200ms
    // animation — so read it from a first open, close again, and only then measure the opening frame.
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    app.refresh();
    let (cx, cy, cw, ch) = app.find_tag_in_overlay("dm-container").expect("the short menu");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));

    // The probes: one at the menu's top EDGE near the pivot (the top edge is the pivot's own y, so it is
    // covered at any scale) and one 6px inside the far corner (only covered once the scale is ~0.94, the
    // last third of the animation).
    let inner = (cx + cw / 2.0, cy + 2.0);
    let outer = (cx + cw - 6.0, cy + ch - 6.0);
    // Read the capture scale BEFORE the measured click: `frame_size` captures and sleeps 120ms of its own.
    let (fw, fh) = app.frame_size().expect("a frame");
    let (sx, sy) = (fw as f32 / 420.0, fh as f32 / 520.0);

    // A raw `c` (not `click`, which sleeps 150ms — three quarters of the animation), then a burst of
    // captures: one frame per sample, ~40ms apart, which is fine-grained enough that the growing frames
    // cannot be missed, and immune to exactly where the first capture lands.
    app.send(&format!("c {} {}", (bx + bw / 2.0) as i32, (by + bh / 2.0) as i32));
    let mut samples = Vec::new();
    for _ in 0..16 {
        let p = app.pixels_at_logical_scaled(&[inner, outer], sx, sy);
        samples.push((p[0], p[1]));
    }
    eprintln!("菜单动画采样: {samples:?}");

    // Settled reference: with the animation over, BOTH points show the surface.
    std::thread::sleep(Duration::from_millis(400));
    let settled = app.pixels_at_logical(&[inner, outer]);
    let (inner_settled, outer_settled) = (settled[0].expect("inner"), settled[1].expect("outer"));
    assert_eq!(
        inner_settled, outer_settled,
        "once settled, both probes must be the menu's own surface: {inner_settled:?} vs {outer_settled:?}"
    );

    // The signature of an enter animation: at least one sampled frame is a PARTIAL blend — neither the page
    // behind the menu nor the menu's own settled surface. Without the animation the menu is at full size
    // and full opacity in its first frame, so every sample is one of those two and nothing in between.
    //
    // The SCALE half of material3's transition (`ClosedScaleTarget = 0.8f`) shares this one ease curve, so
    // by the time the panel is opaque enough to sample, the scale is already ~0.98 and the painted box has
    // all but reached its final size: the growth is real but only measurable with a purpose-built probe.
    // The single shared curve is a recorded deviation (`docs/dropdown-menu.md`).
    let page = samples[0].0.expect("the first sample's pixel");
    let transitional = samples
        .iter()
        .filter_map(|(i, _)| *i)
        .any(|px| px != inner_settled && px != page);
    assert!(
        transitional,
        "some frame must show the menu part-way in — a blend of the page {page:?} and its surface \
         {inner_settled:?}: {samples:?}"
    );
}

/// What:  an open `DropdownMenu`, with the first item's centre read before and during a held press.
/// When:  the pointer goes down on the item and stays there.
/// Then:  the pixel under it changes — the item paints a ripple, material3's
///        `clickable(..., indication = ripple(true))`.
///
/// Read from ONE capture per state: the ripple expands over a few frames, so the press is held for 200ms
/// before the second capture. Without the ripple the press paints nothing (the item has no background of
/// its own) and both reads are the menu's surface.
#[test]
fn dropdown_menu_item_ripples_while_pressed() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    app.refresh();
    let (ix, iy, iw, ih) = app.find_tag_in_overlay("dm-item-new").expect("the first item");
    // Toward the item's trailing edge: inside the item but clear of the label's glyphs, which are
    // anti-aliased and would shrink the measured delta.
    let probe = (ix + iw - 6.0, iy + ih / 2.0);
    let (fw, fh) = app.frame_size().expect("a frame");
    let (sx, sy) = (fw as f32 / 420.0, fh as f32 / 520.0);

    let before = app
        .pixels_at_logical_scaled(&[probe], sx, sy)[0]
        .expect("a pixel before the press");
    // Hold, and poll until the ripple paints. The gesture is re-injected if nothing changes at all: under the
    // full suite's parallel load an injected pointer-down sometimes never reaches the app (measured: the probe
    // stayed at the plain surface for the whole first attempt while the same test passes alone twice), which
    // is a harness artefact and not a menu behaviour. Releasing first keeps each attempt a clean gesture.
    let mut pressed = before;
    for attempt in 0..3 {
        if attempt > 0 {
            app.send(&format!("u {} {}", probe.0 as i32, probe.1 as i32));
            std::thread::sleep(Duration::from_millis(60));
        }
        app.send(&format!("d {} {}", probe.0 as i32, probe.1 as i32));
        let deadline = std::time::Instant::now() + Duration::from_millis(900);
        loop {
            std::thread::sleep(Duration::from_millis(60));
            pressed = app.pixels_at_logical_scaled(&[probe], sx, sy)[0].expect("a pixel while pressed");
            if pressed != before || std::time::Instant::now() > deadline {
                break;
            }
        }
        if pressed != before {
            break;
        }
    }
    app.send(&format!("u {} {}", probe.0 as i32, probe.1 as i32));
    eprintln!("菜单项波纹: 未按下={before:?} 按住中={pressed:?}");

    assert_ne!(
        pressed, before,
        "a held press must paint the item's ripple (probe {probe:?}): {before:?} -> {pressed:?}"
    );
}

// ═══════════════════════════════════════════════════════════════
// 键盘与关闭语义（阶段 4，对齐 material3 的 DefaultMenuProperties）
// ═══════════════════════════════════════════════════════════════

/// Open the fixture's plain menu and leave it up.
fn open_plain_menu(app: &mut UiTest) -> (f32, f32) {
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-toggle").expect("the trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("新建文件", Duration::from_secs(5));
    (bx + bw / 2.0, by + bh / 2.0)
}

/// Press Tab until a tag inside the popup entry has focus, and report whether it ever did.
///
/// The retry is for the HARNESS, not for the menu: under the full suite's parallel load an injected key
/// sometimes never reaches the app at all (`focused: []` with the menu open, while the same test passes
/// alone three times in a row). A second press is only sent when NOTHING is focused — pressing Tab with an
/// item already focused would move on to the next one — so this cannot pass unless Tab really landed on it.
fn tab_into_overlay(app: &mut UiTest, tag: &str) -> bool {
    for attempt in 0..2 {
        app.key("Tab");
        let wait = if attempt == 0 { 900 } else { 4000 };
        if app.wait_until_overlay_focus(tag, Duration::from_millis(wait)) {
            return true;
        }
        if !app.focused_tags().is_empty() {
            return false;
        }
    }
    false
}

/// What:  an open menu, with the keyboard untouched until now.
/// When:  Tab is pressed.
/// Then:  the keyboard belongs to the MENU — an item ends up focused and no page element does.
///        material3: `DefaultMenuProperties = PopupProperties(focusable = true)`
///        (`androidMain/AndroidMenu.android.kt`).
///
/// Before the menu was a focus scope, Tab walked the page behind it (measured: focus landed on
/// `dm-many-toggle`, a button in the main tree, while the menu was open).
#[test]
fn dropdown_menu_takes_the_keyboard_while_open() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_plain_menu(&mut app);
    assert!(
        tab_into_overlay(&mut app, "dm-item-new"),
        "Tab must focus the menu's first focusable item (focused: {:?})",
        app.focused_tags()
    );
    let focused = app.focused_tags();
    assert!(
        focused.iter().all(|t| t.starts_with("dm-item-")),
        "nothing on the page may hold focus while the menu is up: {focused:?}"
    );
}

/// What:  an open menu whose first item was focused with Tab.
/// When:  Enter is pressed.
/// Then:  the item fires and the menu closes — Compose's `clickable` activates on Enter/Space for the
///        focused node, which winia's key dispatcher already does for any focused clickable.
#[test]
fn dropdown_menu_a_focused_item_activates_on_enter() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_plain_menu(&mut app);
    assert!(
        tab_into_overlay(&mut app, "dm-item-new"),
        "Tab focuses the first item (focused: {:?})",
        app.focused_tags()
    );
    app.key("Enter");
    app.expect_text_timeout("dm-picked: new", Duration::from_secs(5));
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
}

/// What:  an open menu.
/// When:  Esc is pressed.
/// Then:  it dismisses through `on_dismiss_request` and nothing is picked — the platform popup's
///        behaviour for a menu, which winia already had (this pins it) and which does not depend on the
///        menu owning the keyboard.
#[test]
fn dropdown_menu_esc_dismisses_it() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_plain_menu(&mut app);
    app.key("Escape");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("dm-picked: none")),
        "Esc must not pick anything: {texts:?}"
    );
}

/// What:  an open menu with focus inside it.
/// When:  Esc closes it.
/// Then:  focus is BACK on the trigger that opened it — the popup restore material3's focusable popup
///        performs, so the keyboard does not stay stranded in a layer that is gone.
#[test]
fn dropdown_menu_returns_focus_to_its_trigger_on_close() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_plain_menu(&mut app);
    assert!(tab_into_overlay(&mut app, "dm-item-new"), "focus is inside the menu");
    app.key("Escape");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    assert!(
        app.wait_until_focus("dm-toggle", Duration::from_secs(2)),
        "focus must return to the trigger (focused: {:?})",
        app.focused_tags()
    );
}

/// What:  the menu's first item, focused with Tab.
/// When:  one capture is read at two points inside the item.
/// Then:  both are tinted — the item marks focus with a state layer (a highlight).
///
/// The other half of this, "no focus RING", is a structural assertion in `overlay.rs`'s
/// `a_menu_item_carries_a_ripple_and_no_focus_ring`, not a pixel one: the ring is a ~1px band just OUTSIDE
/// the item's rect (measured: physical x=23 against the item's edge at 24 is `(197,193,199)`, everything
/// inside is the `(217,211,219)` state layer), which a logical-coordinate probe cannot address reliably —
/// a rounding step lands on either side of it.
#[test]
fn dropdown_menu_highlights_the_focused_item() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_plain_menu(&mut app);
    app.refresh();
    let (ix, iy, iw, ih) = app.find_tag_in_overlay("dm-item-new").expect("the first item");
    let edge = (ix + 3.0, iy + ih / 2.0);
    let middle = (ix + iw - 6.0, iy + ih / 2.0);
    let (fw, fh) = app.frame_size().expect("a frame");
    let (sx, sy) = (fw as f32 / 420.0, fh as f32 / 520.0);

    let before = app.pixels_at_logical_scaled(&[edge, middle], sx, sy);
    assert!(tab_into_overlay(&mut app, "dm-item-new"), "Tab focuses the first item");
    let unfocused = before[1].expect("the item before focus");

    let delta = |a: (u8, u8, u8, u8), b: (u8, u8, u8, u8)| {
        [a.0, a.1, a.2]
            .iter()
            .zip([b.0, b.1, b.2].iter())
            .map(|(x, y)| (*x as i32 - *y as i32).abs())
            .max()
            .unwrap_or(0)
    };
    // The state layer fades in, and a capture can arrive before the key is even processed — so poll until
    // the highlight shows up. Both points come from ONE capture, which is what makes the comparison below
    // a comparison within a single frame.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let (edge_f, middle_f) = loop {
        let px = app.pixels_at_logical_scaled(&[edge, middle], sx, sy);
        let (e, m) = (px[0].expect("edge"), px[1].expect("middle"));
        // Wait for the SETTLED highlight, not merely "something changed": the state layer fades in, and
        // stopping at the first wobble would make the assertion below true by construction. The settled
        // tint measures 242,236,244 -> 217,211,219 (25 units), so 20 leaves room and still means "fully on".
        if delta(m, unfocused) >= 20 || std::time::Instant::now() > deadline {
            break (e, m);
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    eprintln!("菜单项焦点: 未聚焦={before:?} 聚焦后 edge={edge_f:?} middle={middle_f:?}");

    assert!(
        delta(edge_f, middle_f) <= 4,
        "no focus ring: the item's edge must be the same colour as its middle, got {edge_f:?} vs \
         {middle_f:?}"
    );
    assert!(
        delta(middle_f, unfocused) >= 20,
        "focus must HIGHLIGHT the item (the state layer, fully faded in): {unfocused:?} -> {middle_f:?}"
    );
}

/// What:  a menu whose first item has a `leadingIcon` and whose second has a `trailingIcon`.
/// When:  the tree is read.
/// Then:  material3's `DropdownMenuItemContent` geometry holds:
///          leadingIcon   a box at least 24dp wide (`ListItemLeadingIconSize`)
///          label         12dp after that box (and after the item's own 12dp content padding)
///          trailingIcon  the same box mirrored, with 12dp before it
///
/// The label starts at the item's left + 12 + 24 + 12 = +48, which is what makes labels line up when every
/// item carries an icon.
#[test]
fn dropdown_menu_item_icon_geometry_matches_material3() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-open: no", Duration::from_secs(5));
    let (bx, by, bw, bh) = app.find_tag("dm-icons-toggle").expect("the icon-menu trigger");
    app.click(bx + bw / 2.0, by + bh / 2.0);
    app.expect_overlay_text_timeout("带图标", Duration::from_secs(5));
    app.refresh();

    // Leading icon: 24dp box, label 12dp after it.
    let (ix, _, iw, ih) = app.find_tag_in_overlay("dm-item-lead").expect("the leading-icon item");
    let (lx, _, lw, _) = app.find_tag_in_overlay("dm-icon-lead").expect("the leading icon box");
    let (tx, _, _, th) = app.overlay_text_rect("带图标").expect("the leading item's label");
    eprintln!("前导图标: item.x={ix} icon.x={lx} w={lw} label.x={tx} item.w={iw} item.h={ih} label.h={th}");
    assert!(lw >= 24.0, "the leading icon box must be at least 24dp wide, got {lw}");
    assert!(
        (lx - (ix + 12.0)).abs() <= 0.5,
        "the icon box starts after the item's 12dp content padding: {lx} against {}",
        ix + 12.0
    );
    assert!(
        (tx - (lx + lw + 12.0)).abs() <= 0.5,
        "the label follows the icon box by 12dp: {tx} against {}",
        lx + lw + 12.0
    );

    // Trailing icon: the box ends 12dp before the item's content edge.
    let (jx, _, jw, _) = app.find_tag_in_overlay("dm-item-trail").expect("the trailing-icon item");
    let (kx, _, kw, _) = app.find_tag_in_overlay("dm-icon-trail").expect("the trailing icon box");
    let (ux, _, uw, _) = app.overlay_text_rect("尾随").expect("the trailing item's label");
    eprintln!("尾随图标: item.x={jx} label.x={ux} w={uw} icon.x={kx} w={kw} item.w={jw}");
    assert!(kw >= 24.0, "the trailing icon box must be at least 24dp wide, got {kw}");
    // material3 puts the 12dp between the icon and the TEXT BOX, and the text box is the weighted one: the
    // label sits left-aligned inside it, so the gap measured from the LABEL is 12dp plus that slack.
    assert!(
        kx >= ux + uw + 12.0 - 0.5,
        "the trailing icon follows the label by at least 12dp: {kx} against {}",
        ux + uw + 12.0
    );
    assert!(
        (kx + kw - (jx + jw - 12.0)).abs() <= 0.5,
        "and it ends at the item's trailing content edge: {} against {}",
        kx + kw,
        jx + jw - 12.0
    );

    // Every item is the SAME width, and that width is the menu's — material3 gets this from the column's
    // `width(IntrinsicSize.Max)`, and winia's own intrinsic protocol now does the same thing: the column is
    // tightened to its widest item's intrinsic width, and each item's `fillMaxWidth` stretches the row to it.
    // It is not just tidiness: an item narrower than the panel paints its ripple and hover state over part of
    // the row only, which is exactly what a screenshot showed before this (a highlight stopping short of the
    // trailing hint while the panel ran on).
    let (_, _, lead_w, _) = app.find_tag_in_overlay("dm-item-lead").expect("the first item");
    let (_, _, trail_w, _) = app.find_tag_in_overlay("dm-item-trail").expect("the second item");
    let (_, _, menu_w, _) = app.find_tag_in_overlay("dm-icons-container").expect("the menu");
    eprintln!("等宽检查: lead={lead_w} trail={trail_w} menu={menu_w}");
    assert!(
        (lead_w - trail_w).abs() <= 0.5,
        "items share one width: {lead_w} vs {trail_w}"
    );
    assert!(
        (menu_w - lead_w).abs() <= 0.5,
        "and the menu is exactly as wide as its items: {menu_w} vs {lead_w}"
    );
    assert!(
        menu_w < 280.0,
        "the menu takes its widest item's natural width, not the 280dp maximum: {menu_w}"
    );
}

// ═══════════════════════════════════════════════════════════════
// ExposedDropdownMenuBox（阶段 5，对齐 material3 的输入框下拉）
// ═══════════════════════════════════════════════════════════════

/// Click the fixture's read-only exposed-dropdown field, which is 200dp wide.
fn open_exposed_menu(app: &mut UiTest) -> (f32, f32, f32, f32) {
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    let rect = app.find_tag("dm-exposed-anchor").expect("the dropdown's text field");
    app.click(rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
    app.expect_text_timeout("dm-exposed-open: yes", Duration::from_secs(5));
    app.expect_overlay_text_timeout("选项 A", Duration::from_secs(5));
    app.refresh();
    rect
}

/// What:  an `ExposedDropdownMenuBox` whose text field is 200dp wide.
/// When:  the field is clicked.
/// Then:  the menu opens below it, as wide as the FIELD and with items that fill it — material3's
///        `matchAnchorWidth` (`Modifier.exposedDropdownSize`, which forces `minWidth = maxWidth = the
///        anchor's width`) and its 16dp item padding (`ExposedDropdownMenuItemHorizontalPadding`).
#[test]
fn exposed_dropdown_menu_matches_its_anchor_width() {
    let mut app = UiTest::launch("dropdown_menu");
    let (ax, ay, aw, ah) = open_exposed_menu(&mut app);
    // The menu's container is not tagged (the box owns it), but its ITEMS are — and an item is exactly as
    // wide as the menu, so the item's width is what carries the width assertion.
    let (ix, iy, iw, ih) = app
        .find_tag_in_overlay("dm-exposed-item-0")
        .expect("the first item");

    eprintln!("暴露式下拉: anchor=({ax},{ay},{aw},{ah}) item=({ix},{iy},{iw},{ih})");
    assert!(
        (iw - aw).abs() <= 0.5,
        "the menu must be exactly as wide as its field: item width {iw} against the field's {aw}"
    );
    assert!(
        iy >= ay + ah - 0.5,
        "and it hangs below it: item y {iy} against the field's bottom {}",
        ay + ah
    );

    // The items pad 16dp horizontally — not the plain menu's 12dp.
    let (tx, _, _, _) = app.overlay_text_rect("选项 A").expect("the first item's label");
    assert!(
        (tx - (ix + 16.0)).abs() <= 0.5,
        "exposed-dropdown items pad 16dp horizontally: label x {tx} against {}",
        ix + 16.0
    );
}

/// What:  the same box.
/// When:  the menu is open and the user clicks outside it.
/// Then:  it closes and reports that through `onExpandedChange` — the box owns the dismissal, so the
///        caller's state and the menu cannot drift apart.
#[test]
fn exposed_dropdown_dismisses_and_reports_it() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_exposed_menu(&mut app);
    app.click(360.0, 480.0);
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    // The entry lingers while the exit animation plays, so wait for it rather than sampling once.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        app.refresh();
        if app.overlay_count() == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the popup entry must be removed once the menu closes"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What:  a box whose anchor is `PrimaryEditable`, with its menu opened by clicking the field.
/// When:  a letter and the spacebar are typed into the field while the menu is up, then Enter is pressed.
/// Then:  the click opens the menu but leaves the caret in the field — material3's `PrimaryEditable`
///        "will open the menu without focus in order to preserve focus on the soft keyboard (IME)"
///        (`ExposedDropdownMenu.kt:468`); the anchor type decides whether the POPUP takes focus, not
///        whether a click counts, because the pointer path calls `onExpandedChange` on the up event for
///        every type (`:1430-1433`) — the field goes on receiving characters, the spacebar does not
///        toggle the menu ("Primary editable shouldn't expand menu via spacebar", `:1444`), and Enter —
///        which material3 counts as a click — closes it again.
#[test]
fn exposed_dropdown_editable_anchor_opens_without_taking_the_caret() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    let (x, y, w, h) = app.find_tag("dm-editable-anchor").expect("the editable field");

    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_overlay_text_timeout("可编辑项", Duration::from_secs(5));
    assert!(
        app.tag_is_focused("dm-editable-anchor"),
        "an editable anchor's menu opens WITHOUT taking focus, so the field must still hold the caret"
    );

    app.key("a");
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("text(a)")),
        "the field must go on receiving characters while its menu is open: {texts:?}"
    );

    app.key("Space");
    std::thread::sleep(Duration::from_millis(200));
    app.refresh();
    assert_eq!(
        app.overlay_count(),
        1,
        "the spacebar belongs to the text: material3's editable anchor must not toggle on it"
    );

    app.key("Enter");
    std::thread::sleep(Duration::from_millis(250));
    app.refresh();
    assert_eq!(
        app.overlay_count(),
        0,
        "material3 counts Enter as a click on the anchor, so it must close an open menu"
    );
}

/// What:  the `PrimaryNotEditable` box, with its menu opened.
/// When:  Tab is pressed.
/// Then:  the first item is focused: material3's non-editable anchor opens WITH focus
///        (`popupPropertiesForAnchorType`, `ExposedDropdownMenu.kt:354`), so the menu owns the keyboard
///        from its first frame and needs no "reach for the menu" step — the contrast with the editable
///        anchor below.
#[test]
fn exposed_dropdown_non_editable_anchor_opens_with_focus() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    app.click_tag("dm-exposed-anchor");
    app.expect_overlay_text_timeout("选项 A", Duration::from_secs(5));

    app.key("Tab");
    assert!(
        app.overlay_tag_is_focused("dm-exposed-item-0"),
        "a non-editable anchor's menu owns the keyboard from the start, so Tab lands on its first item"
    );
}

/// What:  the `PrimaryEditable` box, with its menu opened (the field keeping the caret).
/// When:  ArrowDown — material3's "reach for the menu" key — is pressed, then Tab.
/// Then:  the menu takes the keyboard: `onPreviewKeyEvent` sets `alwaysFocusable = true` for Tab,
///        ArrowDown and ArrowUp while an editable anchor's menu is expanded
///        (`ExposedDropdownMenu.kt:1449-1457`), so the field stops being the keyboard target and the
///        menu's first item becomes reachable.
#[test]
fn exposed_dropdown_editable_anchor_hands_the_keyboard_over_on_a_reach_key() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    app.click_tag("dm-editable-anchor");
    app.expect_overlay_text_timeout("可编辑项", Duration::from_secs(5));
    assert!(
        app.tag_is_focused("dm-editable-anchor"),
        "the menu opens without focus first: the caret stays in the field"
    );

    app.key("ArrowDown");
    // The hand-over is observed on the NEXT frame: the box recomposes with the popup as a focus scope,
    // and the framework claims the keyboard for it right after the overlays are laid out.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while app.tag_is_focused("dm-editable-anchor") && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        app.refresh();
    }
    assert!(
        !app.tag_is_focused("dm-editable-anchor"),
        "material3's `alwaysFocusable = true` hands the keyboard to the menu, so the field must stop \
         being the keyboard target"
    );

    app.key("Tab");
    assert!(
        app.overlay_tag_is_focused("dm-editable-item-0"),
        "once the keyboard is handed over, Tab must reach the menu's first item"
    );
}

/// The published semantics of the secondary anchor's element: the ONE node in this fixture that declares
/// both a role and an `expanded` state, which is what material3's secondary anchor reports
/// (`ExposedDropdownMenu.kt:1462-1477`: `role = Button` plus the expanded state).
fn secondary_anchor_semantics(snapshot: &serde_json::Value) -> Option<(String, bool)> {
    fn walk(n: &serde_json::Value, out: &mut Option<(String, bool)>) {
        if let Some(arr) = n.as_array() {
            for child in arr {
                walk(child, out);
            }
            return;
        }
        let role = n.get("role").and_then(|v| v.as_str()).map(str::to_string);
        let expanded = n
            .get("state")
            .and_then(|s| s.get("expanded"))
            .and_then(|v| v.as_bool());
        if let (Some(role), Some(expanded)) = (role, expanded) {
            *out = Some((role, expanded));
        }
        for key in ["children", "content", "root", "main", "overlays"] {
            if let Some(child) = n.get(key) {
                walk(child, out);
            }
        }
    }
    let mut out = None;
    walk(snapshot, &mut out);
    out
}

/// What:  a box whose anchor is `SecondaryEditable` — material3's "`menuAnchor` on an element inside the
///        field" shape, here the trailing icon — with nothing focused yet.
/// When:  the icon is clicked once, then a second time.
/// Then:  the first click OPENS the menu and the second closes it: exactly one toggle per click. And the
///        field does not take focus from that click, because the element owns the press target — winia's
///        form of material3's `downEvent.consume()` (`ExposedDropdownMenu.kt:1427-1429`), which exists so
///        the click does not move the caret into the field.
#[test]
fn exposed_dropdown_secondary_anchor_toggles_from_its_icon_without_taking_focus() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    let (fx, fy, fw, fh) = app.find_tag("dm-secondary-anchor").expect("the secondary field");
    let (ix, iy, iw, ih) = app.find_tag("dm-secondary-icon").expect("the anchor icon");
    assert!(
        ix >= fx - 0.5 && ix + iw <= fx + fw + 0.5 && iy >= fy - 0.5 && iy + ih <= fy + fh + 0.5,
        "the anchor element must sit inside the field: icon ({ix},{iy},{iw},{ih}) against field \
         ({fx},{fy},{fw},{fh})"
    );

    app.tap(ix + iw / 2.0, iy + ih / 2.0);
    app.expect_overlay_text_timeout("次级项", Duration::from_secs(5));
    assert_eq!(
        app.overlay_count(),
        1,
        "one click on the anchor element must toggle the menu exactly once"
    );
    assert!(
        !app.tag_is_focused("dm-secondary-anchor"),
        "the element owns the press, so the field must not take focus from a click on the icon"
    );

    // The anchor element reports material3's semantics for a secondary anchor: a button whose expanded
    // state follows the menu.
    let opened = app
        .semantics_until(Duration::from_secs(3), |s| {
            secondary_anchor_semantics(s) == Some(("button".to_string(), true))
        })
        .expect("a semantics snapshot");
    assert_eq!(
        secondary_anchor_semantics(&opened),
        Some(("button".to_string(), true)),
        "the anchor element must publish role button + expanded while the menu is showing"
    );

    app.tap(ix + iw / 2.0, iy + ih / 2.0);
    std::thread::sleep(Duration::from_millis(250));
    app.refresh();
    assert_eq!(
        app.overlay_count(),
        0,
        "a second click on the same element must close the menu"
    );
    let closed = app
        .semantics_until(Duration::from_secs(3), |s| {
            secondary_anchor_semantics(s) == Some(("button".to_string(), false))
        })
        .expect("a semantics snapshot");
    assert_eq!(
        secondary_anchor_semantics(&closed),
        Some(("button".to_string(), false)),
        "…and collapse again when it closes"
    );
}

/// What:  the same `SecondaryEditable` box, with its field focused and holding text.
/// When:  the icon is clicked, then another character is typed.
/// Then:  the menu opens AND the field keeps the keyboard: the caret stays usable while the list is
///        showing, which is the point of material3 opening a secondary anchor's menu the way it does.
#[test]
fn exposed_dropdown_secondary_anchor_keeps_the_caret_in_a_focused_field() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    let (fx, fy, fw, fh) = app.find_tag("dm-secondary-anchor").expect("the secondary field");
    app.tap(fx + 24.0, fy + fh / 2.0);
    app.key("a");
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("text(a)")),
        "the field must take the text before the menu is opened: {texts:?}"
    );

    let (ix, iy, iw, ih) = app.find_tag("dm-secondary-icon").expect("the anchor icon");
    app.tap(ix + iw / 2.0, iy + ih / 2.0);
    app.expect_overlay_text_timeout("次级项", Duration::from_secs(5));
    assert!(
        app.tag_is_focused("dm-secondary-anchor"),
        "an open menu must not take the keyboard away from the field of a secondary anchor"
    );

    app.key("b");
    app.refresh();
    let texts = app.all_texts();
    assert!(
        texts.iter().any(|t| t.contains("text(ab)")),
        "the caret must still be usable while the menu is showing: {texts:?}"
    );
}

/// What:  the exposed-dropdown field, with and without its menu open.
/// When:  a vertical scan through the trailing icon's column is read, plus one point on the field's far
///        side as the background reference.
/// Then:  the icon is painted inside the field, and the scan changes when the menu opens — material3's
///        `TrailingIcon`, an `ArrowDropDown` rotated 180° while expanded.
///
/// A scan rather than a grid, for a measured reason: the glyph is ~10x5 inside a 24dp box, so a coarse grid
/// misses it entirely (a 3x3 grid read only the box's centre, `(73,69,78)` and background elsewhere), and the
/// centre itself is covered in BOTH orientations — the flip only moves the covered rows (y 10..15 closed
/// against 9..14 open).
///
/// The placement half guards a TextField bug fixed on 2026-09-28: the slot was centred in the AVAILABLE
/// height instead of the field's box, which put it 32px below a 56px-tall field (measured slot
/// `(260, 391, 13, 19)` against field `(16, 303, 280, 56)`), and only ever looked right where a scrolling
/// parent handed out an unbounded height.
#[test]
fn exposed_dropdown_trailing_icon_is_inside_the_field_and_rotates() {
    let mut app = UiTest::launch("dropdown_menu");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    app.refresh();
    let (fx, fy, fw, fh) = app.find_tag("dm-exposed-anchor").expect("the field");
    let (ax, ay, aw, ah) = app.find_tag("dm-exposed-arrow").expect("the trailing slot");
    assert!(
        ay >= fy - 0.5 && ay + ah <= fy + fh + 0.5 && ax >= fx - 0.5 && ax + aw <= fx + fw + 0.5,
        "the trailing slot must sit inside the field: slot ({ax},{ay},{aw},{ah}) against field \
         ({fx},{fy},{fw},{fh})"
    );

    let centre_x = ax + aw / 2.0;
    let mut scan = Vec::new();
    for step in 0..=24 {
        scan.push((centre_x, ay + ah * step as f32 / 24.0));
    }
    let background = (fx + 16.0, fy + fh - 6.0);
    let (fw_px, fh_px) = app.frame_size().expect("a frame");
    let (sx, sy) = (fw_px as f32 / 420.0, fh_px as f32 / 520.0);

    let mut probes = scan.clone();
    probes.push(background);
    let closed = app.pixels_at_logical_scaled(&probes, sx, sy);
    let bg = closed[scan.len()].expect("the field's background");
    let painted = closed[..scan.len()]
        .iter()
        .filter(|p| p.is_some_and(|px| px != bg))
        .count();
    eprintln!("暴露式下拉: 背景={bg:?} 槽内着色的扫描点={painted}/25");
    assert!(
        painted > 0,
        "the trailing icon must be painted inside the field ({} points read, all the background {bg:?})",
        scan.len()
    );

    let closed_scan: Vec<_> = closed[..scan.len()].to_vec();
    app.click(fx + fw / 2.0, fy + fh / 2.0);
    app.expect_text_timeout("dm-exposed-open: yes", Duration::from_secs(5));
    // Poll: the field repaints its own frame, which can trail the state change.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let changed = loop {
        let open = app.pixels_at_logical_scaled(&scan, sx, sy);
        let changed = open
            .iter()
            .zip(closed_scan.iter())
            .filter(|(a, b)| a.is_some() && a != b)
            .count();
        if changed > 0 || std::time::Instant::now() > deadline {
            break changed;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    eprintln!("暴露式下拉: 展开后扫描中变化的点={changed}/25");
    assert!(
        changed > 0,
        "the trailing icon must rotate when the menu opens (no point of {} changed)",
        scan.len()
    );
}


/// What:  an exposed dropdown whose caller writes the picked label back into the field.
/// When:  an item is clicked.
/// Then:  the field SHOWS it: the label appears in the main tree and the field's previous text is gone.
///
/// The box cannot do this itself — in material3 too the caller owns the text field's value, and its samples
/// write it from the item's `onClick`. What this pins is the pattern `ExposedDropdownMenuDefaults` is meant
/// to be used with, and the reason the fixture's field starts out showing something else.
#[test]
fn exposed_dropdown_shows_the_picked_item_in_its_field() {
    let mut app = UiTest::launch("dropdown_menu");
    let _ = open_exposed_menu(&mut app);
    assert!(
        app.all_texts().iter().any(|t| t.contains("已选")),
        "the field starts with its own value"
    );

    app.click_overlay_tag("dm-exposed-item-1");
    app.expect_text_timeout("dm-exposed-open: no", Duration::from_secs(5));
    // The label it shows is the taller one, so it cannot be confused with the item's own text in the popup:
    // the popup entry is gone and `all_texts` reads the main tree only.
    let texts = {
        std::thread::sleep(Duration::from_millis(200));
        app.refresh();
        app.all_texts()
    };
    eprintln!("暴露式下拉: 选中后主树文本={:?}", texts.iter().filter(|t| t.contains("选项") || t.contains("已选")).collect::<Vec<_>>());
    assert!(
        texts.iter().any(|t| t.contains("选项 B")),
        "the field must show the picked label: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("已选")),
        "and the value it had before must be gone: {texts:?}"
    );
}

/// What:  a split button laid out by the real renderer.
/// When:  it is composed.
/// Then:  it is a leading button, material3's 2 dp gap, and a trailing button — both the same height,
///        the trailing one keeping its 48 dp minimum.
///
/// The unit tests pin the measure POLICY (trailing measured first, the leading one given the rest);
/// this pins the pair as it comes out of the pipeline that also applies the buttons' own minimums.
#[test]
fn split_button_is_two_buttons_and_a_two_dp_gap() {
    let mut app = UiTest::launch("split_button");
    app.expect_text_timeout("open: no", Duration::from_secs(5));
    let (lx, ly, lw, lh) = app.find_tag("sb-leading").expect("the leading button");
    let (tx, ty, tw, th) = app.find_tag("sb-trailing").expect("the trailing button");
    eprintln!("split button: leading ({lx},{ly},{lw},{lh}) trailing ({tx},{ty},{tw},{th})");
    assert!(
        (tx - (lx + lw + 2.0)).abs() < 0.6,
        "the gap is material3's 2 dp: the leading button ends at {} and the trailing starts at {tx}",
        lx + lw
    );
    assert!(
        (lh - th).abs() < 0.6,
        "both buttons share one height (leading {lh}, trailing {th})"
    );
    assert!((lh - 40.0).abs() < 0.6, "the default size tier is 40 dp tall (got {lh})");
    assert!(tw >= 48.0, "the trailing button keeps its 48 dp minimum (got {tw})");
    assert!(
        ty >= ly - 0.6 && ty + th <= ly + lh + 0.6,
        "the trailing button is centred against the leading one ({ty}..{} vs {ly}..{})",
        ty + th,
        ly + lh
    );
}

/// What:  the trailing button's content in an asymmetric shape.
/// When:  it is laid out at rest.
/// Then:  it sits slightly toward the GAP — material3's optical centring, the rule the M3 spec prints
///        as "menu icon offset when unselected: S -1dp".
///
/// The number is `CenterOpticallyCoefficient * (outer - inner)` = 0.11 * (20 - 4) for the default size,
/// computed from the radii the button actually draws (`HorizontalCenterOptically.kt:61`).
#[test]
fn split_button_nudges_its_menu_icon_toward_the_gap() {
    let mut app = UiTest::launch("split_button");
    let (tx, _, tw, _) = app.find_tag("sb-trailing").expect("the trailing button");
    let (ix, _, iw, _) = app.find_tag("sb-trailing-icon").expect("the menu icon");
    let shift = (tx + tw / 2.0) - (ix + iw / 2.0);
    let expected = 0.11 * (20.0 - 4.0);
    eprintln!("split button: icon shift={shift} expected≈{expected}");
    assert!(
        shift > 0.0,
        "the icon moves toward the gap, not away from it (shift {shift})"
    );
    assert!(
        (shift - expected).abs() < 1.0,
        "the shift should be the optical correction {expected} (got {shift})"
    );
}

/// What:  the two halves of a split button.
/// When:  each is clicked.
/// Then:  each runs its OWN action: the leading button counts, the trailing one owns the menu state
///        (the checked form toggles it with no callback), and neither fires the other's.
#[test]
fn split_button_buttons_run_their_own_actions() {
    let mut app = UiTest::launch("split_button");
    app.expect_text_timeout("clicks: 0", Duration::from_secs(5));

    let (lx, ly, lw, lh) = app.find_tag("sb-leading").expect("the leading button");
    app.click(lx + lw / 2.0, ly + lh / 2.0);
    app.expect_text_timeout("clicks: 1", Duration::from_secs(5));

    let (tx, ty, tw, th) = app.find_tag("sb-trailing").expect("the trailing button");
    app.click(tx + tw / 2.0, ty + th / 2.0);
    app.expect_text_timeout("open: yes", Duration::from_secs(5));
    app.click(tx + tw / 2.0, ty + th / 2.0);
    app.expect_text_timeout("open: no", Duration::from_secs(5));

    // The leading action ran exactly once: the trailing button must not have fired it too.
    assert!(
        app.all_texts().iter().any(|t| t.contains("clicks: 1")),
        "one leading click is one action: {:?}",
        app.all_texts()
    );
}

/// The (left, right) insets of the painted shape's top row, in logical pixels: for a rounded rectangle
/// that inset IS that corner's radius.
///
/// The background is whatever the theme paints just above the button rather than an absolute colour:
/// the fixture follows the OS theme, so a fixed threshold would call a dark surface "painted".
fn painted_row_insets(app: &mut UiTest, x: f32, y: f32, w: f32) -> (i32, i32) {
    let background = app.pixel_at_logical(x, y - 6.0).expect("a background pixel");
    let points: Vec<(f32, f32)> = (0..w.round() as i32).map(|i| (x + i as f32, y + 1.0)).collect();
    let pixels = app.pixels_at_logical(&points);
    let painted: Vec<bool> = pixels
        .iter()
        .map(|p| {
            p.is_some_and(|(r, g, b, _)| {
                let d = |a: u8, c: u8| (a as i32 - c as i32).abs();
                d(r, background.0) + d(g, background.1) + d(b, background.2) > 40
            })
        })
        .collect();
    let first = painted.iter().position(|on| *on).expect("a painted button");
    let last = painted.iter().rposition(|on| *on).expect("a painted button");
    (first as i32, (w.round() as i32 - 1) - last as i32)
}

/// What:  the corner radii the two halves actually paint.
/// When:  the pair is laid out at rest.
/// Then:  they are material3's and they MIRROR: the outer (far) corner is `CornerFull`, i.e. half the
///        button's height, and the inner (gap-side) corner is the size tier's `InnerCornerSize` — the
///        leading button rounds its left corners fully and its right ones by the token, the trailing
///        button the other way round.
///
/// material3 builds them as `RoundedCornerShape(OuterCornerSize, endCornerSize, endCornerSize,
/// OuterCornerSize)` and `RoundedCornerShape(startCornerSize, OuterCornerSize, OuterCornerSize,
/// startCornerSize)` (`SplitButton.kt:424-467`), so the two halves are the same two radii swapped.
///
/// The inset of the top row is the corner radius, measured with the bias antialiasing gives a raster: a
/// coverage threshold cuts the arc a few pixels early, so a true 20 dp corner reads ~13 and a true 4 dp
/// corner ~1 here. The mirror is exact, the magnitudes tolerate that bias — which is what makes this a
/// guard on the SHAPE, not on the rasterizer.
#[test]
fn split_button_paints_the_token_corners_mirrored() {
    let mut app = UiTest::launch("split_button");
    app.expect_text_timeout("open: no", Duration::from_secs(5));
    let (lx, ly, lw, _) = app.find_tag("sb-leading").expect("the leading button");
    let (tx, ty, tw, _) = app.find_tag("sb-trailing").expect("the trailing button");

    let (leading_left, leading_right) = painted_row_insets(&mut app, lx, ly, lw);
    let (trailing_left, trailing_right) = painted_row_insets(&mut app, tx, ty, tw);
    eprintln!(
        "split button painted corners: leading (left {leading_left}, right {leading_right}) \
         trailing (left {trailing_left}, right {trailing_right})"
    );

    assert!(
        leading_left > leading_right + 4 && trailing_right > trailing_left + 4,
        "the outer corners are the round ones: leading ({leading_left}, {leading_right}), \
         trailing ({trailing_left}, {trailing_right})"
    );
    assert!(
        (leading_left - trailing_right).abs() <= 2 && (leading_right - trailing_left).abs() <= 2,
        "the halves mirror each other: leading ({leading_left}, {leading_right}) against \
         trailing ({trailing_left}, {trailing_right})"
    );
    assert!(
        (10..=21).contains(&leading_left),
        "the outer corner is CornerFull (half of 40 dp, so 20) within the raster's bias, got {leading_left}"
    );
    assert!(
        (0..=5).contains(&leading_right),
        "the inner corner is the Small tier's 4 dp within the raster's bias, got {leading_right}"
    );
}


/// What:  the trailing half's content moves to its centred position when the menu opens.
/// When:  the trailing button has been tapped and the morph has settled.
/// Then:  the icon has slid toward the centre of the button: material3 centres the content of an
///        asymmetric shape optically, and an open menu turns that half into a symmetric stadium — the
///        per-size "menu icon offset when unselected" against "the icon becomes centered when
///        selected" (`SplitButton.kt:807-813`).
///
/// Read from the debug tree, not from pixels: the slide is 1.76 dp of a rounded glyph, and the tree
/// reads the layout the offset produced. The offset is the icon box's centre against the button's, so
/// rounding of either rect cannot fake it.
#[test]
fn split_button_centres_its_trailing_icon_when_its_menu_opens() {
    let mut app = UiTest::launch("split_button");
    app.expect_text_timeout("open: no", Duration::from_secs(5));
    let offset = |app: &mut UiTest| {
        app.tree();
        let button = app.find_tag("sb-trailing").expect("the trailing button");
        let icon = app.find_tag("sb-trailing-icon").expect("the trailing icon");
        icon.0 - (button.0 + (button.2 - icon.2) / 2.0)
    };
    // Wait for a reading to settle instead of sleeping a fixed 400 ms: the morph is a 180 ms tween, and a
    // duration picked to be "long enough" is slow and flaky at once. The floor below only gives the
    // animation a chance to start; the criterion is the reading holding still.
    let settled = |app: &mut UiTest, read: &dyn Fn(&mut UiTest) -> f32| -> f32 {
        std::thread::sleep(Duration::from_millis(250));
        let mut last = read(app);
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(16));
            let next = read(app);
            if (next - last).abs() < 0.01 {
                return next;
            }
            last = next;
        }
        last
    };
    let unselected = settled(&mut app, &offset);

    app.click_tag("sb-trailing");
    app.expect_text_timeout("open: yes", Duration::from_secs(5));
    let open = settled(&mut app, &offset);
    // A pointer move is what forces a frame that re-runs the layout, so an offset the animation had
    // already reached gets applied there and then (the reported "the icon only moves when the mouse
    // moves over it"). Measuring both tells apart "the animation never ran" from "the layout never
    // took the animated value".
    let (bx, by, bw, bh) = app.find_tag("sb-trailing").expect("the trailing button");
    app.send(&format!("m {} {}", (bx + bw / 2.0) as i32, (by + bh / 2.0) as i32));
    let after_move = settled(&mut app, &offset);
    eprintln!(
        "trailing icon offset: unselected {unselected} open {open} after a pointer move {after_move}"
    );

    assert!(
        unselected < -1.0,
        "an unselected trailing half offsets its icon toward the gap, measured {unselected}"
    );
    assert!(
        open.abs() <= 1.0,
        "and an open menu centres it, measured {open}"
    );
    assert!(
        after_move.abs() <= 1.0,
        "and a pointer move does not move it again, measured {after_move}"
    );
}

/// The grid lattice the date picker's own geometry defines: `horizontal_padding` (12) in from the container,
/// seven columns of the 48 dp accessibility size, and the rows below the header (120), its divider (1), the
/// month navigation (56) and the weekday row (48).
fn date_picker_cell_centre(x: f32, y: f32, column: f32, row: f32) -> (f32, f32) {
    (
        x + 12.0 + column * 48.0 + 24.0,
        y + 120.0 + 1.0 + 56.0 + 48.0 + row * 48.0 + 24.0,
    )
}

/// What:  the docked date picker's container.
/// When:  it is composed with a title.
/// Then:  it keeps material3's 360 dp minimum width, and its height is the header's 120 plus its divider, the
///        month navigation's 56, the weekday row's 48 and the month grid's 288 — the tokens, summed.
#[test]
fn date_picker_keeps_its_container_and_rows_the_token_sizes() {
    let mut app = UiTest::launch("date_picker");
    app.expect_text_timeout("month: September 2024", Duration::from_secs(5));
    let (x, y, w, h) = app.find_tag("dp-picker").expect("the picker container");
    eprintln!("date picker: container ({x},{y},{w},{h})");
    assert!(
        w >= 360.0,
        "the container keeps the 360 dp minimum width (got {w})"
    );
    // The rows sum to 120 + 1 + 56 + 48 + 288 = 513, material3's content height. winia's container then takes
    // the height its parent offers (measured 606 in this 700 dp window) — a documented deviation; the rows
    // themselves are measured by the pixel tests below.
    // Measured 512 where the tokens sum to 513 — a dp either way, so the guard allows rasterisation rather
    // than pretending the sum is exact.
    let rows = 120.0 + 1.0 + 56.0 + 48.0 + 288.0;
    assert!(
        (h - rows).abs() <= 2.0,
        "the container is the sum of its rows ({rows}, got {h})"
    );
}

/// What:  a day cell and today.
/// When:  the docked picker is drawn.
/// Then:  a day is a 40 dp circle — the selected one filled, today a 1 dp ring with the container showing
///        through it — measured on the grid's own 48 dp lattice.
///
/// The scan line runs through the circle's centre, so its chord IS the 40 dp width, and today's ring is the two
/// painted ends of that chord with nothing painted between them.
#[test]
fn date_picker_paints_a_forty_dp_day_with_a_one_dp_ring_around_today() {
    let mut app = UiTest::launch("date_picker");
    app.expect_text_timeout("month: September 2024", Duration::from_secs(5));
    let (x, y, _, _) = app.find_tag("dp-picker").expect("the picker container");

    // The selection: the 10th, second row, third column (2024-09-01 is a Sunday).
    let (sx, sy) = date_picker_cell_centre(x, y, 2.0, 1.0);
    let scan: Vec<(f32, f32)> = (-26..=26).map(|i| (sx + i as f32, sy)).collect();
    let pixels = app.pixels_at_logical(&scan);
    let background = pixels[0].expect("a container pixel beside the circle");
    let painted: Vec<bool> = pixels.iter().map(|p| p.is_some_and(|c| c != background)).collect();
    let first = painted.iter().position(|on| *on).expect("the selection's left edge");
    let last = painted.iter().rposition(|on| *on).expect("the selection's right edge");
    let width = (last - first) as f32;
    eprintln!(
        "date picker: the selected day spans {width} dp with {} painted pixels",
        painted.iter().filter(|on| **on).count()
    );
    assert!(
        (width - 40.0).abs() <= 2.0,
        "the selected day is 40 dp across (measured {width})"
    );
    assert!(
        painted[first..=last].iter().all(|on| *on),
        "and it is filled rather than a ring"
    );

    // Today: the 5th, first row, fifth column. Its circle is an outline, so the centre row measures the ring's
    // 40 dp while a row 12 dp above the centre — clear of the day number — meets only the two edges, 32 dp
    // apart, which is the chord of a 20 dp radius 12 dp off the middle.
    let (tx, ty) = date_picker_cell_centre(x, y, 4.0, 0.0);
    let scan: Vec<(f32, f32)> = (-26..=26).map(|i| (tx + i as f32, ty)).collect();
    let pixels = app.pixels_at_logical(&scan);
    let background = pixels[0].expect("a container pixel beside the ring");
    let painted: Vec<bool> = pixels.iter().map(|p| p.is_some_and(|c| c != background)).collect();
    let first = painted.iter().position(|on| *on).expect("today's ring's left edge");
    let last = painted.iter().rposition(|on| *on).expect("today's ring's right edge");
    let width = (last - first) as f32;
    eprintln!("date picker: today's ring spans {width} dp");
    assert!(
        (width - 40.0).abs() <= 2.0,
        "today's ring is 40 dp across (measured {width})"
    );

    // A ring, measured where it must be rather than by counting antialiased pixels: 12 dp above the centre,
    // the circle's 20 dp radius is 16 dp out on either side, and the middle of that row is inside it.
    let check = app.pixels_at_logical(&[
        (tx - 16.0, ty - 12.0),
        (tx, ty - 12.0),
        (tx + 16.0, ty - 12.0),
    ]);
    eprintln!("date picker: across the ring {check:?}, background {background:?}");
    assert_eq!(
        check[1],
        Some(background),
        "the ring is hollow: its middle shows the container"
    );
    assert!(
        check[0].is_some_and(|c| c != background),
        "the ring paints its left edge"
    );
    assert!(
        check[2].is_some_and(|c| c != background),
        "the ring paints its right edge"
    );
}

/// What:  the month navigation's arrows.
/// When:  each is tapped.
/// Then:  the calendar steps one month either way, so the header reads August, then September again.
#[test]
fn date_picker_steps_the_month_with_its_arrows() {
    let mut app = UiTest::launch("date_picker");
    app.expect_text_timeout("month: September 2024", Duration::from_secs(5));
    let (x, y, w, _) = app.find_tag("dp-picker").expect("the picker container");
    let nav_y = y + 120.0 + 1.0 + 28.0;

    // The year menu button takes the row's start, so the arrows sit together at its end: two 48 dp icon buttons
    // in the last 96 dp of the 336 dp content row, which puts their centres at 276 and 324 from the container's
    // left edge (measured from the chevrons' ink: one glyph at x = 283, the other at 333-335).
    app.tap(x + 276.0, nav_y);
    app.expect_text_timeout("month: August 2024", Duration::from_secs(5));
    app.tap(x + 324.0, nav_y);
    app.expect_text_timeout("month: September 2024", Duration::from_secs(5));
}

/// What:  the year menu button in the month navigation row.
/// When:  it is tapped, and then a year in the panel is tapped.
/// Then:  the year panel stands in for the calendar at the same height, and picking a year keeps the month.
///
/// The geometry this reads: the panel is `RecommendedSizeForAccessibility * (MaxCalendarRows + 1)` less its
/// divider — the weekday row plus the month grid — so the container must not move when it opens; a year cell is
/// 72 × 36 (`SelectionYearContainerWidth`/`Height`).
#[test]
fn date_picker_opens_its_year_panel_and_picks_a_year() {
    let mut app = UiTest::launch("date_picker");
    app.expect_text_timeout("month: September 2024", Duration::from_secs(5));
    let (x, y, w, h) = app.find_tag("dp-picker").expect("the picker container");

    let (mx, my, mw, mh) = app.find_tag("dp-year-menu").expect("the year menu button");
    app.tap(mx + mw / 2.0, my + mh / 2.0);
    app.refresh();

    let after = app.find_tag("dp-picker").expect("the picker container");
    assert_eq!(
        after,
        (x, y, w, h),
        "the year panel is as tall as the calendar it stands in for"
    );
    let (_yx, _yy, yw, yh) = app.find_tag("dp-year-2024").expect("the displayed year's cell");
    assert_eq!((yw, yh), (72.0, 36.0), "a year cell is 72 x 36");
    // …and the calendar it covers is still composed, which is material3's structure: a `Box` whose first
    // child is the weekdays plus the grid, with the panel expanding over it (`DatePicker.kt:1596-1617`).
    // winia used to swap the calendar out, so this is the assertion that pins the overlay.
    assert!(
        app.find_tag("dp-calendar-panel").is_some(),
        "the calendar stays composed under the year panel"
    );

    // A test tag inside a scrolled list reports the item's position in the list's CONTENT coordinates, so a
    // year cell cannot be tapped through the rectangle `find_tag` returns — it comes back 2079 dp (this list's
    // scroll offset) below where the cell is drawn. The horizontal position does come through, because the list
    // does not scroll sideways. The row is computed the way the panel seeds it (`year_panel_first_row`): the
    // panel opens on the row above the displayed year, which for September 2024 in the default 1900..2100 range
    // is row 40, so 2024 and 2025 are in the row 36 + 16 dp below the panel's top.
    let (nx, _, nw, _) = app.find_tag("dp-year-2025").expect("the next year's cell");
    let cell_y = y + 120.0 + 1.0 + 56.0 + 52.0 + 18.0;
    app.tap(nx + nw / 2.0, cell_y);
    app.expect_text_timeout("month: September 2025", Duration::from_secs(5));
    app.expect_text_timeout("selected: Sep 10, 2024", Duration::from_secs(5));
    // The panel leaves with an exit transition (material3's `shrinkVertically + fadeOut`), so it is still
    // composed for a moment after the pick — wait for it rather than reading the tree once, which is what the
    // instant swap this replaced allowed.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && app.find_tag("dp-year-2025").is_some() {
        app.refresh();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        app.find_tag("dp-year-2025").is_none(),
        "picking a year closes the panel"
    );
}

/// What:  an `AnimatedSize` whose content takes its new size long before the container reaches it.
/// When:  the child grows 60 → 400 dp under a 6 s spec, and a pixel just outside the container is read
///        mid-animation.
/// Then:  nothing of the child is drawn outside the container: Compose's `animateContentSize` starts
///        with `clipToBounds()` (`AnimationModifier.kt:77`) and winia's container used to draw the
///        overflowing child.
///
/// Measured before the clip was added: with the container at 142 dp the pixel 91 dp past its right edge
/// came back `255 0 0` — the child's own colour — against a black page. The two points are read from
/// ONE frame, because the container is moving: a second capture would be a different width.
#[test]
fn animated_size_clips_the_content_it_outgrows() {
    let mut app = UiTest::launch("animated_size_overflow");
    app.expect_text("grow");
    app.click_tag("grow");

    // Let the container start moving: the child is already 400 dp wide.
    let mut outside = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.refresh();
        let Some((bx, by, bw, bh)) = app.find_tag("animated-box") else { continue };
        if bw < 100.0 {
            // Still near its resting width — the window where the child is far wider than the box.
            outside = Some((bx + bw + 40.0, by + bh / 2.0));
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let (ox, oy) = outside.expect("the container is still animating and narrower than the child");

    let (bx, by, bw, bh) = app.find_tag("animated-box").expect("the animated container");
    let inside = (bx + bw / 2.0, by + bh / 2.0);
    let px = app.pixels_at_logical(&[inside, (ox, oy)]);
    assert_eq!(px[0], Some((255, 0, 0, 255)), "inside the container the child is visible");
    assert_eq!(
        px[1],
        Some((0, 0, 0, 255)),
        "outside it the page shows through — the child is clipped to the container"
    );
}

/// What:  the DOCKED picker's panel switch (calendar ⇄ year).
/// When:  the year menu is clicked.
/// Then:  the two panels overlap — the frame the year panel first shows up, the calendar panel is still
///        composed, and only once the switch settles does the calendar panel go away.
///
/// Why this and not just "the year panel appears": the panels are two generations of the docked `Crossfade`,
/// and the crossfade is the point. It used to fade the outgoing panel out to nothing, swap, then fade the
/// incoming one in, which can never show both; the default is now Compose's `tween()` — one 300 ms cross-fade
/// — and this is the only coverage that 300 ms has. It is also the assertion that would catch a return to
/// the serial fade, which no geometry check can see.
#[test]
fn date_picker_panel_switch_cross_fades_both_panels() {
    let mut app = UiTest::launch("date_picker_docked");
    assert!(
        app.find_tag("dp-calendar-panel").is_some(),
        "the docked picker opens on the calendar panel"
    );
    assert!(
        app.find_tag("dp-year-2024").is_none(),
        "…and the year panel is not composed yet"
    );

    app.click_tag("dp-year-menu");

    // Poll instead of sampling once: the click lands ~120 ms in and the tween is 300 ms, so under load a
    // single refresh can arrive after it ended — but the frame the incoming panel first appears is by
    // definition inside the window, and that is where the outgoing one has to still be there.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut overlaps = false;
    while Instant::now() < deadline {
        app.refresh();
        if app.find_tag("dp-year-2024").is_some() {
            overlaps = app.find_tag("dp-calendar-panel").is_some();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        overlaps,
        "both panels are composed while the switch cross-fades — a serial fade can only ever show one"
    );

    // Settled: the outgoing generation is torn down and the year panel is what remains.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && app.find_tag("dp-calendar-panel").is_some() {
        app.refresh();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        app.find_tag("dp-calendar-panel").is_none(),
        "the outgoing panel is removed once the cross-fade ends"
    );
    assert!(app.find_tag("dp-year-2024").is_some(), "and the year panel remains");
}

/// What:  the modal date picker's dialog.
/// When:  it is up, and then a day and the dismiss button are tapped.
/// Then:  the container is the token size, the tap reaches the state the page reads, and the dialog closes and
///        re-opens.
///
/// The geometry this reads: `DatePickerModalTokens.ContainerWidth` (360) and `ContainerHeight` (568), the
/// latter capping a container whose content is the docked picker plus the action row.
#[test]
fn date_picker_dialog_is_the_modal_picker() {
    let mut app = UiTest::launch("date_picker_dialog");
    app.expect_text_timeout("selected: Sep 10, 2024", Duration::from_secs(5));
    wait_for_one_overlay(&mut app);

    let (x, y, w, h) = app.find_tag_in_overlay("dpd-dialog").expect("the dialog");
    assert_eq!(w, 360.0, "ContainerWidth is 360 dp");
    // The dialog is its content, not the cap: the picker's 120 dp header + 56 dp month navigation +
    // 48 dp weekday row + 288 dp month, then the action row's 40 dp button under an 8 dp inset.
    // `ContainerHeight` (568) is a MAXIMUM (`DatePickerDialog.android.kt:84 heightIn(max = ...)`,
    // with `:95`'s `weight(1f, fill = false)` box keeping the dialog free to be shorter), so the
    // content decides and it lands 8 dp short of the cap.
    assert_eq!(
        h, 560.0,
        "the modal picker is as tall as its own content (120 + 56 + 48 + 288 + 48)"
    );

    // The same lattice as the docked picker: today is the first row's fifth column, the selection the second
    // row's third (2024-09-01 is a Sunday).
    let (cx, cy) = date_picker_cell_centre(x, y, 4.0, 0.0);
    app.tap(cx, cy);
    app.expect_text_timeout("selected: Sep 5, 2024", Duration::from_secs(5));

    let (bx, by, bw, bh) = app.find_tag_in_overlay("dpd-cancel").expect("the dismiss button");
    app.tap(bx + bw / 2.0, by + bh / 2.0);
    app.expect_text_timeout("open: no", Duration::from_secs(5));
    // The overlay stays registered while the dialog's exit motion plays, so the count is polled rather than
    // read once.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        if app.overlay_count() == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the dismiss button closes the dialog"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let (ox, oy, ow, oh) = app.find_tag("dpd-open").expect("the page's open button");
    app.tap(ox + ow / 2.0, oy + oh / 2.0);
    app.expect_text_timeout("open: yes", Duration::from_secs(5));
}

/// What:  a day cell.
/// When:  it is tapped.
/// Then:  the picker selects that day.
///
/// This is the guard for a frame-dropping regression, and it has to be a UI test rather than a unit test: the
/// day cell's handler ran and the state changed all along, but `DatePicker::build` was a plain `fn` instead of
/// `#[composable]`, so its nodes had no stable key base. Every month has a different number of day cells, so
/// the node count inside the grid changed, the next compose handed a node a `slot_key` another node already
/// held, winia's `[dup-key]` guard panicked and every later frame was skipped with the previous picture kept.
/// The taps looked dead; the picture was frozen. Removing `#[composable]` again makes this test time out on an
/// unchanged `selected:` text while the state underneath has already moved on.
#[test]
fn date_picker_selects_the_day_that_is_tapped() {
    let mut app = UiTest::launch("date_picker");
    app.expect_text_timeout("selected: Sep 10, 2024", Duration::from_secs(5));
    let (x, y, _, _) = app.find_tag("dp-picker").expect("the picker container");

    // The 5th is today, first row, fifth column (2024-09-01 is a Sunday).
    let (cx, cy) = date_picker_cell_centre(x, y, 4.0, 0.0);
    app.tap(cx, cy);
    app.expect_text_timeout("selected: Sep 5, 2024", Duration::from_secs(5));
}

// ═══════════════════════════════════════════════════════════════
// fixture_date_picker_input：真实窗口 Input 模式交互
// ═══════════════════════════════════════════════════════════════

/// The header toggle swaps the picker between the calendar and the text field, and the state says
/// which one it is on.
///
/// This is the guard on the defect the mode existed but nothing reached: `set_display_mode` wrote a
/// value the build path never read, so the picker stayed a calendar and the mode was a silent
/// no-op. Both directions are checked because a picker that ignored the state and always drew a
/// calendar would pass the calendar half alone.
#[test]
fn the_date_picker_mode_toggle_swaps_the_calendar_for_the_entry_field() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));
    wait_for_one_overlay(&mut app);
    // The calendar's own month navigation is gone: the field replaced it.
    assert!(
        !app.overlay_texts().iter().any(|text| text == "2024"),
        "input mode still composes the month navigation: {:?}",
        app.overlay_texts()
    );

    let toggle = app
        .find_tag_in_overlay("date-picker-mode-toggle")
        .expect("the mode toggle");
    app.tap(toggle.0 + toggle.2 / 2.0, toggle.1 + toggle.3 / 2.0);
    app.expect_text_timeout("mode: picker", Duration::from_secs(5));
    app.expect_overlay_text_timeout("2024", Duration::from_secs(5));

    // The toggle is looked up again rather than reused: the dialog is only as tall as the mode it
    // shows, so switching re-centres every node inside it and the coordinates read a moment ago now
    // point into the calendar (measured: reusing them selected the 7th and left the picker open).
    let toggle = app
        .find_tag_in_overlay("date-picker-mode-toggle")
        .expect("the mode toggle after the switch");
    app.tap(toggle.0 + toggle.2 / 2.0, toggle.1 + toggle.3 / 2.0);
    app.expect_text_timeout("mode: input", Duration::from_secs(5));
    assert!(
        app.find_tag_in_overlay("date-picker-input-field").is_some(),
        "the field is back after the second toggle"
    );
}

/// Wait until the fixture has registered exactly one overlay entry.
///
/// An overlay is registered by its own composer, so it can land a frame after the page's own text:
/// reading `overlay_count()` once right after a launch races that frame — measured under a
/// full-suite run as `left: 0, right: 1` at the modal picker's own assertion. Polling with a
/// deadline keeps a genuinely missing overlay a failure, just a slower one.
fn wait_for_one_overlay(app: &mut UiTest) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        if app.overlay_count() == 1 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected exactly one overlay entry, got {}",
            app.overlay_count()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The rect of a tagged overlay node, read only once two consecutive refreshes agree on it.
///
/// The page's own text is not a synchronisation point for the overlay: the readout updates from the
/// state while the overlay's tree is published by its own layout a frame later, so a single read
/// right after a mode switch can still see the previous frame's dialog (measured: this test failed
/// under a full-suite run and passed alone). Waiting for the value to STOP changing keeps the exact
/// assertion below meaningful — a poll that waited for the expected value would pass on any
/// transient that happened to end up there.
fn settled_overlay_rect(app: &mut UiTest, tag: &str) -> (f32, f32, f32, f32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut previous = None;
    loop {
        app.refresh();
        let rect = app.find_tag_in_overlay(tag);
        if rect.is_some() && rect == previous {
            return rect.expect("checked above");
        }
        previous = rect;
        assert!(
            std::time::Instant::now() < deadline,
            "the overlay's `{tag}` never settled"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Click a tagged node inside an overlay entry. [`UiTest::click_tag`] only looks in the main tree,
/// and a modal picker's own tree lives in the overlay.
fn click_overlay_tag(app: &mut UiTest, tag: &str) {
    app.refresh();
    let (x, y, w, h) = app
        .find_tag_in_overlay(tag)
        .unwrap_or_else(|| panic!("no overlay tag `{tag}`"));
    app.click(x + w / 2.0, y + h / 2.0);
    std::thread::sleep(Duration::from_millis(120));
}

/// Digits typed into the field become the selection, and the delimiters the field draws never reach
/// what it holds — the field is given the eight digits `03122024`, shows `03/12/2024`, and the
/// selection is March 12, 2024. The entry the field opened with has to be cleared first, because
/// the field refuses anything past a full entry's width rather than growing.
#[test]
fn typing_digits_into_the_date_field_selects_that_date() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));

    // The field opens on the initial selection, already written out as digits.
    app.expect_overlay_text_timeout("09/10/2024", Duration::from_secs(5));

    click_overlay_tag(&mut app, "date-picker-input-field");
    for _ in 0..8 {
        app.key("Backspace");
    }
    app.expect_text_timeout("selected: none", Duration::from_secs(5));

    for key in ["0", "3", "1", "2", "2", "0", "2", "4"] {
        app.key(key);
    }
    app.expect_text_timeout("selected: Mar 12, 2024", Duration::from_secs(5));
    // Ten characters shown, eight held: the two delimiters are the visual transformation's work.
    app.expect_overlay_text_timeout("03/12/2024", Duration::from_secs(5));
}

/// Wait for some node to publish this error message.
///
/// The walk is recursive because a clickable container — a dialog, whose scrim dismisses it —
/// claims its whole subtree as children rather than sitting above it (`semantics.rs:645`), and a
/// modal picker is exactly that. The answer is `{"main":[…],"overlays":[…]}` and the picker's tree
/// is an overlay, so both sides are walked.
fn expect_semantics_error(app: &mut UiTest, expected: &str) {
    fn carries(items: &[serde_json::Value], expected: &str) -> bool {
        items.iter().any(|node| {
            node.get("state")
                .and_then(|state| state.get("error"))
                .and_then(|error| error.as_str())
                == Some(expected)
                || carries(
                    node.get("children")
                        .and_then(|children| children.as_array())
                        .map_or(&[][..], Vec::as_slice),
                    expected,
                )
        })
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        app.refresh();
        let found = app.semantics().is_some_and(|snapshot| {
            let main = snapshot
                .get("main")
                .and_then(|main| main.as_array())
                .is_some_and(|items| carries(items, expected));
            let overlays = snapshot
                .get("overlays")
                .and_then(|overlays| overlays.as_array())
                .is_some_and(|entries| {
                    entries.iter().filter_map(|entry| entry.get("tree")).any(|tree| {
                        tree.as_array().is_some_and(|items| carries(items, expected))
                    })
                });
            main || overlays
        });
        if found {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no node published the error `{expected}` within 5s"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// How many pixels in the band under the date field carry material3's `error` role.
///
/// The field draws its supporting text inside its own visual rather than as a child node, so no tree
/// names it — the pixels are where it actually is. `error` is the one red in the picker, so counting
/// red in that band is counting the message. Sampled a row at a time because supporting text is a
/// thin line of glyphs and a single scan line can miss it.
fn error_pixels_below_field(app: &mut UiTest) -> usize {
    app.refresh();
    let (x, y, w, h) = app
        .find_tag_in_overlay("date-picker-input-field")
        .expect("the date field");
    let mut points = Vec::new();
    for row in 0..14 {
        let py = y + h + 1.0 + row as f32 * 1.5;
        for column in 0..90 {
            points.push((x + 2.0 + column as f32 * (w - 4.0) / 90.0, py));
        }
    }
    app.pixels_at_logical(&points)
        .into_iter()
        .flatten()
        .filter(|(r, g, b, _)| *r > 120 && *r > *g + 50 && *r > *b + 50)
        .count()
}

/// An entry the validator refuses is reported under the field and does not become the selection.
/// `13312024` names month 13, which is not a month, so it parses to nothing; the pattern message is
/// what Compose gives for an entry that is a full field's width and still names no date
/// (`DateInput.kt:171-200`).
#[test]
fn an_unparseable_entry_is_reported_and_does_not_become_the_selection() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));

    click_overlay_tag(&mut app, "date-picker-input-field");
    for _ in 0..8 {
        app.key("Backspace");
    }
    assert_eq!(
        error_pixels_below_field(&mut app),
        0,
        "a field with no error is drawing one"
    );

    for key in ["1", "3", "3", "1", "2", "0", "2", "4"] {
        app.key(key);
    }
    assert!(
        error_pixels_below_field(&mut app) > 20,
        "the refused entry is not drawn under the field at all"
    );
    expect_semantics_error(&mut app, "Date does not match expected pattern: MM/DD/YYYY");
    app.expect_text_timeout("selected: none", Duration::from_secs(5));
}

/// A full entry naming a real date outside the picker's year range is refused too, and the message
/// is the range's rather than the pattern's — the ordering of `DateInputValidator.validate` is what
/// makes it so. `01051800` is January 5, 1800: a date, well below the default 1900..=2100 range, so
/// the year check is the only one that can refuse it.
#[test]
fn a_date_outside_the_year_range_is_reported_too() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));

    click_overlay_tag(&mut app, "date-picker-input-field");
    for _ in 0..8 {
        app.key("Backspace");
    }
    for key in ["0", "1", "0", "5", "1", "8", "0", "0"] {
        app.key(key);
    }
    assert!(
        error_pixels_below_field(&mut app) > 20,
        "the out-of-range entry is not drawn under the field at all"
    );
    expect_semantics_error(&mut app, "Date out of expected year range 1900 - 2100");
    app.expect_text_timeout("selected: none", Duration::from_secs(5));
}

/// An entry the field accepts carries no error, so the semantics tree has to say "not in error"
/// rather than stay silent — a reader has to be able to tell an invalid value from a valid one
/// nobody mentioned.
#[test]
fn a_field_with_no_error_says_nothing_about_one() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));
    app.expect_overlay_text_timeout("09/10/2024", Duration::from_secs(5));
    assert!(
        !app.overlay_texts().iter().any(|text| text.starts_with("Date ")),
        "a valid entry left an error message behind: {:?}",
        app.overlay_texts()
    );
}

/// The field asks for focus itself a moment after the modal opens — Compose's delayed
/// `focusRequester?.requestFocus()` (`DateInput.kt:259-266`, after `DurationMedium2`) — and the keys
/// that follow arrive on that focus with no click in between.
///
/// Only a real window can show where a key lands, which is why this is here and not in the lib
/// tests: `winit` delivers the key to the window, then the framework routes it to whatever holds
/// focus in the arena that owns the keyboard.
#[test]
fn the_entry_field_takes_focus_and_typing_without_a_click() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));

    // Nothing is clicked: the picker's own delayed request is the only thing that can focus this.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while app.focused_tags().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        app.focused_tags(),
        vec!["date-picker-input-field".to_string()],
        "the entry field should be the one node the picker focused"
    );

    // Keys go to that focus. A cleared field drops the selection; a full entry commits one.
    for _ in 0..8 {
        app.key("Backspace");
    }
    app.expect_text_timeout("selected: none", Duration::from_secs(5));
    for key in ["0", "3", "1", "2", "2", "0", "2", "4"] {
        app.key(key);
    }
    app.expect_text_timeout("selected: Mar 12, 2024", Duration::from_secs(5));
}

/// The dialog is only as tall as the mode it is showing, which is what material3's
/// `Box(Modifier.weight(1f, fill = false))` is for: the box's share is a MAXIMUM, so the picker's
/// own height decides and the column ends up content + buttons instead of the whole 568 dp cap
/// (`DatePickerDialog.android.kt:90-95`, whose comment reads "Fill is false to support collapsing
/// the dialog's height when switching to input mode").
///
/// The numbers are the assertion, not a screenshot: before that box existed winia reported the cap
/// in both modes — measured 568 dp with the field's content ending around 274.
///
/// Both figures assume the dialog's DEFAULT content, whose title is what gives the header its 120 dp
/// (`DatePicker` supplies `DatePickerDefaults::TITLE`); a caller's `.title(None)` drops the header to
/// 44 and the dialog with it, so this test would have to be re-derived for such a caller.
#[test]
fn the_dialog_is_as_tall_as_the_mode_it_shows() {
    let mut app = UiTest::launch("date_picker_input");
    app.expect_text_timeout("mode: input", Duration::from_secs(5));
    // Settled, not read once: the overlay's tree is published by its own layout, which can lag the
    // page's readout by a frame.
    let (_, _, _, input_h) = settled_overlay_rect(&mut app, "dpi-dialog");
    // 120 dp header + the outlined field's 56 + its 16 dp bottom inset + the 48 dp action row.
    assert_eq!(
        input_h, 240.0,
        "the entry field's content should decide the dialog's height"
    );

    app.click_overlay_tag("date-picker-mode-toggle");
    app.expect_text_timeout("mode: picker", Duration::from_secs(5));
    let (_, _, _, picker_h) = settled_overlay_rect(&mut app, "dpi-dialog");
    // 120 dp header + 56 dp month navigation + 48 dp weekday row + 288 dp month + 48 dp action row.
    // The 568 dp cap is a MAXIMUM, so this content lands 8 dp short of it rather than being padded
    // out — asserting the exact number is what stops a stretch creeping back in unnoticed.
    assert_eq!(
        picker_h, 560.0,
        "the calendar's own content should decide the dialog's height"
    );
}

/// Movable navigation payloads retain real nodes and live State dependencies across bar -> rail -> bar.
///
/// The fixture uses explicit buttons and disables transitions. Acceptance comes from the tagged
/// suite's actual layout children and payload bounds, never from requested-layout text or an
/// initializer count. Counters are read only inside each stored icon Column, while nested label
/// Columns render fresh Strings captured from caller State. The fixture binary uses production keys.
#[test]
fn a_navigation_suites_items_keep_their_state_across_a_shape_switch() {
    let mut app = UiTest::launch("nav_suite_state");
    let mut counts = [0usize; 3];
    let initial = nav_state_snapshot(&mut app, false, counts, 0);
    let identities = nav_state_payload_ids(&initial);
    assert_nav_state_layout(&app, &initial, false, &identities);

    // Unequal values catch shared slots as well as a counter whose dependency never invalidates.
    for index in 0..3 {
        for _ in 0..=index {
            app.refresh();
            app.click_tag(&format!("nav-state-increment-{index}"));
            counts[index] += 1;
            let tree = nav_state_snapshot(&mut app, false, counts, 0);
            assert_nav_state_layout(&app, &tree, false, &identities);
        }
    }
    app.click_tag("nav-state-label-next");
    let tree = nav_state_snapshot(&mut app, false, counts, 1);
    assert_nav_state_layout(&app, &tree, false, &identities);

    app.click_tag("nav-state-show-rail");
    let rail = nav_state_snapshot(&mut app, true, counts, 1);
    assert_nav_state_layout(&app, &rail, true, &identities);
    for index in 0..3 {
        app.refresh();
        app.click_tag(&format!("nav-state-increment-{index}"));
        counts[index] += 1;
        let tree = nav_state_snapshot(&mut app, true, counts, 1);
        assert_nav_state_layout(&app, &tree, true, &identities);
    }
    app.click_tag("nav-state-label-next");
    let tree = nav_state_snapshot(&mut app, true, counts, 2);
    assert_nav_state_layout(&app, &tree, true, &identities);

    app.click_tag("nav-state-show-bar");
    let returned = nav_state_snapshot(&mut app, false, counts, 2);
    assert_nav_state_layout(&app, &returned, false, &identities);
    for index in 0..3 {
        app.refresh();
        app.click_tag(&format!("nav-state-increment-{index}"));
        counts[index] += 1;
        let tree = nav_state_snapshot(&mut app, false, counts, 2);
        assert_nav_state_layout(&app, &tree, false, &identities);
    }
    app.click_tag("nav-state-label-next");
    let tree = nav_state_snapshot(&mut app, false, counts, 3);
    assert_nav_state_layout(&app, &tree, false, &identities);
}

/// Search the parsed TREE, retaining duplicate tags so a leftover payload cannot pass as a move.
fn nav_state_tagged_nodes<'a>(tree: &'a serde_json::Value, tag: &str) -> Vec<&'a serde_json::Value> {
    fn walk<'a>(node: &'a serde_json::Value, tag: &str, out: &mut Vec<&'a serde_json::Value>) {
        if let Some(nodes) = node.as_array() {
            for node in nodes {
                walk(node, tag, out);
            }
            return;
        }
        if node.get("tag").and_then(|value| value.as_str()) == Some(tag) {
            out.push(node);
        }
        for key in ["root", "children"] {
            if let Some(child) = node.get(key) {
                walk(child, tag, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(tree, tag, &mut out);
    out
}

fn nav_state_node<'a>(tree: &'a serde_json::Value, tag: &str) -> &'a serde_json::Value {
    let nodes = nav_state_tagged_nodes(tree, tag);
    assert_eq!(nodes.len(), 1, "exactly one real TREE node must carry `{tag}`");
    nodes[0]
}

fn nav_state_rect(node: &serde_json::Value) -> Option<[f32; 4]> {
    let pos = node.get("pos")?.as_array()?;
    let size = node.get("size")?.as_array()?;
    Some([
        pos.first()?.as_f64()? as f32,
        pos.get(1)?.as_f64()? as f32,
        size.first()?.as_f64()? as f32,
        size.get(1)?.as_f64()? as f32,
    ])
}

fn nav_state_rect_matches(actual: [f32; 4], expected: [f32; 4]) -> bool {
    actual.iter().zip(expected).all(|(actual, expected)| (actual - expected).abs() <= 1.0)
}

/// Poll only after one click: a lost dependency must fail instead of being hidden by another input.
fn nav_state_snapshot(
    app: &mut UiTest,
    rail: bool,
    counts: [usize; 3],
    label_version: usize,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = serde_json::Value::Null;
    loop {
        app.refresh();
        if let Some(tree) = app.tree() {
            let suites = nav_state_tagged_nodes(&tree, "nav-state-suite");
            let layout_matches = suites.first().and_then(|suite| suite.get("children"))
                .and_then(|children| children.as_array())
                .filter(|children| children.len() == 2)
                .map(|children| {
                    let expected = if rail {
                        [[0.0, 0.0, 96.0, 360.0], [96.0, 0.0, 404.0, 360.0]]
                    } else {
                        [[0.0, 0.0, 500.0, 280.0], [0.0, 280.0, 500.0, 80.0]]
                    };
                    children.iter().zip(expected).all(|(child, expected)| {
                        nav_state_rect(child).map(|rect| nav_state_rect_matches(rect, expected))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            let text_matches = (0..3).all(|index| {
                [
                    (format!("nav-state-count-{index}"), format!("text(I{index}: {})", counts[index])),
                    (format!("nav-state-label-text-{index}"), format!("text(L{index}: {label_version})")),
                ].iter().all(|(tag, text)| {
                    let nodes = nav_state_tagged_nodes(&tree, tag);
                    nodes.len() == 1 && nodes[0].get("mod").and_then(|value| value.as_str())
                        .map(|modifier| modifier.contains(text)).unwrap_or(false)
                })
            });
            if suites.len() == 1 && layout_matches && text_matches {
                return tree;
            }
            last = tree;
        }
        assert!(
            Instant::now() < deadline,
            "actual navigation layout and live payloads never matched rail={rail}, counters={counts:?}, label={label_version}; last TREE: {last}"
        );
        std::thread::sleep(Duration::from_millis(60));
    }
}

/// Numeric TREE ids come from LayoutNode.id, not slot text, arena positions, or initializer counts.
fn nav_state_payload_ids(tree: &serde_json::Value) -> Vec<(String, u64)> {
    let mut ids = Vec::new();
    for index in 0..3 {
        for prefix in ["icon", "count", "label", "label-text"] {
            let tag = format!("nav-state-{prefix}-{index}");
            let id = nav_state_node(tree, &tag).get("id").and_then(|value| value.as_u64())
                .unwrap_or_else(|| panic!("TREE must expose the real numeric node id for `{tag}`"));
            ids.push((tag, id));
        }
    }
    let unique: std::collections::HashSet<_> = ids.iter().map(|(_, id)| *id).collect();
    assert_eq!(unique.len(), ids.len(), "each retained payload node must have its own id");
    ids
}

fn assert_nav_state_bounds_inside(inner: [f32; 4], outer: [f32; 4], tag: &str) {
    assert!(
        inner[2] > 0.0 && inner[3] > 0.0
            && inner[0] >= outer[0] - 1.0 && inner[1] >= outer[1] - 1.0
            && inner[0] + inner[2] <= outer[0] + outer[2] + 1.0
            && inner[1] + inner[3] <= outer[1] + outer[3] + 1.0,
        "visible `{tag}` bounds {inner:?} must stay inside {outer:?}"
    );
}

fn assert_nav_state_layout(
    app: &UiTest,
    tree: &serde_json::Value,
    rail: bool,
    identities: &[(String, u64)],
) {
    assert_eq!(nav_state_payload_ids(tree).as_slice(), identities, "payload nodes must move, never rebuild");
    let suite = nav_state_node(tree, "nav-state-suite");
    let (sx, sy, sw, sh) = app.find_tag("nav-state-suite").expect("actual navigation suite bounds");
    assert!(nav_state_rect_matches([sx, sy, sw, sh], [0.0, 80.0, 500.0, 360.0]),
        "the suite must fill the area below both control rows: {:?}", [sx, sy, sw, sh]);
    let children = suite.get("children").and_then(|value| value.as_array()).expect("suite children");
    assert_eq!(children.len(), 2, "the real suite root must contain content and one morph container");
    let (content, morph) = if rail { (&children[1], &children[0]) } else { (&children[0], &children[1]) };
    // TREE exposes node identity and geometry, not MeasurePolicy type names. Direct children pin
    // the actual Column (content then bottom morph) or Row (leading morph then content) behavior.
    assert_eq!(nav_state_tagged_nodes(content, "nav-state-page").len(), 1,
        "the content child must own the page, not the navigation payloads");
    assert!(nav_state_tagged_nodes(morph, "nav-state-page").is_empty(), "the page must not move into navigation");
    let shape_children = morph.get("children").and_then(|value| value.as_array()).expect("morph children");
    assert_eq!(shape_children.len(), 1, "the morph must wrap exactly one actual navigation container");
    let shape = &shape_children[0];
    let items = shape.get("children").and_then(|value| value.as_array()).expect("navigation items");
    assert_eq!(items.len(), 3, "all three real navigation items must remain attached");
    let morph_rect = nav_state_rect(morph).expect("morph layout bounds");
    let navigation_bounds = [sx + morph_rect[0], sy + morph_rect[1], morph_rect[2], morph_rect[3]];
    let (px, py, pw, ph) = app.find_tag("nav-state-page").expect("page content bounds");
    let expected_page = if rail { [96.0, 80.0, 404.0, 360.0] } else { [0.0, 80.0, 500.0, 280.0] };
    assert!(nav_state_rect_matches([px, py, pw, ph], expected_page),
        "content must occupy the actual space left by navigation: {:?}", [px, py, pw, ph]);
    let expected_shape = if rail { [0.0, 0.0, 96.0, 360.0] } else { [0.0, 0.0, 500.0, 80.0] };
    assert!(nav_state_rect_matches(nav_state_rect(shape).expect("shape bounds"), expected_shape),
        "actual navigation container must fill the 96px rail or 80px bar");

    let mut previous_icon: Option<[f32; 4]> = None;
    for index in 0..3 {
        let icon_tag = format!("nav-state-icon-{index}");
        let label_tag = format!("nav-state-label-{index}");
        assert_eq!(nav_state_tagged_nodes(&items[index], &icon_tag).len(), 1, "actual navigation item {index} must own `{icon_tag}`");
        assert_eq!(nav_state_tagged_nodes(&items[index], &label_tag).len(), 1, "actual navigation item {index} must own `{label_tag}`");
        let item_rect = nav_state_rect(&items[index]).expect("actual navigation item bounds");
        let expected_width = if rail { 96.0 } else { (sw - 16.0) / 3.0 };
        assert!((item_rect[2] - expected_width).abs() <= 1.0,
            "actual navigation item {index} must use its allocated width, got {item_rect:?}");
        let (x, y, w, h) = app.find_tag(&icon_tag).expect("visible icon payload bounds");
        let icon = [x, y, w, h];
        assert!((w - (40.0 + index as f32 * 8.0)).abs() <= 1.0 && (h - 28.0).abs() <= 1.0,
            "retained icon {index} must keep its distinct measured size: {icon:?}");
        assert_nav_state_bounds_inside(icon, navigation_bounds, &icon_tag);
        let (lx, ly, lw, lh) = app.find_tag(&label_tag).expect("visible label payload bounds");
        let label = [lx, ly, lw, lh];
        assert!((lw - 56.0).abs() <= 1.0 && (lh - 16.0).abs() <= 1.0, "label {index} must retain its visible size: {label:?}");
        assert_nav_state_bounds_inside(label, navigation_bounds, &label_tag);
        assert!(ly >= y + h, "label {index} must be below its actual icon in both compact layouts");
        assert!(((x + w / 2.0) - (lx + lw / 2.0)).abs() <= 1.5,
            "label {index} and icon must share the actual item centre");
        let expected_centre = if rail { sx + 48.0 } else {
            let item_width = (sw - 16.0) / 3.0;
            sx + index as f32 * (item_width + 8.0) + item_width / 2.0
        };
        assert!((x + w / 2.0 - expected_centre).abs() <= 1.5,
            "icon {index} must occupy its real {} item, got {icon:?}", if rail { "rail" } else { "bar" });
        if let Some(previous) = previous_icon {
            if rail {
                assert!(y > previous[1] + previous[3], "rail icons must stack vertically without overlap");
            } else {
                assert!(x > previous[0] + previous[2] && (y - previous[1]).abs() <= 1.0,
                    "bar icons must form one horizontal row without overlap");
            }
        }
        previous_icon = Some(icon);
        for (tag, owner) in [
            (format!("nav-state-count-{index}"), icon),
            (format!("nav-state-label-text-{index}"), label),
        ] {
            let (x, y, w, h) = app.find_tag(&tag).expect("live payload text bounds");
            assert_nav_state_bounds_inside([x, y, w, h], owner, &tag);
        }
    }
}
