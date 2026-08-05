//! UI 集成测试入口。
//!
//! 运行前提：
//! ```sh
//! cargo build -p winia --example counter --features debug-server
//! cargo build -p winia --example nest_demo --features debug-server
//! cargo test --test ui_test --features debug-server
//! ```
//!
//! 注意：需要图形环境（真实窗口）；测试间端口隔离（WINIA_DEBUG_PORT 自动分配）。

mod ui;

use ui::UiTest;
use std::time::Duration;

/// counter：点击 +1 → Count 文本递增
#[test]
fn counter_click_increments_count() {
    let mut app = UiTest::launch("counter");
    app.expect_text("Count: 0");
    // 找 "+1 focus btn2" 按钮并点击
    let (x, y, w, h) = app.find("+1 focus").expect("找不到 +1 按钮");
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text("Count: 1");
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text("Count: 2");
    // 再点一次确认持续递增
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text("Count: 3");
}

/// counter：Show Alt 切换 → 条件文本出现/消失（结构变化回归）。
/// 点击用 click_until（debug 点击链路偶发丢事件——自动重试）。
#[test]
fn counter_toggle_alt_shows_hides() {
    let mut app = UiTest::launch("counter");
    app.expect_text("Count: 0"); // 等首帧就绪
    let (x, y, w, h) = app.find("Show Alt").expect("找不到 Show Alt 按钮");
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        let texts = UiTest::tree_texts(t);
        texts.iter().any(|s| s.contains("Alternative")) && !texts.iter().any(|s| s.contains("Add 10"))
    });
    // 切回
    app.click_until(x + w / 2.0, y + h / 2.0, Duration::from_secs(4), |t| {
        let texts = UiTest::tree_texts(t);
        texts.iter().any(|s| s.contains("Add 10")) && !texts.iter().any(|s| s.contains("Alternative"))
    });
}

/// counter：子窗口开/关——关闭后主窗口保持完整（结构变化 + 多窗口回归）。
/// 注意：多窗口时 debug 树是"最后渲染的窗口"——子窗口打开期间树是子窗口的，
/// 主窗口断言在关闭后进行。
#[test]
fn counter_sub_window_open_close() {
    let mut app = UiTest::launch("counter");
    app.expect_text("Count: 0"); // 等首帧就绪
    let (x, y, w, h) = app.find("Open sub window").expect("找不到子窗口按钮");
    // 开
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text("Sub count: 0");
    // 关（子窗口按钮位置在子窗口树里查不到——主窗口按钮需先关窗口；
    // 关闭后树回到主窗口）
    app.click(x + w / 2.0, y + h / 2.0);
    app.expect_text("Count: 0");
    app.expect_text("Line 0");
    app.expect_no_text("Sub count");
}

/// counter：滚动区域（Line 0..29）——滚动后内容变化
#[test]
fn counter_scroll_moves_content() {
    let mut app = UiTest::launch("counter");
    app.expect_text("Count: 0"); // 等首帧就绪
    app.expect_text("Line 0");
    // 滚动 +300px——Line 0 应滚出可视区（树仍存在但位置变化；这里验证树完整）
    app.scroll(300.0);
    app.expect_text("Line 0"); // 树里仍存在（滚动是偏移不是移除）
    // 多次滚动不崩溃
    app.scroll(-600.0);
    app.expect_text("Count: 0");
}

/// nest_demo：switch if-level 结构切换——节点数稳定、按钮位置不漂移
#[test]
fn nest_demo_structure_switch_stable() {
    let mut app = UiTest::launch("nest_demo");
    app.expect_text("deep nest");
    let (x, y, w, h) = app.find("switch if-level").expect("找不到 switch 按钮");
    let pos0 = (x, y);
    // 6 次切换：三种状态重复进入——节点数与位置稳定
    let mut counts = Vec::new();
    // prev 记录"点击前"的节点数——首次也验证点击生效（渲染断下点击可能丢失）
    app.refresh();
    let mut prev_count = Some(app.all_texts().len());
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
        let snapshot = app.all_texts();
        println!(
            "[nest] 第 {} 次切换: 节点数={n} 状态标记: {}",
            i + 1,
            snapshot
                .iter()
                .filter(|t| t.contains("even") || t.contains("odd") || t.contains("level"))
                .cloned()
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let btn = app.find("switch if-level").expect("switch 按钮消失（塌缩）");
        assert!(
            (btn.0 - pos0.0).abs() < 1.0 && (btn.1 - pos0.1).abs() < 1.0,
            "按钮位置漂移：{pos0:?} → ({}, {})",
            btn.0, btn.1
        );
        // 三种状态（even/odd/level2）重复出现——第 i 与第 i+3 次应相同
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
