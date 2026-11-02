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

/// The demo is taller than its window, so the body has to scroll — otherwise the lower cards are
/// unreachable (the last one sits at y=917 in a 1040-tall window with a 64-tall app bar above it).
#[test]
fn the_demo_scrolls_to_reach_the_lower_cards() {
    let mut app = UiTest::launch("selection");
    // One sample every 16 px: enough resolution to see the content move, half the round trips.
    let ys: Vec<f32> = (0..60).map(|i| 80.0 + i as f32 * 16.0).collect();
    let column = |app: &mut UiTest| -> Vec<Option<(u8, u8, u8, u8)>> {
        ys.iter().map(|y| app.pixel_at_logical(40.0, *y)).collect()
    };
    let top = column(&mut app);

    app.scroll(-300.0);
    let scrolled = column(&mut app);
    let changed = top.iter().zip(scrolled.iter()).filter(|(a, b)| a != b).count();
    assert!(changed > 10, "scrolling has to move the content, {changed} of {} samples differ", top.len());

    // Scrolling back to the top restores it: the offset is bounded, not accumulated forever.
    app.scroll(4000.0);
    let restored = column(&mut app);
    let same = top.iter().zip(restored.iter()).filter(|(a, b)| a == b).count();
    assert!(same > top.len() - 8, "scrolling back restores the top: {same} of {} samples match", top.len());
}
/// The Windows clipboard, through PowerShell: the only way to see what OTHER applications would
/// paste. `Set-Clipboard -Raw` semantics are close enough for a test (trailing newlines trimmed).
fn read_system_clipboard() -> String {
    // `[Console]::OutputEncoding` first: without it PowerShell writes the console code page and a
    // non-ASCII clipboard comes back as "????" (which is what the restore then put back).
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; Get-Clipboard -Raw",
        ])
        .output()
        .expect("powershell runs");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n").trim_end().to_string()
}

/// Put `text` back after the test. It travels as base64: a command-line argument mangles non-ASCII
/// (a Chinese clipboard came back as "????"), and writing a file first is refused inside the test
/// sandbox — the restore silently did nothing and the test's own text stayed on the clipboard.
fn write_system_clipboard(text: &str) {
    if text.is_empty() { return; }
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        encoded.push(ALPHABET[(n >> 18) as usize & 63] as char);
        encoded.push(ALPHABET[(n >> 12) as usize & 63] as char);
        encoded.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        encoded.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    let script = format!(
        "$b=[Convert]::FromBase64String('{encoded}'); Set-Clipboard -Value ([System.Text.Encoding]::UTF8.GetString($b))"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output();
}
#[test]
fn ctrl_c_puts_the_selection_on_the_clipboard() {
    let previous = read_system_clipboard();
    let mut app = UiTest::launch("selection");
    let a = app.expect_tag("sel-a");
    let b = app.expect_tag("sel-b");
    app.drag(a.0 + 4.0, a.1 + a.3 / 2.0, b.0 + 120.0, b.1 + b.3 / 2.0);
    assert!(
        readout(&mut app, "main").iter().any(|line| line.contains("first run")),
        "the drag has to select something first: {:?}",
        readout(&mut app, "main")
    );

    app.key("ctrl+c");
    let mut got = String::new();
    for _ in 0..40 {
        got = read_system_clipboard();
        if got.contains("first run") { break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    write_system_clipboard(&previous);
    assert!(
        got.contains("first run"),
        "the OS clipboard has to hold the selection, got {got:?}"
    );
}