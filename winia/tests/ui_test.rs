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
#[test]
fn bottom_sheet_drag_routing_keeps_the_list_in_charge_of_its_own_scroll() {
    // `find_tag_in_overlay` reads the cached tree, and the sheet animates its settle, so every read is
    // preceded by a refresh and every expectation is polled.
    fn row_y(app: &mut UiTest, tag: &str) -> Option<f32> {
        app.refresh();
        app.find_tag_in_overlay(tag).map(|(_, y, _, _)| y)
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

    // (2) Now that it is expanded, the same gesture scrolls the LIST: row 0 leaves the viewport.
    let start_y = row_y(&mut app, "bs-row-0").expect("row 0 after expanding") + 20.0;
    app.drag(x, start_y, x, start_y - 140.0);
    wait_for(&mut app, "an expanded sheet must let the list scroll (row 0 should leave)", |a| {
        row_y(a, "bs-row-0").is_none()
    });

    // (3) A downward drag now belongs to the LIST: it scrolls back and the sheet stays open.
    app.refresh();
    let visible = app
        .find_tag_in_overlay("bs-row-3")
        .or_else(|| app.find_tag_in_overlay("bs-row-1"))
        .expect("a visible row to drag down");
    app.drag(visible.0 + 20.0, visible.1 + 15.0, visible.0 + 20.0, visible.1 + 150.0);
    wait_for(&mut app, "a downward drag must scroll the list back, not close the sheet", |a| {
        row_y(a, "bs-row-0").is_some()
    });
    assert_eq!(app.overlay_count(), 1, "the sheet is still open after a downward drag on the list");

    // (4) A long downward drag with the list at its top goes to the SHEET — M3's dismissal gesture — and
    // from Expanded that is PartiallyExpanded first, so the sheet is still there (its footer moves down
    // with the panel).
    let before = row_y(&mut app, "bs-footer").expect("the sheet footer");
    let top = row_y(&mut app, "bs-row-0").expect("row 0 at the top again") + 15.0;
    app.drag(x, top, x, top + 400.0);
    wait_for(&mut app, "a downward drag at the top of the list must move the sheet", |a| {
        row_y(a, "bs-footer").is_some_and(|y| y > before + 50.0)
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

