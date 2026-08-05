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
