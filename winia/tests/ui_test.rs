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
use std::time::Duration;

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
    // 滚动区约在 y 64..214（Column padding 16 + 标题 ~28 + offset 文本 ~20）
    app.drag(100.0, 190.0, 100.0, 90.0);
    let o1 = read_offset(&mut app);
    assert!(o1 > 30.0, "拖拽后 offset 应 > 0（内容跟随手指），实际 {o1}");
    // 松手后惯性 fling 继续推进（300ms 后读两次）
    std::thread::sleep(Duration::from_millis(300));
    let o2 = read_offset(&mut app);
    std::thread::sleep(Duration::from_millis(300));
    let o3 = read_offset(&mut app);
    assert!(o3 > o1, "fling 应继续推进 offset（{o1} -> {o3}）");
    assert!(o3 >= o2, "fling 应单调推进（{o2} -> {o3}）");
    assert!(o3 < 570.0, "fling 不应越过滚动极限 570，实际 {o3}");
}

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
    // 横向区约在 y 94..154（标题 ~32 + offset 文本 ~19 + hoffset 文本 ~19）
    app.drag(250.0, 120.0, 150.0, 120.0);
    let o1 = read_hoffset(&mut app);
    assert!(o1 > 30.0, "拖拽后 hoffset 应 > 0（内容跟随手指），实际 {o1}");
    // 松手后惯性 fling 继续推进
    std::thread::sleep(Duration::from_millis(300));
    let o2 = read_hoffset(&mut app);
    std::thread::sleep(Duration::from_millis(300));
    let o3 = read_hoffset(&mut app);
    assert!(o3 > o1, "fling 应继续推进 hoffset（{o1} -> {o3}）");
    assert!(o3 >= o2, "fling 应单调推进（{o2} -> {o3}）");
    assert!(o3 < 2100.0, "fling 不应越过滚动极限 2100，实际 {o3}");
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
    app.click_tag("username-field");
    for key in ["a", "b", "c"] { app.key(key); }
    app.expect_text_timeout("username: abc", Duration::from_secs(5));
    assert!(app.tag_is_focused("username-field"), "username 应保持焦点");
    app.key("Backspace");
    app.expect_text_timeout("username: ab", Duration::from_secs(5));
}

/// 密码字段只公开长度/有效性，并覆盖 Enter 产生多行的真实键盘路由。
#[test]
fn text_field_password_and_multiline_states_update() {
    let mut app = UiTest::launch("text_field");
    app.expect_text("password-status: invalid");
    app.click_tag("password-field");
    for key in ["p", "a", "s", "s"] { app.key(key); }
    app.expect_text_timeout("password-length: 4", Duration::from_secs(5));
    app.expect_text_timeout("password-status: valid", Duration::from_secs(5));
    app.expect_text("text(••••)");
    assert!(
        !app.all_texts().iter().any(|text| text.contains("text(pass)")),
        "密码输入节点不应暴露明文"
    );

    let (_, _, _, notes_h) = app.find_tag("notes-field").expect("notes tag");
    assert!(notes_h >= 56.0, "min_lines 字段应高于单行，实际 {notes_h}");
    app.click_tag("notes-field");
    app.key("n");
    app.key("Enter");
    app.key("2");
    app.expect_text_timeout("notes-lines: 2", Duration::from_secs(5));
    app.refresh();
    let (_, _, _, notes_h_after) = app.find_tag("notes-field").expect("notes tag after input");
    assert!(notes_h_after >= notes_h, "新增行后 TextField 不应塌缩：{notes_h} -> {notes_h_after}");
}

/// error/read-only/disabled 状态在真实输入路由中保持各自约束。
#[test]
fn text_field_error_readonly_and_disabled_states_are_enforced() {
    let mut app = UiTest::launch("text_field");
    app.expect_text("error-status: required");
    app.click_tag("error-field");
    app.key("x");
    app.expect_text_timeout("error-status: none", Duration::from_secs(5));

    app.click_tag("readonly-field");
    assert!(app.tag_is_focused("readonly-field"), "只读字段仍应获得焦点");
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
