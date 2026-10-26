#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

fn click(app: &mut UiTest, tag: &str) {
    let (x, y, w, h) = app.expect_tag(tag);
    app.click(x + w / 2.0, y + h / 2.0);
}

#[test]
fn probe_trace() {
    let mut app = UiTest::launch("lazy_item_animation");
    app.refresh();
    click(&mut app, "lazy-anim-spec");
    click(&mut app, "lazy-anim-spec");
    app.refresh();
    eprintln!("=== INSERT NOW ===");
    click(&mut app, "lazy-anim-insert-head");
    app.refresh();
    eprintln!("=== DONE ===");
}
