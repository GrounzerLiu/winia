#[path = "ui/support.rs"]
mod ui;

use std::time::Duration;
use ui::UiTest;

fn info(app: &mut UiTest) -> String {
    for _ in 0..6 {
        if let Some(t) = app.all_texts().into_iter().find(|t| t.contains(" items | spec ")) {
            return t;
        }
        app.refresh();
    }
    String::new()
}

fn click_tag(app: &mut UiTest, tag: &str) {
    let (x, y, w, h) = app.expect_tag(tag);
    app.click(x + w / 2.0, y + h / 2.0);
}

#[test]
fn probe_bottom_insert_demo() {
    let mut app = UiTest::launch("lazy_item_animation");
    app.refresh();
    for _ in 0..7 {
        click_tag(&mut app, "lazy-anim-insert-head");
    }
    std::thread::sleep(Duration::from_millis(600));
    app.refresh();
    println!("after 7 inserts: {}", info(&mut app));
    // Scroll to the very bottom by dragging up inside the list.
    let list = app.expect_tag("lazy-anim-list");
    for _ in 0..6 {
        app.drag(list.0 + list.2 / 2.0, list.1 + 400.0, list.0 + list.2 / 2.0, list.1 + 60.0);
        std::thread::sleep(Duration::from_millis(120));
    }
    std::thread::sleep(Duration::from_millis(500));
    app.refresh();
    println!("at bottom: {}", info(&mut app));
    // Now insert at the head and look for placement animations.
    click_tag(&mut app, "lazy-anim-insert-head");
    let anims = app.animations();
    let placements: Vec<&String> = anims.iter().filter(|l| l.contains("components=2")).collect();
    println!("anims {} placements {}", anims.len(), placements.len());
    for line in placements.iter().take(4) {
        println!("  {line}");
    }
    app.refresh();
    println!("after insert: {}", info(&mut app));
}
