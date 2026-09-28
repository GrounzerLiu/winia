//! UI-test fixture: `DropdownMenu` — a popup menu anchored to a trigger.
//!
//! Drives `dropdown_menu_*` in `ui_test.rs`. The menu had no test of its own before this round, so these
//! tests pin the behaviour the component already had (open, item click, dismiss on an outside click, a
//! disabled item doing nothing) before the Compose-alignment work changes its geometry and API.
//!
//! What the fixture shows in the MAIN tree is the state a test can read without the popup:
//! `dm-open`, and `dm-picked` after an item fires. The menu itself lives in an overlay entry, which the
//! harness reads through `overlay_texts()` / `*_overlay_tag`.

use winia::prelude::*;
use winia::ui::overlay::{DropdownMenu, DropdownMenuItem};

#[composable]
fn dropdown_menu_fixture(ctx: &mut ComposeCtx) {
    let expanded = ctx.remember(|| false);
    let picked = ctx.remember(|| String::from("none"));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("DropdownMenu fixture").font_size(20.0).build(ctx);

            let e = expanded.clone();
            Button::text()
                .on_click(move || {
                    let v = e.get();
                    e.set(!v);
                })
                .modifier(Modifier::new().test_tag("dm-toggle"))
                .build(ctx, |ctx| Text::new("Open menu").build(ctx));

            Text::new(if expanded.get() { "dm-open: yes" } else { "dm-open: no" }).build(ctx);
            Text::new(format!("dm-picked: {}", picked.get())).build(ctx);

            // Anchor: the menu opens below this row (the trigger sits above it, so the popup lands in
            // the empty space under the header where a click cannot hit anything else).
            DropdownMenu::new(expanded.clone())
                .on_dismiss_request({
                    let e = expanded.clone();
                    move || e.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    {
                        let e_new = expanded.clone();
                        let e_rename = expanded.clone();
                        let p_new = picked.clone();
                        let p_rename = picked.clone();
                        move |ctx| {
                            // The menu closure is `Fn` (it runs again on every recomposition), so each
                            // item gets its own clones — the handles are `State<T>` and cheap to clone.
                            let (e_new, p_new) = (e_new.clone(), p_new.clone());
                            DropdownMenuItem::new("新建文件")
                                .modifier(Modifier::new().test_tag("dm-item-new"))
                                .on_click(move || {
                                    p_new.set(String::from("new"));
                                    e_new.set(false);
                                })
                                .build(ctx);
                            let (e_rename, p_rename) = (e_rename.clone(), p_rename.clone());
                            DropdownMenuItem::new("重命名")
                                .modifier(Modifier::new().test_tag("dm-item-rename"))
                                .on_click(move || {
                                    p_rename.set(String::from("rename"));
                                    e_rename.set(false);
                                })
                                .build(ctx);
                            // Disabled: it must not fire, and (per Compose) it must not dismiss either.
                            let p_delete = picked.clone();
                            DropdownMenuItem::new("删除")
                                .modifier(Modifier::new().test_tag("dm-item-delete"))
                                .enabled(false)
                                .on_click(move || p_delete.set(String::from("deleted")))
                                .build(ctx);
                        }
                    },
                );
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 520.0)
                .title("DropdownMenu Fixture")
                .build(ctx, |ctx| dropdown_menu_fixture(ctx));
        });
    });
}
