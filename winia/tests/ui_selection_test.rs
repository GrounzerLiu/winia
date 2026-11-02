//! UI coverage for the selection demo: the state panel observes a drag and drives the container,
//! \`DisableSelection\` keeps a run out of the table, and a double click selects one word.

#[path = "ui/support.rs"]
mod ui;

use ui::UiTest;

/// The payloads of the tree's text nodes. `all_texts` answers the nodes' `mod` descriptions, so a
/// text node arrives as `text(<payload>)`; the snapshot only refreshes on a query.
fn texts(app: &mut UiTest) -> Vec<String> {
    app.refresh();
    app.all_texts()
        .into_iter()
        .filter_map(|t| {
            let inner = t.strip_prefix("text(")?.strip_suffix(')')?;
            Some(inner.to_string())
        })
        .collect()
}

/// The readout lines a card prefixed with `label`.
fn readout(app: &mut UiTest, label: &str) -> Vec<String> {
    let prefix = format!("[{label}]");
    texts(app)
        .into_iter()
        .filter(|t| t.starts_with(&prefix))
        .collect()
}

/// Press and release at a tag's centre, through the real pointer path (`tap` is down + up; the
/// debug `c` click is a synthetic path the selection code never sees).
fn tap_tag(app: &mut UiTest, tag: &str) {
    let (x, y, w, h) = app.expect_tag(tag);
    app.tap(x + w / 2.0, y + h / 2.0);
}

#[test]
fn the_panel_observes_a_drag() {
    let mut app = UiTest::launch("selection");
    assert!(
        readout(&mut app, "main").iter().any(|t| t.contains("no selection")),
        "the panel starts empty: {:?}",
        readout(&mut app, "main")
    );

    let a = app.expect_tag("sel-a");
    let b = app.expect_tag("sel-b");
    app.drag(a.0 + 4.0, a.1 + a.3 / 2.0, b.0 + 120.0, b.1 + b.3 / 2.0);

    let lines = readout(&mut app, "main");
    assert!(
        lines.iter().any(|t| t.contains("first run")),
        "a drag has to reach the state: {lines:?}"
    );
}

#[test]
fn select_all_and_clear_drive_the_container() {
    let mut app = UiTest::launch("selection");
    tap_tag(&mut app, "sel-select-all");
    let lines = readout(&mut app, "main");
    assert!(
        lines.iter().any(|t| t.contains("first run") && t.contains("second run")),
        "select all covers the whole container: {lines:?}"
    );

    tap_tag(&mut app, "sel-clear");
    assert!(
        readout(&mut app, "main").iter().any(|t| t.contains("no selection")),
        "clear empties it: {:?}",
        readout(&mut app, "main")
    );
}

#[test]
fn a_disabled_run_is_not_selectable() {
    let mut app = UiTest::launch("selection");
    tap_tag(&mut app, "sel-disabled-select-all");

    let lines = readout(&mut app, "disabled");
    assert!(
        lines.iter().any(|t| t.contains("selectable")),
        "the selectable runs are covered: {lines:?}"
    );
    assert!(
        !lines.iter().any(|t| t.contains("excluded")),
        "the disabled run is not: {lines:?}"
    );
}

#[test]
fn a_double_click_selects_one_word() {
    let mut app = UiTest::launch("selection");
    let words = app.expect_tag("sel-words");
    let at = (words.0 + 55.0, words.1 + words.3 / 2.0);
    app.tap(at.0, at.1);
    app.tap(at.0, at.1);

    let lines = readout(&mut app, "words");
    let quoted = lines
        .iter()
        .find(|t| t.contains('"'))
        .unwrap_or_else(|| panic!("the readout has a quoted line: {lines:?}"))
        .clone();
    let inner = quoted.split('"').nth(1).unwrap_or_default();
    assert!(!inner.is_empty(), "a word is selected: {quoted}");
    assert!(!inner.contains(' '), "one word, not a run of them: {inner:?}");
}
